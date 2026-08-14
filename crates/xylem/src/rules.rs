//! The discovery, compile, and link rules.
//!
//! Header dependencies are discovered, not declared by hand (decision 0034).
//! The discovery pass is its own action — the preprocessor over the source
//! with the whole header universe staged, capturing a depfile — and the
//! compile entry parses that depfile and requests the compile with the
//! discovered set as a request input. The compile's `plan()` resolves each
//! discovered path against the universe it was registered with, so the
//! contract it digests names exactly the headers the source includes.
//!
//! Every request input here carries what dispatch and caching need (the
//! toolchain, the source or object identities, the discovered set). The source
//! `ContentId` reaches `plan()` through `inputs`, so one registration of the
//! compile rule serves every source file: a different source is a different
//! request, which plans a different contract, which computes a different action
//! key (decision 0031).

use pith_core::{
    Action, ActionInput, ActionOutput, ActionSpec, Content, EnvironmentVariable, NetworkPolicy,
    OutputKind, PlatformRequirement, Pure, Request, Rule, RuleIdentity, RuleRevision, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionRule, ProducedOutput, PureRule, PureRuleFrame, PureStep, Resumption,
};
use pith_ids::ContentId;

use crate::depfile;
use crate::toolchain::Toolchain;
use crate::types;

/// The revision every xylem rule derives its identity from. Bumping this
/// invalidates every cached xylem result, which is what a semantic change to a
/// rule body should do.
fn rule_revision(label: &str) -> RuleRevision {
    let identity = RuleIdentity::of_module_declaration("xylem", label);
    RuleRevision::of_manifest(identity, b"xylem-v2")
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
    fn resolve(&self, path: &str) -> Option<ContentId> {
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
    toolchain: Toolchain,
    universe: HeaderUniverse,
}

impl HeaderDiscoveryAction {
    #[must_use]
    pub fn new(toolchain: Toolchain, universe: HeaderUniverse) -> Self {
        Self {
            toolchain,
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

const SOURCE_PATH: &str = "source.c";
const OBJECT_PATH: &str = "source.o";
const DEPFILE_PATH: &str = "deps.d";

impl ActionRule for HeaderDiscoveryAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
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
            executable: self.toolchain.driver.clone(),
            toolchain: self.toolchain.closure.clone(),
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
            content: Content::Blob(depfile),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the discovery action captured no depfile"));
        };
        Ok(types::depfile(*depfile))
    }
}

/// Compiles one C source to one object. The toolchain, source, and discovered
/// header set arrive as the request inputs; each discovered path is resolved
/// against the registered universe, and the resolved files are the compile's
/// declared inputs, so the contract digests exactly what the source includes.
pub struct CompileAction {
    toolchain: Toolchain,
    universe: HeaderUniverse,
}

impl CompileAction {
    #[must_use]
    pub fn new(toolchain: Toolchain, universe: HeaderUniverse) -> Self {
        Self {
            toolchain,
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
        let source = blob_of(input(inputs, 1)?, types::C_SOURCE)?;
        let Value::List(discovered) = input(inputs, 2)? else {
            return Err(diag(
                "the compile request carried no discovered header list as its third input",
            ));
        };
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
            match self.universe.resolve(path) {
                Some(id) => action_inputs.push(ActionInput {
                    path: path.clone(),
                    content: Content::Blob(id),
                }),
                None => {
                    return Err(diag(&format!(
                        "the depfile named `{path}`, which the registered header universe \
                         does not offer; add it to the universe or fix the include"
                    )));
                }
            }
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
        Box::new(CompileEntryFrame {
            toolchain_value,
            source,
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
                    [self.toolchain_value.clone(), self.source.clone()],
                    Span::none(),
                );
                Ok(PureStep::NeedAction(request))
            }
            CompilePhase::Depfile => {
                let depfile = match input.and_then(Resumption::one) {
                    Some(value) => value,
                    None => return Err(diag("the discovery pass completed without a depfile")),
                };
                let depfile_id = blob_of(&depfile, types::DEPFILE)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
            program_path: "/bin".into(),
            tool_directory: "/bin".into(),
        };
        CompileAction::new(toolchain, universe())
    }

    fn compile_inputs(discovered: &[&str]) -> Vec<Value> {
        vec![
            types::toolchain("/bin/cc"),
            types::c_source(ContentId::of_blob(b"source")),
            types::headers(discovered.iter().copied()),
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
            .plan(&compile_inputs(&["answer.h"]))
            .expect("the compile plans");

        let paths: Vec<&str> = spec.inputs.iter().map(|i| i.path.as_ref()).collect();
        assert_eq!(paths, ["source.c", "answer.h"]);
    }

    #[test]
    fn a_discovered_path_the_universe_does_not_offer_fails_the_plan() {
        // The loud half of declared-first: a depfile naming a path outside the
        // universe is a diagnostic, never a silently narrowed input set.
        let error = compile_action()
            .plan(&compile_inputs(&["elsewhere.h"]))
            .expect_err("the path is not in the universe");
        let message = error.iter().next().map(|d| d.message.clone());
        assert!(message.is_some_and(|m| m.0.contains("elsewhere.h")));
    }

    #[test]
    fn the_discovery_pass_stages_the_whole_universe() {
        let toolchain = Toolchain {
            driver: "/bin/cc".into(),
            closure: Box::new([]),
            program_path: "/bin".into(),
            tool_directory: "/bin".into(),
        };
        let discovery = HeaderDiscoveryAction::new(toolchain, universe());
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
