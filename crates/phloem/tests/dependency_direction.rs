//! The dependency direction 0009 and scope's build-without-package base case
//! rest on: phloem produces build requests against interfaces xylem already
//! declares, and xylem knows nothing about packages or about phloem. The
//! manifests are where that direction lives, so the manifests are what this
//! asserts — a reversed edge would appear here first.

const PHLOEM_MANIFEST: &str = include_str!("../Cargo.toml");
const XYLEM_MANIFEST: &str = include_str!("../../xylem/Cargo.toml");

#[test]
fn phloem_consumes_xylem_and_xylem_does_not_depend_on_phloem() {
    assert!(
        PHLOEM_MANIFEST.contains("xylem.workspace = true"),
        "phloem's manifest should declare the xylem dependency"
    );
    assert!(
        !XYLEM_MANIFEST.contains("phloem"),
        "xylem's manifest should not name phloem anywhere: {XYLEM_MANIFEST}"
    );
}
