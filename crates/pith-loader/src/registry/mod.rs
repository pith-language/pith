//! The module registry: a signed, append-only, metadata-only index.
//!
//! An entry names the revision a released version was measured from and
//! the digest it measured to; the trees those revisions name live beside
//! the index, in the serving host. `line` renders and `parse` reads the
//! documents, `trust` reads an index as a client, `publish` and `host`
//! are the registry's side, and `store` serves what passed.

mod host;
mod keys;
mod line;
mod parse;
mod publish;
mod range;
mod reading;
mod store;
mod token;
mod tree;
mod trust;

pub use host::Host;
pub use keys::{Detached, Public, Signing, SpellingError};
pub use line::{KeySet, Line, Pin, Release, Withdrawal};
pub use publish::{Draft, derive};
pub use store::{Location as RegistryLocation, RegistryStore};
pub use token::Malformed;
pub use tree::{Measured, measure};
pub use trust::{Record, Verified, read as read_index};

/// The index layout: a root file, one key-set document per domain, and one
/// append-only line file per subject.
pub(crate) const ROOT_FILE: &str = "root";
pub(crate) const DOMAINS_DIRECTORY: &str = "domains";
pub(crate) const INDEX_DIRECTORY: &str = "index";
/// Where a serving host keeps the trees its revisions name.
pub(crate) const SOURCES_DIRECTORY: &str = "sources";
