//! Nominal type names and value constructors for the xylem build graph.
//!
//! Decision 0026 makes nominal identity a declaration attribute. The names
//! below are the declarations xylem owns: a toolchain, and the content roles a
//! C build moves through. They are nominal over their content identity so two
//! rules producing content never collapse to the same `() -> Blob` interface
//! and collide as `E-1102` ambiguity (the blocker Phase 0 lifted).
//!
//! The discovered header set is the one input here that is not nominal: it is
//! a `List<Text>` of include paths as the depfile spelled them, the landed
//! slice of 0026's `List<T>` constructor. Its content identities are resolved
//! by the compile action against the header universe it was registered with.

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

/// The make-syntax depfile a discovery pass captured, identified by its
/// content. The entry rule parses it; the paths it names are the source's
/// header dependencies.
pub const DEPFILE: &str = "xylem.Depfile";

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

#[must_use]
pub fn depfile(id: ContentId) -> Value {
    Value::Nominal {
        name: DEPFILE.into(),
        representation: Box::new(Value::Blob(id)),
    }
}

/// The type of a discovered header set: include paths as the depfile spelled
/// them, canonically sorted and deduplicated by the parser that produced them.
#[must_use]
pub fn headers_type() -> Type {
    Type::List(Box::new(Type::Text))
}

/// A discovered header set value over `paths`.
#[must_use]
pub fn headers<I, S>(paths: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: Into<Box<str>>,
{
    Value::List(
        paths
            .into_iter()
            .map(|path| Value::Text(path.into()))
            .collect(),
    )
}

/// `(Toolchain, CSource) -> Depfile`: the interface of the discovery pass. The
/// preprocessor runs over the source with the whole header universe staged and
/// captures the depfile naming what the source actually includes.
#[must_use]
pub fn discovery_interface() -> Interface {
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
            name: DEPFILE.into(),
        },
    }
}

/// `(Toolchain, CSource, List<Text>) -> Object`: the compile action's
/// interface. The third input is the discovered header set; the action resolves
/// each path against the universe it was registered with and declares the
/// resolved files as its inputs.
#[must_use]
pub fn compile_action_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: TOOLCHAIN.into(),
            },
            Type::Nominal {
                name: C_SOURCE.into(),
            },
            headers_type(),
        ]),
        output: Type::Nominal {
            name: OBJECT.into(),
        },
    }
}

/// `(Toolchain, CSource) -> Object`: the compile entry a build requests. The
/// entry runs discovery, parses the depfile, and requests the compile with the
/// discovered set, so a caller names a source and nothing about its headers.
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

/// `(Toolchain, List<Object>) -> Executable`: the link interface over any
/// number of objects (decision 0035). The elements keep their nominal
/// identity, so this is `List<xylem.Object>` and not `List<Blob>` — the
/// distinction that keeps rule selection unambiguous now that the input is a
/// list.
#[must_use]
pub fn link_interface() -> Interface {
    Interface {
        inputs: Box::new([
            Type::Nominal {
                name: TOOLCHAIN.into(),
            },
            Type::List(Box::new(Type::Nominal {
                name: OBJECT.into(),
            })),
        ]),
        output: Type::Nominal {
            name: EXECUTABLE.into(),
        },
    }
}

/// A pure request to compile `source` under `toolchain_value`, discovering its
/// header dependencies first.
#[must_use]
pub fn compile_request(toolchain_value: Value, source: ContentId) -> Request<Pure> {
    Request::<Pure>::new(
        "compile-entry",
        compile_interface(),
        [toolchain_value, c_source(source)],
        Span::none(),
    )
}

/// A pure request to link `objects`, in the order given, under
/// `toolchain_value`.
#[must_use]
pub fn link_request<I>(toolchain_value: Value, objects: I) -> Request<Pure>
where
    I: IntoIterator<Item = ContentId>,
{
    Request::<Pure>::new(
        "link-entry",
        link_interface(),
        [
            toolchain_value,
            Value::List(objects.into_iter().map(object).collect()),
        ],
        Span::none(),
    )
}
