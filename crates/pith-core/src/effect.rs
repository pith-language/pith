//! The closed effect-category model: the category markers, the value-level
//! enumeration of them, and the traits that carry each category's machinery.

use pith_ids::{
    ActionComputationDigest, ManifestDigest, ObservationComputationDigest, PureComputationDigest,
};

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

/// Computes from immutable values. Terminating by construction; caches
/// indefinitely under its computation identity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pure;

/// Bounded external work with declared inputs, outputs, platform, and
/// capabilities. Cacheable by content identity when the executor honors the
/// declared contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Action;

/// Reads external state, recording source, revision, and freshness. Not
/// cacheable across revisions; carries a revision pin.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Observation;

/// Changes external state. Not cacheable as a result.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mutation;

/// Unmodeled effectful work: the adoption on-ramp and escape hatch. A
/// fixed-output boundary whose interior the engine cannot inspect.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Which effect category a request targets, a dependency edges over, or a
/// computation is of: the category markers as data. Blob fetches and
/// capability uses are not categories and stay out of it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EffectKind {
    /// A pure computation. A chain never stops for one: the synchronous
    /// core serves the step itself.
    Pure,
    /// An action computation. A chain stops for the step, which a driver
    /// must plan, authorize, and execute.
    Action,
    /// An observation computation. A chain stops for the step, which a
    /// driver serves by deriving the subject and attesting the revision it
    /// saw.
    Observation,
}

impl EffectKind {
    /// Every kind, in the order diagnostics list them.
    pub const ALL: [Self; 3] = [Self::Pure, Self::Action, Self::Observation];

    /// The `Need` construct a body yields to request this kind, spelled as
    /// diagnostics name steps.
    pub const fn step_name(self) -> &'static str {
        match self {
            Self::Pure => "Need",
            Self::Action => "NeedAction",
            Self::Observation => "NeedObservation",
        }
    }

    /// Whether this is the pure kind: the one whose steps a pure-only
    /// evaluation serves rather than refuses.
    pub const fn is_pure(self) -> bool {
        matches!(self, Self::Pure)
    }
}

/// An effect category whose rule applications carry a persistent computation
/// key, hashed under a digest domain of the category's own.
///
/// ```compile_fail
/// use pith_core::{ComputationCategory, EffectCategory};
///
/// struct Custom;
/// impl EffectCategory for Custom {
///     const CACHEABLE_AS_RESULT: bool = false;
/// }
/// impl ComputationCategory for Custom {
///     type Digest = pith_ids::PureComputationDigest;
/// }
/// ```
pub trait ComputationCategory: EffectCategory {
    /// The digest kind a keyed application of this category hashes its
    /// manifest under.
    type Digest: ManifestDigest;
}

impl ComputationCategory for Pure {
    type Digest = PureComputationDigest;
}

impl ComputationCategory for Action {
    type Digest = ActionComputationDigest;
}

impl ComputationCategory for Observation {
    type Digest = ObservationComputationDigest;
}

const _: () = {
    assert!(Pure::CACHEABLE_AS_RESULT);
    assert!(Action::CACHEABLE_AS_RESULT);
    assert!(!Observation::CACHEABLE_AS_RESULT);
    assert!(!Mutation::CACHEABLE_AS_RESULT);
    assert!(!Opaque::CACHEABLE_AS_RESULT);
};
