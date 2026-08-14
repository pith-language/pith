//! Building a record that shared validation accepts.
//!
//! Generated steps name attempts by selector, so edges are first resolved to
//! positions in the tracking table and then stamped with one store's
//! identifiers. The same generated record has to reach both stores, and each
//! names its own attempts.

use pith_core::{CapabilityRequirement, PureComputationKey};
use pith_ids::ContentId;

use crate::graph::canonical_capabilities;
use crate::state::{
    DurableAttemptId, DurableComputation, DurableDependency, DurableProvenance,
    DurableReuseDecision, DurableReuseReason,
};

use super::fixtures::content_id;
use super::run::{TerminalKind, Tracked, pick};
use super::scenario::GeneratedDependency;

/// Which store's identifier space a record is being built for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Space {
    Model,
    Subject,
}

impl Space {
    pub(super) fn identifier(self, entry: &Tracked) -> DurableAttemptId {
        match self {
            Self::Model => entry.model,
            Self::Subject => entry.subject,
        }
    }
}

/// A dependency edge resolved to a position in the tracking table, before it is
/// stamped with either store's identifiers.
pub(super) enum ResolvedDependency {
    Pure {
        computation: PureComputationKey,
        tracked: usize,
    },
    Action {
        tracked: usize,
    },
    Blob {
        content: ContentId,
    },
}

/// Resolve generated selectors against attempts that exist and satisfy
/// `eligible`. A selector matching nothing contributes no edge, so the step
/// stays valid rather than being discarded.
pub(super) fn resolve_dependencies(
    dependencies: &[GeneratedDependency],
    tracked: &[Tracked],
    eligible: fn(TerminalKind) -> bool,
) -> Vec<ResolvedDependency> {
    let candidates = |want_pure: bool| -> Vec<usize> {
        tracked
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.terminal.is_some_and(eligible)
                    && matches!(&entry.computation, DurableComputation::Pure(_)) == want_pure
            })
            .map(|(position, _)| position)
            .collect()
    };
    let pure_candidates = candidates(true);
    let action_candidates = candidates(false);

    let mut edges = Vec::new();
    for dependency in dependencies {
        match dependency {
            GeneratedDependency::Pure(selector) => {
                if let Some(position) = pick(*selector, &pure_candidates).copied()
                    && let Some(entry) = tracked.get(position)
                    && let DurableComputation::Pure(computation) = &entry.computation
                {
                    edges.push(ResolvedDependency::Pure {
                        computation: *computation,
                        tracked: position,
                    });
                }
            }
            GeneratedDependency::Action(selector) => {
                if let Some(position) = pick(*selector, &action_candidates).copied() {
                    edges.push(ResolvedDependency::Action { tracked: position });
                }
            }
            GeneratedDependency::Blob(seed) => edges.push(ResolvedDependency::Blob {
                content: content_id(*seed),
            }),
        }
    }
    edges
}

pub(super) fn materialize(
    resolved: &[ResolvedDependency],
    tracked: &[Tracked],
    space: Space,
) -> Vec<DurableDependency> {
    resolved
        .iter()
        .filter_map(|dependency| match dependency {
            ResolvedDependency::Pure {
                computation,
                tracked: position,
            } => Some(DurableDependency::Pure {
                computation: *computation,
                attempt: space.identifier(tracked.get(*position)?),
            }),
            ResolvedDependency::Action { tracked: position } => Some(DurableDependency::Action {
                attempt: space.identifier(tracked.get(*position)?),
            }),
            ResolvedDependency::Blob { content } => {
                Some(DurableDependency::Blob { content: *content })
            }
        })
        .collect()
}

/// The capability requirements a completed record carries (decision 0033):
/// an action's own declared contract, or the union of what the dependencies of
/// a pure computation carry. Derived rather than generated, because shared
/// validation rederives it the same way.
pub(super) fn required_capabilities(
    computation: &DurableComputation,
    resolved: &[ResolvedDependency],
    tracked: &[Tracked],
) -> Box<[CapabilityRequirement]> {
    match computation {
        DurableComputation::Action { plan, .. } => {
            canonical_capabilities(plan.spec().capabilities.iter())
        }
        DurableComputation::Pure(_) => canonical_capabilities(
            resolved
                .iter()
                .filter_map(|dependency| match dependency {
                    ResolvedDependency::Pure {
                        tracked: position, ..
                    }
                    | ResolvedDependency::Action {
                        tracked: position, ..
                    } => tracked.get(*position),
                    ResolvedDependency::Blob { .. } => None,
                })
                .flat_map(|entry| entry.capabilities.iter()),
        ),
    }
}

/// Shared validation compares capability-use edges against the executor report
/// canonically, so they are derived from the report rather than generated.
pub(super) fn with_capability_edges(
    mut edges: Vec<DurableDependency>,
    provenance: &DurableProvenance,
) -> Box<[DurableDependency]> {
    if let DurableProvenance::Action(action) = provenance {
        for capability in canonical_capabilities(action.capabilities_used()) {
            edges.push(DurableDependency::CapabilityUse { capability });
        }
    }
    edges.into_boxed_slice()
}

#[derive(Clone, Copy)]
pub(super) enum ReuseOutcome {
    Reusable,
    NotReusable(Refusal),
}

#[derive(Clone, Copy)]
pub(super) enum Refusal {
    ActionCachingDisabled,
    DependencyNotReusable { tracked: usize },
}

impl ReuseOutcome {
    pub(super) fn materialize(self, tracked: &[Tracked], space: Space) -> DurableReuseDecision {
        let reason = match self {
            Self::Reusable => return DurableReuseDecision::Reusable,
            Self::NotReusable(Refusal::ActionCachingDisabled) => {
                DurableReuseReason::ActionCachingDisabled
            }
            Self::NotReusable(Refusal::DependencyNotReusable { tracked: position }) => {
                match tracked.get(position) {
                    Some(entry) => DurableReuseReason::DependencyNotReusable {
                        attempt: space.identifier(entry),
                    },
                    None => return DurableReuseDecision::Reusable,
                }
            }
        };
        DurableReuseDecision::NotReusable(reason)
    }
}

pub(super) fn reuse_decision(
    computation: &DurableComputation,
    dependencies: &[ResolvedDependency],
    tracked: &[Tracked],
    corrupt: bool,
) -> ReuseOutcome {
    let first_non_reusable = dependencies.iter().find_map(|dependency| {
        let position = match dependency {
            ResolvedDependency::Pure { tracked, .. } | ResolvedDependency::Action { tracked } => {
                *tracked
            }
            ResolvedDependency::Blob { .. } => return None,
        };
        let entry = tracked.get(position)?;
        (entry.terminal != Some(TerminalKind::CompleteReusable)).then_some(position)
    });
    // An action edge no longer refuses reuse on its own (decision 0033), so the
    // only edge that does is one that is not itself reusable. This mirrors
    // `state::validate::validate_reuse_decision`, which is the derivation both
    // adapters are held to.
    let honest = match first_non_reusable {
        Some(tracked) => ReuseOutcome::NotReusable(Refusal::DependencyNotReusable { tracked }),
        None => ReuseOutcome::Reusable,
    };
    match (corrupt, honest) {
        (false, honest) => honest,
        // Refusing to index is sound for an action, so the corrupt flag reaches
        // a second valid decision here, which both adapters must accept. This
        // is the generated path that records `ActionCachingDisabled`.
        (true, ReuseOutcome::Reusable)
            if matches!(computation, DurableComputation::Action { .. }) =>
        {
            ReuseOutcome::NotReusable(Refusal::ActionCachingDisabled)
        }
        (true, ReuseOutcome::Reusable) => {
            ReuseOutcome::NotReusable(Refusal::DependencyNotReusable {
                tracked: usize::MAX,
            })
        }
        (true, ReuseOutcome::NotReusable(_)) => ReuseOutcome::Reusable,
    }
}
