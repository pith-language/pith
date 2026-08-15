use pith_core::Value;
use pith_engine::ExecutionPlatform;
use pith_ids::ContentId;

use crate::identity::PackageVersion;
use crate::lock::Origin;

use super::model::{Admission, Admitted, BinaryOffer};

/// Which clause of the admission test turned an offer down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
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

impl std::fmt::Display for Refusal {
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

/// Applies the admission test to an offer and its bytes.
///
/// # Errors
/// Returns the first admission clause that rejects the offer.
pub fn admit(
    admission: &Admission<'_>,
    offer: &BinaryOffer,
    bytes: &[u8],
) -> Result<Admitted, Refusal> {
    let entry = admission.entry;
    if entry.package != offer.package {
        return Err(Refusal::Coordinates {
            bound: entry.package.clone(),
            offered: offer.package.clone(),
        });
    }
    if entry.features != offer.features {
        return Err(Refusal::Features {
            bound: entry.features.clone(),
            offered: offer.features.clone(),
        });
    }
    if entry.source != offer.built_from {
        return Err(Refusal::Source {
            bound: entry.source,
            offered: offer.built_from,
        });
    }
    if admission.platform != &offer.platform {
        return Err(Refusal::Platform {
            running: admission.platform.clone(),
            offered: offer.platform.clone(),
        });
    }
    if admission.toolchain != &offer.toolchain {
        return Err(Refusal::Toolchain {
            running: admission.toolchain.clone(),
            offered: offer.toolchain.clone(),
        });
    }
    let measured = ContentId::of_blob(bytes);
    if measured != offer.claimed {
        return Err(Refusal::Content {
            claimed: offer.claimed,
            measured,
        });
    }
    let Some(authorized_by) = admission.origins.covering(&offer.origin) else {
        return Err(Refusal::Unauthorized {
            origin: offer.origin.clone(),
            admitted: admission.origins.0.clone(),
        });
    };
    Ok(Admitted {
        package: entry.package.clone(),
        features: entry.features.clone(),
        built_from: entry.source,
        platform: offer.platform.clone(),
        toolchain: admission.toolchain.clone(),
        measured,
        authorized_by: authorized_by.clone(),
    })
}
