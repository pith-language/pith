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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pure;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Action;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Observation;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mutation;

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
