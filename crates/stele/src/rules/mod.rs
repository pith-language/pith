//! The rules: three merges, three renders, one assembly action, and the entry
//! that requests them all.
//!
//! The splits follow the seam xylem's rules set. What can be decided from the
//! values alone — every merge, every text projection — is a pure rule, where a
//! disagreement is a diagnostic and no process starts. Publishing a tree is
//! the action's, because content enters the store only through an executor's
//! capture (decision 0045's ground), and one action is one tool invocation
//! (0032): the shell script the planner derives places the staged bytes,
//! writes the rendered texts, sets the declared modes, and creates the
//! symlinks.

pub(crate) mod assemble;
pub(crate) mod etc;
pub(crate) mod render;
pub(crate) mod unit;
pub(crate) mod users;

pub(crate) use assemble::{AssembleAction, ComposeSystem};
pub(crate) use etc::ComposeEtc;
pub(crate) use render::{RenderBoot, RenderPasswd, RenderUnit};
pub(crate) use unit::ComposeUnit;
pub(crate) use users::ComposeUsers;

use pith_core::Value;
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{PureRuleFrame, PureStep, Resumption};

use crate::types::{self, FileBody};

/// The code every diagnostic from this domain carries.
///
/// The 9000 range is where xylem (9002) and phloem (9004) already stamp
/// theirs, and pith-diag still documents no allocation rule for it. Decision
/// 0056 names that gap; this crate extends the convention it names rather
/// than the rule that does not exist.
pub(crate) const DOMAIN_CODE: StableCode = StableCode(9006);

pub(crate) fn diag(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        DOMAIN_CODE,
        Span::none(),
        message,
    ));
    sink
}

/// A pure rule that answers in one step from its inputs, without suspending.
pub(crate) struct Leaf {
    inputs: Vec<Value>,
    compute: fn(&[Value]) -> PithResult<Value>,
    answered: bool,
}

impl Leaf {
    pub(crate) fn new(inputs: &[Value], compute: fn(&[Value]) -> PithResult<Value>) -> Self {
        Self {
            inputs: inputs.to_vec(),
            compute,
            answered: false,
        }
    }
}

impl PureRuleFrame for Leaf {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        if self.answered {
            return Err(diag("the engine stepped a rule that had already completed"));
        }
        self.answered = true;
        Ok(PureStep::Complete((self.compute)(&self.inputs)?))
    }
}

/// The representation a value of `declared` carries, refusing a value of any
/// other type.
pub(crate) fn representation_of<'a>(
    value: &'a Value,
    declared: &types::Declared,
) -> PithResult<&'a Value> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == declared.name() => Ok(representation),
        other => Err(diag(&format!(
            "expected a {} value, found {}",
            declared.name(),
            other.describe()
        ))),
    }
}

/// The text a value carries, refusing anything else.
pub(crate) fn text_of(value: &Value) -> PithResult<&str> {
    match value {
        Value::Text(text) => Ok(text),
        other => Err(diag(&format!("expected text, found {}", other.describe()))),
    }
}

/// The texts a list value carries.
pub(crate) fn text_list_of(value: &Value) -> PithResult<Vec<Box<str>>> {
    match value {
        Value::List(items) => items
            .iter()
            .map(text_of)
            .map(|item| item.map(Into::into))
            .collect(),
        other => Err(diag(&format!(
            "expected a list of texts, found {}",
            other.describe()
        ))),
    }
}

/// One field of a record, or `None` when the record does not carry it.
pub(crate) fn field_of<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    let Value::Record(record) = value else {
        return None;
    };
    record
        .iter()
        .find(|entry| entry.name.as_ref() == field)
        .map(|entry| &entry.payload)
}

/// The contributions a contribution list carries: each owner and the payload
/// under `payload_field`.
pub(crate) fn contributions_of(
    value: &Value,
    payload_field: &str,
) -> PithResult<Vec<(Box<str>, Value)>> {
    let Value::List(items) = value else {
        return Err(diag(&format!(
            "expected a list of contributions, found {}",
            value.describe()
        )));
    };
    let mut contributions = Vec::with_capacity(items.len());
    for item in items {
        let Value::Record(fields) = item else {
            return Err(diag(&format!(
                "a contribution was {}, not an owner-and-payload record",
                item.describe()
            )));
        };
        let owner = fields
            .iter()
            .find(|entry| entry.name.as_ref() == types::OWNER)
            .map(|entry| entry.payload.clone());
        let payload = fields
            .iter()
            .find(|entry| entry.name.as_ref() == payload_field)
            .map(|entry| entry.payload.clone());
        match (owner, payload) {
            (Some(Value::Text(owner)), Some(payload)) => {
                contributions.push((owner, payload));
            }
            _ => {
                return Err(diag("a contribution was missing its owner or its payload"));
            }
        }
    }
    Ok(contributions)
}

/// The entries a file set value carries, in the sorted order the value
/// already has.
pub(crate) fn file_entries_of(value: &Value) -> PithResult<Vec<(Box<str>, FileBody)>> {
    let representation = representation_of(value, types::file_set())?;
    let Value::List(items) = representation else {
        return Err(diag(&format!(
            "a {} value carried {} rather than a list",
            types::file_set().name(),
            representation.describe()
        )));
    };
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let path = field_of(item, types::PATH);
        let body = field_of(item, "body");
        let (Some(path), Some(body)) = (path, body) else {
            return Err(diag("a file entry was missing its path or its body"));
        };
        entries.push((text_of(path)?.into(), file_body_of(body)?));
    }
    Ok(entries)
}

fn file_body_of(value: &Value) -> PithResult<FileBody> {
    let Value::Sum {
        type_name,
        constructor,
        payload,
    } = value
    else {
        return Err(diag(&format!(
            "a file entry's body was {}, not a declared {}",
            value.describe(),
            types::file_body().name()
        )));
    };
    if type_name.as_ref() != types::file_body().name() {
        return Err(diag(&format!(
            "a file entry's body named {type_name}, not {}",
            types::file_body().name()
        )));
    }
    let payload = payload.as_deref();
    match (constructor.as_ref(), payload) {
        ("file", Some(record)) => {
            let Some(Value::Blob(content)) = field_of(record, types::CONTENT) else {
                return Err(diag("a file body did not carry content"));
            };
            let Some(Value::Bool(executable)) = field_of(record, types::EXECUTABLE) else {
                return Err(diag("a file body did not carry an executable flag"));
            };
            Ok(FileBody::File {
                content: *content,
                executable: *executable,
            })
        }
        ("symlink", Some(record)) => {
            let Some(target) = field_of(record, types::TARGET) else {
                return Err(diag("a symlink body did not carry a target"));
            };
            Ok(FileBody::Symlink {
                target: text_of(target)?.into(),
            })
        }
        _ => Err(diag(&format!(
            "a file entry's body named the `{constructor}` constructor, which {} does not \
             declare",
            types::file_body().name()
        ))),
    }
}

/// The accounts a user table value carries, in the sorted order the value
/// already has.
pub(crate) fn user_entries_of(value: &Value) -> PithResult<Vec<(Box<str>, Value)>> {
    let representation = representation_of(value, types::user_table())?;
    let Value::List(items) = representation else {
        return Err(diag(&format!(
            "a {} value carried {} rather than a list",
            types::user_table().name(),
            representation.describe()
        )));
    };
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let Some(name) = field_of(item, types::NAME) else {
            return Err(diag("a user account was missing its name"));
        };
        entries.push((text_of(name)?.into(), item.clone()));
    }
    Ok(entries)
}

/// The fields a unit value carries, as the render and the entry read them.
pub(crate) struct UnitParts {
    pub name: Box<str>,
    pub description: Box<str>,
    pub exec: Box<str>,
    pub after: Vec<Box<str>>,
    pub wants: Vec<Box<str>>,
}

pub(crate) fn unit_parts_of(value: &Value) -> PithResult<UnitParts> {
    let record = representation_of(value, types::unit())?;
    let Some(name) = field_of(record, types::NAME) else {
        return Err(diag("a unit was missing its name"));
    };
    let Some(description) = field_of(record, types::DESCRIPTION) else {
        return Err(diag("a unit was missing its description"));
    };
    let Some(exec) = field_of(record, types::EXEC) else {
        return Err(diag("a unit was missing its exec command"));
    };
    let after = match field_of(record, types::AFTER) {
        Some(after) => text_list_of(after)?,
        None => Vec::new(),
    };
    let wants = match field_of(record, types::WANTS) {
        Some(wants) => text_list_of(wants)?,
        None => Vec::new(),
    };
    Ok(UnitParts {
        name: text_of(name)?.into(),
        description: text_of(description)?.into(),
        exec: text_of(exec)?.into(),
        after,
        wants,
    })
}
