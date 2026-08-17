//! The test action and the pure entry that requests it.

use pith_core::{
    Action, ActionProgram, ActionSpec, ExitStatusContract, NetworkPolicy, PlatformRequirement,
    Pure, Request, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionExit, ActionRule, PureRule, PureRuleFrame};

use super::{ActionRequestFrame, blob_of, diag, input, requested_toolchain, rule_revision};
use crate::toolchain::Toolchains;
use crate::types;

/// Runs a built executable and reads its verdict from how it ended.
///
/// The program is the executable itself, as content the graph produced
/// (decision 0036), so the contract names the bytes under test. The toolchain
/// closure is
/// declared because a dynamically linked binary needs the loader it names in
/// `PT_INTERP` and the libraries on its `RUNPATH`, and under a nix toolchain
/// both are store paths inside that closure.
///
/// The contract reports the exit status instead of failing on it (decision
/// 0037): a test that exits nonzero has produced a finding, and the graph
/// records and reuses findings.
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
        Rule::<Action>::new(
            rule_revision("test"),
            "test",
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
            // A test says what it found by how it ends. Declaring an output
            // would specify a report format the program has to write, which is a
            // harness pith would be imposing on it.
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
        // Only a clean zero passes. A program killed by a signal reported
        // nothing, and reading that as a pass would call a crash a success; it
        // would also call a confinement kill one, which is the reading 0037's
        // unresolved section warns about.
        Ok(types::test_report(exit == ActionExit::Code(0)))
    }
}

/// The pure entry a build requests to run a test, so the verdict is a pure
/// result that reuse and hydration reach (decision 0033).
pub struct TestRule;

impl TestRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::new(
            rule_revision("test-entry"),
            "test-entry",
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
