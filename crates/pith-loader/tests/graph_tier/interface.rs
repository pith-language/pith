//! The surface artifact and the tier's canonical inputs: what an
//! `interface-of` produces, when it moves, and the one order the inputs
//! admit.

use core::assert_matches;

use pith_ids::ContentId;
use pith_loader::{
    FrontendImport, FrontendImportEnv, FrontendInputError, FrontendSource, InterfaceSurface,
};

use super::{ALPHA, Driver, frontend_source};

#[test]
fn the_surface_artifact_round_trips_and_moves_only_for_semantic_edits() {
    let surface = Driver::load_module("alpha", super::ALPHA).interface_surface();
    assert_eq!(
        InterfaceSurface::decode(&surface.encode()),
        Ok(surface.clone()),
        "the artifact did not round-trip"
    );

    let doc_edited = Driver::load_module("alpha", super::ALPHA_DOC_EDIT).interface_surface();
    assert_eq!(
        doc_edited.encode(),
        surface.encode(),
        "a documentation edit moved the interface surface"
    );
    assert_eq!(
        doc_edited.abi_digest(),
        surface.abi_digest(),
        "a documentation edit moved the ABI digest"
    );
    let label_edited = Driver::load_module("alpha", super::ALPHA_LABEL_EDIT).interface_surface();
    assert_eq!(label_edited.encode(), surface.encode());

    let representation_edited =
        Driver::load_module("alpha", super::ALPHA_REPRESENTATION_EDIT).interface_surface();
    assert_ne!(
        representation_edited.encode(),
        surface.encode(),
        "a representation edit left the interface surface unchanged"
    );
    assert_ne!(
        representation_edited.abi_digest(),
        surface.abi_digest(),
        "a representation edit left the ABI digest unchanged"
    );
}

#[test]
fn frontend_inputs_have_one_canonical_order_and_refuse_duplicate_keys() {
    let first = ContentId::of_blob(b"first");
    let second = ContentId::of_blob(b"second");
    let sorted = frontend_source("module", [("a.pi".into(), first), ("b.pi".into(), second)]);
    let reversed = frontend_source("module", [("b.pi".into(), second), ("a.pi".into(), first)]);
    assert_eq!(sorted, reversed);
    assert_matches!(
        FrontendSource::new(
            "module",
            [("same.pi".into(), first), ("same.pi".into(), second)]
        ),
        Err(FrontendInputError::DuplicateSourcePath { path }) if path.as_ref() == "same.pi"
    );

    let alpha = Driver::load_module("alpha", ALPHA).interface_surface();
    let alpha_import =
        FrontendImport::new("alpha", "alpha", alpha.abi_digest(), alpha.content_id());
    let duplicate = FrontendImport::new("alpha", "other", alpha.abi_digest(), alpha.content_id());
    assert_matches!(
        FrontendImportEnv::new([alpha_import, duplicate]),
        Err(FrontendInputError::DuplicateImportBinding { binding })
            if binding.as_ref() == "alpha"
    );
}
