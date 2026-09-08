//! The constraint model every admitting domain shares: version ranges over
//! a declared ordering, and admission as one refusal clause per claim.
//! Nothing here knows a spelling, a digest, or a store; both halves are
//! pure, so the caller composes them into effects at its own boundary.

mod admit;
mod range;

pub use admit::{Refusal, admit};
pub use range::{ByOrdering, Compare, Edge, Range};
