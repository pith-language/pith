//! The module registry: a signed, append-only, metadata-only index.
//!
//! An entry says which content a released version is — the revision it
//! was measured from and the digest it measured to — and every field but
//! the signatures and the admission time is derived from that revision.
//! The index stores no module bytes; a serving host keeps content beside
//! it, addressed by the revision an entry names.

mod keys;
mod line;
mod tree;

pub use keys::{Detached, Public, Signing, SpellingError};
pub use line::{KeySet, Line, Pin, Release, Withdrawal};
pub use tree::{Measured, measure};
