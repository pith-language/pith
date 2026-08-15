use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::diag;

use super::model::{PackageBuild, SourceTree, build_type, tree_from_value, tree_type};

/// The revision the package-build rule derives its identity from. Bumping
/// this invalidates every cached package build, which is what a semantic
/// change to the procedure should do.
const PACKAGE_BUILD_MANIFEST: &[u8] = b"phloem-package-build-v1";

/// `(Toolchain, Tree, Build) -> Executable`: the interface of the package
/// build. The output is xylem's nominal executable — the artifact
/// interface both a build and a deployment can name — and the inputs are
/// the same request-input half of a realization's identity every other
/// build in the graph runs on.
#[must_use]
pub fn package_build_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
            tree_type(),
            build_type(),
        ]),
        output: Type::Nominal {
            name: xylem::types::EXECUTABLE.into(),
        },
    }
}

/// A pure request to build one package: compile `build`'s sources out of
/// `tree` under `toolchain_value` and link the objects into one
/// executable.
#[must_use]
pub fn build_request(
    toolchain_value: Value,
    tree: &SourceTree,
    build: &PackageBuild,
) -> Request<Pure> {
    Request::<Pure>::new(
        "package-build",
        package_build_interface(),
        [toolchain_value, tree.to_value(), build.to_value()],
        Span::none(),
    )
}

/// Resolve the build's declared source paths against the tree, in link
/// order. A path the tree does not hold is a diagnostic naming it: the
/// build prescribes content by location inside its own source, and a
/// prescription the source cannot fill is refused loudly rather than
/// narrowed.
pub(super) fn resolve_sources(
    tree: &SourceTree,
    build: &PackageBuild,
) -> PithResult<Box<[ContentId]>> {
    if build.sources.is_empty() {
        return Err(diag(
            "the build prescribes no source, and a package with nothing to compile has no \
             procedure to run",
        ));
    }
    let mut resolved = Vec::with_capacity(build.sources.len());
    for path in build.sources.iter() {
        match tree.content_at(path) {
            Some(content) => resolved.push(content),
            None => {
                return Err(diag(format!(
                    "the build prescribes `{path}`, which the unpacked tree does not hold"
                )));
            }
        }
    }
    Ok(resolved.into())
}

/// The package build as one pure rule: compile each declared source
/// through xylem's compile entry — discovery included — then link the
/// objects through xylem's link entry. The rule requests interfaces xylem
/// declares and plans nothing itself, which is the peer boundary 0009
/// draws: this rule knows what a package's build is, and xylem knows
/// nothing about packages.
pub struct PackageBuildRule;

impl PackageBuildRule {
    /// The rule, ready to register against the package-build interface.
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        let identity = RuleIdentity::of_module_declaration("phloem", "package-build");
        Rule::<Pure>::new(
            RuleRevision::of_manifest(identity, PACKAGE_BUILD_MANIFEST),
            "package-build",
            package_build_interface(),
            Span::none(),
        )
    }
}

impl PureRule for PackageBuildRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(PackageBuildFrame {
            toolchain_value: inputs.first().cloned().unwrap_or(Value::Unit),
            tree: inputs.get(1).cloned().unwrap_or(Value::Unit),
            build: inputs.get(2).cloned().unwrap_or(Value::Unit),
            phase: BuildPhase::Compiling,
        })
    }
}

enum BuildPhase {
    Compiling,
    Linking,
    Done,
}

struct PackageBuildFrame {
    toolchain_value: Value,
    tree: Value,
    build: Value,
    phase: BuildPhase,
}

impl PureRuleFrame for PackageBuildFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            BuildPhase::Compiling => {
                let tree = tree_from_value(&self.tree)?;
                let build = PackageBuild::from_value(&self.build)?;
                let sources = resolve_sources(&tree, &build)?;
                let compiles = sources
                    .iter()
                    .map(|source| {
                        xylem::types::compile_request(self.toolchain_value.clone(), *source)
                    })
                    .collect::<Vec<_>>();
                self.phase = BuildPhase::Linking;
                // The sources of one package compile independently, and
                // saying so is the body's declaration (0029), not a guess.
                Ok(PureStep::NeedAll(compiles.into_boxed_slice()))
            }
            BuildPhase::Linking => {
                let objects = match input {
                    Some(Resumption::Many(values)) if !values.is_empty() => values,
                    _ => return Err(diag("the compiles did not return one object per source")),
                };
                let identities = objects
                    .iter()
                    .map(|object| blob_of(object, xylem::types::OBJECT))
                    .collect::<Result<Vec<_>, _>>()?;
                self.phase = BuildPhase::Done;
                let link = xylem::types::link_request(self.toolchain_value.clone(), identities);
                Ok(PureStep::Need(link))
            }
            BuildPhase::Done => match input.and_then(Resumption::one) {
                Some(executable) => Ok(PureStep::Complete(executable)),
                None => Err(diag("the link completed without an executable")),
            },
        }
    }
}

/// Extract the blob identity from a nominal content value.
fn blob_of(value: &Value, expected_name: &str) -> PithResult<ContentId> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == expected_name => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            _ => Err(diag(format!(
                "a {expected_name} value carried {representation:?} rather than a blob"
            ))),
        },
        _ => Err(diag(format!(
            "expected a {expected_name} value, found {}",
            value.describe()
        ))),
    }
}
