//! The test action and the pure entry that requests it.

use pith_core::{
    Action, ActionProgram, ActionSpec, BodyRevision, ExitStatusContract, NetworkPolicy,
    PlatformRequirement, Pure, Request, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionExit, ActionRule, PureRule, PureRuleFrame};

use super::{ActionRequestFrame, blob_of, diag, input, requested_toolchain};
use crate::toolchain::Toolchains;
use crate::types;

/// Runs a built executable and reads its verdict from how it ended.
///
/// The program is the executable as content the graph produced, so the
/// contract names the bytes under test. The toolchain closure is declared
/// because a dynamically linked binary needs the loader in `PT_INTERP` and the
/// libraries on its `RUNPATH`, both store paths inside a nix toolchain's
/// closure. The contract reports the exit status rather than failing on it: a
/// nonzero exit is a finding, and findings are recorded and reused.
pub struct TestAction {
    toolchains: Toolchains,
}

impl TestAction {
    #[must_use]
    pub fn new(toolchains: Toolchains) -> Self {
        Self { toolchains }
    }

    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::declared(
            types::MODULE,
            "test",
            BodyRevision(1),
            types::test_interface(),
            Span::none(),
        )
    }
}

impl ActionRule for TestAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let executable = blob_of(input(inputs, 1)?, types::executable_name())?;
        Ok(ActionSpec {
            executable: ActionProgram::Content(executable),
            toolchain: toolchain.closure.clone(),
            arguments: Box::new([]),
            inputs: Box::new([]),
            // A test says what it found by how it ends; a declared output
            // would impose a report format on the program.
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::Reported,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        let Some(exit) = execution.exit else {
            return Err(diag(
                "the executor reported no exit status, so the test has no verdict",
            ));
        };
        // Only exit code zero passes. A program killed by a signal reported
        // nothing, and reading that as a pass would call a crash, or a
        // confinement kill, a success.
        Ok(types::test_report(exit == ActionExit::Code(0)))
    }
}

/// The pure entry a build requests to run a test, so the verdict is a pure
/// result that reuse and hydration reach.
pub struct TestRule;

impl TestRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            types::MODULE,
            "test-entry",
            BodyRevision(1),
            types::test_interface(),
            Span::none(),
        )
    }
}

impl PureRule for TestRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let request = Request::<Action>::new(
            "test",
            types::test_interface(),
            inputs.to_vec(),
            Span::none(),
        );
        Box::new(ActionRequestFrame {
            action: Some(request),
        })
    }
}
