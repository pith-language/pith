//! A full engine -> policy -> local executor -> content store vertical slice.

#![cfg(target_os = "linux")]

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, Content, Interface, OutputKind, Pure, Request,
    Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionRule, AllowAllActions, DependencyEdge, Engine, EvaluationSource,
    PureRule, PureRuleFrame, PureStep, Resumption, TokioRuntime,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;

mod support;

/// A runtime for one test. Built per call: constructing a thread pool is
/// cheap next to what these tests do, and it keeps each test independent.
fn runtime() -> TokioRuntime {
    match TokioRuntime::new() {
        Ok(runtime) => runtime,
        Err(error) => unreachable!("could not build a tokio runtime: {error:?}"),
    }
}

struct RunScript {
    executable: &'static str,
    input: ContentId,
}

impl ActionRule for RunScript {
    fn plan(&self, _inputs: &[Value]) -> PithResult<ActionSpec> {
        let mut spec = ActionSpec::isolated(self.executable);
        // The script is shell builtins only, so the shell is the whole closure.
        spec.toolchain = support::closure_for(&[self.executable]);
        spec.arguments = [
            "-c".into(),
            "IFS= read -r value < operand; printf 'processed:%s' \"$value\" > result".into(),
        ]
        .into();
        spec.inputs = [ActionInput {
            path: "operand".into(),
            content: Content::Blob(self.input),
        }]
        .into();
        spec.outputs = [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into();
        Ok(spec)
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution
            .report
            .outputs
            .first()
            .map(|output| &output.content)
        {
            Some(Content::Blob(identity)) => Ok(Value::Blob(*identity)),
            _ => Err(fixture_error(
                "local action did not produce its declared blob",
            )),
        }
    }
}

struct DependOnAction {
    request: Request<Action>,
}

impl PureRule for DependOnAction {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(DependOnActionFrame {
            request: self.request.clone(),
            requested: false,
        })
    }
}

struct DependOnActionFrame {
    request: Request<Action>,
    requested: bool,
}

impl PureRuleFrame for DependOnActionFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedAction(self.request.clone()));
        }
        input
            .and_then(Resumption::one)
            .map(PureStep::Complete)
            .ok_or_else(|| fixture_error("action dependency resumed without a value"))
    }
}

struct ReadBlobLength {
    identity: ContentId,
}

impl PureRule for ReadBlobLength {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ReadBlobLengthFrame {
            identity: self.identity,
            requested: false,
        })
    }
}

struct ReadBlobLengthFrame {
    identity: ContentId,
    requested: bool,
}

impl PureRuleFrame for ReadBlobLengthFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if !self.requested {
            self.requested = true;
            return Ok(PureStep::NeedBlob(self.identity));
        }
        match input.and_then(Resumption::one) {
            Some(Value::Bytes(bytes)) => Ok(PureStep::Complete(Value::int(
                i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            ))),
            _ => Err(fixture_error("blob dependency resumed without bytes")),
        }
    }
}

fn interface(inputs: &[Type], output: Type) -> Interface {
    Interface {
        inputs: inputs.into(),
        output,
    }
}

fn rule<K>(label: &str, interface: Interface) -> Rule<K>
where
    K: pith_core::EffectCategory,
{
    let identity = RuleIdentity::of_module_declaration("local-executor-vertical-slice", label);
    let revision = RuleRevision::of_manifest(identity, b"v1");
    Rule::new(
        "local-executor-vertical-slice",
        revision,
        label,
        interface,
        Span::none(),
    )
}

fn request<K>(label: &str, interface: Interface) -> Request<K>
where
    K: pith_core::EffectCategory,
{
    Request::new(
        label,
        interface,
        Vec::<Value>::new().into_boxed_slice(),
        Span::none(),
    )
}

fn fixture_error(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(1211),
        Span::none(),
        message,
    ));
    sink
}

#[test]
fn engine_executes_imports_and_rematerializes_a_real_local_action() {
    if std::fs::read("/bin/sh").is_err() {
        return;
    }
    let mut engine = Engine::new();
    let Some(input) = engine.put_blob(b"hello\n").ok() else {
        return;
    };
    let action_interface = interface(&[], Type::Blob);
    let root_interface = interface(&[], Type::Blob);
    engine.register_action_rule(
        rule::<Action>("process", action_interface.clone()),
        RunScript {
            executable: "/bin/sh",
            input,
        },
    );
    engine.register_rule(
        rule::<Pure>("root", root_interface.clone()),
        DependOnAction {
            request: request::<Action>("process", action_interface),
        },
    );

    let evaluation = engine.run(
        &request::<Pure>("root", root_interface),
        &runtime(),
        &AllowAllActions,
        &LocalExecutor::new(),
    );
    assert!(evaluation.as_ref().is_ok_and(Result::is_ok));
    let Some(evaluation) = evaluation.ok().and_then(Result::ok) else {
        return;
    };
    assert!(matches!(evaluation.value, Value::Blob(_)));
    let Value::Blob(output_identity) = evaluation.value else {
        return;
    };

    assert_eq!(evaluation.source, EvaluationSource::Computed);
    assert_eq!(output_identity, ContentId::of_blob(b"processed:hello"));

    let action_record_is_complete = {
        let query = engine.query();
        let action_computation = query
            .dependencies_of(evaluation.computation)
            .and_then(|dependencies| dependencies.first())
            .and_then(DependencyEdge::computation_id);
        let action_record = action_computation
            .and_then(|computation| query.computation(computation))
            .and_then(|node| node.action.as_ref());
        action_record.is_some_and(|record| {
            record
                .executor_report
                .as_ref()
                .is_some_and(|report| report.executor.as_ref() == "pith-executor-local")
                && record.imported_report.as_ref().is_some_and(|report| {
                    report.outputs.first().is_some_and(|output| {
                        output.path.as_ref() == "result"
                            && output.content == Content::Blob(output_identity)
                    })
                })
        })
    };
    assert!(action_record_is_complete);

    let length_interface = interface(&[], Type::Int);
    engine.register_rule(
        rule::<Pure>("read-imported-output", length_interface.clone()),
        ReadBlobLength {
            identity: output_identity,
        },
    );
    let rematerialized = engine.run(
        &request::<Pure>("read-imported-output", length_interface),
        &runtime(),
        &AllowAllActions,
        &LocalExecutor::new(),
    );

    assert_eq!(
        rematerialized
            .ok()
            .and_then(Result::ok)
            .map(|result| result.value),
        Some(Value::int(15))
    );
}
