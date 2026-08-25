use std::path::Path;

use crate::report::Report;

pub(crate) mod determinism;
pub(crate) mod docs;
pub(crate) mod elaborator;

pub(crate) type Check = fn(&Path) -> Report;

/// The checks run by `cargo run -p xtask -- check`.
///
/// Registering a check here makes it part of the aggregate local and CI check.
pub(crate) const ALL: &[Check] = &[determinism::run, docs::run, elaborator::run];
