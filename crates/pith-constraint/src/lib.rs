//! The constraint model every admitting domain shares: version ranges over
//! a declared ordering, and admission as one refusal clause per claim.
//!
//! Both halves are mechanics without a subject. A domain supplies the
//! ordering its versions compare under and the clause vocabulary its
//! admissions refuse with; the interval algebra and the first-refusal
//! conjunction are the same machinery for every caller, which is what keeps
//! a second resolver algebra or a second admission boundary from growing
//! beside the first.
//!
//! Nothing here knows a spelling, a digest, or a store. Ranges evaluate
//! against whatever ordering the caller passes, and an admission never
//! reads: both are pure, so a caller composes them into effects at its own
//! boundary.

mod admit;
mod range;

pub use admit::{Refusal, admit};
pub use range::{ByOrdering, Compare, Edge, Range};
