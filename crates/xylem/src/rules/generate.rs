//! The generate action and the pure entry that requests it.

use pith_core::{
    Action, ActionOutput, ActionProgram, ActionSpec, Content, ExitStatusContract, NetworkPolicy,
    OutputKind, PlatformRequirement, Pure, Request, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionRule, ProducedOutput, PureRule, PureRuleFrame};

use super::{ActionRequestFrame, GENERATED_PATH, blob_of, diag, input, requested_toolchain};
use crate::toolchain::Toolchains;
use crate::types;

/// Runs a generator the build produced and takes the C source it wrote.
///
/// A codegen tool takes its output path as an argument, so the contract names
/// the path, the program receives it, and the captured file is the result. A
/// generator that wrote somewhere else produces no declared output and fails the
/// action, which is the failure a baked-in path would hide.
pub struct GenerateAction {
    toolchains: Toolchains,
}

impl GenerateAction {
    #[must_use]
    pub fn new(toolchains: Toolchains) -> Self {
        Self { toolchains }
    }

    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::declared(
            types::MODULE,
            "generate",
            types::generate_interface(),
            Span::none(),
        )
    }
}

impl ActionRule for GenerateAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let generator = blob_of(input(inputs, 1)?, types::executable_name())?;
        Ok(ActionSpec {
            executable: ActionProgram::Content(generator),
            toolchain: toolchain.closure.clone(),
            arguments: [GENERATED_PATH.into()].into(),
            inputs: Box::new([]),
            outputs: [ActionOutput {
                path: GENERATED_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: Box::new([]),
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
        let Some(ProducedOutput {
            content: Content::Blob(source),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the generate action captured no source"));
        };
        Ok(types::c_source(*source))
    }
}

/// The pure entry a build requests to generate a source, so the generated source
/// is a pure result later compiles depend on through the graph.
pub struct GenerateRule;

impl GenerateRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            types::MODULE,
            "generate-entry",
            types::generate_interface(),
            Span::none(),
        )
    }
}

impl PureRule for GenerateRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let request = Request::<Action>::new(
            "generate",
            types::generate_interface(),
            inputs.to_vec(),
            Span::none(),
        );
        Box::new(ActionRequestFrame {
            action: Some(request),
        })
    }
}
