//! The header universe and the discovery action that stages it.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionProgram, ActionSpec, Content, ExitStatusContract,
    NetworkPolicy, OutputKind, PlatformRequirement, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionRule, ProducedOutput};
use pith_ids::ContentId;

use super::{
    DEPFILE_PATH, SOURCE_PATH, blob_of, compiler_environment, diag, input, requested_toolchain,
    rule_revision,
};
use crate::toolchain::Toolchains;
use crate::types;

/// The headers a build offers to `#include`: each at the path the compiler
/// names it in a depfile, with its content identity.
///
/// The universe is host configuration the build library assembles before the
/// run, on the same terms as [`Toolchain::discover`](crate::Toolchain::discover):
/// decision 0007 forbids discovering dependencies during evaluation, and the
/// universe is not a dependency — it is the declared set of candidates the
/// discovery pass may choose from. Which of them a compile actually reads is
/// the discovered fact.
#[derive(Clone, Debug)]
pub struct HeaderUniverse {
    entries: Box<[(Box<str>, ContentId)]>,
}

impl HeaderUniverse {
    /// Build a universe from `(path, content)` pairs. The paths are relative
    /// and must be spelled as the sources include them (`answer.h`,
    /// `lib/util.h`).
    #[must_use]
    pub fn new(entries: Box<[(Box<str>, ContentId)]>) -> Self {
        Self { entries }
    }

    /// An empty universe, for sources that include nothing outside the
    /// toolchain.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Box::new([]),
        }
    }

    /// The `(path, content)` pairs in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &(Box<str>, ContentId)> {
        self.entries.iter()
    }

    /// The content identity a depfile path resolves to, or `None` when the
    /// universe does not offer that path.
    #[must_use]
    pub(crate) fn resolve(&self, path: &str) -> Option<ContentId> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate.as_ref() == path)
            .map(|(_, id)| *id)
    }
}

/// Runs the preprocessor over one source and captures the depfile naming what
/// it includes. The universe is staged whole — the preprocessor reads what the
/// source asks for, and landlock confines it to what was staged — and the
/// depfile is the kernel's own record of which files those were.
pub struct HeaderDiscoveryAction {
    toolchains: Toolchains,
    universe: HeaderUniverse,
}

impl HeaderDiscoveryAction {
    #[must_use]
    pub fn new(toolchains: Toolchains, universe: HeaderUniverse) -> Self {
        Self {
            toolchains,
            universe,
        }
    }

    /// The action rule, ready to register against the discovery interface.
    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::new(
            rule_revision("discover"),
            "discover",
            types::discovery_interface(),
            Span::none(),
        )
    }
}

impl ActionRule for HeaderDiscoveryAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let source = blob_of(input(inputs, 1)?, types::C_SOURCE)?;
        let mut action_inputs = vec![ActionInput {
            path: SOURCE_PATH.into(),
            content: Content::Blob(source),
        }];
        for (path, id) in self.universe.iter() {
            action_inputs.push(ActionInput {
                path: path.clone(),
                content: Content::Blob(*id),
            });
        }
        Ok(ActionSpec {
            executable: ActionProgram::HostPath(toolchain.driver.clone()),
            toolchain: toolchain.closure.clone(),
            arguments: [
                "-MM".into(),
                "-MF".into(),
                DEPFILE_PATH.into(),
                SOURCE_PATH.into(),
            ]
            .into(),
            inputs: action_inputs.into_boxed_slice(),
            outputs: [ActionOutput {
                path: DEPFILE_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: compiler_environment(toolchain),
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
            content: Content::Blob(depfile),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the discovery action captured no depfile"));
        };
        Ok(types::depfile(*depfile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain::Toolchain;

    fn universe() -> HeaderUniverse {
        HeaderUniverse::new(
            vec![
                ("answer.h".into(), ContentId::of_blob(b"answer")),
                ("unused.h".into(), ContentId::of_blob(b"unused")),
            ]
            .into_boxed_slice(),
        )
    }

    #[test]
    fn the_discovery_pass_stages_the_whole_universe() {
        let toolchain = Toolchain {
            driver: "/bin/cc".into(),
            closure: Box::new([]),
            program_path: Some("/bin".into()),
            tool_directory: "/bin".into(),
        };
        let discovery = HeaderDiscoveryAction::new(Toolchains::one(toolchain), universe());
        let inputs = vec![
            types::toolchain("/bin/cc"),
            types::c_source(ContentId::of_blob(b"source")),
        ];
        let spec = discovery.plan(&inputs).expect("the discovery plans");

        let paths: Vec<&str> = spec.inputs.iter().map(|i| i.path.as_ref()).collect();
        assert_eq!(paths, ["source.c", "answer.h", "unused.h"]);
        assert!(
            spec.arguments
                .iter()
                .any(|arg| arg.as_ref() == DEPFILE_PATH),
            "the discovery pass writes a depfile: {spec:?}"
        );
    }
}
