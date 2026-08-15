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

use pith_core::{Type, Value};
use pith_diag::PithResult;
use pith_engine::ExecutionPlatform;
use pith_ids::ContentId;

use crate::codec::{
    FIELD_ARCHITECTURE, FIELD_OPERATING_SYSTEM, FIELD_TOOLCHAIN, field_of, record_type,
    record_value, text_field, value_content_id,
};
use crate::constraint::{Constraint, constraint_type};
use crate::diag;
use crate::document::{Lock, lock_type};
use crate::lock::{Origin, origin_type};
use crate::substitution::{Admitted, AdmittedOrigins, substitution_type};

pub use crate::environmentdiff::{EnvironmentChange, diff};
pub use crate::environmentfile::{lock_path, record_path, render};
pub use crate::environmentresolve::{Offer, Realized, Refused};

/// The digest domain for one revision of an environment document,
/// NUL-terminated so it is self-delimiting against the canonical bytes that
/// follow, mirroring the domain separation `pith-ids` applies to every
/// digest kind it owns.
const ENVIRONMENT_DOMAIN: &[u8] = b"phloem.environment-v1\0";

/// The environment whose lock is the repository's default written lock.
pub const DEFAULT_ENVIRONMENT: &str = "default";

const NAME: &str = "name";
const CONSTRAINTS: &str = "constraints";
const ORIGINS: &str = "origins";
const LOCK: &str = "lock";
const SUBSTITUTIONS: &str = "substitutions";

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
        (FIELD_OPERATING_SYSTEM, Type::Text),
        (FIELD_ARCHITECTURE, Type::Text),
        (
            FIELD_TOOLCHAIN,
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
                FIELD_OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                FIELD_ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (FIELD_TOOLCHAIN, self.toolchain.clone()),
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
        let operating_system = text_field(fields, FIELD_OPERATING_SYSTEM)?;
        let architecture = text_field(fields, FIELD_ARCHITECTURE)?;
        let toolchain = match field_of(fields, FIELD_TOOLCHAIN) {
            Some(payload) => {
                toolchain_driver(payload)?;
                payload.clone()
            }
            None => {
                return Err(diag(format!(
                    "the declaration carried no {FIELD_TOOLCHAIN}"
                )));
            }
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
/// The document records the declaration name and every realized input. A
/// moved recorded input produces a different document whose diff names the
/// input. Two platforms under one declaration share the lock — it binds
/// source only — and differ in the realization coordinates, so per-platform
/// environments are per-platform realizations of one selection.
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
        (FIELD_OPERATING_SYSTEM, Type::Text),
        (FIELD_ARCHITECTURE, Type::Text),
        (
            FIELD_TOOLCHAIN,
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
        ),
        (SUBSTITUTIONS, Type::List(Box::new(substitution_type()))),
    ])
}

impl EnvironmentDocument {
    /// The document's own content identity: a digest over its canonical
    /// encoding, domain-separated the way every phloem digest is. The
    /// rendered record never feeds a digest; this is what "the same
    /// environment" means.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        value_content_id(ENVIRONMENT_DOMAIN, &self.to_value())
    }

    /// The document as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([
            (NAME, Value::Text(self.name.clone())),
            (LOCK, self.lock.to_value()),
            (
                FIELD_OPERATING_SYSTEM,
                Value::Text(self.platform.operating_system.clone()),
            ),
            (
                FIELD_ARCHITECTURE,
                Value::Text(self.platform.architecture.clone()),
            ),
            (FIELD_TOOLCHAIN, self.toolchain.clone()),
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
        let operating_system = text_field(fields, FIELD_OPERATING_SYSTEM)?;
        let architecture = text_field(fields, FIELD_ARCHITECTURE)?;
        let toolchain = match field_of(fields, FIELD_TOOLCHAIN) {
            Some(payload) => {
                toolchain_driver(payload)?;
                payload.clone()
            }
            None => return Err(diag(format!("the document carried no {FIELD_TOOLCHAIN}"))),
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::preference::PreferenceList;

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
