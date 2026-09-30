//! The measured identity of a project's file set: what a lock records for
//! a resolved input and a registry pins for a release.

use std::collections::BTreeMap;

use pith_ids::{ContentDigest, DigestDomain};

/// A project file set's identity: the project file and its includes under
/// one domain-separated digest.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Measured(ContentDigest);

impl Measured {
    #[must_use]
    pub const fn digest(self) -> ContentDigest {
        self.0
    }

    /// The revision spelling a host stores the tree under: the tree's own
    /// measured identity, which authenticates itself.
    #[must_use]
    pub fn revision(self) -> Box<str> {
        self.0.to_string().into()
    }
}

/// Measures a project from its project file and its includes, in the map's
/// canonical order, so equal file sets measure equal whatever order their
/// bytes were assembled in.
#[must_use]
pub fn measure(project: &str, includes: &BTreeMap<Box<str>, Box<str>>) -> Measured {
    const TREE_DOMAIN: DigestDomain = DigestDomain::new("module-tree", 1);
    let mut encoded = Vec::new();
    pith_core::manifest::encode_str(&mut encoded, project);
    for (path, text) in includes {
        pith_core::manifest::encode_str(&mut encoded, path);
        pith_core::manifest::encode_str(&mut encoded, text);
    }
    Measured(TREE_DOMAIN.digest(&encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(pairs: &[(&str, &str)]) -> BTreeMap<Box<str>, Box<str>> {
        pairs
            .iter()
            .map(|(path, text)| ((*path).into(), (*text).into()))
            .collect()
    }

    #[test]
    fn equal_file_sets_measure_equal_in_any_assembly_order() {
        let first = measure(
            "module a/b 1.0\n",
            &sources(&[("types.pi", "1"), ("rules.pi", "2")]),
        );
        let second = measure(
            "module a/b 1.0\n",
            &sources(&[("rules.pi", "2"), ("types.pi", "1")]),
        );
        assert_eq!(first, second);
    }

    #[test]
    fn every_file_moves_the_measurement() {
        let base = measure("module a/b 1.0\n", &sources(&[("types.pi", "1")]));
        assert_ne!(
            base,
            measure("module a/c 1.0\n", &sources(&[("types.pi", "1")]))
        );
        assert_ne!(
            base,
            measure("module a/b 1.1\n", &sources(&[("types.pi", "1")]))
        );
        assert_ne!(
            base,
            measure("module a/b 1.0\n", &sources(&[("types.pi", "2")]))
        );
        assert_ne!(
            base,
            measure(
                "module a/b 1.0\n",
                &sources(&[("types.pi", "1"), ("rules.pi", "2")])
            )
        );
    }

    #[test]
    fn a_path_boundary_cannot_be_forged_by_content() {
        let joined = measure("m\n", &sources(&[("a.pi", "12")]));
        let split = measure("m\n", &sources(&[("a.pi", "1"), ("a?.pi", "2")]));
        assert_ne!(joined, split);
    }
}
