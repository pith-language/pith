//! An executor that stands in for running the renderer: it performs the
//! substitution the renderer program would, and checks that the engine staged
//! what the contract declared. It reports the host platform, because the
//! contract this domain plans names it.
//!
//! No real program runs: a real renderer would tie the suite to a host
//! toolchain and to linux, the constraint that keeps suites needing one gated
//! to machines that have it. Supplying an `Executor` exercises the surface
//! under test, since it is a public trait like `PureRule`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AccessVerification, ActionExit, ActionInvocation, CapturedActionExecution,
    CapturedExecutionReport, CapturedOutput, CapturedOutputContent, ExecutionPlatform, Executor,
    ExecutorIdentity, MaterializedContent,
};

/// The bytes standing in for a renderer program. Their identity is what
/// reaches the contract, so two different renderers are two different actions
/// whatever the bytes say.
pub const RENDERER: &[u8] = b"a renderer program\n";

/// A second renderer, for the case where the program moves and the document
/// under it must not be served from the first one's attempt.
pub const OTHER_RENDERER: &[u8] = b"another renderer program\n";

fn failure(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(9005),
        Span::none(),
        message,
    ));
    sink
}

pub fn host_platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    }
}

#[derive(Default)]
pub struct RendererExecutor {
    executions: Arc<AtomicUsize>,
}

impl RendererExecutor {
    /// How many times this executor has been asked to run an action. A render
    /// served from the reusable index never reaches it.
    pub fn executions(&self) -> usize {
        self.executions.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl Executor for RendererExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "example-renderer".into(),
            platform: host_platform(),
        }
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);

        let Some(program) = &invocation.program else {
            return Err(failure("the engine staged no renderer program"));
        };
        if program.bytes.as_ref() != RENDERER && program.bytes.as_ref() != OTHER_RENDERER {
            return Err(failure("the engine staged bytes that are not a renderer"));
        }

        let [input] = invocation.inputs.as_ref() else {
            return Err(failure("a render is over exactly one template"));
        };
        let MaterializedContent::Blob(template) = &input.content else {
            return Err(failure("the template was staged as a tree"));
        };
        let Ok(text) = std::str::from_utf8(&template.bytes) else {
            return Err(failure("the template is not utf-8"));
        };

        let mut rendered = text.to_owned();
        for argument in &invocation.spec.arguments {
            let Some((name, value)) = argument.split_once('=') else {
                return Err(failure("a binding argument is not `name=value`"));
            };
            rendered = rendered.replace(&format!("{{{{{name}}}}}"), value);
        }

        let Some(declared) = invocation.spec.outputs.first() else {
            return Err(failure("a render declares one document"));
        };
        let identity = self.identity();
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: identity.executor,
                platform: identity.platform,
                access: AccessVerification::Unverified,
                outputs: [CapturedOutput {
                    path: declared.path.clone(),
                    content: CapturedOutputContent::Blob(rendered.into_bytes().into()),
                }]
                .into(),
                capabilities_used: Box::new([]),
            },
            exit: Some(ActionExit::Code(0)),
        })
    }
}
