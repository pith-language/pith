//! The canonical-order discipline of the graph tier's two inputs: one source
//! set and one import set have exactly one request spelling, whatever order
//! the driver assembled them in, and a repeated key never becomes one.

use proptest::prelude::*;

use pith_ids::{ContentDigest, ContentId, ModuleAbiDigest};
use pith_loader::{FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource};

/// Permute by sorting on generated keys, so every ordering is reachable
/// without a shuffle strategy.
fn permuted<T>(items: Vec<T>, keys: Vec<u32>) -> Vec<T> {
    let mut decorated = items.into_iter().zip(keys).collect::<Vec<_>>();
    decorated.sort_by_key(|(_, key)| *key);
    decorated.into_iter().map(|(item, _)| item).collect()
}

fn source_files(pairs: &[(Vec<u8>, u32)]) -> Vec<(Box<str>, ContentId)> {
    pairs
        .iter()
        .enumerate()
        .map(|(index, (bytes, _))| {
            (
                format!("module/file-{index}.pi").into(),
                ContentId::of_blob(bytes),
            )
        })
        .collect()
}

fn import_entries(pairs: &[([u8; 32], u32)]) -> Vec<FrontendImport> {
    pairs
        .iter()
        .enumerate()
        .map(|(index, (bytes, _))| {
            let prefix = u8::try_from(index).unwrap_or(0);
            FrontendImport::new(
                format!("binding-{index}"),
                format!("module-{index}"),
                ModuleAbiDigest::from_digest(ContentId::of_blob(bytes).digest()),
                ContentId::of_blob(
                    &[prefix]
                        .into_iter()
                        .chain(bytes.iter().copied())
                        .collect::<Vec<u8>>(),
                ),
            )
        })
        .collect()
}

proptest! {
    #[test]
    fn a_source_set_has_one_canonical_spelling(
        pairs in prop::collection::vec((any::<Vec<u8>>(), any::<u32>()), 0..8),
    ) {
        let order = pairs.iter().map(|(_, key)| *key).collect();
        let canonical = source_files(&pairs);
        let source = canonical_source(canonical.clone());
        let shuffled = canonical_source(permuted(canonical, order));
        prop_assert_eq!(source, shuffled);
    }

    #[test]
    fn an_import_environment_has_one_canonical_spelling(
        pairs in prop::collection::vec((any::<[u8; 32]>(), any::<u32>()), 0..8),
    ) {
        let order = pairs.iter().map(|(_, key)| *key).collect();
        let canonical = import_entries(&pairs);
        let imports = canonical_imports(canonical.clone());
        let shuffled = canonical_imports(permuted(canonical, order));
        prop_assert_eq!(imports, shuffled);
    }
}

fn canonical_source(files: Vec<(Box<str>, ContentId)>) -> FrontendSource {
    match FrontendSource::new("module", files) {
        Ok(source) => source,
        Err(error) => unreachable!("distinct paths are never refused: {error}"),
    }
}

fn canonical_imports(entries: Vec<FrontendImport>) -> FrontendImportEnv {
    match FrontendImportEnv::new(entries) {
        Ok(imports) => imports,
        Err(error) => unreachable!("distinct bindings are never refused: {error}"),
    }
}

#[test]
fn a_repeated_source_path_is_refused() {
    let files = vec![
        ("module/a.pi".into(), ContentId::of_blob(b"first")),
        ("module/b.pi".into(), ContentId::of_blob(b"second")),
        ("module/a.pi".into(), ContentId::of_blob(b"third")),
    ];
    assert_eq!(
        FrontendSource::new("module", files).unwrap_err(),
        FrontendInputError::DuplicateSourcePath {
            path: "module/a.pi".into(),
        }
    );
}

#[test]
fn a_repeated_import_binding_is_refused() {
    let digest = ModuleAbiDigest::from_digest(ContentDigest::from_bytes([0u8; 32]));
    let entries = vec![
        FrontendImport::new("alpha", "alpha", digest, ContentId::of_blob(b"surface")),
        FrontendImport::new(
            "alpha",
            "alpha-elsewhere",
            digest,
            ContentId::of_blob(b"other"),
        ),
    ];
    assert_eq!(
        FrontendImportEnv::new(entries).unwrap_err(),
        FrontendInputError::DuplicateImportBinding {
            binding: "alpha".into(),
        }
    );
}
