//! A value naming `xylem.Object` while holding a `Text` must not inhabit
//! xylem's link interface from any crate in the workspace. `Type::Nominal`
//! carries its declaration and `is_type` checks the representation against it,
//! at `is_type` and at the request-input gate that calls it.

use pith_core::{Type, Value};
use pith_ids::ContentId;

#[test]
fn a_value_naming_object_with_a_text_representation_does_not_inhabit_the_link_interface() {
    let fabricated = Value::Nominal {
        name: xylem::types::object_name().into(),
        representation: Box::new(Value::Text("not a content identity".into())),
    };
    let list = Value::List(vec![fabricated.clone()].into_boxed_slice());
    let declared = Type::List(Box::new(xylem::types::object_type()));

    assert!(
        !list.is_type(&declared),
        "a wrong-representation value must not inhabit {declared}"
    );

    // And through a real request: the link entry's input list is validated with
    // the same check, so a fabricated object never reaches the rule.
    let toolchain = xylem::types::toolchain("/bin/cc");
    let request = xylem::types::link_request(toolchain, [ContentId::of_blob(b"real")]);
    let [toolchain_input, _objects] = request.inputs.as_ref() else {
        unreachable!("a link request always has toolchain and object-list inputs");
    };
    let fabricated_inputs = [toolchain_input.clone(), list];
    let fabricated_request = pith_core::Request::<pith_core::Pure>::new(
        "link-entry",
        xylem::types::link_interface(),
        fabricated_inputs,
        pith_diag::Span::none(),
    );
    assert!(
        fabricated_request.validate_inputs().is_err(),
        "a link request over a wrong-representation object must be refused"
    );
}

#[test]
fn a_genuine_object_still_inhabits_the_link_interface() {
    // The other half, so the refusal above is not the check rejecting
    // everything: a value over the declared representation is admitted.
    let genuine = Value::Nominal {
        name: xylem::types::object_name().into(),
        representation: Box::new(Value::Blob(ContentId::of_blob(b"real"))),
    };
    let list = Value::List(vec![genuine].into_boxed_slice());
    assert!(list.is_type(&Type::List(Box::new(xylem::types::object_type()))));

    let toolchain = xylem::types::toolchain("/bin/cc");
    let request = xylem::types::link_request(toolchain, [ContentId::of_blob(b"real")]);
    assert!(request.validate_inputs().is_ok());
}
