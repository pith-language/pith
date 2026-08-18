//! The declarations this domain owns, and the values over them.
//!
//! The table is built the same way xylem's is (decision 0047), through
//! `DeclarationTable` from `pith-core`: a coordinate `example.<name>` per
//! declaration, a revision derived from the representation, and no registration
//! against anything outside this crate. Nothing here is reachable from the
//! kernel, which is the point — a module identity is a string the registration
//! boundary accepts, so a domain the workspace does not know about declares its
//! types on the same terms the first-party ones do.

use std::sync::OnceLock;

use pith_core::{DeclarationTable, Interface, Pure, RecordField, Request, Type, Value};
use pith_diag::Span;
use pith_ids::ContentId;

/// The module identity this domain's declarations and rules are registered
/// under.
pub const MODULE: &str = "example";

/// The field of a binding naming the placeholder it fills.
pub const BINDING_NAME: &str = "name";

/// The field of a binding carrying the text substituted for the placeholder.
pub const BINDING_VALUE: &str = "value";

/// One declared type: the use-site type and the coordinate spelling a value of
/// it carries. Both are derived from the table entry, so a declaration is
/// named once.
pub struct Declared {
    declared_type: Type,
    spelling: Box<str>,
}

impl Declared {
    /// The type to write in an interface.
    #[must_use]
    pub fn declared_type(&self) -> Type {
        self.declared_type.clone()
    }

    /// The coordinate spelling a value of this type carries.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.spelling
    }

    /// A value of this type over `representation`.
    ///
    /// The representation is not checked here: a value is data, and a value
    /// whose representation does not match its declaration is refused at the
    /// request-input gate, which is where the declaration table put that check.
    #[must_use]
    pub fn value(&self, representation: Value) -> Value {
        Value::Nominal {
            name: self.spelling.clone(),
            representation: Box::new(representation),
        }
    }

    /// A value of this type over the content `id` names.
    #[must_use]
    pub fn content(&self, id: ContentId) -> Value {
        self.value(Value::Blob(id))
    }
}

struct Declarations {
    table: DeclarationTable,
    renderer: Declared,
    template: Declared,
    bindings: Declared,
    document: Declared,
}

fn declarations() -> &'static Declarations {
    static DECLARATIONS: OnceLock<Declarations> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let mut table = DeclarationTable::new(MODULE);
        let mut declare = |name: &str, representation: Type| {
            let declared_type = match table.nominal(name, representation) {
                Ok(declared) => declared,
                Err(error) => unreachable!("this domain declares each name once: {error}"),
            };
            Declared {
                spelling: format!("{MODULE}.{name}").into(),
                declared_type,
            }
        };
        // The renderer is a program the graph produced, so it enters the
        // contract as content (decision 0036) and is a request input: a
        // rebuilt renderer is a different request, and the document it wrote
        // is not served for the old one.
        let renderer = declare("Renderer", Type::Blob);
        let template = declare("Template", Type::Blob);
        let bindings = declare("Bindings", binding_list());
        let document = declare("Document", Type::Blob);
        Declarations {
            table,
            renderer,
            template,
            bindings,
            document,
        }
    })
}

/// The structural representation behind `example.Bindings`: `(name, value)`
/// pairs, which the constructor below keeps sorted by name.
fn binding_list() -> Type {
    let binding = Type::record([
        RecordField {
            name: BINDING_NAME.into(),
            payload: Type::Text,
        },
        RecordField {
            name: BINDING_VALUE.into(),
            payload: Type::Text,
        },
    ])
    .unwrap_or_else(|error| unreachable!("the binding record names two distinct fields: {error}"));
    Type::List(Box::new(binding))
}

/// This domain's declaration table, for a reader that wants the coordinates
/// and revisions behind the types below.
#[must_use]
pub fn table() -> &'static DeclarationTable {
    &declarations().table
}

/// The renderer program, as content the graph produced.
#[must_use]
pub fn renderer() -> &'static Declared {
    &declarations().renderer
}

/// The template text a document is rendered from.
#[must_use]
pub fn template() -> &'static Declared {
    &declarations().template
}

/// The substitutions a render applies.
#[must_use]
pub fn bindings() -> &'static Declared {
    &declarations().bindings
}

/// A rendered document.
#[must_use]
pub fn document() -> &'static Declared {
    &declarations().document
}

/// A bindings value over `pairs`, sorted by name.
///
/// Sorting is what makes two callers who list the same substitutions in
/// different orders one request: the computation key is over the request
/// inputs, so an unsorted list would compute one document under two keys. A name bound twice survives this constructor and is
/// refused by the rule, where the diagnostic can name it.
#[must_use]
pub fn bindings_value<I, N, V>(pairs: I) -> Value
where
    I: IntoIterator<Item = (N, V)>,
    N: Into<Box<str>>,
    V: Into<Box<str>>,
{
    let mut entries: Vec<(Box<str>, Box<str>)> = pairs
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let records = entries
        .into_iter()
        .map(|(name, value)| {
            Value::record([
                RecordField {
                    name: BINDING_NAME.into(),
                    payload: Value::Text(name),
                },
                RecordField {
                    name: BINDING_VALUE.into(),
                    payload: Value::Text(value),
                },
            ])
            .unwrap_or_else(|error| {
                unreachable!("a binding record has two distinct fields: {error}")
            })
        })
        .collect();
    bindings().value(Value::List(records))
}

/// `(Renderer, Template, Bindings) -> Document`: what this domain computes.
///
/// The pure entry and the action share it, the way xylem's test entry shares
/// its interface with the action it requests: the entry is what a caller
/// requests and what reuse and hydration reach (decision 0033), and the action
/// under it is the invocation.
#[must_use]
pub fn render_interface() -> Interface {
    Interface {
        inputs: Box::new([
            renderer().declared_type(),
            template().declared_type(),
            bindings().declared_type(),
        ]),
        output: document().declared_type(),
    }
}

/// A pure request to render `template` with `bindings` through `renderer`.
#[must_use]
pub fn render_request(
    renderer_program: ContentId,
    template_text: ContentId,
    bound: Value,
) -> Request<Pure> {
    Request::<Pure>::new(
        RENDER_ENTRY,
        render_interface(),
        [
            renderer().content(renderer_program),
            template().content(template_text),
            bound,
        ],
        Span::none(),
    )
}

/// The label of the pure entry rule, which is also the name half of its
/// coordinate.
pub const RENDER_ENTRY: &str = "render-entry";

/// The label of the action rule.
pub const RENDER: &str = "render";
