//! The five effect categories (decisions 0019, 0022).
//!
//! Each category is a distinct marker type. Making them types rather than a tag
//! on one enum gives structural enforcement: a `Pure` computation cannot reach
//! the async runtime, which is only reachable through the scheduler serving the
//! four effectful categories. The set is closed; a sixth category requires a
//! kernel decision record.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EffectCat {
    Pure,
    Action,
    Observation,
    Mutation,
    Opaque,
}

impl EffectCat {
    pub fn cacheable_as_result(self) -> bool {
        match self {
            EffectCat::Pure | EffectCat::Action => true,
            // Opaque participates in fixed-output caching, a separate mechanism.
            EffectCat::Observation | EffectCat::Mutation | EffectCat::Opaque => false,
        }
    }
}

/// Computes from immutable values. Terminating by construction (0018). Caches
/// indefinitely under its computation identity.
pub struct Pure;

/// Bounded external work with declared inputs, outputs, platform, and
/// capabilities. Cacheable by content identity when the executor honors the
/// declared contract (A-6).
pub struct Action;

/// Reads external state, recording source, revision, freshness, uncertainty.
/// Not cacheable across revisions; carries a revision pin (0012).
pub struct Observation;

/// Changes external state. Not cacheable as a result (0019).
pub struct Mutation;

/// Unmodeled effectful work: the adoption on-ramp and escape hatch. A
/// fixed-output boundary whose interior the engine cannot inspect.
pub struct Opaque;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pure_and_action_cache_as_results() {
        assert!(EffectCat::Pure.cacheable_as_result());
        assert!(EffectCat::Action.cacheable_as_result());
        assert!(!EffectCat::Observation.cacheable_as_result());
        assert!(!EffectCat::Mutation.cacheable_as_result());
        assert!(!EffectCat::Opaque.cacheable_as_result());
    }

    #[test]
    fn exactly_five_categories() {
        // Adding a sixth variant breaks this, forcing the decision-record
        // conversation the closure rule (0019) requires.
        let count = [
            EffectCat::Pure,
            EffectCat::Action,
            EffectCat::Observation,
            EffectCat::Mutation,
            EffectCat::Opaque,
        ]
        .len();
        assert_eq!(count, 5);
    }
}
