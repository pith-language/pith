//! The closed effect-category model (decisions 0019, 0022).

mod private {
    pub trait Sealed {}
}

/// A kernel effect category.
///
/// This trait is sealed. Adding a category requires changing the kernel.
///
/// ```compile_fail
/// use pith_core::EffectCategory;
///
/// struct Custom;
/// impl EffectCategory for Custom {
///     const CACHEABLE_AS_RESULT: bool = false;
/// }
/// ```
pub trait EffectCategory: private::Sealed + 'static {
    const CACHEABLE_AS_RESULT: bool;
}

/// Computes from immutable values. Terminating by construction (0018); caches
/// indefinitely under its computation identity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pure;

/// Bounded external work with declared inputs, outputs, platform, and
/// capabilities. Cacheable by content identity when the executor honors the
/// declared contract (A-6).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Action;

/// Reads external state, recording source, revision, and freshness. Not
/// cacheable across revisions; carries a revision pin (0012).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Observation;

/// Changes external state. Not cacheable as a result (0019).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mutation;

/// Unmodeled effectful work: the adoption on-ramp and escape hatch. A
/// fixed-output boundary whose interior the engine cannot inspect.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Opaque;

macro_rules! effect_category {
    ($category:ty, $cacheable:expr) => {
        impl private::Sealed for $category {}

        impl EffectCategory for $category {
            const CACHEABLE_AS_RESULT: bool = $cacheable;
        }
    };
}

effect_category!(Pure, true);
effect_category!(Action, true);
effect_category!(Observation, false);
effect_category!(Mutation, false);
effect_category!(Opaque, false);

const _: () = {
    assert!(Pure::CACHEABLE_AS_RESULT);
    assert!(Action::CACHEABLE_AS_RESULT);
    assert!(!Observation::CACHEABLE_AS_RESULT);
    assert!(!Mutation::CACHEABLE_AS_RESULT);
    assert!(!Opaque::CACHEABLE_AS_RESULT);
};
