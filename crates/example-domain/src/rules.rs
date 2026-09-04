//! The domain's host half: the action body that runs the renderer.
//!
//! The module's declarations — the nominal types, the action's signature, and
//! the entry that checks the template against its bindings — are authored in
//! `example.pi` and arrive as loaded declarations; registration binds this
//! body to the coordinate the file declares. The seam the module follows is
//! the one xylem's rules do. What can be decided from the values alone — that
//! every placeholder the template spells is bound, and that no name is bound
//! twice — is decided in the entry, where a failure is a diagnostic and no
//! process starts. Substituting the text is the renderer's, because the
//! renderer is a program and running one is an action.

use pith_core::{
    ActionInput, ActionOutput, ActionProgram, ActionSpec, Content, ExitStatusContract,
    NetworkPolicy, OutputKind, PlatformRequirement, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{ActionExecution, ActionRule};
use pith_ids::ContentId;

use crate::types;

/// The staged path of the template the renderer reads.
const TEMPLATE_PATH: &str = "template";

/// The staged path of the document the renderer writes.
const DOCUMENT_PATH: &str = "document";

/// The code every diagnostic from this domain carries.
///
/// The 9000 range is where xylem (9002) and phloem (9004) already stamp their
/// diagnostics, and pith-diag documents no allocation rule for it: it reserves
/// a 1000-based engine namespace and a 2000-based composition namespace and
/// says nothing about the rest. A domain from outside the workspace picks a
/// number and hopes. Decision 0056 records that; this crate does not fix it.
const DOMAIN_CODE: StableCode = StableCode(9005);

fn diag(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        DOMAIN_CODE,
        Span::none(),
        message,
    ));
    sink
}

/// The content identity a nominal blob value carries.
fn content_of(value: &Value, declared: &types::Declared) -> PithResult<ContentId> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == declared.name() => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            other => Err(diag(&format!(
                "a {} value carried {} rather than content",
                declared.name(),
                other.describe()
            ))),
        },
        other => Err(diag(&format!(
            "expected a {} value, found {}",
            declared.name(),
            other.describe()
        ))),
    }
}

/// The `(name, value)` pairs a bindings value carries, refusing a name bound
/// twice. The constructor sorts, so a repeated name is adjacent here.
fn pairs_of(value: &Value) -> PithResult<Vec<(Box<str>, Box<str>)>> {
    let Value::Nominal {
        name,
        representation,
    } = value
    else {
        return Err(diag(&format!(
            "expected a {} value, found {}",
            types::bindings().name(),
            value.describe()
        )));
    };
    if name.as_ref() != types::bindings().name() {
        return Err(diag(&format!(
            "expected a {} value, found one named {name}",
            types::bindings().name()
        )));
    }
    let Value::List(entries) = representation.as_ref() else {
        return Err(diag(&format!(
            "a {} value carried {} rather than a list",
            types::bindings().name(),
            representation.describe()
        )));
    };

    let mut pairs: Vec<(Box<str>, Box<str>)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let Value::Record(fields) = entry else {
            return Err(diag(&format!(
                "a binding was {}, not a name-and-value record",
                entry.describe()
            )));
        };
        let mut bound_name = None;
        let mut bound_value = None;
        for field in fields {
            match (field.name.as_ref(), &field.payload) {
                (types::BINDING_NAME, Value::Text(text)) => bound_name = Some(text.clone()),
                (types::BINDING_VALUE, Value::Text(text)) => bound_value = Some(text.clone()),
                _ => {}
            }
        }
        let (Some(bound_name), Some(bound_value)) = (bound_name, bound_value) else {
            return Err(diag("a binding was missing its name or its value"));
        };
        if let Some((previous, _)) = pairs.last()
            && *previous == bound_name
        {
            return Err(diag(&format!(
                "`{bound_name}` is bound twice, so a render of this template has two answers"
            )));
        }
        pairs.push((bound_name, bound_value));
    }
    Ok(pairs)
}

/// The renderer, the template, and the bindings a request supplies.
fn request_parts(inputs: &[Value]) -> PithResult<(ContentId, ContentId, &Value)> {
    let [renderer, template, bound] = inputs else {
        return Err(diag(&format!(
            "a render request supplies a renderer, a template, and bindings; this one supplied {}",
            inputs.len()
        )));
    };
    Ok((
        content_of(renderer, types::renderer())?,
        content_of(template, types::template())?,
        bound,
    ))
}

/// Renders a template by running the renderer over it.
///
/// The contract names the renderer as content (decision 0036), so what it
/// covers is the bytes that will run, and the bindings reach the program as
/// arguments in the canonical order the value already carries.
pub struct RenderAction;

impl ActionRule for RenderAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let (renderer, template, bound) = request_parts(inputs)?;
        let arguments = pairs_of(bound)?
            .into_iter()
            .map(|(name, value)| format!("{name}={value}").into_boxed_str())
            .collect();
        Ok(ActionSpec {
            executable: ActionProgram::Content(renderer),
            toolchain: Box::new([]),
            arguments,
            inputs: Box::new([ActionInput {
                path: TEMPLATE_PATH.into(),
                content: Content::Blob(template),
            }]),
            outputs: Box::new([ActionOutput {
                path: DOCUMENT_PATH.into(),
                kind: OutputKind::Blob,
            }]),
            environment: Box::new([]),
            // A renderer is a built program, so the platform it ran on is part
            // of what makes a recorded execution answer for this run.
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(output) => match &output.content {
                Content::Blob(id) => Ok(types::document().content(*id)),
                Content::Tree(_) => Err(diag(
                    "the renderer wrote a tree where a document was declared",
                )),
            },
            None => Err(diag("the renderer produced no document")),
        }
    }
}
