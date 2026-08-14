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
    Action, ActionInput, ActionOutput, ActionProgram, ActionSpec, Content, EnvironmentVariable,
    ExitStatusContract, NetworkPolicy, OutputKind, PlatformRequirement, Pure, Request, Rule,
    RuleIdentity, RuleRevision, Value,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    ActionExecution, ActionExit, ActionRule, ProducedOutput, PureRule, PureRuleFrame, PureStep,
    Resumption,
};
use pith_ids::ContentId;

use crate::depfile;
use crate::toolchain::{Toolchain, Toolchains};
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

/// The search paths a driver needs to find the rest of itself. `COMPILER_PATH`
/// appears only for a driver that has a separate compiler to find, since an
/// empty one would be a declaration with nothing behind it.
fn compiler_environment(toolchain: &Toolchain) -> Box<[EnvironmentVariable]> {
    let mut environment = Vec::with_capacity(2);
    if let Some(program_path) = &toolchain.program_path {
        environment.push(EnvironmentVariable {
            name: "COMPILER_PATH".into(),
            value: program_path.clone(),
        });
    }
    environment.push(EnvironmentVariable {
        name: "PATH".into(),
        value: toolchain.tool_directory.clone(),
    });
    environment.into_boxed_slice()
}

/// The driver path a toolchain value carries, which is its identity for dispatch.
fn driver_of(value: &Value) -> PithResult<&str> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == types::TOOLCHAIN => match representation.as_ref() {
            Value::Text(driver) => Ok(driver),
            _ => Err(diag("a Toolchain value carried no driver path")),
        },
        _ => Err(diag(&format!(
            "expected a {} value, found {}",
            types::TOOLCHAIN,
            value.describe()
        ))),
    }
}

/// The registered toolchain the request's first input names.
fn requested_toolchain<'a>(
    toolchains: &'a Toolchains,
    inputs: &[Value],
) -> PithResult<&'a Toolchain> {
    let driver = driver_of(input(inputs, 0)?)?;
    toolchains.resolve(driver).ok_or_else(|| {
        diag(&format!(
            "the request named the toolchain `{driver}`, which this build was not registered with"
        ))
    })
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

const SOURCE_PATH: &str = "source.c";
const OBJECT_PATH: &str = "source.o";
const DEPFILE_PATH: &str = "deps.d";

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

/// Links any number of objects into one executable. The objects arrive as one
/// request input, a `List<Object>` (decision 0035); each is staged at a path
/// derived from its position, which is the order the caller's list gave and is
/// the order the driver receives them in.
pub struct LinkAction {
    toolchains: Toolchains,
}

impl LinkAction {
    #[must_use]
    pub fn new(toolchains: Toolchains) -> Self {
        Self { toolchains }
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

const EXECUTABLE_PATH: &str = "out";

/// The staged path of the object at `position`: `object-0.o`, `object-1.o`, ….
/// Derived from position so the contract is a function of the list and nothing
/// else; two link requests over the same objects in the same order plan the
/// same contract and share a cache entry.
fn object_path(position: usize) -> Box<str> {
    format!("object-{position}.o").into()
}

impl ActionRule for LinkAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let Value::List(objects) = input(inputs, 1)? else {
            return Err(diag(
                "the link request carried no object list as its second input",
            ));
        };
        if objects.is_empty() {
            return Err(diag("the link request carried an empty object list"));
        }
        let mut action_inputs = Vec::with_capacity(objects.len());
        let mut arguments = Vec::with_capacity(objects.len().saturating_add(2));
        for (position, value) in objects.iter().enumerate() {
            let id = blob_of(value, types::OBJECT)?;
            let path = object_path(position);
            arguments.push(path.clone());
            action_inputs.push(ActionInput {
                path,
                content: Content::Blob(id),
            });
        }
        arguments.push("-o".into());
        arguments.push(EXECUTABLE_PATH.into());
        Ok(ActionSpec {
            executable: ActionProgram::HostPath(toolchain.driver.clone()),
            toolchain: toolchain.closure.clone(),
            arguments: arguments.into(),
            inputs: action_inputs.into_boxed_slice(),
            outputs: [ActionOutput {
                path: EXECUTABLE_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: [EnvironmentVariable {
                name: "PATH".into(),
                value: toolchain.tool_directory.clone(),
            }]
            .into(),
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
        Rule::<Action>::new(
            rule_revision("generate"),
            "generate",
            types::generate_interface(),
            Span::none(),
        )
    }
}

const GENERATED_PATH: &str = "generated.c";

impl ActionRule for GenerateAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let generator = blob_of(input(inputs, 1)?, types::EXECUTABLE)?;
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
        Rule::<Pure>::new(
            rule_revision("generate-entry"),
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
        let executable = blob_of(input(inputs, 1)?, types::EXECUTABLE)?;
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
            program_path: Some("/bin".into()),
            tool_directory: "/bin".into(),
        };
        CompileAction::new(Toolchains::one(toolchain), universe())
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

    fn link_action() -> LinkAction {
        let toolchain = Toolchain {
            driver: "/bin/cc".into(),
            closure: Box::new([]),
            program_path: Some("/bin".into()),
            tool_directory: "/bin".into(),
        };
        LinkAction::new(Toolchains::one(toolchain))
    }

    #[test]
    fn a_link_plans_one_staged_path_per_object_in_list_order() {
        let first = ContentId::of_blob(b"first");
        let second = ContentId::of_blob(b"second");
        let third = ContentId::of_blob(b"third");
        let request = types::link_request(types::toolchain("/bin/cc"), [first, second, third]);
        let spec = link_action()
            .plan(request.inputs.as_ref())
            .expect("the link plans");

        let paths: Vec<&str> = spec.inputs.iter().map(|i| i.path.as_ref()).collect();
        assert_eq!(paths, ["object-0.o", "object-1.o", "object-2.o"]);
        assert_eq!(
            spec.arguments
                .iter()
                .map(|a| a.as_ref())
                .collect::<Vec<_>>(),
            ["object-0.o", "object-1.o", "object-2.o", "-o", "out"]
        );
        // Each staged input carries the content identity at its position, so
        // the contract — and the action key derived from it — is a function of
        // the list.
        let staged: Vec<ContentId> = spec
            .inputs
            .iter()
            .map(|i| match i.content {
                Content::Blob(id) => id,
                Content::Tree(_) => unreachable!("an object stages as a blob"),
            })
            .collect();
        assert_eq!(staged, [first, second, third]);
    }

    #[test]
    fn a_reordering_of_the_same_objects_plans_a_different_contract() {
        let first = ContentId::of_blob(b"first");
        let second = ContentId::of_blob(b"second");
        let forward = types::link_request(types::toolchain("/bin/cc"), [first, second]);
        let reversed = types::link_request(types::toolchain("/bin/cc"), [second, first]);
        let forward_spec = link_action()
            .plan(forward.inputs.as_ref())
            .expect("the forward link plans");
        let reversed_spec = link_action()
            .plan(reversed.inputs.as_ref())
            .expect("the reversed link plans");

        // Object order reaches the driver, and the contract says so: a
        // reordered link is a different request, not a cache hit on the same
        // set. (A linker is free to make order observable through symbol
        // resolution and layout.)
        assert_ne!(
            forward_spec.digest().expect("the forward spec digests"),
            reversed_spec.digest().expect("the reversed spec digests")
        );
    }

    #[test]
    fn an_empty_object_list_fails_the_plan() {
        let request = types::link_request(types::toolchain("/bin/cc"), std::iter::empty());
        let error = link_action()
            .plan(request.inputs.as_ref())
            .expect_err("an empty link is not a plan");
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("empty object list")),
            "the diagnostics should name the empty list: {error:?}"
        );
    }

    #[test]
    fn a_bare_blob_in_the_object_list_fails_the_plan() {
        // The list's element type is nominal: a `Value::Blob` that skipped the
        // `xylem.Object` constructor is a type error the planner reports
        // rather than links.
        let request = Request::<Pure>::new(
            "link-entry",
            types::link_interface(),
            [
                types::toolchain("/bin/cc"),
                Value::List(
                    vec![Value::Blob(ContentId::of_blob(b"not-an-object"))].into_boxed_slice(),
                ),
            ],
            Span::none(),
        );
        let error = link_action()
            .plan(request.inputs.as_ref())
            .expect_err("a bare blob is not an object value");
        assert!(
            error.iter().any(|d| d.message.0.contains("xylem.Object")),
            "the diagnostics should name the expected nominal type: {error:?}"
        );
    }
}
