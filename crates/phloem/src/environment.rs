//! The development environment: a value over the lock (decision 0043).
//!
//! An environment is one resolution plus the realization coordinates it
//! declares: the lock 0041 fixed, held unchanged, beside the platform and
//! the toolchain — the request-input half of a realization's identity
//! (0039) — and the substitution records 0042 returned to the caller. A
//! project declares one (`Environment`), resolves it through the ordinary
//! solver, and realizes its entries against the offers this machine holds,
//! which produces the document (`EnvironmentDocument`) beside the refusals
//! the realization produced. A refused offer leaves the build running from
//! source, the same realization an absent offer produces, so the refusals
//! return with the answer and do not enter the document: they would move
//! its digest though nothing the environment serves changed.
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
use crate::identity::{PackageVersion, version_scheme_value};
use crate::lock::{Origin, origin_type};
use crate::locktext::{SHA256, token};
use crate::preference::PreferenceList;
use crate::resolution::{Resolution, resolve_request};
use crate::substitution::{
    Admission, Admitted, AdmittedOrigins, BinaryOffer, Realization, Refusal, realize,
    substitution_type,
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
            Some(payload) => {
                toolchain_driver(payload)?;
                payload.clone()
            }
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
            &crate::preference::preference_list_value(preferences),
            budget,
        );
        let answer = engine.evaluate_pure(&request)?;
        let resolution = Resolution::from_value(&answer.value)?;
        let lock = Lock::from_resolution(scheme, preferences, &resolution)?;
        let (substitutions, refusals) = realize_entries(declaration, &lock, offers);
        Ok(Realized {
            document: Self {
                name: declaration.name.clone(),
                lock,
                platform: declaration.platform.clone(),
                toolchain: declaration.toolchain.clone(),
                substitutions: substitutions.into(),
            },
            refusals: refusals.into(),
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
            Some(payload) => {
                toolchain_driver(payload)?;
                payload.clone()
            }
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

/// Realize every lock entry against the offers that claim it, collecting
/// the admitted substitutions and the refusals. A refused or absent offer
/// builds, which is 0042's fallback and not this module's concern.
///
/// Every offer claiming the entry's identity is tested, in the canonical
/// order over claims, and the first that admits serves. a refusal is
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

/// The toolchain's driver path. The declared type is the nominal alone, and
/// `is_type` cannot see a nominal's representation, so the reader refuses a
/// toolchain whose representation is not the driver path text.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming what was found when the value is
/// not the nominal toolchain over the driver path text.
fn toolchain_driver(toolchain: &Value) -> PithResult<&str> {
    let Value::Nominal { representation, .. } = toolchain else {
        return Err(diag(format!(
            "the toolchain is {}, and the declared type is xylem's nominal toolchain over \
             the driver path",
            toolchain.describe()
        )));
    };
    let Value::Text(driver) = representation.as_ref() else {
        return Err(diag(format!(
            "the toolchain's representation is {} rather than the driver path text",
            representation.describe()
        )));
    };
    Ok(driver)
}

/// The toolchain in this format's token spelling. The declaration's type is
/// xylem's nominal toolchain, whose representation is the driver path, and
/// the driver gets the same quoting every other text field on the line
/// gets, so a path with a space renders a line this grammar still accepts.
fn toolchain_token(toolchain: &Value) -> String {
    let driver = match toolchain {
        Value::Nominal { representation, .. } => match representation.as_ref() {
            Value::Text(driver) => return token(driver),
            // A toolchain value xylem never produces, quoted so the line
            // still parses.
            other => other.describe(),
        },
        other => other.describe(),
    };
    token(&driver)
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
        toolchain_token(&document.toolchain),
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

/// The derivation's one rule: a declared environment name becomes a file
/// name beside the lock, so it must be a single path component. A name with
/// a separator or a `..` would derive a path outside the project root. the
/// refusal surfaces at the derivation, before any caller acts on a path.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name and why it cannot name an
/// environment when it is empty or not one path component.
fn component_name(environment: &str) -> PithResult<()> {
    let mut components = Path::new(environment).components();
    let single = matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    );
    if single {
        return Ok(());
    }
    Err(diag(format!(
        "the environment name `{environment}` is not a single path component, and a name \
         that becomes a file name beside the lock cannot carry a separator or climb out \
         of the project"
    )))
}

/// Where the environment's lock lives: in `project`, named for the
/// declaration that produced it — `pith.lock` for the default environment,
/// `<name>.pith.lock` beside it for a named one. One lock per resolution,
/// and an environment is one resolution (0043). The derivation is a pure
/// function the library owns, and it refuses a name that is not a single
/// path component.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name when it cannot name an
/// environment.
pub fn lock_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_LOCK_FILE))
    } else {
        Ok(project.join(format!("{environment}{LOCK_SUFFIX}")))
    }
}

/// Where the environment's own record lives, beside its lock.
///
/// # Errors
/// A [`pith_diag::DiagnosticSink`] naming the name when it cannot name an
/// environment.
pub fn record_path(project: &Path, environment: &str) -> PithResult<PathBuf> {
    component_name(environment)?;
    if environment == DEFAULT_ENVIRONMENT {
        Ok(project.join(DEFAULT_RECORD_FILE))
    } else {
        Ok(project.join(format!("{environment}{RECORD_SUFFIX}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(toolchain: Value) -> EnvironmentDocument {
        EnvironmentDocument {
            name: DEFAULT_ENVIRONMENT.into(),
            lock: Lock::new(
                "r",
                "numeric-segments",
                ContentId::of_blob(b"universe"),
                PreferenceList(Box::new([])),
                Vec::new(),
            )
            .unwrap(),
            platform: ExecutionPlatform {
                operating_system: "linux".into(),
                architecture: "x86_64".into(),
            },
            toolchain,
            substitutions: Box::new([]),
        }
    }

    #[test]
    fn a_toolchain_path_with_a_space_renders_in_the_line_token_spelling() {
        // The toolchain is a text field on the header line like the name and
        // the platform, so it takes the quoting they take. `describe` would
        // spell the value bare, and a driver path with a space would render
        // a line whose own grammar cannot read back.
        let rendered = render(&document(xylem::types::toolchain(
            "/nix/store/my compiler toolchain",
        )));
        let header = rendered.lines().next().unwrap();
        assert!(
            header.contains("toolchain \"/nix/store/my compiler toolchain\""),
            "the driver path renders quoted, the way every other field on the \
             line does: {header}"
        );
        assert_eq!(
            header.matches('"').count(),
            2,
            "one quoted token, no bare spaces leaking out of it: {header}"
        );
    }

    #[test]
    fn a_toolchain_whose_representation_is_not_the_driver_path_is_refused_on_read() {
        let wrong = Value::Nominal {
            name: xylem::types::TOOLCHAIN.into(),
            representation: Box::new(Value::Int(7)),
        };
        let value = document(wrong.clone()).to_value();
        assert!(
            value.is_type(&environment_document_type()),
            "the nominal type cannot see the representation; the reader must"
        );
        let error = EnvironmentDocument::from_value(&value).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("driver path")),
            "the diagnostic names what the toolchain had to carry: {error:?}"
        );
    }

    #[test]
    fn the_lock_and_record_paths_derive_from_the_declaration() {
        let project = Path::new("/repo");
        assert_eq!(
            lock_path(project, DEFAULT_ENVIRONMENT).unwrap(),
            Path::new("/repo/pith.lock")
        );
        assert_eq!(
            lock_path(project, "cross").unwrap(),
            Path::new("/repo/cross.pith.lock")
        );
        assert_eq!(
            record_path(project, DEFAULT_ENVIRONMENT).unwrap(),
            Path::new("/repo/pith.env")
        );
        assert_eq!(
            record_path(project, "cross").unwrap(),
            Path::new("/repo/cross.pith.env")
        );
    }

    #[test]
    fn a_name_that_is_not_one_path_component_is_refused_at_the_derivation() {
        // The derivation interpolates the name into the project root, so a
        // separator or a `..` would name a path outside it. The refusal
        // surfaces at the derivation, before any caller acts on a path.
        for name in ["", "a/b", "..", ".", "a/../b", "/abs"] {
            for result in [
                lock_path(Path::new("/repo"), name),
                record_path(Path::new("/repo"), name),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.message.0.contains(name)),
                    "the diagnostic names the refused name `{name}`: {error:?}"
                );
            }
        }
    }
}
