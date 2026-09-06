//! The measured identity of a module tree.

use std::collections::BTreeMap;

use pith_ids::{ContentDigest, DigestDomain};

/// A normalized module tree's identity: the manifest and the owned source
/// files under one domain-separated digest.
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

/// Measures a module tree from its manifest text and its module-relative
/// sources. The map's canonical order is the measurement's order, so equal
/// trees measure equal whatever order their bytes were assembled in, and
/// one path names one file because a map holds no duplicates.
#[must_use]
pub fn measure(manifest: &str, sources: &BTreeMap<Box<str>, Box<str>>) -> Measured {
    const TREE_DOMAIN: DigestDomain = DigestDomain::new("module-tree", 1);
    let mut encoded = Vec::new();
    pith_core::manifest::encode_str(&mut encoded, manifest);
    for (path, text) in sources {
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
    fn equal_trees_measure_equal_in_any_assembly_order() {
        let first = measure(
            "module a/b 1.0\n",
            &sources(&[("src/a.pi", "1"), ("src/b.pi", "2")]),
        );
        let second = measure(
            "module a/b 1.0\n",
            &sources(&[("src/b.pi", "2"), ("src/a.pi", "1")]),
        );
        assert_eq!(first, second);
    }

    #[test]
    fn every_input_moves_the_measurement() {
        let base = measure("module a/b 1.0\n", &sources(&[("src/a.pi", "1")]));
        assert_ne!(
            base,
            measure("module a/c 1.0\n", &sources(&[("src/a.pi", "1")]))
        );
        assert_ne!(
            base,
            measure("module a/b 1.1\n", &sources(&[("src/a.pi", "1")]))
        );
        assert_ne!(
            base,
            measure("module a/b 1.0\n", &sources(&[("src/a.pi", "2")]))
        );
        assert_ne!(
            base,
            measure(
                "module a/b 1.0\n",
                &sources(&[("src/a.pi", "1"), ("src/b.pi", "2")])
            )
        );
    }

    #[test]
    fn a_path_boundary_cannot_be_forged_by_content() {
        let joined = measure("m\n", &sources(&[("src/a.pi", "12")]));
        let split = measure("m\n", &sources(&[("src/a.pi", "1"), ("src/a?.pi", "2")]));
        assert_ne!(joined, split);
    }
}
