//! State inspection and the dry-run preview, over the same roots a driver
//! resolves: a real filesystem content store and a real sqlite state
//! database.

use pith_core::{
    Interface, Pure, PureComputationKey, Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::Span;
use pith_engine::state::{
    CompletedAttempt, DurableComputation, DurableDependency, DurableProvenance,
    DurableReuseDecision, EncodedValue, EngineStateStore,
};
use pith_ids::ContentId;
use pith_query::{Roots, Session};
use pith_state_sqlite::SqliteEngineStateStore;
use pith_store::{ContentStore, FilesystemContentStore, Tree, TreeEntry, TreeEntryContent};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn interface() -> Interface {
    Interface {
        inputs: Box::new([]),
        output: Type::Int,
    }
}

fn key(label: &str) -> (Rule<Pure>, PureComputationKey) {
    let identity = RuleIdentity::of_module_declaration("pith-query-fixture", label);
    let revision = RuleRevision::of_manifest(identity, b"pith-query-fixture-v1");
    let rule = Rule::<Pure>::new(
        "pith-query-fixture",
        revision,
        label,
        interface(),
        Span::none(),
    );
    let request = Request::<Pure>::new(label, interface(), [], Span::none());
    let key = PureComputationKey::new(&rule, &request);
    (rule, key)
}

fn complete(dependencies: Box<[DurableDependency]>, result: Value) -> CompletedAttempt {
    CompletedAttempt {
        dependencies,
        result: EncodedValue::from_value(&result),
        provenance: DurableProvenance::Pure,
        reuse: DurableReuseDecision::Reusable,
        capabilities: Box::new([]),
    }
}

/// A fixture store with named blobs, one unreferenced blob, and a tree whose
/// file blob is only reachable through the tree.
struct Content {
    referenced: ContentId,
    in_result: ContentId,
    unreferenced: ContentId,
    tree: ContentId,
    in_tree: ContentId,
}

fn admit_content(roots: &Roots) -> TestResult<Content> {
    let mut store = FilesystemContentStore::open(roots.store())?;
    let referenced = store.put_blob(b"referenced by an edge")?;
    let in_result = store.put_blob(b"named by a result value")?;
    let unreferenced = store.put_blob(b"admitted and held nowhere")?;
    let in_tree = store.put_blob(b"inside a tree")?;
    let tree = Tree::new([TreeEntry::new(
        "nested",
        TreeEntryContent::File(pith_store::FileContent {
            content: in_tree,
            executable: false,
        }),
    )?])?;
    let tree = store.put_tree(tree)?;
    Ok(Content {
        referenced,
        in_result,
        unreferenced,
        tree,
        in_tree,
    })
}

/// Four attempts over three keys: two publications of one key (the second
/// supersedes the first in the index), one attempt whose edge keeps the
/// superseded publication retained, and one attempt that never finished.
fn record_attempts(roots: &Roots, content: &Content) -> TestResult<()> {
    let store = SqliteEngineStateStore::open(roots.state())?;
    let (_, first_key) = key("constant");

    let superseded = store.create_pending_attempt(DurableComputation::Pure(first_key))?;
    store.publish_complete(
        superseded,
        complete(
            Box::new([DurableDependency::Blob {
                content: content.referenced,
            }]),
            Value::Int(1.into()),
        ),
    )?;

    let latest = store.create_pending_attempt(DurableComputation::Pure(first_key))?;
    store.publish_complete(latest, complete(Box::new([]), Value::Blob(content.tree)))?;

    let (_, dependent_key) = key("dependent");
    let dependent = store.create_pending_attempt(DurableComputation::Pure(dependent_key))?;
    store.publish_complete(
        dependent,
        complete(
            Box::new([DurableDependency::Pure {
                computation: first_key,
                attempt: superseded,
            }]),
            Value::Blob(content.in_result),
        ),
    )?;

    let (_, unfinished_key) = key("unfinished");
    let _ = store.create_pending_attempt(DurableComputation::Pure(unfinished_key))?;
    Ok(())
}

#[test]
fn a_machine_that_recorded_nothing_still_gets_a_report() -> TestResult {
    let home = tempfile::tempdir()?;
    let roots = Roots::under(home.path());
    let session = Session::<pith_query::ReadOnly>::open(roots)?;

    let info = session.state_info()?;
    assert_eq!(info.attempts.total, 0, "{info:?}");
    assert_eq!(info.reusable_index, 0, "{info:?}");
    assert_eq!(
        info.schema_version,
        SqliteEngineStateStore::current_versions().schema.get()
    );
    assert_eq!(session.state_check()?.records, 0);

    let preview = session.gc_preview()?;
    assert_eq!(preview.roots, 0, "{preview:?}");
    assert_eq!(preview.content.blobs, 0, "{preview:?}");
    Ok(())
}

#[test]
fn inspection_reports_the_counts_the_records_hold() -> TestResult {
    let home = tempfile::tempdir()?;
    let roots = Roots::under(home.path());
    let content = admit_content(&roots)?;
    record_attempts(&roots, &content)?;

    let session = Session::<pith_query::ReadOnly>::open(roots)?;
    let info = session.state_info()?;
    assert_eq!(info.attempts.total, 4, "{info:?}");
    assert_eq!(info.attempts.complete, 3, "{info:?}");
    assert_eq!(info.attempts.pending, 1, "{info:?}");
    assert_eq!(info.reusable_index, 2, "{info:?}");
    assert_eq!(session.state_check()?.records, 4);
    Ok(())
}

/// The root set is the reusable index, an edge keeps a superseded publication
/// retained, a tree keeps its file live, and content no retained record names
/// is the reclaimable half. The attempt left pending is reclaimable too: a
/// root set of completed attempts cannot reach it.
#[test]
fn the_dry_run_keeps_what_the_roots_reach_and_names_the_rest() -> TestResult {
    let home = tempfile::tempdir()?;
    let roots = Roots::under(home.path());
    let content = admit_content(&roots)?;
    record_attempts(&roots, &content)?;

    let session = Session::<pith_query::ReadOnly>::open(roots)?;
    let preview = session.gc_preview()?;

    assert_eq!(preview.roots, 2, "{preview:?}");
    assert_eq!(preview.retained_attempts, 3, "{preview:?}");
    assert_eq!(preview.reclaimable_attempts, 1, "{preview:?}");

    let content_preview = &preview.content;
    assert_eq!(content_preview.blobs, 4, "{content_preview:?}");
    assert_eq!(content_preview.trees, 1, "{content_preview:?}");
    assert_eq!(content_preview.live_blobs, 3, "{content_preview:?}");
    assert_eq!(content_preview.live_trees, 1, "{content_preview:?}");
    assert_eq!(content_preview.reclaimable_blobs, 1, "{content_preview:?}");
    assert_eq!(content_preview.reclaimable_trees, 0, "{content_preview:?}");

    let store = FilesystemContentStore::open(session.roots().store())?;
    let unreferenced = store
        .get_blob(content.unreferenced)?
        .unwrap_or_else(|| unreachable!("the fixture blob is missing"));
    let unreferenced_size = u64::try_from(unreferenced.as_bytes().len()).unwrap_or_default();
    assert_eq!(
        content_preview.reclaimable_bytes, unreferenced_size,
        "{content_preview:?}"
    );

    // The live bytes are exactly the edge-named blob, the result-named blob,
    // the tree, and the tree's own file — the last reached only through the
    // tree's entries.
    let object_size = |id: ContentId| -> TestResult<u64> {
        let blob = store
            .get_blob(id)?
            .unwrap_or_else(|| unreachable!("the fixture blob is missing"));
        Ok(u64::try_from(blob.as_bytes().len()).unwrap_or_default())
    };
    let manifest_size = store
        .inventory()?
        .iter()
        .find(|entry| entry.id == content.tree)
        .map(|entry| entry.size)
        .unwrap_or_default();
    let expected_live = object_size(content.referenced)?
        .saturating_add(object_size(content.in_result)?)
        .saturating_add(object_size(content.in_tree)?)
        .saturating_add(manifest_size);
    assert_eq!(
        content_preview.live_bytes, expected_live,
        "{content_preview:?}"
    );
    Ok(())
}

/// Content admitted without engine state has no roots to retain it.
#[test]
fn content_without_state_is_all_reclaimable() -> TestResult {
    let home = tempfile::tempdir()?;
    let roots = Roots::under(home.path());
    let _ = admit_content(&roots)?;

    let session = Session::<pith_query::ReadOnly>::open(roots)?;
    let preview = session.gc_preview()?;

    assert_eq!(preview.roots, 0, "{preview:?}");
    assert_eq!(preview.content.live_blobs, 0, "{preview:?}");
    assert_eq!(preview.content.reclaimable_blobs, 4, "{preview:?}");
    assert_eq!(preview.content.reclaimable_trees, 1, "{preview:?}");
    Ok(())
}

/// An existing file that is not a state database is a failure to read, not an
/// empty store: the presence check is the only thing standing between a
/// mistyped `--state` and a report of zeros over it.
#[test]
fn a_broken_state_database_is_an_error_and_not_an_empty_store() -> TestResult {
    let home = tempfile::tempdir()?;
    let roots = Roots::under(home.path());
    std::fs::write(roots.state(), b"not a database")?;

    let session = Session::<pith_query::ReadOnly>::open(roots)?;
    assert!(session.state_info().is_err(), "a broken file read as empty");
    Ok(())
}
