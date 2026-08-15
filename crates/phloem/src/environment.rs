//! The development environment: a value over the lock (decision 0043).
//!
//! An environment is one resolution plus the realization coordinates it
//! declares: the lock 0041 fixed, held unchanged, beside the platform and
//! the toolchain — the request-input half of a realization's identity
//! (0039) — and the substitution records 0042 returned to the caller. A
//! project declares one (`Environment`), resolves it through the ordinary
//! solver, and realizes its entries against the offers this machine holds,
//! which produces the document (`EnvironmentDocument`).
//!
//! Computing all of that is pure, on the ground 0041 put the lock's write
//! on: resolving, locking, realizing, digesting, and rendering touch no
//! path and read no process environment, and the toolchain a declaration
//! carries was discovered caller-side before any request existed (0028,
//! 0030). Writing the lock and the record, and one day entering the
//! environment, are caller effects; `nix print-dev-env` is the precedent
//! for what ships instead of entering — the environment as data. The
//! confinement the executor owns is claimed nowhere here: an interactive
//! shell is the caller's process, not a sandboxed action (0028, 0030), and
//! the record says so rather than letting a reader infer a sandbox.
//!
//! "Reproducible" is scoped to 0014's first two properties: the selection
//! is a pure function of recorded inputs (0040) and the rendering is a
//! total deterministic function of the document (0041's render
//! discipline). Bit-for-bit reproducibility of the packages themselves is
//! a property of build instructions, verified by rebuild, and nothing in
//! this module asserts it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_engine::{Engine, ExecutionPlatform};
use pith_ids::{ContentDigest, ContentId};

use crate::codec::{field_of, record_type, record_value, text_field};
use crate::constraint::{Constraint, constraint_type};
use crate::diag;
use crate::document::{Lock, LockChange, diff as diff_locks, lock_type};
use crate::identity::version_scheme_value;
use crate::lock::{Origin, origin_type};
use crate::locktext::{SHA256, token};
use crate::preference::PreferenceList;
use crate::resolution::{Resolution, resolve_request};
use crate::substitution::{
    Admission, Admitted, AdmittedOrigins, BinaryOffer, Realization, realize, substitution_type,
};
use crate::universe::CandidateUniverse;

/// The digest domain for one revision of an environment document,
/// NUL-terminated so it is self-delimiting against the canonical bytes that
/// follow, mirroring the domain separation `pith-ids` applies to every
/// digest kind it owns.
const ENVIRONMENT_DOMAIN: &[u8] = b"phloem.environment-v1\0";

/// The environment whose lock is the repository's default written lock.
pub const DEFAULT_ENVIRONMENT: &str = "default";

const DEFAULT_LOCK_FILE: &str = "pith.lock";
const DEFAULT_RECORD_FILE: &str = "pith.env";
const LOCK_SUFFIX: &str = ".pith.lock";
const RECORD_SUFFIX: &str = ".pith.env";

const NAME: &str = "name";
const CONSTRAINTS: &str = "constraints";
const OPERATING_SYSTEM: &str = "operating-system";
const ARCHITECTURE: &str = "architecture";
const TOOLCHAIN: &str = "toolchain";
const ORIGINS: &str = "origins";
const LOCK: &str = "lock";
const SUBSTITUTIONS: &str = "substitutions";
const PLATFORM: &str = "platform";
const ENV: &str = "env";
const SUBSTITUTE: &str = "substitute";

/// A project's declaration of one environment: what it asks the package
/// domain for, the realization coordinates its builds run under, and the
/// origins whose binary offers it admits.
///
/// The toolchain is the value the run's build requests carry, on the terms
/// 0042's admission leg fixed — one value, so the declaration, the
/// admission test, and the derived requests cannot drift apart. The
/// origins are 0042's local policy, which is a person's configuration and
/// therefore belongs to the declared thing rather than to any run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    pub name: Box<str>,
    /// What this environment asks for, as an ordinary constraint set.
    pub constraints: Box<[Constraint]>,
    pub platform: ExecutionPlatform,
    pub toolchain: Value,
    pub origins: AdmittedOrigins,
}

/// The declared environment's record type: the name, the constraint set,
/// the realization coordinates, and the admitted origins.
#[must_use]
pub fn environment_type() -> Type {
    record_type([
        (NAME, Type::Text),
        (CONSTRAINTS, Type::List(Box::new(constraint_type()))),
        (OPERATING_SYSTEM, Type::Text),
        (ARCHITECTURE, Type::Text),
        (
            TOOLCHAIN,
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
        ),
        (ORIGINS, Type::List(Box::new(origin_type()))),
    ])
}

impl Environment {
    /// The declaration as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (NAME, Value::Text(self.name.clone())),
            (
                CONSTRAINTS,
                Value::List(self.constraints.iter().map(Constraint::to_value).collect()),
            ),
            (
                OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (TOOLCHAIN, self.toolchain.clone()),
            (
                ORIGINS,
                Value::List(self.origins.0.iter().map(Origin::to_value).collect()),
            ),
        ])
    }

    /// Read a declaration from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not an environment declaration.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&environment_type()) {
            return Err(diag(format!(
                "expected a value of the environment declaration type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the environment declaration type, found {}",
                value.describe()
            )));
        };
        let name = text_field(fields, NAME)?;
        let mut constraints = Vec::new();
        if let Some(Value::List(elements)) = field_of(fields, CONSTRAINTS) {
            for element in elements.iter() {
                constraints.push(Constraint::from_value(element)?);
            }
        }
        let operating_system = text_field(fields, OPERATING_SYSTEM)?;
        let architecture = text_field(fields, ARCHITECTURE)?;
        let toolchain = match field_of(fields, TOOLCHAIN) {
            Some(payload) => payload.clone(),
            None => return Err(diag(format!("the declaration carried no {TOOLCHAIN}"))),
        };
        let mut origins = Vec::new();
        if let Some(Value::List(elements)) = field_of(fields, ORIGINS) {
            for element in elements.iter() {
                origins.push(Origin::from_value(element)?);
            }
        }
        Ok(Self {
            name,
            constraints: constraints.into(),
            platform: ExecutionPlatform {
                operating_system,
                architecture,
            },
            toolchain,
            origins: AdmittedOrigins(origins.into()),
        })
    }
}

/// One environment as computed: the lock of its resolution, held unchanged,
/// beside the realization coordinates it declares and the substitutions it
/// served.
///
/// Two documents over different declarations are different environments
/// even where their constraints overlap, and one document re-resolved under
/// a moved input is a different document whose diff names the input. Two
/// platforms under one declaration share the lock — it binds source only —
/// and differ in the realization coordinates, so per-platform environments
/// are per-platform realizations of one selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentDocument {
    pub name: Box<str>,
    pub lock: Lock,
    pub platform: ExecutionPlatform,
    pub toolchain: Value,
    /// The substitutions this environment admitted, in the order the lock's
    /// entries sort. The lock records source only; these records are the
    /// environment's own answer to which binaries it served (0042's third
    /// option, decided here).
    pub substitutions: Box<[Admitted]>,
}

/// The environment document's record type: the name, the lock, the
/// realization coordinates, and the substitution records.
#[must_use]
pub fn environment_document_type() -> Type {
    record_type([
        (NAME, Type::Text),
        (LOCK, lock_type()),
        (OPERATING_SYSTEM, Type::Text),
        (ARCHITECTURE, Type::Text),
        (
            TOOLCHAIN,
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
        ),
        (SUBSTITUTIONS, Type::List(Box::new(substitution_type()))),
    ])
}

/// One offer an environment is realized against: the claim and the bytes it
/// claims an identity for.
pub struct Offer<'a> {
    pub offer: &'a BinaryOffer,
    pub bytes: &'a [u8],
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
    ) -> PithResult<Self> {
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
            &crate::preference::preference_list_value(preferences),
            budget,
        );
        let answer = engine.evaluate_pure(&request)?;
        let resolution = Resolution::from_value(&answer.value)?;
        let lock = Lock::from_resolution(scheme, preferences, &resolution)?;
        let substitutions = realize_entries(declaration, &lock, offers);
        Ok(Self {
            name: declaration.name.clone(),
            lock,
            platform: declaration.platform.clone(),
            toolchain: declaration.toolchain.clone(),
            substitutions: substitutions.into(),
        })
    }

    /// The document's own content identity: a digest over its canonical
    /// encoding, domain-separated the way every phloem digest is. The
    /// rendered record never feeds a digest; this is what "the same
    /// environment" means.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        let canonical = self.to_value().encode_canonical();
        let mut domain_prefixed = ENVIRONMENT_DOMAIN.to_vec();
        domain_prefixed.extend_from_slice(&canonical);
        ContentId::from_digest(ContentDigest::of_bytes(&domain_prefixed))
    }

    /// The document as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (NAME, Value::Text(self.name.clone())),
            (LOCK, self.lock.to_value()),
            (
                OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (TOOLCHAIN, self.toolchain.clone()),
            (
                SUBSTITUTIONS,
                Value::List(self.substitutions.iter().map(Admitted::to_value).collect()),
            ),
        ])
    }

    /// Read a document from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not an environment document.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        if !value.is_type(&environment_document_type()) {
            return Err(diag(format!(
                "expected a value of the environment document type, found {}",
                value.describe()
            )));
        }
        let Value::Record(fields) = value else {
            return Err(diag(format!(
                "expected a value of the environment document type, found {}",
                value.describe()
            )));
        };
        let name = text_field(fields, NAME)?;
        let lock = match field_of(fields, LOCK) {
            Some(payload) => Lock::from_value(payload)?,
            None => return Err(diag(format!("the document carried no {LOCK}"))),
        };
        let operating_system = text_field(fields, OPERATING_SYSTEM)?;
        let architecture = text_field(fields, ARCHITECTURE)?;
        let toolchain = match field_of(fields, TOOLCHAIN) {
            Some(payload) => payload.clone(),
            None => return Err(diag(format!("the document carried no {TOOLCHAIN}"))),
        };
        let mut substitutions = Vec::new();
        if let Some(Value::List(elements)) = field_of(fields, SUBSTITUTIONS) {
            for element in elements.iter() {
                substitutions.push(Admitted::from_value(element)?);
            }
        }
        Ok(Self {
            name,
            lock,
            platform: ExecutionPlatform {
                operating_system,
                architecture,
            },
            toolchain,
            substitutions: substitutions.into(),
        })
    }
}

/// Realize every lock entry against the offers that claim it, collecting
/// the admitted substitutions. A refused or absent offer builds, which is
/// 0042's fallback and not this module's concern.
fn realize_entries(declaration: &Environment, lock: &Lock, offers: &[Offer<'_>]) -> Vec<Admitted> {
    let mut admitted = Vec::new();
    for entry in lock.entries.iter() {
        let matching = offers
            .iter()
            .find(|offered| offered.offer.package.identity() == entry.package.identity());
        let offered = matching.map(|offered| (offered.offer, offered.bytes));
        let admission = Admission {
            entry,
            platform: &declaration.platform,
            toolchain: &declaration.toolchain,
            origins: &declaration.origins,
        };
        if let Realization::Substituted(record) = realize(&admission, offered) {
            admitted.push(record);
        }
    }
    admitted
}

/// One moved input of an environment diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentChange {
    /// A moved input or entry of the lock the environment holds, reported
    /// by the lock's own diff.
    Lock(LockChange),
    Platform {
        from: ExecutionPlatform,
        to: ExecutionPlatform,
    },
    Toolchain {
        from: Value,
        to: Value,
    },
    /// The set of served substitutions moved, which happens when offers or
    /// the admission policy moved and the selection did not.
    Substitutions,
}

/// What moved between two revisions of one environment: each moved lock
/// input or entry, and each moved realization coordinate. The staleness
/// check a caller runs before resolving again, on the same shape as the
/// lock's own.
#[must_use]
pub fn diff(before: &EnvironmentDocument, after: &EnvironmentDocument) -> Box<[EnvironmentChange]> {
    let mut changes = Vec::new();
    for change in diff_locks(&before.lock, &after.lock).changes.iter() {
        changes.push(EnvironmentChange::Lock(change.clone()));
    }
    if before.platform != after.platform {
        changes.push(EnvironmentChange::Platform {
            from: before.platform.clone(),
            to: after.platform.clone(),
        });
    }
    if before.toolchain != after.toolchain {
        changes.push(EnvironmentChange::Toolchain {
            from: before.toolchain.clone(),
            to: after.toolchain.clone(),
        });
    }
    if before.substitutions != after.substitutions {
        changes.push(EnvironmentChange::Substitutions);
    }
    changes.into()
}

/// The environment's rendered record: its projection, deterministic and
/// line-oriented, on the terms of the lock's file. The text names the lock
/// by digest, the realization coordinates, and every served substitution;
/// the lock's own file is unchanged by all of it and stays the place the
/// selection is read.
///
/// The record is a report over the value, not a format with a reader: the
/// document is the artifact that crosses processes, and a consumer for
/// this text waits until one exists (0043's unresolved).
#[must_use]
pub fn render(document: &EnvironmentDocument) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{ENV} {} {PLATFORM} {}/{} {TOOLCHAIN} {} {LOCK} {SHA256}{}",
        token(&document.name),
        document.platform.operating_system,
        document.platform.architecture,
        document.toolchain.describe(),
        document.lock.content_id().digest(),
    );
    let mut lines: Vec<String> = document
        .substitutions
        .iter()
        .map(|substitution| {
            format!(
                "{SUBSTITUTE} {} {} {} [{}] {SHA256}{} {}",
                token(substitution.package.identity().domain().as_str()),
                token(substitution.package.identity().name()),
                token(substitution.package.version()),
                substitution
                    .features
                    .iter()
                    .map(|feature| token(feature))
                    .collect::<Vec<_>>()
                    .join(","),
                substitution.measured.digest(),
                substitution.authorized_by,
            )
        })
        .collect();
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    out
}

/// Where the environment's lock lives: in `project`, named for the
/// declaration that produced it — `pith.lock` for the default environment,
/// `<name>.pith.lock` beside it for a named one. One lock per resolution,
/// and an environment is one resolution (0043).
#[must_use]
pub fn lock_path(project: &Path, environment: &str) -> PathBuf {
    if environment == DEFAULT_ENVIRONMENT {
        project.join(DEFAULT_LOCK_FILE)
    } else {
        project.join(format!("{environment}{LOCK_SUFFIX}"))
    }
}

/// Where the environment's own record lives, beside its lock.
#[must_use]
pub fn record_path(project: &Path, environment: &str) -> PathBuf {
    if environment == DEFAULT_ENVIRONMENT {
        project.join(DEFAULT_RECORD_FILE)
    } else {
        project.join(format!("{environment}{RECORD_SUFFIX}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_and_record_paths_derive_from_the_declaration() {
        let project = Path::new("/repo");
        assert_eq!(
            lock_path(project, DEFAULT_ENVIRONMENT),
            Path::new("/repo/pith.lock")
        );
        assert_eq!(
            lock_path(project, "cross"),
            Path::new("/repo/cross.pith.lock")
        );
        assert_eq!(
            record_path(project, DEFAULT_ENVIRONMENT),
            Path::new("/repo/pith.env")
        );
        assert_eq!(
            record_path(project, "cross"),
            Path::new("/repo/cross.pith.env")
        );
    }
}
