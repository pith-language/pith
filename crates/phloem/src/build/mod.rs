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

mod model;
mod rule;
mod unpack;

pub use self::model::{PackageBuild, SourceFile, SourceTree, build_type, tree_type};
pub use self::rule::{PackageBuildRule, build_request, package_build_interface};
pub use self::unpack::unpack;

#[cfg(test)]
use self::model::tree_from_value;
#[cfg(test)]
use self::rule::resolve_sources;

#[cfg(test)]
mod tests {
    use pith_core::Value;
    use pith_engine::Engine;
    use pith_engine::state::MemoryEngineStateStore;
    use pith_store::MemoryContentStore;

    use super::*;

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
