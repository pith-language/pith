//! Development environment declarations and resolved documents.
//!
//! An environment combines package constraints, realization coordinates,
//! and admitted substitution origins. Resolving it produces a lock and the
//! substitutions that served its entries. Rendering and path derivation do
//! not write files.

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

mod diff;
mod file;
mod resolve;

pub use self::diff::{EnvironmentChange, diff};
pub use self::file::{lock_path, record_path, render};
pub use self::resolve::{Offer, Realized, Refused};

/// Digest domain for environment document values.
const ENVIRONMENT_DOMAIN: &[u8] = b"phloem.environment-v1\0";

/// The environment whose lock is the repository's default written lock.
pub const DEFAULT_ENVIRONMENT: &str = "default";

const NAME: &str = "name";
const CONSTRAINTS: &str = "constraints";
const ORIGINS: &str = "origins";
const LOCK: &str = "lock";
const SUBSTITUTIONS: &str = "substitutions";

/// A project's package constraints and realization settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    pub name: Box<str>,
    /// Package constraints requested by the environment.
    pub constraints: Box<[Constraint]>,
    pub platform: ExecutionPlatform,
    pub toolchain: Value,
    pub origins: AdmittedOrigins,
}

/// Returns the declared environment record type.
#[must_use]
pub fn environment_type() -> Type {
    record_type([
        (NAME, Type::Text),
        (CONSTRAINTS, Type::List(Box::new(constraint_type()))),
        (FIELD_OPERATING_SYSTEM, Type::Text),
        (FIELD_ARCHITECTURE, Type::Text),
        (FIELD_TOOLCHAIN, xylem::types::toolchain_type()),
        (ORIGINS, Type::List(Box::new(origin_type()))),
    ])
}

impl Environment {
    /// Encodes the declaration as its declared record type.
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

    /// Decodes an environment declaration from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not an environment declaration.
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

/// A resolved environment and its admitted substitutions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentDocument {
    pub name: Box<str>,
    pub lock: Lock,
    pub platform: ExecutionPlatform,
    pub toolchain: Value,
    /// Admitted substitutions in lock-entry order.
    pub substitutions: Box<[Admitted]>,
}

/// Returns the declared environment document record type.
#[must_use]
pub fn environment_document_type() -> Type {
    record_type([
        (NAME, Type::Text),
        (LOCK, lock_type()),
        (FIELD_OPERATING_SYSTEM, Type::Text),
        (FIELD_ARCHITECTURE, Type::Text),
        (FIELD_TOOLCHAIN, xylem::types::toolchain_type()),
        (SUBSTITUTIONS, Type::List(Box::new(substitution_type()))),
    ])
}

impl EnvironmentDocument {
    /// Returns the content identity of the document's canonical value encoding.
    #[must_use]
    pub fn content_id(&self) -> ContentId {
        value_content_id(ENVIRONMENT_DOMAIN, &self.to_value())
    }

    /// Encodes the document as its declared record type.
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

    /// Decodes an environment document from `value`.
    ///
    /// # Errors
    /// Returns a diagnostic when `value` is not an environment document.
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

/// Returns the driver path represented by a toolchain value.
///
/// # Errors
/// Returns a diagnostic when the toolchain does not represent a text path.
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
    fn a_toolchain_whose_representation_is_not_the_driver_path_is_refused_by_the_type() {
        // Before decision 0047 the nominal type could not see the
        // representation, so this refusal was the reader's alone and the
        // document still inhabited its own type. The declaration xylem now
        // registers says `Toolchain` is over `Text`, so an `Int` inside one is
        // refused at the type — one layer earlier and for every consumer rather
        // than only for the readers that thought to check.
        let wrong = Value::Nominal {
            name: xylem::types::toolchain_name().into(),
            representation: Box::new(Value::Int(7)),
        };
        let value = document(wrong).to_value();
        assert!(
            !value.is_type(&environment_document_type()),
            "an Int inside a Toolchain must not inhabit the document type"
        );
        // The reader refuses it too, and now at its own type guard rather than
        // at the driver-path branch further in: the type it checks against
        // carries the declaration, so the wrong representation never reaches the
        // field read. The driver-path diagnostic survives as defense in depth
        // for a caller that skips the guard, and is no longer the only thing
        // standing between an `Int` and a toolchain.
        let error = EnvironmentDocument::from_value(&value).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("environment document type")),
            "the refusal names the type the value failed to inhabit: {error:?}"
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
