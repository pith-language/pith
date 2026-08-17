//! Xylem's rule revisions derive from the declarations their interfaces name
//! (decision 0047), where they used to share one hand-bumped constant.
//!
//! The constant was `b"xylem-v3"`, and it had both failure modes at once: one
//! author edit moved every xylem rule, and a change to a nominal type's
//! representation moved none. What replaces it is `Rule::declared`, which
//! derives the revision from the canonical interface encoding — and since a
//! nominal type now carries its declaration, that encoding contains every
//! representation the interface reaches.

use pith_core::{Pure, Rule, RuleIdentity, Type};
use xylem::rules::{CompileRule, GenerateRule, LinkRule, TestRule};

/// The pure entry rules, whose interfaces differ from one another.
fn entry_rules() -> [(&'static str, Rule<Pure>); 4] {
    [
        ("compile-entry", CompileRule::rule()),
        ("link-entry", LinkRule::rule()),
        ("test-entry", TestRule::rule()),
        ("generate-entry", GenerateRule::rule()),
    ]
}

#[test]
fn rules_with_different_interfaces_no_longer_share_one_revision() {
    let rules = entry_rules();
    for (index, (label, rule)) in rules.iter().enumerate() {
        for (other_label, other) in rules.iter().skip(index.saturating_add(1)) {
            // Same interface would mean the two are ambiguous under 0015 and
            // could never both be selected, so distinct interfaces is the case
            // that matters and the case the constant collapsed.
            assert_ne!(
                rule.interface, other.interface,
                "`{label}` and `{other_label}` declare one interface"
            );
            assert_ne!(
                rule.revision, other.revision,
                "`{label}` and `{other_label}` still share a revision"
            );
        }
    }
}

#[test]
fn a_rules_identity_is_its_declaration_coordinate() {
    for (label, rule) in entry_rules() {
        assert_eq!(
            rule.identity,
            RuleIdentity::of_module_declaration(xylem::types::MODULE, label),
            "`{label}` is not identified by its coordinate"
        );
        assert_eq!(rule.revision.rule_identity(), rule.identity);
    }
}

#[test]
fn the_compile_interface_reaches_the_declarations_whose_change_should_move_it() {
    // The derivation is only as good as what the interface encoding contains.
    // A nominal participant carries its declared representation, so the encoded
    // interface — and therefore the revision — is a function of it.
    let compile = CompileRule::rule();
    let interface_reaches = |declared: &Type| {
        compile
            .interface
            .inputs
            .iter()
            .any(|input| reaches(input, declared))
            || reaches(&compile.interface.output, declared)
    };
    assert!(
        interface_reaches(&xylem::types::c_source_type()),
        "the compile entry's interface does not name CSource"
    );
    assert!(
        interface_reaches(&xylem::types::object_type()),
        "the compile entry's interface does not name Object"
    );
    assert!(
        !interface_reaches(&xylem::types::test_report_type()),
        "the compile entry's interface names TestReport, so an unrelated \
         declaration would move its revision"
    );
}

/// Whether `haystack` reaches `needle` structurally, which is what the encoding
/// walks.
fn reaches(haystack: &Type, needle: &Type) -> bool {
    if haystack == needle {
        return true;
    }
    match haystack {
        Type::List(element) => reaches(element, needle),
        Type::Record(fields) => fields.iter().any(|field| reaches(&field.payload, needle)),
        Type::Nominal(declared) => reaches(&declared.representation, needle),
        Type::Sum(declared) => declared.constructors.iter().any(|constructor| {
            constructor
                .payload
                .as_ref()
                .is_some_and(|payload| reaches(payload, needle))
        }),
        Type::Unit | Type::Bool | Type::Int | Type::Text | Type::Bytes | Type::Blob | Type::Cut => {
            false
        }
    }
}
