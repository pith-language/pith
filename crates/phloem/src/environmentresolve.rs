use pith_core::Value;
use pith_diag::PithResult;
use pith_engine::Engine;

use crate::constraint::Constraint;
use crate::document::Lock;
use crate::environment::{Environment, EnvironmentDocument};
use crate::identity::{PackageVersion, version_scheme_value};
use crate::preference::{PreferenceList, preference_list_value};
use crate::resolution::{Resolution, resolve_request};
use crate::substitution::{Admission, Admitted, BinaryOffer, Realization, Refusal, realize};
use crate::universe::CandidateUniverse;

/// One offer an environment is realized against: the claim and the bytes it
/// claims an identity for.
pub struct Offer<'a> {
    pub offer: &'a BinaryOffer,
    pub bytes: &'a [u8],
}

/// One offer the realization tested and refused: the binding it claimed,
/// carried by the entry's coordinates, and the clause that turned it down.
/// The refusal is 0042's value, the failing clause and both sides of the
/// comparison, so a caller can tell a tampered artifact from an
/// unauthorized origin without re-running the test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub package: PackageVersion,
    pub refusal: Refusal,
}

/// A declaration realized: the environment document, and the refusals its
/// realization produced.
///
/// The refusals are returned beside the document and kept out of it. A
/// refused offer leaves the build running from source, the same
/// realization an absent offer produces, so a refusal in the document
/// would move its digest though nothing the environment serves changed.
/// The refusals arrive in lock-entry order, each entry's offers in the
/// canonical order over claims.
pub struct Realized {
    pub document: EnvironmentDocument,
    pub refusals: Box<[Refused]>,
}

impl EnvironmentDocument {
    /// Resolve the declaration through the engine, lock the answer, and
    /// realize the lock's entries against `offers`.
    ///
    /// Pure on 0041's terms: the engine computes the resolution, the lock
    /// and the document are values, and no path is touched until a caller
    /// writes one. Only a solved resolution selects; the other three
    /// constructors are facts about the problem, and an environment is not
    /// one of them.
    ///
    /// The answer carries the refusals beside the document: an offer that
    /// was tested and turned down returns 0042's value, so the caller sees
    /// the explanation on every resolve that produces it.
    ///
    /// # Errors
    /// The engine's diagnostics when the resolution fails, and the lock's
    /// when the answer does not select or a candidate carries no content
    /// identity to bind.
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

/// Realize every lock entry against the offers that claim it, collecting
/// the admitted substitutions and the refusals. A refused or absent offer
/// builds, which is 0042's fallback and not this module's concern.
///
/// Every offer claiming the entry's identity is tested, in the canonical
/// order over claims, and the first that admits serves. A refusal is
/// carried out for every offer that was tested and refused while none
/// served. When a substitution serves, the offers after it in the
/// canonical order are not examined: the build they stood in for is not
/// running.
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
            match realize(&admission, Some((offered.offer, offered.bytes))) {
                Realization::Substituted(record) => {
                    admitted.push(record);
                    break;
                }
                Realization::Built {
                    refused: Some(refusal),
                } => refused.push(Refused {
                    package: entry.package.clone(),
                    refusal,
                }),
                Realization::Built { refused: None } => {
                    unreachable!("an offer was handed to the admission test")
                }
            }
        }
    }
    (admitted, refused)
}
