//! Preview collection from reusable attempts and everything they transitively
//! reference. Reclaimable counts are upper bounds until a retention policy
//! adds any additional pinned roots.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use pith_core::{Content, Value};
use pith_engine::state::{
    DurableActionProvenance, DurableAttempt, DurableAttemptId, DurableAttemptState,
    DurableComputation, DurableDependency, DurableProvenance, EncodedValue, EngineStateReader,
};
use pith_ids::ContentId;
use pith_output::dto::{ContentPreview, GcPreview};
use pith_store::{ContentStore, InventoryKind, TreeEntryContent};

use crate::error::QueryError;
use crate::session::{ReadOnly, Session, read_failure, store_failure};

impl Session<ReadOnly> {
    /// The dry-run preview `pith gc --dry-run` reports.
    ///
    /// # Errors
    /// [`QueryError`] when a present state database cannot be read, or the
    /// content store cannot be walked.
    pub fn gc_preview(&self) -> Result<GcPreview, QueryError> {
        let (all, roots) = match self.state_if_present()? {
            Some(state) => (
                state.all_attempts().map_err(read_failure)?,
                state.reusable_index_attempts().map_err(read_failure)?,
            ),
            None => (Box::default(), Box::default()),
        };
        let retained = retained_attempts(&all, &roots);
        let mut live = BTreeSet::new();
        for attempt in &retained {
            attempt_content_references(attempt, &mut live)?;
        }
        self.expand_live_trees(&mut live)?;
        let content = self.content_preview(&live)?;
        let retained_count = retained.len().try_into().unwrap_or(u64::MAX);
        Ok(GcPreview {
            roots: count(&roots),
            retained_attempts: retained_count,
            reclaimable_attempts: count(&all).saturating_sub(retained_count),
            content,
        })
    }

    /// A live tree keeps its entries live, and a live subtree does the same,
    /// so the closure follows every tree the seeds reached.
    fn expand_live_trees(&self, live: &mut BTreeSet<ContentId>) -> Result<(), QueryError> {
        let mut queue: VecDeque<ContentId> = live.iter().copied().collect();
        let mut expanded = BTreeSet::new();
        while let Some(id) = queue.pop_front() {
            if !expanded.insert(id) {
                continue;
            }
            let Some(tree) = self.content_store()?.get_tree(id).map_err(store_failure)? else {
                continue;
            };
            for entry in tree.entries() {
                match entry.content() {
                    TreeEntryContent::File(file) => {
                        let _ = live.insert(file.content);
                    }
                    TreeEntryContent::Tree(subtree) => {
                        if live.insert(*subtree) {
                            queue.push_back(*subtree);
                        }
                    }
                    TreeEntryContent::Symlink { .. } => {}
                }
            }
        }
        Ok(())
    }

    /// The inventory split into what the retained set keeps and what a
    /// collection under the R1 roots would reclaim.
    fn content_preview(&self, live: &BTreeSet<ContentId>) -> Result<ContentPreview, QueryError> {
        let inventory = self.content_store()?.inventory().map_err(store_failure)?;
        let mut preview = ContentPreview::default();
        for entry in inventory {
            let present = match entry.kind {
                InventoryKind::Blob => {
                    preview.blobs = preview.blobs.saturating_add(1);
                    &mut preview.live_blobs
                }
                InventoryKind::Tree => {
                    preview.trees = preview.trees.saturating_add(1);
                    &mut preview.live_trees
                }
            };
            preview.total_bytes = preview.total_bytes.saturating_add(entry.size);
            if live.contains(&entry.id) {
                *present = present.saturating_add(1);
                preview.live_bytes = preview.live_bytes.saturating_add(entry.size);
            }
        }
        preview.reclaimable_blobs = preview.blobs.saturating_sub(preview.live_blobs);
        preview.reclaimable_trees = preview.trees.saturating_sub(preview.live_trees);
        preview.reclaimable_bytes = preview.total_bytes.saturating_sub(preview.live_bytes);
        Ok(preview)
    }
}

fn count(attempts: &[Arc<DurableAttempt>]) -> u64 {
    attempts.len().try_into().unwrap_or(u64::MAX)
}

/// The attempts reachable from the roots over recorded dependency edges, in
/// identifier order.
fn retained_attempts<'a>(
    all: &'a [Arc<DurableAttempt>],
    roots: &[Arc<DurableAttempt>],
) -> Vec<&'a DurableAttempt> {
    let by_id: BTreeMap<DurableAttemptId, &DurableAttempt> =
        all.iter().map(|attempt| (attempt.id, &**attempt)).collect();
    let mut visited = BTreeSet::new();
    let mut frontier: VecDeque<DurableAttemptId> = roots.iter().map(|root| root.id).collect();
    while let Some(id) = frontier.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(attempt) = by_id.get(&id) {
            frontier.extend(
                dependencies_of(attempt)
                    .iter()
                    .filter_map(dependency_attempt),
            );
        }
    }
    visited
        .into_iter()
        .filter_map(|id| by_id.get(&id).copied())
        .collect()
}

fn dependencies_of(attempt: &DurableAttempt) -> &[DurableDependency] {
    match &attempt.state {
        DurableAttemptState::Pending => &[],
        DurableAttemptState::Complete(completion) => &completion.dependencies,
        DurableAttemptState::Failed(stopped) | DurableAttemptState::Cancelled(stopped) => {
            &stopped.dependencies
        }
    }
}

fn dependency_attempt(dependency: &DurableDependency) -> Option<DurableAttemptId> {
    match dependency {
        DurableDependency::Pure { attempt, .. }
        | DurableDependency::Action { attempt }
        | DurableDependency::Observation { attempt } => Some(*attempt),
        DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => None,
    }
}

/// Every content object one retained attempt keeps live.
fn attempt_content_references(
    attempt: &DurableAttempt,
    live: &mut BTreeSet<ContentId>,
) -> Result<(), QueryError> {
    for dependency in dependencies_of(attempt) {
        if let DurableDependency::Blob { content } = dependency {
            let _ = live.insert(*content);
        }
    }
    computation_references(&attempt.computation, live)?;
    match &attempt.state {
        DurableAttemptState::Complete(completion) => {
            value_references(&decoded(&completion.result)?, live);
            provenance_references(&completion.provenance, live);
        }
        DurableAttemptState::Failed(stopped) | DurableAttemptState::Cancelled(stopped) => {
            provenance_references(&stopped.provenance, live);
        }
        DurableAttemptState::Pending => {}
    }
    Ok(())
}

/// The inputs a computation was keyed over are part of what re-validating it
/// needs, so they keep their content live with it.
fn computation_references(
    computation: &DurableComputation,
    live: &mut BTreeSet<ContentId>,
) -> Result<(), QueryError> {
    match computation {
        DurableComputation::Pure(_) => Ok(()),
        DurableComputation::Action { request, .. } => references(&request.inputs, live),
        DurableComputation::Observation {
            request, subject, ..
        } => {
            references(&request.inputs, live)?;
            value_references(&decoded(subject)?, live);
            Ok(())
        }
    }
}

fn provenance_references(provenance: &DurableProvenance, live: &mut BTreeSet<ContentId>) {
    let DurableProvenance::Action(DurableActionProvenance::Imported { imported_report }) =
        provenance
    else {
        return;
    };
    for output in &imported_report.outputs {
        match output.content {
            Content::Blob(id) | Content::Tree(id) => {
                let _ = live.insert(id);
            }
        }
    }
}

fn references(encoded: &[EncodedValue], live: &mut BTreeSet<ContentId>) -> Result<(), QueryError> {
    for value in encoded {
        value_references(&decoded(value)?, live);
    }
    Ok(())
}

fn decoded(value: &EncodedValue) -> Result<Value, QueryError> {
    value.decode().map_err(|error| {
        QueryError::internal(format!("a retained result does not decode: {error}"))
    })
}

fn value_references(value: &Value, live: &mut BTreeSet<ContentId>) {
    match value {
        Value::Blob(content) => {
            let _ = live.insert(*content);
        }
        Value::Nominal { representation, .. } => value_references(representation, live),
        Value::List(items) => {
            for item in items.iter() {
                value_references(item, live);
            }
        }
        Value::Record(fields) => {
            for field in fields.iter() {
                value_references(&field.payload, live);
            }
        }
        Value::Sum { payload, .. } => {
            if let Some(payload) = payload {
                value_references(payload, live);
            }
        }
        Value::Unit | Value::Bool(_) | Value::Int(_) | Value::Text(_) | Value::Bytes(_) => {}
    }
}
