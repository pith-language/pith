//! The compile and link rules.
//!
//! Both are action rules whose request inputs carry the typed values dispatch
//! and caching need (the toolchain and the source or object identities). The
//! source `ContentId` reaches `plan()` through `inputs`, so one registration of
//! the compile rule serves every source file: a different source is a different
//! request, which plans a different contract, which computes a different action
//! key (decision 0031). The pure wrappers request the action and hand back its
//! result.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, Content, EnvironmentVariable, NetworkPolicy,
    OutputKind, PlatformRequirement, Pure, Request, Rule, RuleIdentity, RuleRevision, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionRule, ProducedOutput, PureRule, PureRuleFrame, PureStep, Resumption,
};
use pith_ids::ContentId;

use crate::toolchain::Toolchain;
use crate::types;

/// The revision every xylem rule derives its identity from. Bumping this
/// invalidates every cached xylem result, which is what a semantic change to a
/// rule body should do.
fn rule_revision(label: &str) -> RuleRevision {
    let identity = RuleIdentity::of_module_declaration("xylem", label);
    RuleRevision::of_manifest(identity, b"xylem-v1")
}

fn diag(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(9002),
        Span::none(),
        message,
    ));
    sink
}

/// Extracts the blob identity from a nominal content value
/// (`xylem.CSource`, `xylem.Object`), or diagnoses a value that is not one.
fn blob_of(value: &Value, expected_name: &str) -> PithResult<ContentId> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == expected_name => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            _ => Err(diag(&format!(
                "a {expected_name} value carried {representation:?} rather than a blob"
            ))),
        },
        _ => Err(diag(&format!(
            "expected a {expected_name} value, found {}",
            value.describe()
        ))),
    }
}

/// Extract input `index` from `inputs`, diagnosing a request that did not
/// supply enough values.
fn input(inputs: &[Value], index: usize) -> PithResult<&Value> {
    inputs.get(index).ok_or_else(|| {
        diag(&format!(
            "the request supplied {len} input(s); input {index} is missing",
            len = inputs.len()
        ))
    })
}

/// Compiles one C source to one object. The toolchain and source arrive as the
/// request inputs; the header is a declared dependency held by the rule, since
/// it is shared across compiles and is not part of the compile's interface.
pub struct CompileAction {
    toolchain: Toolchain,
    /// The header the source includes, declared so the executor may read it.
    /// `None` when the source includes nothing.
    header: Option<(Box<str>, ContentId)>,
}

impl CompileAction {
    /// A compile rule over `toolchain` whose sources include `header` at
    /// `path`. Sources that include nothing pass `None`.
    #[must_use]
    pub fn new(toolchain: Toolchain, header: Option<(Box<str>, ContentId)>) -> Self {
        Self { toolchain, header }
    }

    /// The action rule, ready to register against the compile interface.
    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::new(
            rule_revision("compile"),
            "compile",
            types::compile_interface(),
            Span::none(),
        )
    }
}

const SOURCE_PATH: &str = "source.c";
const OBJECT_PATH: &str = "source.o";

impl ActionRule for CompileAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let source = blob_of(input(inputs, 1)?, types::C_SOURCE)?;
        let mut action_inputs = vec![ActionInput {
            path: SOURCE_PATH.into(),
            content: Content::Blob(source),
        }];
        if let Some((path, id)) = &self.header {
            action_inputs.push(ActionInput {
                path: path.clone(),
                content: Content::Blob(*id),
            });
        }
        Ok(ActionSpec {
            executable: self.toolchain.driver.clone(),
            toolchain: self.toolchain.closure.clone(),
            arguments: [
                "-c".into(),
                SOURCE_PATH.into(),
                "-o".into(),
                OBJECT_PATH.into(),
            ]
            .into(),
            inputs: action_inputs.into_boxed_slice(),
            outputs: [ActionOutput {
                path: OBJECT_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: [
                EnvironmentVariable {
                    name: "COMPILER_PATH".into(),
                    value: self.toolchain.program_path.clone(),
                },
                EnvironmentVariable {
                    name: "PATH".into(),
                    value: self.toolchain.tool_directory.clone(),
                },
            ]
            .into(),
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        let Some(ProducedOutput {
            content: Content::Blob(object),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the compile action captured no object file"));
        };
        Ok(types::object(*object))
    }
}

/// Links two objects into one executable. Fixed arity two; linking more needs a
/// list or tree-valued content variant (decision 0026) and is its own work.
pub struct LinkAction {
    toolchain: Toolchain,
}

impl LinkAction {
    #[must_use]
    pub fn new(toolchain: Toolchain) -> Self {
        Self { toolchain }
    }

    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::new(
            rule_revision("link"),
            "link",
            types::link_interface(),
            Span::none(),
        )
    }
}

const FIRST_OBJECT_PATH: &str = "first.o";
const SECOND_OBJECT_PATH: &str = "second.o";
const EXECUTABLE_PATH: &str = "out";

impl ActionRule for LinkAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let first = blob_of(input(inputs, 1)?, types::OBJECT)?;
        let second = blob_of(input(inputs, 2)?, types::OBJECT)?;
        Ok(ActionSpec {
            executable: self.toolchain.driver.clone(),
            toolchain: self.toolchain.closure.clone(),
            arguments: [
                FIRST_OBJECT_PATH.into(),
                SECOND_OBJECT_PATH.into(),
                "-o".into(),
                EXECUTABLE_PATH.into(),
            ]
            .into(),
            inputs: [
                ActionInput {
                    path: FIRST_OBJECT_PATH.into(),
                    content: Content::Blob(first),
                },
                ActionInput {
                    path: SECOND_OBJECT_PATH.into(),
                    content: Content::Blob(second),
                },
            ]
            .into(),
            outputs: [ActionOutput {
                path: EXECUTABLE_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: [EnvironmentVariable {
                name: "PATH".into(),
                value: self.toolchain.tool_directory.clone(),
            }]
            .into(),
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        let Some(ProducedOutput {
            content: Content::Blob(executable),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the link action captured no executable"));
        };
        Ok(types::executable(*executable))
    }
}

/// Requests a compile action and hands back its object. One registration
/// serves every source: the toolchain and source arrive as request inputs, so
/// dispatch selects this rule and the computation key distinguishes sources.
pub struct CompileRule;

impl CompileRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::new(
            rule_revision("compile-entry"),
            "compile-entry",
            types::compile_interface(),
            Span::none(),
        )
    }
}

impl PureRule for CompileRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let request = Request::<Action>::new(
            "compile",
            types::compile_interface(),
            inputs.to_vec(),
            Span::none(),
        );
        Box::new(ActionRequestFrame {
            action: Some(request),
        })
    }
}

/// Requests a link action and hands back its executable.
pub struct LinkRule;

impl LinkRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::new(
            rule_revision("link-entry"),
            "link-entry",
            types::link_interface(),
            Span::none(),
        )
    }
}

impl PureRule for LinkRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let request = Request::<Action>::new(
            "link",
            types::link_interface(),
            inputs.to_vec(),
            Span::none(),
        );
        Box::new(ActionRequestFrame {
            action: Some(request),
        })
    }
}

/// A pure frame that yields one action request and completes with its result.
struct ActionRequestFrame {
    action: Option<Request<Action>>,
}

impl PureRuleFrame for ActionRequestFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if let Some(action) = self.action.take() {
            return Ok(PureStep::NeedAction(action));
        }
        match input.and_then(Resumption::one) {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(diag("an action completed without a value")),
        }
    }
}
