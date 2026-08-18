//! The declarations this domain owns, and the values over them.
//!
//! The table is built the way xylem's is (decision
//! 0047): a coordinate `stele.<name>` per declaration, a revision derived
//! from the representation, and no registration against anything outside
//! this crate. Every constructor the representations use was already in the
//! calculus before this domain opened — records, declared sums, lists, and
//! the scalars — which is the convergence question M-5a exists to answer.

use std::sync::OnceLock;

use pith_core::{
    DeclarationTable, Interface, Pure, RecordField, Request, SumConstructor, Type, Value,
};
use pith_diag::Span;
use pith_ids::ContentId;

/// The module identity this domain's declarations and rules are registered
/// under. The stele is the central cylinder of a stem, the structure that
/// holds every tissue in one axis; the system library composes the machine's
/// parts into one tree the same way.
pub const MODULE: &str = "stele";

/// The field naming the contribution's owner, on every contribution record.
pub const OWNER: &str = "owner";

/// The field of a file entry carrying its path.
pub const PATH: &str = "path";

/// The field of a symlink body carrying its target.
pub const TARGET: &str = "target";

/// The field of a file body naming the content it holds.
pub const CONTENT: &str = "content";

/// The field of a file body naming whether the file is executable.
pub const EXECUTABLE: &str = "executable";

/// The field naming a user account, a unit, or a machine.
pub const NAME: &str = "name";

/// The field of a user account carrying its user id.
pub const UID: &str = "uid";

/// The field of a user account carrying its group id.
pub const GID: &str = "gid";

/// The field of a user account carrying its home directory.
pub const HOME: &str = "home";

/// The field of a user account carrying its login shell.
pub const SHELL: &str = "shell";

/// The field of a unit carrying its one-line description.
pub const DESCRIPTION: &str = "description";

/// The field of a unit carrying the command it runs.
pub const EXEC: &str = "exec";

/// The field of a unit carrying the units ordered after it.
pub const AFTER: &str = "after";

/// The field of a unit carrying the units it wants beside it.
pub const WANTS: &str = "wants";

/// The field of a policy entry naming the unit field it governs.
pub const FIELD: &str = "field";

/// The field of a policy entry naming the merge behavior for that field.
pub const BEHAVIOR: &str = "behavior";

/// The field of a replacement naming the owner it expects to replace.
pub const EXPECTED_OWNER: &str = "expected-owner";

/// The field of a replacement carrying the value that wins.
pub const VALUE: &str = "value";

/// The field of a file contribution carrying its file set.
pub const FILES: &str = "files";

/// The field of a user contribution carrying its accounts.
pub const USERS: &str = "users";

/// The field of a unit contribution carrying its unit record.
pub const UNIT: &str = "unit";

/// The field of a boot description carrying the machine it boots.
pub const MACHINE: &str = "machine";

/// The field of a boot description carrying the kernel image path.
pub const KERNEL: &str = "kernel";

/// The field of a boot description carrying the initrd path.
pub const INITRD: &str = "initrd";

/// The fields of a tools record, naming the host programs assembly uses and
/// the closure they need at run time.
pub const TOOL_SHELL: &str = "shell";
pub const TOOL_MKDIR: &str = "mkdir";
pub const TOOL_CAT: &str = "cat";
pub const TOOL_CHMOD: &str = "chmod";
pub const TOOL_LN: &str = "ln";
pub const TOOL_CLOSURE: &str = "closure";

/// One declared type: the use-site type and the coordinate spelling a value
/// of it carries. Both derived from the table entry, so a declaration is
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

    /// A value of this type over `representation`. The representation is not
    /// checked here: the request-input gate is where the declaration table
    /// put that check.
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

/// The body of one file entry: stored content with a mode, or a symlink to a
/// target the tree records verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileBody {
    File {
        content: ContentId,
        executable: bool,
    },
    Symlink {
        target: Box<str>,
    },
}

/// One user account, as the user table carries it.
#[derive(Clone, Debug)]
pub struct UserEntry {
    pub name: Box<str>,
    pub uid: i64,
    pub gid: i64,
    pub home: Box<str>,
    pub shell: Box<str>,
}

/// One merge behavior the unit policy can declare for a field, decision
/// 0052's closed constructor set as this library ships it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    /// Every contribution carrying the field must agree on its value.
    Agree,
    /// Every contribution's list is concatenated, then canonicalized.
    Concat,
}

struct Declarations {
    table: DeclarationTable,
    file_body: Declared,
    file_set: Declared,
    user_table: Declared,
    unit: Declared,
    field_behavior: Declared,
    unit_policy: Declared,
    tools: Declared,
    boot: Declared,
    unit_text: Declared,
    passwd_text: Declared,
    boot_text: Declared,
    system_tree: Declared,
}

fn declarations() -> &'static Declarations {
    static DECLARATIONS: OnceLock<Declarations> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let mut table = DeclarationTable::new(MODULE);
        fn declare_sum(
            table: &mut DeclarationTable,
            name: &str,
            constructors: &[(&str, Option<Type>)],
        ) -> Declared {
            let constructors: Box<[SumConstructor]> = constructors
                .iter()
                .map(|(constructor, payload)| SumConstructor {
                    name: (*constructor).into(),
                    payload: payload.clone(),
                })
                .collect();
            let declared_type = match table.sum(name, constructors) {
                Ok(declared) => declared,
                Err(error) => unreachable!("this domain declares each name once: {error}"),
            };
            Declared {
                spelling: format!("{MODULE}.{name}").into(),
                declared_type,
            }
        }
        fn declare(table: &mut DeclarationTable, name: &str, representation: Type) -> Declared {
            let declared_type = match table.nominal(name, representation) {
                Ok(declared) => declared,
                Err(error) => unreachable!("this domain declares each name once: {error}"),
            };
            Declared {
                spelling: format!("{MODULE}.{name}").into(),
                declared_type,
            }
        }

        let file_record = Type::record([
            RecordField {
                name: CONTENT.into(),
                payload: Type::Blob,
            },
            RecordField {
                name: EXECUTABLE.into(),
                payload: Type::Bool,
            },
        ])
        .unwrap_or_else(|error| unreachable!("the file record names two distinct fields: {error}"));
        let link_record = Type::record([RecordField {
            name: TARGET.into(),
            payload: Type::Text,
        }])
        .unwrap_or_else(|error| unreachable!("the link record names one field: {error}"));
        let file_body = declare_sum(
            &mut table,
            "FileBody",
            &[("file", Some(file_record)), ("symlink", Some(link_record))],
        );

        let entry_record = Type::record([
            RecordField {
                name: PATH.into(),
                payload: Type::Text,
            },
            RecordField {
                name: "body".into(),
                payload: file_body.declared_type(),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the entry record names two distinct fields: {error}")
        });
        let file_set = declare(&mut table, "FileSet", Type::List(Box::new(entry_record)));

        let user_record = Type::record([
            RecordField {
                name: NAME.into(),
                payload: Type::Text,
            },
            RecordField {
                name: UID.into(),
                payload: Type::Int,
            },
            RecordField {
                name: GID.into(),
                payload: Type::Int,
            },
            RecordField {
                name: HOME.into(),
                payload: Type::Text,
            },
            RecordField {
                name: SHELL.into(),
                payload: Type::Text,
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the user record names five distinct fields: {error}")
        });
        let user_table = declare(&mut table, "UserTable", Type::List(Box::new(user_record)));

        let unit_record = Type::record([
            RecordField {
                name: NAME.into(),
                payload: Type::Text,
            },
            RecordField {
                name: DESCRIPTION.into(),
                payload: Type::Text,
            },
            RecordField {
                name: EXEC.into(),
                payload: Type::Text,
            },
            RecordField {
                name: AFTER.into(),
                payload: text_list_type(),
            },
            RecordField {
                name: WANTS.into(),
                payload: text_list_type(),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the unit record names five distinct fields: {error}")
        });
        let unit = declare(&mut table, "Unit", unit_record);

        let field_behavior = declare_sum(
            &mut table,
            "FieldBehavior",
            &[("agree", None), ("concat", None)],
        );
        let policy_record = Type::record([
            RecordField {
                name: FIELD.into(),
                payload: Type::Text,
            },
            RecordField {
                name: BEHAVIOR.into(),
                payload: field_behavior.declared_type(),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the policy record names two distinct fields: {error}")
        });
        let unit_policy = declare(
            &mut table,
            "UnitPolicy",
            Type::List(Box::new(policy_record)),
        );

        let tools_record = Type::record([
            RecordField {
                name: TOOL_SHELL.into(),
                payload: Type::Text,
            },
            RecordField {
                name: TOOL_MKDIR.into(),
                payload: Type::Text,
            },
            RecordField {
                name: TOOL_CAT.into(),
                payload: Type::Text,
            },
            RecordField {
                name: TOOL_CHMOD.into(),
                payload: Type::Text,
            },
            RecordField {
                name: TOOL_LN.into(),
                payload: Type::Text,
            },
            RecordField {
                name: TOOL_CLOSURE.into(),
                payload: Type::List(Box::new(Type::Text)),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the tools record names six distinct fields: {error}")
        });
        let tools = declare(&mut table, "Tools", tools_record);

        let boot_record = Type::record([
            RecordField {
                name: MACHINE.into(),
                payload: Type::Text,
            },
            RecordField {
                name: KERNEL.into(),
                payload: Type::Text,
            },
            RecordField {
                name: INITRD.into(),
                payload: Type::Text,
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the boot record names three distinct fields: {error}")
        });
        let boot = declare(&mut table, "Boot", boot_record);

        let unit_text = declare(&mut table, "UnitText", Type::Text);
        let passwd_text = declare(&mut table, "PasswdText", Type::Text);
        let boot_text = declare(&mut table, "BootText", Type::Text);
        let system_tree = declare(&mut table, "SystemTree", Type::Blob);

        Declarations {
            table,
            file_body,
            file_set,
            user_table,
            unit,
            field_behavior,
            unit_policy,
            tools,
            boot,
            unit_text,
            passwd_text,
            boot_text,
            system_tree,
        }
    })
}

/// This domain's declaration table, for a reader that wants the coordinates
/// and revisions behind the types below.
#[must_use]
pub fn table() -> &'static DeclarationTable {
    &declarations().table
}

/// The sum a file entry's body carries: stored content or a symlink target.
#[must_use]
pub fn file_body() -> &'static Declared {
    &declarations().file_body
}

/// A path-sorted set of file and symlink entries, 0040's keyed spelling.
#[must_use]
pub fn file_set() -> &'static Declared {
    &declarations().file_set
}

/// A name-sorted table of user accounts.
#[must_use]
pub fn user_table() -> &'static Declared {
    &declarations().user_table
}

/// One service unit, composed from the fragments that declare pieces of it.
#[must_use]
pub fn unit() -> &'static Declared {
    &declarations().unit
}

/// The closed set of merge behaviors a policy can name for a field.
#[must_use]
pub fn field_behavior() -> &'static Declared {
    &declarations().field_behavior
}

/// The declared merge policy for a unit's fields.
#[must_use]
pub fn unit_policy() -> &'static Declared {
    &declarations().unit_policy
}

/// The host programs an assembly runs, as declared inputs on 0030's terms.
#[must_use]
pub fn tools() -> &'static Declared {
    &declarations().tools
}

/// The machine a boot entry describes.
#[must_use]
pub fn boot() -> &'static Declared {
    &declarations().boot
}

/// A rendered unit file.
#[must_use]
pub fn unit_text() -> &'static Declared {
    &declarations().unit_text
}

/// A rendered user table.
#[must_use]
pub fn passwd_text() -> &'static Declared {
    &declarations().passwd_text
}

/// A rendered boot entry.
#[must_use]
pub fn boot_text() -> &'static Declared {
    &declarations().boot_text
}

/// The composed artifact: one tree's content identity. Tree-ness lives in the
/// store's tree model and in the action contract that declares a tree output;
/// the value layer carries the identity, which is all M-5b's activation half
/// will need to name a composed system.
#[must_use]
pub fn system_tree() -> &'static Declared {
    &declarations().system_tree
}

/// A file body value over `body`. The body is a declared sum, so the value
/// names its constructor and carries the constructor's payload.
#[must_use]
pub fn file_body_value(body: &FileBody) -> Value {
    let (constructor, payload) = match body {
        FileBody::File {
            content,
            executable,
        } => (
            "file",
            Value::record([
                RecordField {
                    name: CONTENT.into(),
                    payload: Value::Blob(*content),
                },
                RecordField {
                    name: EXECUTABLE.into(),
                    payload: Value::Bool(*executable),
                },
            ])
            .unwrap_or_else(|error| {
                unreachable!("the file record names two distinct fields: {error}")
            }),
        ),
        FileBody::Symlink { target } => (
            "symlink",
            Value::record([RecordField {
                name: TARGET.into(),
                payload: Value::Text(target.clone()),
            }])
            .unwrap_or_else(|error| unreachable!("the link record names one field: {error}")),
        ),
    };
    Value::Sum {
        type_name: file_body().name().into(),
        constructor: constructor.into(),
        payload: Some(Box::new(payload)),
    }
}

/// A file set value over `entries`, sorted by path. Sorting is what makes two
/// callers who list the same files in different orders one request.
#[must_use]
pub fn file_set_value<I, P>(entries: I) -> Value
where
    I: IntoIterator<Item = (P, FileBody)>,
    P: Into<Box<str>>,
{
    let mut entries: Vec<(Box<str>, FileBody)> = entries
        .into_iter()
        .map(|(path, body)| (path.into(), body))
        .collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let records = entries
        .into_iter()
        .map(|(path, body)| {
            Value::record([
                RecordField {
                    name: PATH.into(),
                    payload: Value::Text(path),
                },
                RecordField {
                    name: "body".into(),
                    payload: file_body_value(&body),
                },
            ])
            .unwrap_or_else(|error| {
                unreachable!("the entry record names two distinct fields: {error}")
            })
        })
        .collect();
    file_set().value(Value::List(records))
}

/// A user table value over `entries`, sorted by account name.
#[must_use]
pub fn user_table_value(entries: &[UserEntry]) -> Value {
    let mut sorted: Vec<&UserEntry> = entries.iter().collect();
    sorted.sort_by(|left, right| left.name.cmp(&right.name));
    let records = sorted.into_iter().map(user_record_value).collect();
    user_table().value(Value::List(records))
}

fn user_record_value(user: &UserEntry) -> Value {
    Value::record([
        RecordField {
            name: NAME.into(),
            payload: Value::Text(user.name.clone()),
        },
        RecordField {
            name: UID.into(),
            payload: Value::int(user.uid),
        },
        RecordField {
            name: GID.into(),
            payload: Value::int(user.gid),
        },
        RecordField {
            name: HOME.into(),
            payload: Value::Text(user.home.clone()),
        },
        RecordField {
            name: SHELL.into(),
            payload: Value::Text(user.shell.clone()),
        },
    ])
    .unwrap_or_else(|error| unreachable!("the user record names five distinct fields: {error}"))
}

/// A unit value. The two list fields are canonicalized here, so a unit
/// contribution's spelling cannot carry an assembly order into the merge.
#[must_use]
pub fn unit_value(
    name: &str,
    description: &str,
    exec: &str,
    after: &[&str],
    wants: &[&str],
) -> Value {
    unit().value(
        Value::record([
            RecordField {
                name: NAME.into(),
                payload: Value::Text(name.into()),
            },
            RecordField {
                name: DESCRIPTION.into(),
                payload: Value::Text(description.into()),
            },
            RecordField {
                name: EXEC.into(),
                payload: Value::Text(exec.into()),
            },
            RecordField {
                name: AFTER.into(),
                payload: text_list(after),
            },
            RecordField {
                name: WANTS.into(),
                payload: text_list(wants),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the unit record names five distinct fields: {error}")
        }),
    )
}

/// Canonical list order: sorted by canonical encoding, duplicates collapsed.
/// The constructors and the merge operator share this one spelling of a set,
/// so a value built by either is the value the digest sees.
#[must_use]
pub fn canonical_list(items: Vec<Value>) -> Value {
    let mut items = items;
    items.sort_by_key(|item| item.encode_canonical());
    items.dedup();
    Value::List(items.into())
}

/// A canonical list of texts.
#[must_use]
pub fn text_list(items: &[&str]) -> Value {
    canonical_list(
        items
            .iter()
            .map(|item| Value::Text((*item).into()))
            .collect(),
    )
}

/// A unit policy value over `entries`, sorted by field name. A field the
/// policy does not name must agree; naming it `concat` is how a list field
/// accumulates.
#[must_use]
pub fn unit_policy_value(entries: &[(&str, Behavior)]) -> Value {
    let mut entries: Vec<(&str, Behavior)> = entries.to_vec();
    entries.sort_by_key(|(field, _)| *field);
    let records = entries
        .into_iter()
        .map(|(field, behavior)| {
            Value::record([
                RecordField {
                    name: FIELD.into(),
                    payload: Value::Text(field.into()),
                },
                RecordField {
                    name: BEHAVIOR.into(),
                    payload: behavior_value(behavior),
                },
            ])
            .unwrap_or_else(|error| {
                unreachable!("the policy record names two distinct fields: {error}")
            })
        })
        .collect();
    unit_policy().value(Value::List(records))
}

/// A behavior value over `behavior`.
#[must_use]
pub fn behavior_value(behavior: Behavior) -> Value {
    let constructor = match behavior {
        Behavior::Agree => "agree",
        Behavior::Concat => "concat",
    };
    Value::Sum {
        type_name: field_behavior().name().into(),
        constructor: constructor.into(),
        payload: None,
    }
}

/// A tools value naming the five host programs an assembly runs and the
/// closure they need, on 0030's terms: a tool enters a contract as a host
/// path plus a declared closure, and the closure is what the loader opens —
/// the interpreter above all — not only the tools themselves.
#[must_use]
pub fn tools_value(
    shell: &str,
    mkdir: &str,
    cat: &str,
    chmod: &str,
    ln: &str,
    closure: &[&str],
) -> Value {
    tools().value(
        Value::record([
            RecordField {
                name: TOOL_SHELL.into(),
                payload: Value::Text(shell.into()),
            },
            RecordField {
                name: TOOL_MKDIR.into(),
                payload: Value::Text(mkdir.into()),
            },
            RecordField {
                name: TOOL_CAT.into(),
                payload: Value::Text(cat.into()),
            },
            RecordField {
                name: TOOL_CHMOD.into(),
                payload: Value::Text(chmod.into()),
            },
            RecordField {
                name: TOOL_LN.into(),
                payload: Value::Text(ln.into()),
            },
            RecordField {
                name: TOOL_CLOSURE.into(),
                payload: Value::List(
                    closure
                        .iter()
                        .map(|path| Value::Text((*path).into()))
                        .collect(),
                ),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the tools record names six distinct fields: {error}")
        }),
    )
}

/// A boot description value.
#[must_use]
pub fn boot_value(machine: &str, kernel: &str, initrd: &str) -> Value {
    boot().value(
        Value::record([
            RecordField {
                name: MACHINE.into(),
                payload: Value::Text(machine.into()),
            },
            RecordField {
                name: KERNEL.into(),
                payload: Value::Text(kernel.into()),
            },
            RecordField {
                name: INITRD.into(),
                payload: Value::Text(initrd.into()),
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("the boot record names three distinct fields: {error}")
        }),
    )
}

/// A file set value over path-and-body-value pairs, sorted by path. The merge
/// operator composes body values; this rebuilds the declared spelling around
/// what it produced.
#[must_use]
pub fn file_set_of(entries: Vec<(Box<str>, Value)>) -> Value {
    let mut entries = entries;
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let records = entries
        .into_iter()
        .map(|(path, body)| {
            Value::record([
                RecordField {
                    name: PATH.into(),
                    payload: Value::Text(path),
                },
                RecordField {
                    name: "body".into(),
                    payload: body,
                },
            ])
            .unwrap_or_else(|error| {
                unreachable!("the entry record names two distinct fields: {error}")
            })
        })
        .collect();
    file_set().value(Value::List(records))
}

/// A user table value over name-and-record pairs, sorted by account name.
#[must_use]
pub fn user_table_of(entries: Vec<(Box<str>, Value)>) -> Value {
    let mut entries = entries;
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    user_table().value(Value::List(
        entries
            .into_iter()
            .map(|(_, record)| record)
            .collect::<Vec<_>>()
            .into(),
    ))
}

fn contribution_value(payload_field: &str, owner: &str, payload: Value) -> Value {
    Value::record([
        RecordField {
            name: OWNER.into(),
            payload: Value::Text(owner.into()),
        },
        RecordField {
            name: payload_field.into(),
            payload,
        },
    ])
    .unwrap_or_else(|error| {
        unreachable!("a contribution record names two distinct fields: {error}")
    })
}

fn contributions_value(payload_field: &str, contributions: &[(&str, Value)]) -> Value {
    let mut contributions: Vec<(&str, &Value)> =
        contributions.iter().map(|(o, v)| (*o, v)).collect();
    contributions.sort_by_key(|(owner, _)| *owner);
    Value::List(
        contributions
            .into_iter()
            .map(|(owner, payload)| contribution_value(payload_field, owner, payload.clone()))
            .collect(),
    )
}

/// File contributions, one record per owner, sorted by owner. Sorting is what
/// makes the merge a function of the set of contributions rather than their
/// order, decision 0052's order-insensitivity.
#[must_use]
pub fn etc_contributions(contributions: &[(&str, Value)]) -> Value {
    contributions_value(FILES, contributions)
}

/// User contributions, one record per owner, sorted by owner.
#[must_use]
pub fn user_contributions(contributions: &[(&str, Value)]) -> Value {
    contributions_value(USERS, contributions)
}

/// Unit contributions, one record per owner, sorted by owner.
#[must_use]
pub fn unit_contributions(contributions: &[(&str, Value)]) -> Value {
    contributions_value(UNIT, contributions)
}

/// A replacement list, sorted by field then expected owner. A replacement is
/// decision 0052's deliberate way a value wins: it names the field, the owner
/// it expects to replace, and the value that takes the field.
#[must_use]
pub fn unit_replacements(replacements: &[(&str, &str, &str)]) -> Value {
    let mut replacements: Vec<(&str, &str, &str)> = replacements.to_vec();
    replacements.sort();
    Value::List(
        replacements
            .into_iter()
            .map(|(field, expected_owner, value)| {
                Value::record([
                    RecordField {
                        name: FIELD.into(),
                        payload: Value::Text(field.into()),
                    },
                    RecordField {
                        name: EXPECTED_OWNER.into(),
                        payload: Value::Text(expected_owner.into()),
                    },
                    RecordField {
                        name: VALUE.into(),
                        payload: Value::Text(value.into()),
                    },
                ])
                .unwrap_or_else(|error| {
                    unreachable!("a replacement record names three distinct fields: {error}")
                })
            })
            .collect(),
    )
}

fn text_list_type() -> Type {
    Type::List(Box::new(Type::Text))
}

fn list_of_records(element: Type) -> Type {
    Type::List(Box::new(element))
}

fn contribution_type(payload_field: &str, payload: Type) -> Type {
    list_of_records(
        Type::record([
            RecordField {
                name: OWNER.into(),
                payload: Type::Text,
            },
            RecordField {
                name: payload_field.into(),
                payload,
            },
        ])
        .unwrap_or_else(|error| {
            unreachable!("a contribution record names two distinct fields: {error}")
        }),
    )
}

/// `(contributions) -> FileSet`: what the etc merge computes.
#[must_use]
pub fn etc_interface() -> Interface {
    Interface {
        inputs: Box::new([contribution_type(FILES, file_set().declared_type())]),
        output: file_set().declared_type(),
    }
}

/// `(contributions) -> UserTable`: what the user merge computes.
#[must_use]
pub fn users_interface() -> Interface {
    Interface {
        inputs: Box::new([contribution_type(USERS, user_table().declared_type())]),
        output: user_table().declared_type(),
    }
}

/// `(policy, contributions, replacements) -> Unit`: what the unit merge
/// computes. The policy and the replacements are declared inputs, so two
/// merges run under different policies or replacements are different
/// computations on 0023's terms.
#[must_use]
pub fn unit_interface() -> Interface {
    Interface {
        inputs: Box::new([
            unit_policy().declared_type(),
            contribution_type(UNIT, unit().declared_type()),
            list_of_records(
                Type::record([
                    RecordField {
                        name: FIELD.into(),
                        payload: Type::Text,
                    },
                    RecordField {
                        name: EXPECTED_OWNER.into(),
                        payload: Type::Text,
                    },
                    RecordField {
                        name: VALUE.into(),
                        payload: Type::Text,
                    },
                ])
                .unwrap_or_else(|error| {
                    unreachable!("a replacement record names three distinct fields: {error}")
                }),
            ),
        ]),
        output: unit().declared_type(),
    }
}

/// `(Unit) -> UnitText`: the unit file projection.
#[must_use]
pub fn render_unit_interface() -> Interface {
    Interface {
        inputs: Box::new([unit().declared_type()]),
        output: unit_text().declared_type(),
    }
}

/// `(UserTable) -> PasswdText`: the user table projection.
#[must_use]
pub fn render_passwd_interface() -> Interface {
    Interface {
        inputs: Box::new([user_table().declared_type()]),
        output: passwd_text().declared_type(),
    }
}

/// `(Boot) -> BootText`: the boot entry projection.
#[must_use]
pub fn render_boot_interface() -> Interface {
    Interface {
        inputs: Box::new([boot().declared_type()]),
        output: boot_text().declared_type(),
    }
}

/// `(Tools, machine, unit file name, FileSet, UnitText, PasswdText, BootText)
/// -> SystemTree`: what the assembly action computes. The machine and file
/// names are plain texts: they name paths inside the artifact, and the two
/// rendered kinds beside them already carry the domain's own identity.
#[must_use]
pub fn assemble_interface() -> Interface {
    Interface {
        inputs: Box::new([
            tools().declared_type(),
            Type::Text,
            Type::Text,
            file_set().declared_type(),
            unit_text().declared_type(),
            passwd_text().declared_type(),
            boot_text().declared_type(),
        ]),
        output: system_tree().declared_type(),
    }
}

/// `(Tools, Boot, file contributions, user contributions, policy, unit
/// contributions, replacements) -> SystemTree`: what a caller requests.
#[must_use]
pub fn compose_system_interface() -> Interface {
    Interface {
        inputs: Box::new([
            tools().declared_type(),
            boot().declared_type(),
            contribution_type(FILES, file_set().declared_type()),
            contribution_type(USERS, user_table().declared_type()),
            unit_policy().declared_type(),
            contribution_type(UNIT, unit().declared_type()),
            list_of_records(
                Type::record([
                    RecordField {
                        name: FIELD.into(),
                        payload: Type::Text,
                    },
                    RecordField {
                        name: EXPECTED_OWNER.into(),
                        payload: Type::Text,
                    },
                    RecordField {
                        name: VALUE.into(),
                        payload: Type::Text,
                    },
                ])
                .unwrap_or_else(|error| {
                    unreachable!("a replacement record names three distinct fields: {error}")
                }),
            ),
        ]),
        output: system_tree().declared_type(),
    }
}

/// The rule labels, which are also the name halves of their coordinates.
pub const COMPOSE_ETC: &str = "compose-etc";
pub const COMPOSE_USERS: &str = "compose-users";
pub const COMPOSE_UNIT: &str = "compose-unit";
pub const RENDER_UNIT: &str = "render-unit";
pub const RENDER_PASSWD: &str = "render-passwd";
pub const RENDER_BOOT: &str = "render-boot";
pub const ASSEMBLE: &str = "assemble";
pub const COMPOSE_SYSTEM: &str = "compose-system";

/// A request to merge file contributions into one file set.
#[must_use]
pub fn compose_etc_request(contributions: Value) -> Request<Pure> {
    Request::<Pure>::new(COMPOSE_ETC, etc_interface(), [contributions], Span::none())
}

/// A request to merge user contributions into one user table.
#[must_use]
pub fn compose_users_request(contributions: Value) -> Request<Pure> {
    Request::<Pure>::new(
        COMPOSE_USERS,
        users_interface(),
        [contributions],
        Span::none(),
    )
}

/// A request to merge unit contributions under `policy`, with `replacements`
/// applied first.
#[must_use]
pub fn compose_unit_request(
    policy: Value,
    contributions: Value,
    replacements: Value,
) -> Request<Pure> {
    Request::<Pure>::new(
        COMPOSE_UNIT,
        unit_interface(),
        [policy, contributions, replacements],
        Span::none(),
    )
}

/// A request to render a unit as its file text.
#[must_use]
pub fn render_unit_request(unit: Value) -> Request<Pure> {
    Request::<Pure>::new(RENDER_UNIT, render_unit_interface(), [unit], Span::none())
}

/// A request to render a user table as its passwd text.
#[must_use]
pub fn render_passwd_request(table: Value) -> Request<Pure> {
    Request::<Pure>::new(
        RENDER_PASSWD,
        render_passwd_interface(),
        [table],
        Span::none(),
    )
}

/// A request to render a boot description as its loader entry.
#[must_use]
pub fn render_boot_request(boot: Value) -> Request<Pure> {
    Request::<Pure>::new(RENDER_BOOT, render_boot_interface(), [boot], Span::none())
}

/// A request to assemble the artifact tree from a merged file set and the
/// three rendered texts.
#[must_use]
pub fn assemble_request(
    tools: Value,
    machine: &str,
    unit_name: &str,
    files: Value,
    unit_text: Value,
    passwd_text: Value,
    boot_text: Value,
) -> Request<pith_core::Action> {
    Request::<pith_core::Action>::new(
        ASSEMBLE,
        assemble_interface(),
        [
            tools,
            Value::Text(machine.into()),
            Value::Text(unit_name.into()),
            files,
            unit_text,
            passwd_text,
            boot_text,
        ],
        Span::none(),
    )
}

/// A request to compose a whole system: merge each kind of contribution,
/// render the texts, and assemble the artifact.
#[must_use]
pub fn compose_system_request(
    tools: Value,
    boot: Value,
    etc: Value,
    users: Value,
    policy: Value,
    units: Value,
    replacements: Value,
) -> Request<Pure> {
    Request::<Pure>::new(
        COMPOSE_SYSTEM,
        compose_system_interface(),
        [tools, boot, etc, users, policy, units, replacements],
        Span::none(),
    )
}
