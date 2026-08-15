use pith_core::{Pure, Request, Value};

use super::admission::{Refusal, admit};
use super::model::{Admission, Admitted, BinaryOffer};

/// What served a locked package version's binding: an admitted binary in
/// place of the build, or the build itself.
///
/// The two constructors are the record 0039 asks for: a build from source and
/// a substituted binary are different provenance claims about one package
/// version, and the package's identity absorbs neither. This is also where a
/// reader tells the two reuse paths apart. 0031 and 0033's reuse is the
/// engine's word about a computation it ran, reported as an
/// `EvaluationSource`; a substitution is phloem's word about a computation
/// nobody ran here, and it never reaches the engine at all.
///
/// The name is 0045's correction of an earlier one: this value says *how a
/// binding was served*, not what a realization is. A realization is the
/// attempt the engine already holds — the artifact's content identity and
/// the computation that produced it — and nothing in this module
/// constructs one. How-it-was-obtained and what-it-is are different
/// facts, and a name shared by both blurs which one the type carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Serving {
    /// An admitted binary stands in, and the build does not run.
    Substituted(Admitted),
    /// The build runs. `refused` is present when an offer was made and the
    /// admission test turned it down, which is a record rather than a fault.
    Built { refused: Option<Refusal> },
}

/// What served the locked package version: an admitted binary, or the
/// build. An absent offer builds with nothing to report.
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

/// The build request a serving leaves standing: none under a substitution,
/// the one package-build request under a build. This is the substitution's
/// whole observable effect on the graph. Nothing is diverted, redirected,
/// or served under another key; the request is simply not made, which is
/// what "in place of running the build" means.
#[must_use]
pub fn serving_request(
    serving: &Serving,
    toolchain: Value,
    tree: &crate::build::SourceTree,
    build: &crate::build::PackageBuild,
) -> Option<Request<Pure>> {
    match serving {
        Serving::Substituted(_) => None,
        Serving::Built { .. } => Some(crate::build::build_request(toolchain, tree, build)),
    }
}
