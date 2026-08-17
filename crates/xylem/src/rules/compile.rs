//! The compile action and the pure entry that discovers, then compiles.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionProgram, ActionSpec, Content, ExitStatusContract,
    NetworkPolicy, OutputKind, PlatformRequirement, Pure, Request, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{
    ActionExecution, ActionRule, ProducedOutput, PureRule, PureRuleFrame, PureStep, Resumption,
};

use super::{
    OBJECT_PATH, SOURCE_PATH, blob_of, compiler_environment, diag, effective_headers, input,
    provided_headers_of, requested_toolchain, rule_revision,
};
use crate::HeaderUniverse;
use crate::depfile;
use crate::toolchain::Toolchains;
use crate::types;

/// Compiles one C source to one object. The toolchain, source, and discovered
/// header set arrive as the request inputs; each discovered path is resolved
/// against the registered universe, and the resolved files are the compile's
/// declared inputs, so the contract digests exactly what the source includes.
pub struct CompileAction {
    toolchains: Toolchains,
    universe: HeaderUniverse,
}

impl CompileAction {
    #[must_use]
    pub fn new(toolchains: Toolchains, universe: HeaderUniverse) -> Self {
        Self {
            toolchains,
            universe,
        }
    }

    /// The action rule, ready to register against the compile interface.
    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::new(
            rule_revision("compile"),
            "compile",
            types::compile_action_interface(),
            Span::none(),
        )
    }
}

impl ActionRule for CompileAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let source = blob_of(input(inputs, 1)?, types::c_source_name())?;
        let Value::List(discovered) = input(inputs, 2)? else {
            return Err(diag(
                "the compile request carried no discovered header list as its third input",
            ));
        };
        let provided = provided_headers_of(input(inputs, 3)?)?;
        let headers = effective_headers(&self.universe, &provided)?;
        let mut action_inputs = vec![ActionInput {
            path: SOURCE_PATH.into(),
            content: Content::Blob(source),
        }];
        for path in discovered.iter() {
            let Value::Text(path) = path else {
                return Err(diag(
                    "the discovered header list carried a non-text entry; \
                     only the depfile parser produces this list",
                ));
            };
            // A provided header that agrees with the registered universe is
            // already staged by name; either spelling resolves to one content.
            match headers
                .iter()
                .find(|(offered, _)| offered.as_ref() == path.as_ref())
            {
                Some((offered, id)) => action_inputs.push(ActionInput {
                    path: offered.clone(),
                    content: Content::Blob(*id),
                }),
                None => {
                    return Err(diag(&format!(
                        "the depfile named `{path}`, which neither the registered header \
                         universe nor the provided headers offer; add it to the universe, \
                         provide it, or fix the include"
                    )));
                }
            }
        }
        Ok(ActionSpec {
            executable: ActionProgram::HostPath(toolchain.driver.clone()),
            toolchain: toolchain.closure.clone(),
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
            content: Content::Blob(object),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the compile action captured no object file"));
        };
        Ok(types::object(*object))
    }
}

/// Compiles one source, discovering its header dependencies first. One
/// registration serves every source: the toolchain and source arrive as request
/// inputs, so dispatch selects this rule and the computation key distinguishes
/// sources.
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
        let toolchain_value = match input(inputs, 0) {
            Ok(value) => value.clone(),
            Err(_) => unreachable!("selection checked the interface"),
        };
        let source = match input(inputs, 1) {
            Ok(value) => value.clone(),
            Err(_) => unreachable!("selection checked the interface"),
        };
        let provided = match input(inputs, 2) {
            Ok(value) => value.clone(),
            Err(_) => unreachable!("selection checked the interface"),
        };
        Box::new(CompileEntryFrame {
            toolchain_value,
            source,
            provided,
            phase: CompilePhase::Discover,
        })
    }
}

/// Where one compile entry application is: requesting the discovery pass,
/// reading the depfile it captured, requesting the compile with the discovered
/// set, or holding the object it produced.
enum CompilePhase {
    Discover,
    Depfile,
    Compile,
    Done,
}

struct CompileEntryFrame {
    toolchain_value: Value,
    source: Value,
    provided: Value,
    phase: CompilePhase,
}

impl PureRuleFrame for CompileEntryFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            CompilePhase::Discover => {
                self.phase = CompilePhase::Depfile;
                let request = Request::<Action>::new(
                    "discover",
                    types::discovery_interface(),
                    [
                        self.toolchain_value.clone(),
                        self.source.clone(),
                        self.provided.clone(),
                    ],
                    Span::none(),
                );
                Ok(PureStep::NeedAction(request))
            }
            CompilePhase::Depfile => {
                let depfile = match input.and_then(Resumption::one) {
                    Some(value) => value,
                    None => return Err(diag("the discovery pass completed without a depfile")),
                };
                let depfile_id = blob_of(&depfile, types::depfile_name())?;
                self.phase = CompilePhase::Compile;
                Ok(PureStep::NeedBlob(depfile_id))
            }
            CompilePhase::Compile => {
                let bytes = match input.and_then(Resumption::one) {
                    Some(Value::Bytes(bytes)) => bytes,
                    Some(other) => {
                        return Err(diag(&format!(
                            "the depfile blob materialized as {other:?} rather than bytes"
                        )));
                    }
                    None => return Err(diag("the depfile blob did not materialize")),
                };
                let discovered = depfile::parse(&bytes)
                    .ok_or_else(|| diag("the captured depfile is not valid UTF-8"))?;
                // The source is the depfile's first prerequisite and is staged
                // by the compile on its own; the third input carries the rest.
                let headers: Box<[Box<str>]> = discovered
                    .iter()
                    .filter(|path| path.as_ref() != SOURCE_PATH)
                    .cloned()
                    .collect();
                let request = Request::<Action>::new(
                    "compile",
                    types::compile_action_interface(),
                    [
                        self.toolchain_value.clone(),
                        self.source.clone(),
                        depfile::discovered_value(&headers),
                        self.provided.clone(),
                    ],
                    Span::none(),
                );
                self.phase = CompilePhase::Done;
                Ok(PureStep::NeedAction(request))
            }
            CompilePhase::Done => match input.and_then(Resumption::one) {
                Some(object) => Ok(PureStep::Complete(object)),
                None => Err(diag("the compile completed without an object")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::HeaderUniverse;
    use crate::toolchain::Toolchain;
    use pith_ids::ContentId;

    fn universe() -> HeaderUniverse {
        HeaderUniverse::new(
            vec![
                ("answer.h".into(), ContentId::of_blob(b"answer")),
                ("unused.h".into(), ContentId::of_blob(b"unused")),
            ]
            .into_boxed_slice(),
        )
    }

    /// `plan()` receives no toolchain here and does not read one: the spec's
    /// executable and closure fields are what the toolchain fills in, and this
    /// test is about which inputs the discovered set selects. A placeholder
    /// driver outside `/nix/store` keeps the spec valid without discovering a
    /// real toolchain.
    fn compile_action() -> CompileAction {
        let toolchain = Toolchain {
            driver: "/bin/cc".into(),
            closure: Box::new([]),
            program_path: Some("/bin".into()),
            tool_directory: "/bin".into(),
        };
        CompileAction::new(Toolchains::one(toolchain), universe())
    }

    fn compile_inputs(discovered: &[&str], provided: &[(Box<str>, ContentId)]) -> Vec<Value> {
        vec![
            types::toolchain("/bin/cc"),
            types::c_source(ContentId::of_blob(b"source")),
            types::headers(discovered.iter().copied()),
            types::provided_headers(provided.to_vec()),
        ]
    }

    #[test]
    fn the_discovered_set_selects_which_headers_the_compile_declares() {
        // The universe offers two headers; the source includes one. The planned
        // contract stages the source and the included header, and not the
        // unincluded one, which is the fine-grained claim the depfile pass
        // exists to make. The entry frame has already dropped the source's own
        // prerequisite token, which is why it is absent from the list.
        let spec = compile_action()
            .plan(&compile_inputs(&["answer.h"], &[]))
            .expect("the compile plans");

        let paths: Vec<&str> = spec.inputs.iter().map(|i| i.path.as_ref()).collect();
        assert_eq!(paths, ["source.c", "answer.h"]);
    }

    #[test]
    fn a_discovered_path_the_universe_does_not_offer_fails_the_plan() {
        // The loud half of declared-first: a depfile naming a path outside the
        // universe is a diagnostic, never a silently narrowed input set.
        let error = compile_action()
            .plan(&compile_inputs(&["elsewhere.h"], &[]))
            .expect_err("the path is not in the universe");
        let message = error.iter().next().map(|d| d.message.clone());
        assert!(message.is_some_and(|m| m.0.contains("elsewhere.h")));
    }

    #[test]
    fn a_provided_header_resolves_a_path_the_universe_does_not_offer() {
        // The package-dependency case: the include comes from another build's
        // output, arrives as request data, and the contract stages its content.
        let provided = [("util-1.0/util.h".into(), ContentId::of_blob(b"util header"))];
        let spec = compile_action()
            .plan(&compile_inputs(&["util-1.0/util.h"], &provided))
            .expect("the compile plans over a provided header");
        let staged: Vec<(&str, &pith_ids::ContentId)> = spec
            .inputs
            .iter()
            .map(|i| (i.path.as_ref(), content_of(i)))
            .collect();
        assert_eq!(
            staged,
            [
                ("source.c", &ContentId::of_blob(b"source")),
                ("util-1.0/util.h", &ContentId::of_blob(b"util header")),
            ],
            "the contract stages the provided content under the include's own spelling"
        );
    }

    /// The blob identity a staged input carries.
    fn content_of(input: &pith_core::ActionInput) -> &ContentId {
        match &input.content {
            pith_core::Content::Blob(id) => id,
            _ => unreachable!("the planned inputs are blobs"),
        }
    }
}
