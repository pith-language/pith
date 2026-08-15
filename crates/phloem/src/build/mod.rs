//! Package build declarations and execution rules.
//!
//! A build selects source paths from a measured source tree and names the
//! include paths it offers. The pure build rules compile those sources through
//! xylem and link the resulting objects — as one executable for the package
//! itself, or as a library (objects plus the offered headers) for whatever
//! depends on it. Archive unpacking imports source files at the caller's
//! effect boundary.

mod model;
mod rule;
mod unpack;

pub use self::model::{
    Dependency, Library, PackageBuild, SourceFile, SourceTree, build_type, dependency_list_type,
    dependency_list_value, dependency_type, library_type, tree_type,
};
pub use self::rule::{
    PackageBuildRule, PackageLibraryRule, build_request, library_request, package_build_interface,
    package_library_interface,
};
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
            includes: Box::new([]),
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
        let mut declared = build(&["zlib-1.3/zlib.c", "zlib-1.3/adler32.c"]);
        declared.includes = Box::new(["zlib-1.3/zlib.h".into()]);
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
    fn a_library_round_trips_through_its_value() {
        let library = Library {
            objects: Box::new([pith_ids::ContentId::of_blob(b"object")]),
            headers: Box::new([(
                "util-1.0/util.h".into(),
                pith_ids::ContentId::of_blob(b"header"),
            )]),
        };
        let value = library.to_value();
        assert!(value.is_type(&library_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(Library::from_value(&decoded).unwrap(), library);
    }

    #[test]
    fn a_dependency_round_trips_through_its_value() {
        let dependency = Dependency {
            tree: tree_of(&[("util-1.0/util.c", b"u")]),
            build: PackageBuild {
                sources: Box::new(["util-1.0/util.c".into()]),
                includes: Box::new(["util-1.0/util.h".into()]),
            },
        };
        let value = dependency.to_value();
        assert!(value.is_type(&dependency_type()));
        let decoded = Value::decode_canonical(&value.encode_canonical()).unwrap();
        assert_eq!(Dependency::from_value(&decoded).unwrap(), dependency);
    }

    #[test]
    fn the_build_request_names_the_interface_and_carries_all_four_inputs() {
        let tree = tree_of(&[("zlib-1.3/zlib.c", b"z")]);
        let request = build_request(
            xylem::types::toolchain("/nix/store/cc"),
            &tree,
            &build(&["zlib-1.3/zlib.c"]),
            &[],
        );
        assert_eq!(request.interface, package_build_interface());
        assert_eq!(request.inputs.len(), 4);

        let library = library_request(
            xylem::types::toolchain("/nix/store/cc"),
            &tree,
            &build(&["zlib-1.3/zlib.c"]),
        );
        assert_eq!(library.interface, package_library_interface());
        assert_eq!(library.inputs.len(), 3);
    }
}
