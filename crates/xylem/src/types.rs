//! The declarations xylem owns, and the value constructors over them.
//!
//! Decision 0047 makes a declaration an entry in a per-module table: its
//! identity is the coordinate `xylem.<name>`, and its revision is a digest over
//! its representation. The table below is the single place each of xylem's six
//! nominal types is named; the interfaces and the value constructors derive
//! their spellings and their types from it, where they previously restated a
//! string constant and built `Type::Nominal { name }` inline at eighteen sites.
//!
//! They are nominal over their content identity so two rules producing content
//! never collapse to the same `() -> Blob` interface and collide as `E-1102`
//! ambiguity (the blocker Phase 0 lifted).
//!
//! The discovered header set is the one input here that is not nominal: it is
//! a `List<Text>` of include paths as the depfile spelled them, the landed
//! slice of 0026's `List<T>` constructor. Its content identities are resolved
//! by the compile action against the header universe it was registered with.

use std::sync::OnceLock;

use pith_core::{DeclarationTable, Interface, Pure, RecordField, Request, Type, Value};
use pith_diag::Span;
use pith_ids::ContentId;

/// The module identity every xylem declaration is registered under, matching
/// the one its rule identities already carry (decision 0023).
pub const MODULE: &str = "xylem";

/// One declared type: the use-site type and the coordinate spelling a value of
/// it carries.
struct Declared {
    declared_type: Type,
    spelling: Box<str>,
}

/// Xylem's declaration table and the six types declared in it.
struct Declarations {
    table: DeclarationTable,
    toolchain: Declared,
    c_source: Declared,
    object: Declared,
    executable: Declared,
    depfile: Declared,
    test_report: Declared,
}

fn declarations() -> &'static Declarations {
    static DECLARATIONS: OnceLock<Declarations> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let mut table = DeclarationTable::new(MODULE);
        let mut declare = |name: &str, representation: Type| {
            let declared_type = match table.nominal(name, representation) {
                Ok(declared) => declared,
                Err(error) => unreachable!("xylem declares each name once: {error}"),
            };
            Declared {
                spelling: format!("{MODULE}.{name}").into(),
                declared_type,
            }
        };
        // A toolchain's driver path is its identity for dispatch. Two compiles
        // over different drivers are different requests because this value is a
        // request input; the closure the executor confines is declared
        // separately in the action spec.
        let toolchain = declare("Toolchain", Type::Text);
        let c_source = declare("CSource", Type::Blob);
        let object = declare("Object", Type::Blob);
        let executable = declare("Executable", Type::Blob);
        // The make-syntax depfile a discovery pass captured. The entry rule
        // parses it; the paths it names are the source's header dependencies.
        let depfile = declare("Depfile", Type::Blob);
        // A verdict is nominal over `Bool`, which is the whole of what a build
        // asks a test. A report carrying the exit status, the captured output,
        // and a per-assertion breakdown wants a record, and the rule that
        // produces this reads the status and decides, so the distinction
        // between an exit code and a signal is made where the information is.
        let test_report = declare("TestReport", Type::Bool);
        Declarations {
            table,
            toolchain,
            c_source,
            object,
            executable,
            depfile,
            test_report,
        }
    })
}

/// Xylem's declaration table, for a rule revision derived from the declarations
/// its interface names (decision 0047).
#[must_use]
pub fn table() -> &'static DeclarationTable {
    &declarations().table
}

macro_rules! declared_accessors {
    ($($field:ident => $type_fn:ident, $name_fn:ident, $doc:literal;)*) => {
        $(
            #[doc = concat!("The declared type of ", $doc, ".")]
            #[must_use]
            pub fn $type_fn() -> Type {
                declarations().$field.declared_type.clone()
            }

            #[doc = concat!("The coordinate spelling a value of ", $doc, " carries.")]
            #[must_use]
            pub fn $name_fn() -> &'static str {
                &declarations().$field.spelling
            }
        )*
    };
}

declared_accessors! {
    toolchain => toolchain_type, toolchain_name, "a discovered C toolchain";
    c_source => c_source_type, c_source_name, "a C source file";
    object => object_type, object_name, "a compiled object file";
    executable => executable_type, executable_name, "a linked executable";
    depfile => depfile_type, depfile_name, "a captured make-syntax depfile";
    test_report => test_report_type, test_report_name, "a test verdict";
}

/// A toolchain value carrying `driver` as its identity. The full closure lives
/// on the discovered [`crate::Toolchain`] struct the action rule holds; this
/// value is what the rule graph sees.
#[must_use]
pub fn toolchain(driver: &str) -> Value {
    Value::Nominal {
        name: toolchain_name().into(),
        representation: Box::new(Value::Text(driver.into())),
    }
}

#[must_use]
pub fn c_source(id: ContentId) -> Value {
    Value::Nominal {
        name: c_source_name().into(),
        representation: Box::new(Value::Blob(id)),
    }
}

#[must_use]
pub fn object(id: ContentId) -> Value {
    Value::Nominal {
        name: object_name().into(),
        representation: Box::new(Value::Blob(id)),
    }
}

#[must_use]
pub fn executable(id: ContentId) -> Value {
    Value::Nominal {
        name: executable_name().into(),
        representation: Box::new(Value::Blob(id)),
    }
}

/// A test verdict value. `true` is the program reporting success.
#[must_use]
pub fn test_report(passed: bool) -> Value {
    Value::Nominal {
        name: test_report_name().into(),
        representation: Box::new(Value::Bool(passed)),
    }
}

#[must_use]
pub fn depfile(id: ContentId) -> Value {
    Value::Nominal {
        name: depfile_name().into(),
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

/// The staged-path field of a provided header record.
pub const HEADER_PATH: &str = "path";

/// The content field of a provided header record.
pub const HEADER_CONTENT: &str = "content";

/// The type of a provided header set: `(path, content)` pairs a request
/// declares beside a source, so a compile whose headers come from another
/// build's output sees them as request data rather than engine registration.
/// The path is both the include spelling and the staged path.
#[must_use]
pub fn provided_headers_type() -> Type {
    let header = Type::record([
        RecordField {
            name: HEADER_PATH.into(),
            payload: Type::Text,
        },
        RecordField {
            name: HEADER_CONTENT.into(),
            payload: Type::Blob,
        },
    ])
    .unwrap_or_else(|error| unreachable!("{error}"));
    Type::List(Box::new(header))
}

/// A provided header set value over `(path, content)` pairs.
#[must_use]
pub fn provided_headers<I, P>(pairs: I) -> Value
where
    I: IntoIterator<Item = (P, ContentId)>,
    P: Into<Box<str>>,
{
    Value::List(
        pairs
            .into_iter()
            .map(|(path, content)| {
                Value::record([
                    RecordField {
                        name: HEADER_PATH.into(),
                        payload: Value::Text(path.into()),
                    },
                    RecordField {
                        name: HEADER_CONTENT.into(),
                        payload: Value::Blob(content),
                    },
                ])
                .unwrap_or_else(|error| unreachable!("{error}"))
            })
            .collect(),
    )
}

/// `(Toolchain, CSource, Headers) -> Depfile`: the interface of the discovery
/// pass. The preprocessor runs over the source with the registered header
/// universe plus the request's provided headers staged, and captures the
/// depfile naming what the source actually includes. Provided headers are how
/// a compile whose headers come from another build's output — a package
/// dependency's includes — sees them as request data rather than engine
/// registration.
#[must_use]
pub fn discovery_interface() -> Interface {
    Interface {
        inputs: Box::new([toolchain_type(), c_source_type(), provided_headers_type()]),
        output: depfile_type(),
    }
}

/// `(Toolchain, CSource, List<Text>, Headers) -> Object`: the compile action's
/// interface. The third input is the discovered header set; the fourth is the
/// provided headers; the action resolves each discovered path against the
/// registered universe plus the provided set and declares the resolved files
/// as its inputs.
#[must_use]
pub fn compile_action_interface() -> Interface {
    Interface {
        inputs: Box::new([
            toolchain_type(),
            c_source_type(),
            headers_type(),
            provided_headers_type(),
        ]),
        output: object_type(),
    }
}

/// `(Toolchain, CSource, Headers) -> Object`: the compile entry a build
/// requests. The entry runs discovery over the registered universe plus the
/// provided headers, parses the depfile, and requests the compile with the
/// discovered set, so a caller names a source and — where its headers come
/// from elsewhere — those headers, and nothing about the registered universe.
#[must_use]
pub fn compile_interface() -> Interface {
    Interface {
        inputs: Box::new([toolchain_type(), c_source_type(), provided_headers_type()]),
        output: object_type(),
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
        inputs: Box::new([toolchain_type(), Type::List(Box::new(object_type()))]),
        output: executable_type(),
    }
}

/// `(Toolchain, Executable) -> TestReport`: run a built executable and read its
/// verdict. The toolchain is an input because the program needs its loader and
/// libc to run at all, and because a verdict from one toolchain's build does not
/// answer for another's.
#[must_use]
pub fn test_interface() -> Interface {
    Interface {
        inputs: Box::new([toolchain_type(), executable_type()]),
        output: test_report_type(),
    }
}

/// `(Toolchain, Executable) -> CSource`: run a generator the build produced and
/// take the source it wrote. Shares its input types with [`test_interface`] and
/// differs in its output, which is what keeps the two rules unambiguous under
/// 0015's selection: a nominal output type is a distinguishing part of an
/// interface, and this is the collision 0026's nominal identity exists to stop.
#[must_use]
pub fn generate_interface() -> Interface {
    Interface {
        inputs: Box::new([toolchain_type(), executable_type()]),
        output: c_source_type(),
    }
}

/// A pure request to run `generator` and take the C source it writes.
#[must_use]
pub fn generate_request(toolchain_value: Value, generator: ContentId) -> Request<Pure> {
    Request::<Pure>::new(
        "generate-entry",
        generate_interface(),
        [toolchain_value, executable(generator)],
        Span::none(),
    )
}

/// A pure request to run `executable` as a test under `toolchain_value`.
#[must_use]
pub fn test_request(toolchain_value: Value, executable: ContentId) -> Request<Pure> {
    Request::<Pure>::new(
        "test-entry",
        test_interface(),
        [toolchain_value, self::executable(executable)],
        Span::none(),
    )
}

/// A pure request to compile `source` under `toolchain_value`, discovering its
/// header dependencies first over the registered universe plus `provided`.
/// The provided set is the request's own headers — `provided_headers(&[])`
/// for a source whose includes all live in the registered universe.
#[must_use]
pub fn compile_request(
    toolchain_value: Value,
    source: ContentId,
    provided: Value,
) -> Request<Pure> {
    Request::<Pure>::new(
        "compile-entry",
        compile_interface(),
        [toolchain_value, c_source(source), provided],
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
