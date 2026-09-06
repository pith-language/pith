//! The module registry: a signed, append-only, metadata-only index.
//!
//! An entry says which content a released version is — the revision it
//! was measured from and the digest it measured to — and every field but
//! the signatures and the admission time is derived from that revision, so
//! verifying the index is re-deriving it. The index stores no module
//! bytes; a serving host keeps content beside it, addressed by the
//! revision an entry names.
//!
//! Trust is four checks, each refused at its own boundary: the pinned
//! root key over the domain key sets, key-set generations against the
//! consumer's record, each line's signature against its domain's keys,
//! and the whole index against the append-only state the consumer
//! previously admitted.

mod host;
mod keys;
mod line;
mod publish;
mod store;
mod tree;
mod trust;

pub use host::Host;
pub use keys::{Detached, Public, Signing, SpellingError};
pub use line::{KeySet, Line, Malformed, Pin, Release, Withdrawal};
pub use publish::{Draft, derive};
pub use store::{Location as RegistryLocation, RegistryStore};
pub use tree::{Measured, measure};
pub use trust::{Record, Verified, read as read_index};

/// The index layout: a root file, one key-set document per domain, and one
/// append-only line file per subject.
pub(crate) const ROOT_FILE: &str = "root";
pub(crate) const DOMAINS_DIRECTORY: &str = "domains";
pub(crate) const INDEX_DIRECTORY: &str = "index";
/// Where a serving host keeps the trees its revisions name.
pub(crate) const SOURCES_DIRECTORY: &str = "sources";
