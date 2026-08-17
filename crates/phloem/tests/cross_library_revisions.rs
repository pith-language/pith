//! Phloem's rule revisions reach xylem's declarations (decisions 0047, 0049).
//!
//! This is the property whose absence made the stale-hydration case in 0049
//! reachable across a library boundary. Before 0047, phloem derived its
//! revisions from its own hand-written manifests, so a change to a xylem
//! declaration moved xylem's revisions and left phloem's alone — and a phloem
//! package-build attempt kept hydrating xylem results derived from the
//! superseded bodies.
//!
//! It is asserted structurally rather than by mutating xylem's table, which is
//! registered once per process. The pith-core tests establish that a changed
//! representation moves the revision of a rule whose interface names it; what is
//! left to show is that phloem's interfaces do name xylem's declarations, so the
//! two compose.

use pith_core::Type;

/// Whether `haystack` reaches `needle` structurally, which is what the canonical
/// interface encoding the revision digests walks.
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
    // Two modules declaring one short label are two rules, so a coordinate is
    // the pair rather than the name (0047). Nothing in the tree relies on this
    // yet; it is the property that makes the module identity load-bearing.
    assert_ne!(xylem::types::MODULE, phloem::declarations::MODULE);
    assert_ne!(
        pith_core::RuleIdentity::of_module_declaration(xylem::types::MODULE, "compile"),
        pith_core::RuleIdentity::of_module_declaration(phloem::declarations::MODULE, "compile"),
    );
}
