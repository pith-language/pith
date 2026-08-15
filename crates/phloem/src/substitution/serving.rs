use pith_core::{Pure, Request, Value};

use super::admission::{Refusal, admit};
use super::model::{Admission, Admitted, BinaryOffer};

/// How a locked package binding was served.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Serving {
    /// An admitted binary replaced the build.
    Substituted(Admitted),
    /// The build ran, optionally after an offer was refused.
    Built { refused: Option<Refusal> },
}

/// Selects substitution or build service for an optional offer.
#[must_use]
pub fn serve(admission: &Admission<'_>, offer: Option<(&BinaryOffer, &[u8])>) -> Serving {
    match offer {
        None => Serving::Built { refused: None },
        Some((offer, bytes)) => match admit(admission, offer, bytes) {
            Ok(admitted) => Serving::Substituted(admitted),
            Err(refusal) => Serving::Built {
                refused: Some(refusal),
            },
        },
    }
}

/// Returns the build request required by `serving`. A served substitution
/// builds nothing; a built one drives the package build over the package's
/// own tree with no dependencies, because a substitution stands in for one
/// binding's realization and carries no edge of its own.
#[must_use]
pub fn serving_request(
    serving: &Serving,
    toolchain: Value,
    tree: &crate::build::SourceTree,
    build: &crate::build::PackageBuild,
) -> Option<Request<Pure>> {
    match serving {
        Serving::Substituted(_) => None,
        Serving::Built { .. } => Some(crate::build::build_request(toolchain, tree, build, &[])),
    }
}
