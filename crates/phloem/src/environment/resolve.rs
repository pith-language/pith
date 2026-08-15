use pith_core::Value;
use pith_diag::PithResult;
use pith_engine::Engine;

use crate::constraint::Constraint;
use crate::document::Lock;
use crate::identity::{PackageVersion, version_scheme_value};
use crate::preference::{PreferenceList, preference_list_value};
use crate::resolution::{Resolution, resolve_request};
use crate::substitution::{Admission, Admitted, BinaryOffer, Refusal, Serving, serve};
use crate::universe::CandidateUniverse;

use super::{Environment, EnvironmentDocument};

/// A binary offer and the bytes covered by its claim.
pub struct Offer<'a> {
    pub offer: &'a BinaryOffer,
    pub bytes: &'a [u8],
}

/// A binary offer rejected while realizing a package binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub package: PackageVersion,
    pub refusal: Refusal,
}

/// A resolved environment document and its rejected offers.
pub struct Realized {
    pub document: EnvironmentDocument,
    pub refusals: Box<[Refused]>,
}

impl EnvironmentDocument {
    /// Resolves a declaration and tests matching binary offers.
    ///
    /// # Errors
    /// Returns diagnostics from resolution or lock construction.
    pub fn resolve(
        declaration: &Environment,
        engine: &mut Engine,
        universe: &CandidateUniverse,
        scheme: &str,
        preferences: &PreferenceList,
        budget: u64,
        offers: &[Offer<'_>],
    ) -> PithResult<Realized> {
        let request = resolve_request(
            &version_scheme_value(scheme),
            &Value::List(
                declaration
                    .constraints
                    .iter()
                    .map(Constraint::to_value)
                    .collect(),
            ),
            &universe.to_value(),
            &preference_list_value(preferences),
            budget,
        );
        let answer = engine.evaluate_pure(&request)?;
        let resolution = Resolution::from_value(&answer.value)?;
        realize_resolution(declaration, scheme, preferences, &resolution, offers)
    }
}

fn realize_resolution(
    declaration: &Environment,
    scheme: &str,
    preferences: &PreferenceList,
    resolution: &Resolution,
    offers: &[Offer<'_>],
) -> PithResult<Realized> {
    let lock = Lock::from_resolution(scheme, preferences, resolution)?;
    let (substitutions, refusals) = realize_entries(declaration, &lock, offers);
    Ok(Realized {
        document: EnvironmentDocument {
            name: declaration.name.clone(),
            lock,
            platform: declaration.platform.clone(),
            toolchain: declaration.toolchain.clone(),
            substitutions: substitutions.into(),
        },
        refusals: refusals.into(),
    })
}

/// Tests matching offers in canonical order for each lock entry.
fn realize_entries(
    declaration: &Environment,
    lock: &Lock,
    offers: &[Offer<'_>],
) -> (Vec<Admitted>, Vec<Refused>) {
    let mut admitted = Vec::new();
    let mut refused = Vec::new();
    for entry in lock.entries.iter() {
        let admission = Admission {
            entry,
            platform: &declaration.platform,
            toolchain: &declaration.toolchain,
            origins: &declaration.origins,
        };
        let mut claiming: Vec<&Offer<'_>> = offers
            .iter()
            .filter(|offered| offered.offer.package.identity() == entry.package.identity())
            .collect();
        claiming.sort_by_key(|offered| offered.offer.canonical_key());
        for offered in claiming {
            match serve(&admission, Some((offered.offer, offered.bytes))) {
                Serving::Substituted(record) => {
                    admitted.push(record);
                    break;
                }
                Serving::Built {
                    refused: Some(refusal),
                } => refused.push(Refused {
                    package: entry.package.clone(),
                    refusal,
                }),
                Serving::Built { refused: None } => {
                    unreachable!("an offer was handed to the admission test")
                }
            }
        }
    }
    (admitted, refused)
}
