//! Observation planning and the observer adapter boundary (decision 0060).
//!
//! An observation is the third effect category with an operational path: a
//! rule body yields [`PureStep::NeedObservation`](crate::PureStep), the engine
//! selects an observation rule by interface, the rule derives a *subject* from
//! the request inputs the way an action rule plans a contract, and the host's
//! [`Observer`] does the looking. The revision the observer attests is the
//! freshness half of observation identity: the engine never interprets it, it
//! checks equality (0012, 0060).

use pith_core::Value;
use pith_diag::PithResult;

use crate::RunBound;

/// Deterministically derive what a request observes from its typed inputs.
///
/// The subject is a value the observer and the rule agree on — a record naming
/// a path, a resource, a host. It participates in the observation computation
/// key, so two requests over one rule derive one subject only if their inputs
/// say so.
pub trait ObservationRule: Send + Sync {
    /// # Errors
    /// Returns structured diagnostics when the inputs cannot name a subject.
    fn subject(&self, inputs: &[Value]) -> PithResult<Value>;
}

/// What one observation returned: the value the requesting body resumes with,
/// and the revision the observer attested for the world it read. The revision
/// is recorded beside the attempt and re-attested when the attempt is
/// considered for reuse; it reaches rule bodies only if the observer also
/// embeds it in the value, because a plan's pin set is a projection of the
/// recorded graph, not a fact bodies carry (0012).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    pub value: Value,
    pub revision: Value,
}

/// Who observed, before anything has been observed. A recorded attempt
/// attested by one observer is not admitted by another, on 0031's own split:
/// the attestation is the observer's semantics, and two observers may attest
/// different revisions for one world.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObserverIdentity {
    pub observer: Box<str>,
}

/// Adapter boundary for observing external state: the read-only counterpart of
/// [`Executor`](crate::Executor).
///
/// `attest` is the cheap half — the conditional-GET, the stat, the
/// metadata-only read — and exists so a recorded attempt can be admitted
/// without reobserving. An adapter with no cheaper attestation implements it
/// by observing and returning the same revision; the engine cannot tell the
/// difference and does not need to.
#[async_trait::async_trait]
pub trait Observer: Send + Sync {
    /// Who this observer is, asked before any recorded attempt is admitted.
    fn identity(&self) -> ObserverIdentity;

    /// Attest the world's current revision of `subject` without producing the
    /// observed value. `bound` is the caller's authority for this run; an
    /// adapter that can block is responsible for enforcing its deadline.
    ///
    /// # Errors
    /// Returns structured diagnostics when the world cannot be reached.
    async fn attest(&self, subject: &Value, bound: &RunBound) -> PithResult<Value>;

    /// Observe `subject`, returning the value and the revision it was read at.
    /// `bound` has the same meaning as it does for [`Self::attest`].
    ///
    /// # Errors
    /// Returns structured diagnostics when the world cannot be read.
    async fn observe(&self, subject: &Value, bound: &RunBound) -> PithResult<Observed>;
}
