//! The representation hole 0047 exists to close, demonstrated before it is
//! closed.
//!
//! `Type::Nominal { name }` matches any value carrying the same string, so a
//! value that names `xylem.Object` while holding a `Text` representation
//! inhabits the link interface from any crate in the workspace. This test
//! asserts that it does; the declaration-table round inverts the assertion
//! when `is_type` verifies the representation against the declaration the
//! type carries.

use pith_core::{Type, Value};
use pith_ids::ContentId;

#[test]
fn a_value_naming_object_with_a_text_representation_inhabits_the_link_interface() {
    let fabricated = Value::Nominal {
        name: xylem::types::OBJECT.into(),
        representation: Box::new(Value::Text("not a content identity".into())),
    };
    let list = Value::List(vec![fabricated.clone()].into_boxed_slice());
    let declared = Type::List(Box::new(Type::Nominal {
        name: xylem::types::OBJECT.into(),
    }));

    // The hole: the name matches and nothing checks what the representation
    // holds, so a text masquerading as content identity passes the check every
    // request-input and result gate runs.
    assert!(
        list.is_type(&declared),
        "a wrong-representation value should not inhabit {declared}, but the bare-name match accepts it"
    );

    // And through a real request: the link entry's input list is validated
    // with the same check, so the fabricated object reaches the rule.
    let toolchain = xylem::types::toolchain("/bin/cc");
    let request = xylem::types::link_request(toolchain, [ContentId::of_blob(b"real")]);
    let [toolchain, _objects] = request.inputs.as_ref() else {
        unreachable!("a link request always has toolchain and object-list inputs");
    };
    let fabricated_inputs = [toolchain.clone(), list];
    let fabricated_request = pith_core::Request::<pith_core::Pure>::new(
        "link-entry",
        xylem::types::link_interface(),
        fabricated_inputs,
        pith_diag::Span::none(),
    );
    assert!(
        fabricated_request.validate_inputs().is_ok(),
        "a link request over a wrong-representation object validates today"
    );
    let _ = fabricated;
}
