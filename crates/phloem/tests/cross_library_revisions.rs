//! Phloem's rule revisions reach xylem's declarations: a change to a xylem
//! declaration moves both crates' revisions together, so a phloem
//! package-build attempt cannot keep hydrating results derived from
//! superseded xylem bodies. Checked structurally: xylem's table is
//! registered once per process, so what is left to show is that phloem's
//! interfaces name xylem's declarations.

use pith_core::Type;

/// Whether `haystack` reaches `needle` structurally, over the canonical
/// encoding the revision digest covers.
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

fn interface_reaches(interface: &pith_core::Interface, needle: &Type) -> bool {
    interface.inputs.iter().any(|input| reaches(input, needle))
        || reaches(&interface.output, needle)
}

#[test]
fn the_package_build_interface_names_xylem_declarations() {
    let interface = phloem::build::package_build_interface();
    for (what, declared) in [
        ("Toolchain", xylem::types::toolchain_type()),
        ("Executable", xylem::types::executable_type()),
    ] {
        assert!(
            interface_reaches(&interface, &declared),
            "phloem's package build does not name xylem's {what}, so a change to \
             it would not move this rule's revision"
        );
    }
}

#[test]
fn the_package_library_interface_names_xylem_declarations() {
    let interface = phloem::build::package_library_interface();
    for (what, declared) in [
        ("Toolchain", xylem::types::toolchain_type()),
        ("Object", xylem::types::object_type()),
    ] {
        assert!(
            interface_reaches(&interface, &declared),
            "phloem's package library does not name xylem's {what}"
        );
    }
}

#[test]
fn phloem_and_xylem_rules_carrying_one_label_are_distinct() {
    // Two modules declaring one short label are two rules: a rule identity
    // is the module-label pair, not the label alone.
    assert_ne!(xylem::types::MODULE, phloem::declarations::MODULE);
    assert_ne!(
        pith_core::RuleIdentity::of_module_declaration(xylem::types::MODULE, "compile"),
        pith_core::RuleIdentity::of_module_declaration(phloem::declarations::MODULE, "compile"),
    );
}
