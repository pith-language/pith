use pith_core::Value;
use pith_engine::ExecutionPlatform;
use pith_ids::ContentId;

use crate::identity::PackageVersion;
use crate::lock::Origin;

use super::model::{Admission, Admitted, BinaryOffer};

/// The clauses of the admission test, one per claim: a variant exists for
/// each fact the test consults, and a refusal names exactly one, with both
/// of its sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clause {
    Coordinates {
        bound: PackageVersion,
        offered: PackageVersion,
    },
    Features {
        bound: Box<[Box<str>]>,
        offered: Box<[Box<str>]>,
    },
    Source {
        bound: ContentId,
        offered: ContentId,
    },
    Platform {
        running: ExecutionPlatform,
        offered: ExecutionPlatform,
    },
    Toolchain {
        running: Value,
        offered: Value,
    },
    Content {
        claimed: ContentId,
        measured: ContentId,
    },
    Unauthorized {
        origin: Origin,
        admitted: Box<[Origin]>,
    },
}

impl std::fmt::Display for Clause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinates { bound, offered } => write!(
                formatter,
                "the lock binds `{}` in `{}` version {}, and the binary is offered for \
                 `{}` in `{}` version {}",
                bound.identity().name(),
                bound.identity().domain().as_str(),
                bound.version(),
                offered.identity().name(),
                offered.identity().domain().as_str(),
                offered.version(),
            ),
            Self::Features { bound, offered } => write!(
                formatter,
                "the lock binds features [{}] and the binary was built with [{}]: \
                 features are coordinates, so these are two realizations",
                bound.join(", "),
                offered.join(", "),
            ),
            Self::Source { bound, offered } => write!(
                formatter,
                "the lock binds this version to source `{}`, and the binary claims to \
                 have been built from `{}`: the offer realizes another binding",
                bound.digest(),
                offered.digest(),
            ),
            Self::Platform { running, offered } => write!(
                formatter,
                "this run realizes on {}/{} and the binary was built for {}/{}",
                running.operating_system,
                running.architecture,
                offered.operating_system,
                offered.architecture,
            ),
            Self::Toolchain { running, offered } => write!(
                formatter,
                "this run realizes under {} and the binary was built under {}",
                running.describe(),
                offered.describe(),
            ),
            Self::Content { claimed, measured } => write!(
                formatter,
                "the binary claims content `{}` and its bytes measure `{}`",
                claimed.digest(),
                measured.digest(),
            ),
            Self::Unauthorized { origin, admitted } => write!(
                formatter,
                "no local policy admits substitutions from {origin}; this run admits {}",
                admitted_list(admitted),
            ),
        }
    }
}

fn admitted_list(admitted: &[Origin]) -> String {
    if admitted.is_empty() {
        return "no origin".into();
    }
    admitted
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The substitution admission's refusal: the shared refusal over this
/// mechanism's clauses, so a refused source elsewhere refuses with the same
/// machinery under a different vocabulary.
pub type Refusal = pith_constraint::Refusal<Clause>;

fn differ(held: bool, clause: Clause) -> Result<(), Clause> {
    held.then_some(()).ok_or(clause)
}

/// Whether the offer names the binding's package.
fn coordinates(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.entry.package == offer.package,
        Clause::Coordinates {
            bound: admission.entry.package.clone(),
            offered: offer.package.clone(),
        },
    )
}

/// Whether the offer carries exactly the binding's features.
fn features(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.entry.features == offer.features,
        Clause::Features {
            bound: admission.entry.features.clone(),
            offered: offer.features.clone(),
        },
    )
}

/// Whether the offer was built from the source the binding pins.
fn source(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.entry.source == offer.built_from,
        Clause::Source {
            bound: admission.entry.source,
            offered: offer.built_from,
        },
    )
}

/// Whether the offer targets the platform this run realizes on.
fn platform(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.platform == &offer.platform,
        Clause::Platform {
            running: admission.platform.clone(),
            offered: offer.platform.clone(),
        },
    )
}

/// Whether the offer was built under the toolchain this run realizes under.
fn toolchain(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.toolchain == &offer.toolchain,
        Clause::Toolchain {
            running: admission.toolchain.clone(),
            offered: offer.toolchain.clone(),
        },
    )
}

/// Whether the bytes measure what the offer claims.
fn content(offer: &BinaryOffer, measured: ContentId) -> Result<(), Clause> {
    differ(
        measured == offer.claimed,
        Clause::Content {
            claimed: offer.claimed,
            measured,
        },
    )
}

/// Whether an admitted origin covers the offer's origin.
fn authorization(admission: &Admission<'_>, offer: &BinaryOffer) -> Result<(), Clause> {
    differ(
        admission.origins.covering(&offer.origin).is_some(),
        Clause::Unauthorized {
            origin: offer.origin.clone(),
            admitted: admission.origins.0.clone(),
        },
    )
}

/// Applies the admission test to an offer and its bytes: each claim in
/// order, the first failure the refusal.
///
/// # Errors
/// Returns the first admission clause that rejects the offer.
pub fn admit(
    admission: &Admission<'_>,
    offer: &BinaryOffer,
    bytes: &[u8],
) -> Result<Admitted, Refusal> {
    let measured = ContentId::of_blob(bytes);
    pith_constraint::admit([
        coordinates(admission, offer),
        features(admission, offer),
        source(admission, offer),
        platform(admission, offer),
        toolchain(admission, offer),
        content(offer, measured),
        authorization(admission, offer),
    ])?;
    // The authorization claim held, so a covering origin exists here.
    let Some(authorized_by) = admission.origins.covering(&offer.origin) else {
        return Err(pith_constraint::Refusal {
            clause: Clause::Unauthorized {
                origin: offer.origin.clone(),
                admitted: admission.origins.0.clone(),
            },
        });
    };
    Ok(Admitted {
        package: admission.entry.package.clone(),
        features: admission.entry.features.clone(),
        built_from: admission.entry.source,
        platform: offer.platform.clone(),
        toolchain: admission.toolchain.clone(),
        measured,
        authorized_by: authorized_by.clone(),
    })
}
