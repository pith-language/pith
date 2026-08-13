//! Nominal type names and value constructors for the xylem build graph.
//!
//! Decision 0026 makes nominal identity a declaration attribute. The names
//! below are the declarations xylem owns: a toolchain, and the three content
//! roles a C build moves through. They are nominal over their content identity
//! so two rules producing content never collapse to the same `() -> Blob`
//! interface and collide as `E-1102` ambiguity (the blocker Phase 0 lifted).

use pith_core::{Interface, Pure, Request, Type, Value};
use pith_diag::Span;
use pith_ids::ContentId;

/// A discovered C toolchain: the driver path is its identity for dispatch. Two
/// compiles over different drivers are different requests because this value is
/// a request input; the closure the executor confines is declared separately in
/// the action spec.
pub const TOOLCHAIN: &str = "xylem.Toolchain";

/// A C source file identified by its content.
pub const C_SOURCE: &str = "xylem.CSource";

/// A compiled object file identified by its content.
pub const OBJECT: &str = "xylem.Object";

/// A linked executable identified by its content.
pub const EXECUTABLE: &str = "xylem.Executable";

/// A toolchain value carrying `driver` as its identity. The full closure lives
/// on the discovered [`crate::Toolchain`] struct the action rule holds; this
/// value is what the rule graph sees.
#[must_use]
pub fn toolchain(driver: &str) -> Value {
    Value::Nominal {
        name: TOOLCHAIN.into(),
        representation: Box::new(Value::Text(driver.into())),
    }
}

#[must_use]
pub fn c_source(id: ContentId) -> Value {
    Value::Nominal {
        name: C_SOURCE.into(),
        representation: Box::new(Value::Blob(id)),
    }
}

#[must_use]
pub fn object(id: ContentId) -> Value {
    Value::Nominal {
        name: OBJECT.into(),
        representation: Box::new(Value::Blob(id)),
    }
}

#[must_use]
pub fn executable(id: ContentId) -> Value {
    Value::Nominal {
        name: EXECUTABLE.into(),
        representation: Box::new(Value::Blob(id)),
    }
}

/// `(Toolchain, CSource) -> Object`.
#[must_use]
pub fn compile_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: TOOLCHAIN.into(),
            },
            Type::Nominal {
                name: C_SOURCE.into(),
            },
        ]),
        output: Type::Nominal {
            name: OBJECT.into(),
        },
    }
}

/// `(Toolchain, Object, Object) -> Executable`. Fixed arity two for now;
/// linking more than two objects needs a list or tree-valued content variant
/// (decision 0026) and is its own follow-up.
#[must_use]
pub fn link_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: TOOLCHAIN.into(),
            },
            Type::Nominal {
                name: OBJECT.into(),
            },
            Type::Nominal {
                name: OBJECT.into(),
            },
        ]),
        output: Type::Nominal {
            name: EXECUTABLE.into(),
        },
    }
}

/// A pure request to compile `source` under `toolchain_value`.
#[must_use]
pub fn compile_request(toolchain_value: Value, source: ContentId) -> Request<Pure> {
    Request::<Pure>::new(
        "compile-entry",
        compile_interface(),
        [toolchain_value, c_source(source)],
        Span::none(),
    )
}

/// A pure request to link `first` and `second` under `toolchain_value`.
#[must_use]
pub fn link_request(toolchain_value: Value, first: ContentId, second: ContentId) -> Request<Pure> {
    Request::<Pure>::new(
        "link-entry",
        link_interface(),
        [toolchain_value, object(first), object(second)],
        Span::none(),
    )
}
