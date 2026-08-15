//! A locked source becomes a built artifact (decision 0045).
//!
//! A package's build is data: the description declares a procedure from a
//! closed set — which sources of the unpacked tree compile, and that the
//! objects link into one executable — and phloem owns the procedure the
//! way nix's stdenv and Guix's build systems own theirs. The packager
//! writes the declaration and inherits the procedure; the alternative,
//! a script the package ships, is Debian's and Homebrew's position, and a
//! fetched source cannot carry host code this engine could run. The door
//! that position lives behind is 0038's represented rule bodies.
//!
//! The procedure runs as one pure rule in the graph, the peer-consumer
//! shape 0009 and 0039 fix: the rule requests xylem's declared entry
//! interfaces and plans no action itself, so every action in a package
//! build is planned and confined by xylem's rules, and the whole build is
//! one request that reuses and hydrates on the engine's machinery (0031,
//! 0033) with phloem adding nothing — the measurement 0039 was owed.
//!
//! The tree the build runs over comes from the fetch: the caller unpacks
//! the archive (parse in [`crate::archive`], import here) and the tree
//! value — a canonically sorted list of paths and measured file
//! identities — is the request input the computation key covers, so a
//! republished archive is a different tree, a different key, a rebuild.

use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{Engine, PureRule, PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::codec::{blob_field, field_of, record_type, record_value, text_field, text_list};
use crate::diag;

/// The revision the package-build rule derives its identity from. Bumping
/// this invalidates every cached package build, which is what a semantic
/// change to the procedure should do.
const PACKAGE_BUILD_MANIFEST: &[u8] = b"phloem-package-build-v1";

const PATH: &str = "path";
const CONTENT: &str = "content";
const SOURCES: &str = "sources";

/// The tree one archive unpacked into: the measured files, sorted by path
/// so the tree a build runs over is a function of the archive and not of
/// the order the reader happened to hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceTree {
    pub files: Box<[SourceFile]>,
}

/// One file of an unpacked tree: its path within the tree and the content
/// identity measured from its bytes when it was imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: Box<str>,
    pub content: ContentId,
}

impl SourceTree {
    /// The content identity at `path`, or `None` when the tree does not
    /// hold it. Binary search over the sorted files.
    #[must_use]
    pub fn content_at(&self, path: &str) -> Option<ContentId> {
        self.files
            .binary_search_by(|file| file.path.as_ref().cmp(path))
            .ok()
            .and_then(|index| self.files.get(index).map(|file| file.content))
    }

    /// The tree as a value: `List<{path: Text, content: Blob}>`, sorted by
    /// path.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::List(
            self.files
                .iter()
                .map(|file| {
                    record_value([
                        (PATH, Value::Text(file.path.clone())),
                        (CONTENT, Value::Blob(file.content)),
                    ])
                })
                .collect(),
        )
    }
}

/// The type of a source tree: a list of path-and-content records.
#[must_use]
pub fn tree_type() -> Type {
    Type::List(Box::new(record_type([
        (PATH, Type::Text),
        (CONTENT, Type::Blob),
    ])))
}

/// What a package declares as its build: the sources that compile, in link
/// order, over the tree its archive unpacked into. The procedure around
/// them — compile each through xylem's entry, link the objects through
/// xylem's link entry — is the library's, on the stdenv ground that a
/// fixed, inspectable procedure beats a script per package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageBuild {
    pub sources: Box<[Box<str>]>,
}

impl PackageBuild {
    /// The build as a value of the declared record type.
    #[must_use]
    pub fn to_value(&self) -> Value {
        record_value([(
            SOURCES,
            Value::List(
                self.sources
                    .iter()
                    .map(|source| Value::Text(source.clone()))
                    .collect(),
            ),
        )])
    }

    /// Read a build from a value.
    ///
    /// # Errors
    /// A [`pith_diag::DiagnosticSink`] naming the declared type and the
    /// value found when the value is not a package build.
    pub fn from_value(value: &Value) -> PithResult<Self> {
        let build = record_type([(SOURCES, Type::List(Box::new(Type::Text)))]);
        if !value.is_type(&build) {
            return Err(diag(format!(
                "expected a value of the package build type, found {}",
                value.describe()
            )));
        }
        let sources = match value {
            Value::Record(fields) => match field_of(fields, SOURCES) {
                Some(payload) => text_list(payload, SOURCES)?,
                None => return Err(diag(format!("the record carried no {SOURCES} list"))),
            },
            _ => unreachable!("is_type accepted a non-record"),
        };
        Ok(Self {
            sources: sources.into(),
        })
    }
}

/// The type of a declared package build.
#[must_use]
pub fn build_type() -> Type {
    record_type([(SOURCES, Type::List(Box::new(Type::Text)))])
}

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

/// Unpack a fetched archive into the engine's store: parse the tar, import
/// each file, and return the measured tree.
///
/// A caller-side effect in the position 0044 put the fetch: the parse is
/// the pure half ([`crate::archive`]), the import is this half, and the
/// tree value it returns is the declared input a build request carries.
/// The engine has no path by which a pure rule publishes new store content
/// — imports are what action capture does — so the import belongs to the
/// caller, and the file identities it returns are measured from the bytes
/// rather than taken from any claim.
///
/// # Errors
/// The parse's diagnostics when the archive is not ustar-shaped, and the
/// store's when a file cannot be imported.
pub fn unpack(engine: &mut Engine, archive: &[u8]) -> PithResult<SourceTree> {
    let files = crate::archive::parse(archive)?;
    let mut imported: Vec<SourceFile> = Vec::with_capacity(files.len());
    for file in files.iter() {
        let content = engine
            .put_blob(&file.bytes)
            .map_err(|error| diag(format!("importing `{}` failed: {error}", file.path)))?;
        imported.push(SourceFile {
            path: file.path.clone(),
            content,
        });
    }
    imported.sort_by(|left, right| left.path.as_ref().cmp(right.path.as_ref()));
    Ok(SourceTree {
        files: imported.into(),
    })
}

/// Resolve the build's declared source paths against the tree, in link
/// order. A path the tree does not hold is a diagnostic naming it: the
/// build prescribes content by location inside its own source, and a
/// prescription the source cannot fill is refused loudly rather than
/// narrowed.
fn resolve_sources(tree: &SourceTree, build: &PackageBuild) -> PithResult<Box<[ContentId]>> {
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

/// Read a tree from its value, in the sorted order the value spells.
fn tree_from_value(value: &Value) -> PithResult<SourceTree> {
    if !value.is_type(&tree_type()) {
        return Err(diag(format!(
            "expected a value of the source tree type, found {}",
            value.describe()
        )));
    }
    let Value::List(files) = value else {
        return Err(diag("the source tree was not a list of files"));
    };
    let mut imported = Vec::with_capacity(files.len());
    for file in files.iter() {
        let Value::Record(fields) = file else {
            return Err(diag(format!(
                "a tree entry was {}, not a path-and-content record",
                file.describe()
            )));
        };
        let path = text_field(fields, PATH)?;
        let content = blob_field(fields, CONTENT)?;
        imported.push(SourceFile { path, content });
    }
    Ok(SourceTree {
        files: imported.into(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use pith_engine::state::MemoryEngineStateStore;
    use pith_store::MemoryContentStore;

    fn tree_of(files: &[(&str, &[u8])]) -> SourceTree {
        let mut engine = Engine::with_state_store(
            MemoryContentStore::default(),
            MemoryEngineStateStore::default(),
        );
        let mut imported: Vec<SourceFile> = files
            .iter()
            .map(|(path, bytes)| SourceFile {
                path: (*path).into(),
                content: engine.put_blob(bytes).unwrap(),
            })
            .collect();
        imported.sort_by(|left, right| left.path.as_ref().cmp(right.path.as_ref()));
        SourceTree {
            files: imported.into(),
        }
    }

    fn build(paths: &[&str]) -> PackageBuild {
        PackageBuild {
            sources: paths.iter().map(|path| (*path).into()).collect(),
        }
    }

    #[test]
    fn a_tree_round_trips_through_its_value() {
        let tree = tree_of(&[("zlib-1.3/zlib.c", b"z"), ("zlib-1.3/adler32.c", b"a")]);
        let value = tree.to_value();
        assert!(value.is_type(&tree_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(tree_from_value(&decoded).unwrap(), tree);
    }

    #[test]
    fn a_build_round_trips_through_its_value() {
        let declared = build(&["zlib-1.3/zlib.c", "zlib-1.3/adler32.c"]);
        let value = declared.to_value();
        assert!(value.is_type(&build_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(PackageBuild::from_value(&decoded).unwrap(), declared);
    }

    #[test]
    fn the_sources_resolve_in_link_order_and_a_missing_path_is_named() {
        let tree = tree_of(&[("zlib-1.3/zlib.c", b"z"), ("zlib-1.3/adler32.c", b"a")]);
        let declared = build(&["zlib-1.3/adler32.c", "zlib-1.3/zlib.c"]);
        let resolved = resolve_sources(&tree, &declared).unwrap();
        assert_eq!(
            resolved,
            Box::from([
                tree.content_at("zlib-1.3/adler32.c").unwrap(),
                tree.content_at("zlib-1.3/zlib.c").unwrap()
            ]),
            "the declared order is the link order, and the resolution keeps it"
        );

        let missing = build(&["zlib-1.3/nope.c"]);
        let error = resolve_sources(&tree, &missing).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("zlib-1.3/nope.c")),
            "the diagnostic names the prescribed path: {error:?}"
        );

        let error = resolve_sources(&tree, &build(&[])).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.0.contains("no source")),
            "a build prescribing nothing is refused: {error:?}"
        );
    }

    #[test]
    fn the_build_request_names_the_interface_and_carries_all_three_inputs() {
        let tree = tree_of(&[("zlib-1.3/zlib.c", b"z")]);
        let request = build_request(
            xylem::types::toolchain("/nix/store/cc"),
            &tree,
            &build(&["zlib-1.3/zlib.c"]),
        );
        assert_eq!(request.interface, package_build_interface());
        assert_eq!(request.inputs.len(), 3);
    }
}
