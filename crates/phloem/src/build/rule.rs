use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::diag;

use super::model::{PackageBuild, SourceTree, build_type, tree_from_value, tree_type};

/// Identity input for the package build rule revision.
const PACKAGE_BUILD_MANIFEST: &[u8] = b"phloem-package-build-v1";

/// Returns the `(Toolchain, Tree, Build) -> Executable` interface.
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

/// Creates a request to build `build` from `tree` with `toolchain_value`.
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

/// Resolves the declared source paths in link order.
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

/// Compiles a package's declared sources and links one executable.
pub struct PackageBuildRule;

impl PackageBuildRule {
    /// Creates the package build rule declaration.
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

/// Extracts a blob identity from a nominal content value.
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
