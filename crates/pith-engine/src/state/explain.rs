//! Building an invalidation chain over the recorded dependency graph.
//!
//! Both adapters share this walk. Each has its own way to read an attempt (an
//! in-memory map, a `SELECT`), so they supply an [`AttemptLookup`] the way they
//! do for [`validate_publication`](super::validate::validate_publication). The
//! chain follows the single dependency the attempt's own
//! [`DurableReuseReason`] names, so the explanation
//! matches the reuse decision the attempt was published with rather than
//! re-deriving one.

use std::sync::Arc;

use super::validate::AttemptLookup;
use super::{
    CompletedAttempt, DurableAttempt, DurableAttemptId, DurableAttemptState, DurableComputation,
    DurableDependency, DurableReuseDecision, DurableReuseReason, EngineStateError,
    InvalidationExplanation, InvalidationReason,
};

/// Build the explanation for `attempt`, given how to read other attempts it
/// depends on.
///
/// `Ok(None)` when `attempt` is not a completed `NotReusable` record (there is
/// nothing to explain: a reusable attempt, a pending attempt, or a failed
/// attempt have no invalidation chain). `Ok(Some(_))` is the chain rooted at
/// `attempt`.
///
/// # Errors
/// Returns an adapter error only when `lookup` fails.
pub fn explain_invalidation(
    lookup: &impl AttemptLookup,
    attempt: DurableAttemptId,
) -> Result<Option<InvalidationExplanation>, EngineStateError> {
    build(lookup, attempt)
}

/// The recursive step. Visited is carried by reference so a cycle in the
/// recorded graph (which the publication validator rejects, but a lookup that
/// races with a concurrent writer could in principle surface) terminates
/// rather than overflows the stack.
fn build(
    lookup: &impl AttemptLookup,
    attempt_id: DurableAttemptId,
) -> Result<Option<InvalidationExplanation>, EngineStateError> {
    let Some(attempt) = lookup.lookup(attempt_id)? else {
        // The starting id is always supplied by the adapter from its own index;
        // reaching a missing record here means a corrupted graph, which the
        // caller surfaces as `Ok(None)` (no explanation available) rather than
        // a fault, mirroring the read methods' tolerance of a missing attempt.
        return Ok(None);
    };
    let computation = attempt.computation.clone();
    let DurableAttemptState::Complete(CompletedAttempt {
        dependencies,
        reuse: DurableReuseDecision::NotReusable(reason),
        ..
    }) = &attempt.state
    else {
        // Reusable, Pending, or Failed: no invalidation to explain.
        return Ok(None);
    };
    Ok(Some(explain_node(
        lookup,
        attempt_id,
        computation,
        reason,
        dependencies,
    )?))
}

/// Resolve `reason` against the attempt's recorded dependency list. A reason
/// that names a specific dependency edge recurses into that dependency's
/// explanation; a reason that does not (`ActionCachingDisabled`,
/// `DependencyMissing`) becomes a leaf.
fn explain_node(
    lookup: &impl AttemptLookup,
    attempt: DurableAttemptId,
    computation: DurableComputation,
    reason: &DurableReuseReason,
    dependencies: &[DurableDependency],
) -> Result<InvalidationExplanation, EngineStateError> {
    let named = named_dependency(reason);
    let Some(edge) = named.and_then(|target| {
        dependencies
            .iter()
            .find(|edge| edge_target(edge) == Some(target))
    }) else {
        return Ok(InvalidationExplanation {
            attempt,
            computation,
            reason: InvalidationReason::Leaf(reason.clone()),
        });
    };
    // A reason that names a dependency the recorded edges do not contain is a
    // leaf too: the chain cannot follow an edge that is not there, and the
    // stored reason still explains the attempt's own non-reuse.
    let Some(child) = build(lookup, edge_target(edge).unwrap_or(attempt))? else {
        return Ok(InvalidationExplanation {
            attempt,
            computation,
            reason: InvalidationReason::Leaf(reason.clone()),
        });
    };
    Ok(InvalidationExplanation {
        attempt,
        computation,
        reason: InvalidationReason::DependencyInvalidated {
            edge: edge.clone(),
            child: Box::new(child),
        },
    })
}

/// The attempt id a reuse reason names as the cause of non-reuse, if any.
fn named_dependency(reason: &DurableReuseReason) -> Option<DurableAttemptId> {
    match reason {
        DurableReuseReason::EffectfulDependency { attempt }
        | DurableReuseReason::DependencyPending { attempt }
        | DurableReuseReason::DependencyNotReusable { attempt } => Some(*attempt),
        DurableReuseReason::ActionCachingDisabled
        | DurableReuseReason::DependencyMissing { .. } => None,
    }
}

/// The target attempt of a dependency edge, if it has one. `Blob` and
/// `CapabilityUse` edges name no attempt and cannot anchor a chain.
fn edge_target(edge: &DurableDependency) -> Option<DurableAttemptId> {
    match edge {
        DurableDependency::Pure { attempt, .. } | DurableDependency::Action { attempt } => {
            Some(*attempt)
        }
        DurableDependency::Blob { .. } | DurableDependency::CapabilityUse { .. } => None,
    }
}

/// Read the latest completed reusable attempt for `computation` from the
/// adapter and explain it. Adapters that keep a reusable index call this with
/// the index result; the engine reuses it for the live-graph mirror by reading
/// the durable attempt of an arena computation.
///
/// `Ok(None)` when there is no reusable-completed attempt for the key, or when
/// that attempt is `Reusable` (nothing to explain).
///
/// # Errors
/// Returns an adapter error only when `lookup` fails.
pub fn explain_latest(
    lookup: &impl AttemptLookup,
    latest: Option<Arc<DurableAttempt>>,
) -> Result<Option<InvalidationExplanation>, EngineStateError> {
    let Some(latest) = latest else {
        return Ok(None);
    };
    let DurableAttemptState::Complete(CompletedAttempt {
        reuse: DurableReuseDecision::NotReusable(_),
        ..
    }) = &latest.state
    else {
        return Ok(None);
    };
    build(lookup, latest.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DurableProvenance, EncodedValue};
    use indexmap::IndexMap;
    use pith_core::{PureComputationKey, RuleIdentity, RuleRevision, Value};
    use pith_ids::{ContentDigest, DIGEST_LEN, PureComputationDigest};

    /// A lookup backed by a map, for exercising the chain walk without an
    /// adapter. Mirrors the in-memory adapter's `AttemptLookup for Records`.
    struct MapLookup(IndexMap<DurableAttemptId, Arc<DurableAttempt>>);

    impl MapLookup {
        fn from_attempts(attempts: impl IntoIterator<Item = Arc<DurableAttempt>>) -> Self {
            Self(attempts.into_iter().map(|a| (a.id, a)).collect())
        }
    }

    impl AttemptLookup for MapLookup {
        fn lookup(
            &self,
            attempt: DurableAttemptId,
        ) -> Result<Option<Arc<DurableAttempt>>, EngineStateError> {
            Ok(self.0.get(&attempt).cloned())
        }
    }

    fn pure_key(rule: u8, input: u8) -> PureComputationKey {
        let identity =
            RuleIdentity::of_module_declaration("pith-explain-tests", &format!("rule-{rule}"));
        PureComputationKey {
            rule_identity: identity,
            rule_revision: RuleRevision::of_manifest(identity, &[rule]),
            digest: PureComputationDigest::from_digest(ContentDigest::from_bytes(
                [input; DIGEST_LEN],
            )),
        }
    }

    fn completed(
        id: u64,
        computation: DurableComputation,
        reuse: DurableReuseDecision,
        dependencies: &[DurableDependency],
    ) -> Arc<DurableAttempt> {
        Arc::new(DurableAttempt {
            id: DurableAttemptId::from_raw(id),
            computation,
            state: DurableAttemptState::Complete(CompletedAttempt {
                dependencies: dependencies.to_vec().into_boxed_slice(),
                result: EncodedValue::from_value(&Value::Unit),
                provenance: DurableProvenance::Pure,
                reuse,
            }),
        })
    }

    fn not_reusable(
        id: u64,
        computation: DurableComputation,
        reason: DurableReuseReason,
        dependencies: &[DurableDependency],
    ) -> Arc<DurableAttempt> {
        completed(
            id,
            computation,
            DurableReuseDecision::NotReusable(reason),
            dependencies,
        )
    }

    #[test]
    fn reusable_attempt_has_nothing_to_explain() {
        let lookup = MapLookup::from_attempts([completed(
            1,
            DurableComputation::Pure(pure_key(1, 0)),
            DurableReuseDecision::Reusable,
            &[],
        )]);
        assert!(
            explain_invalidation(&lookup, DurableAttemptId::from_raw(1))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn leaf_reason_terminates_the_chain() {
        let lookup = MapLookup::from_attempts([not_reusable(
            1,
            DurableComputation::Pure(pure_key(1, 0)),
            DurableReuseReason::ActionCachingDisabled,
            &[],
        )]);
        let explanation = explain_invalidation(&lookup, DurableAttemptId::from_raw(1))
            .unwrap()
            .unwrap();
        assert_eq!(explanation.attempt, DurableAttemptId::from_raw(1));
        assert!(matches!(
            explanation.reason,
            InvalidationReason::Leaf(DurableReuseReason::ActionCachingDisabled)
        ));
    }

    #[test]
    fn chain_follows_a_named_dependency() {
        // Root names a non-reusable pure dependency; the chain must recurse.
        let child_key = pure_key(2, 0);
        let child = not_reusable(
            2,
            DurableComputation::Pure(child_key),
            DurableReuseReason::ActionCachingDisabled,
            &[],
        );
        let root = not_reusable(
            1,
            DurableComputation::Pure(pure_key(1, 0)),
            DurableReuseReason::DependencyNotReusable {
                attempt: DurableAttemptId::from_raw(2),
            },
            &[DurableDependency::Pure {
                computation: child_key,
                attempt: DurableAttemptId::from_raw(2),
            }],
        );
        let lookup = MapLookup::from_attempts([root, child]);
        let explanation = explain_invalidation(&lookup, DurableAttemptId::from_raw(1))
            .unwrap()
            .unwrap();
        let InvalidationReason::DependencyInvalidated { child, .. } = explanation.reason else {
            unreachable!("expected a chained reason, got {:?}", explanation.reason);
        };
        assert_eq!(child.attempt, DurableAttemptId::from_raw(2));
        assert!(matches!(
            child.reason,
            InvalidationReason::Leaf(DurableReuseReason::ActionCachingDisabled)
        ));
    }

    #[test]
    fn dependency_missing_is_a_leaf() {
        // DependencyMissing names a computation, not an attempt, so the chain
        // cannot follow it.
        let missing_key = pure_key(9, 0);
        let lookup = MapLookup::from_attempts([not_reusable(
            1,
            DurableComputation::Pure(pure_key(1, 0)),
            DurableReuseReason::DependencyMissing {
                computation: missing_key,
            },
            &[],
        )]);
        let explanation = explain_invalidation(&lookup, DurableAttemptId::from_raw(1))
            .unwrap()
            .unwrap();
        assert!(matches!(
            explanation.reason,
            InvalidationReason::Leaf(DurableReuseReason::DependencyMissing { .. })
        ));
    }
}
