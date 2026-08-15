use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::diag;

use super::model::{
    Dependency, HeaderPair, HeaderSet, Library, PackageBuild, SourceTree, build_type,
    dependency_list_type, dependency_list_value, library_type, tree_from_value, tree_type,
};

/// Identity inputs for the package rule revisions.
const PACKAGE_BUILD_MANIFEST: &[u8] = b"phloem-package-build-v2";
const PACKAGE_LIBRARY_MANIFEST: &[u8] = b"phloem-package-library-v1";

/// Returns the `(Toolchain, Tree, Build, Dependencies) -> Executable`
/// interface, where `Dependencies` is a list of `{tree, build}` records in
/// link order.
#[must_use]
pub fn package_build_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
            tree_type(),
            build_type(),
            dependency_list_type(),
        ]),
        output: Type::Nominal {
            name: xylem::types::EXECUTABLE.into(),
        },
    }
}

/// Returns the `(Toolchain, Tree, Build) -> Library` interface: the same
/// declared procedure a package's own build runs, producing the objects and
/// headers a dependent consumes instead of one executable.
#[must_use]
pub fn package_library_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: xylem::types::TOOLCHAIN.into(),
            },
            tree_type(),
            build_type(),
        ]),
        output: library_type(),
    }
}

/// Creates a request to build `build` from `tree` with `toolchain_value`,
/// linking `dependencies`' libraries after the package's own objects.
#[must_use]
pub fn build_request(
    toolchain_value: Value,
    tree: &SourceTree,
    build: &PackageBuild,
    dependencies: &[Dependency],
) -> Request<Pure> {
    Request::<Pure>::new(
        "package-build",
        package_build_interface(),
        [
            toolchain_value,
            tree.to_value(),
            build.to_value(),
            dependency_list_value(dependencies),
        ],
        Span::none(),
    )
}

/// Creates a request to build `build` from `tree` as a library.
#[must_use]
pub fn library_request(
    toolchain_value: Value,
    tree: &SourceTree,
    build: &PackageBuild,
) -> Request<Pure> {
    Request::<Pure>::new(
        "package-library",
        package_library_interface(),
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

/// Resolves the declared include paths against the tree, canonically sorted.
/// An include the tree does not hold is a diagnostic, the same refusal a
/// prescribed source meets: a declaration naming content the package does not
/// carry is not a build.
pub(super) fn resolve_includes(tree: &SourceTree, build: &PackageBuild) -> PithResult<HeaderSet> {
    let mut resolved: Vec<HeaderPair> = Vec::with_capacity(build.includes.len());
    for path in build.includes.iter() {
        match tree.content_at(path) {
            Some(content) => resolved.push((path.clone(), content)),
            None => {
                return Err(diag(format!(
                    "the build offers `{path}` as an include, which the unpacked tree does \
                     not hold",
                )));
            }
        }
    }
    resolved.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    resolved.dedup_by(|left, right| left.0 == right.0);
    Ok(resolved.into())
}

/// Merges provided header sets — the package's own includes with its
/// dependencies' — refusing one spelling that names two contents. Agreeing
/// duplicates collapse; the result is canonically sorted, so the value is a
/// function of the header set and not of the assembly order.
pub(super) fn merge_provided(own: HeaderSet, libraries: &[Library]) -> PithResult<Value> {
    let mut merged: Vec<HeaderPair> = own.into();
    for library in libraries {
        for (path, content) in library.headers.iter() {
            if let Some((_, existing)) = merged.iter().find(|(offered, _)| offered == path) {
                if existing != content {
                    return Err(diag(format!(
                        "the include path `{path}` resolves to two contents, `{}` and \
                         `{}`: one spelling cannot name two headers",
                        existing.digest(),
                        content.digest(),
                    )));
                }
                continue;
            }
            merged.push((path.clone(), *content));
        }
    }
    merged.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    Ok(xylem::types::provided_headers(merged))
}

/// Builds a package as one executable, its dependencies' libraries built
/// in-graph and linked after its own objects.
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
            dependencies: inputs.get(3).cloned().unwrap_or(Value::Unit),
            objects: Vec::new(),
            phase: BuildPhase::Libraries,
        })
    }
}

/// Where one dependent build is: requesting the dependencies' libraries,
/// compiling its own sources over the merged header set, linking, or holding
/// the executable. A build with no dependencies passes through an empty
/// library batch, so one procedure serves every depth.
enum BuildPhase {
    Libraries,
    Compiling,
    Linking,
    Done,
}

struct PackageBuildFrame {
    toolchain_value: Value,
    tree: Value,
    build: Value,
    dependencies: Value,
    objects: Vec<ContentId>,
    phase: BuildPhase,
}

impl PureRuleFrame for PackageBuildFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            BuildPhase::Libraries => {
                let dependencies = dependencies_of(&self.dependencies)?;
                let libraries = dependencies
                    .iter()
                    .map(|dependency| {
                        library_request(
                            self.toolchain_value.clone(),
                            &dependency.tree,
                            &dependency.build,
                        )
                    })
                    .collect::<Vec<_>>();
                self.phase = BuildPhase::Compiling;
                Ok(PureStep::NeedAll(libraries.into_boxed_slice()))
            }
            BuildPhase::Compiling => {
                let libraries = match input {
                    Some(Resumption::Many(values)) => values
                        .iter()
                        .map(Library::from_value)
                        .collect::<PithResult<Vec<_>>>()?,
                    _ => return Err(diag("the library builds did not return one per dependency")),
                };
                let tree = tree_from_value(&self.tree)?;
                let build = PackageBuild::from_value(&self.build)?;
                let sources = resolve_sources(&tree, &build)?;
                let provided = merge_provided(resolve_includes(&tree, &build)?, &libraries)?;
                let compiles = sources
                    .iter()
                    .map(|source| {
                        xylem::types::compile_request(
                            self.toolchain_value.clone(),
                            *source,
                            provided.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.objects = libraries
                    .iter()
                    .flat_map(|library| library.objects.iter().copied())
                    .collect();
                self.phase = BuildPhase::Linking;
                Ok(PureStep::NeedAll(compiles.into_boxed_slice()))
            }
            BuildPhase::Linking => {
                let own = match input {
                    Some(Resumption::Many(values)) if !values.is_empty() => values
                        .iter()
                        .map(|object| blob_of(object, xylem::types::OBJECT))
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(diag("the compiles did not return one object per source")),
                };
                // The dependent's objects link before its dependencies': a
                // symbol the package defines is resolved in its own objects,
                // and the dependency's follow for what they provide.
                let mut objects = own;
                objects.append(&mut self.objects);
                self.phase = BuildPhase::Done;
                let link = xylem::types::link_request(self.toolchain_value.clone(), objects);
                Ok(PureStep::Need(link))
            }
            BuildPhase::Done => match input.and_then(Resumption::one) {
                Some(executable) => Ok(PureStep::Complete(executable)),
                None => Err(diag("the link completed without an executable")),
            },
        }
    }
}

/// Builds a package as a library: its objects and the headers it offers.
pub struct PackageLibraryRule;

impl PackageLibraryRule {
    /// Creates the package library rule declaration.
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        let identity = RuleIdentity::of_module_declaration("phloem", "package-library");
        Rule::<Pure>::new(
            RuleRevision::of_manifest(identity, PACKAGE_LIBRARY_MANIFEST),
            "package-library",
            package_library_interface(),
            Span::none(),
        )
    }
}

impl PureRule for PackageLibraryRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(PackageLibraryFrame {
            toolchain_value: inputs.first().cloned().unwrap_or(Value::Unit),
            tree: inputs.get(1).cloned().unwrap_or(Value::Unit),
            build: inputs.get(2).cloned().unwrap_or(Value::Unit),
            phase: LibraryPhase::Compiling,
        })
    }
}

enum LibraryPhase {
    Compiling,
    Done,
}

struct PackageLibraryFrame {
    toolchain_value: Value,
    tree: Value,
    build: Value,
    phase: LibraryPhase,
}

impl PureRuleFrame for PackageLibraryFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            LibraryPhase::Compiling => {
                let tree = tree_from_value(&self.tree)?;
                let build = PackageBuild::from_value(&self.build)?;
                let sources = resolve_sources(&tree, &build)?;
                let provided = merge_provided(resolve_includes(&tree, &build)?, &[])?;
                let compiles = sources
                    .iter()
                    .map(|source| {
                        xylem::types::compile_request(
                            self.toolchain_value.clone(),
                            *source,
                            provided.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                self.phase = LibraryPhase::Done;
                Ok(PureStep::NeedAll(compiles.into_boxed_slice()))
            }
            LibraryPhase::Done => {
                let objects = match input {
                    Some(Resumption::Many(values)) if !values.is_empty() => values
                        .iter()
                        .map(|object| blob_of(object, xylem::types::OBJECT))
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => return Err(diag("the compiles did not return one object per source")),
                };
                let tree = tree_from_value(&self.tree)?;
                let build = PackageBuild::from_value(&self.build)?;
                let library = Library {
                    objects: objects.into(),
                    headers: resolve_includes(&tree, &build)?,
                };
                Ok(PureStep::Complete(library.to_value()))
            }
        }
    }
}

/// Decodes the dependency-list input, refusing a value that is not the
/// declared list shape.
fn dependencies_of(value: &Value) -> PithResult<Vec<Dependency>> {
    let Value::List(entries) = value else {
        return Err(diag(format!(
            "expected a dependency list, found {}",
            value.describe()
        )));
    };
    entries.iter().map(Dependency::from_value).collect()
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
