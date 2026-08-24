use core::assert_matches;
use phloem::declarations::registered;

const DECLARED: &[&str] = &[
    "phloem.Origin",
    "phloem.Preference",
    "phloem.Range",
    "phloem.Resolution",
    "phloem.Source",
    "phloem.Substitution",
    "phloem.VersionScheme",
];

#[test]
fn the_table_is_complete_before_anything_is_touched() {
    let registered: Vec<(String, _)> = registered();
    let spellings: Vec<&str> = registered
        .iter()
        .map(|(spelling, _)| spelling.as_str())
        .collect();
    assert_eq!(spellings, DECLARED);
}

#[test]
fn the_substitution_coordinate_is_in_the_table() {
    assert!(
        registered()
            .iter()
            .any(|(spelling, _)| spelling == "phloem.Substitution"),
        "the substitution coordinate must be declared, not merely named"
    );
    assert_matches!(
        phloem::substitution::substitution_type(),
        pith_core::Type::Record(fields) if fields.len() == 10
    );
}
