//! The build rules: discovery, compile, link, generate, test.
//!
//! Header dependencies are discovered, not declared by hand (decision 0034).
//! The discovery pass is its own action — the preprocessor over the source
//! with the whole header universe staged, capturing a depfile — and the
//! compile entry parses that depfile and requests the compile with the
//! discovered set as a request input. The compile's `plan()` resolves each
//! discovered path against the universe it was registered with, so the
//! contract it digests names exactly the headers the source includes.
//!
//! Every request input here carries what dispatch and caching need (the
//! toolchain, the source or object identities, the discovered set). The source
//! `ContentId` reaches `plan()` through `inputs`, so one registration of the
//! compile rule serves every source file: a different source is a different
//! request, which plans a different contract, which computes a different action
//! key (decision 0031).
//!
//! Each rule lives in its own module beside this one; what they share — the
//! revision they derive identity from, the diagnostics they speak, the
//! request-input and value readers, the staged path names — lives here.

mod compile;
mod discover;
mod generate;
mod link;
mod test;

pub use compile::{CompileAction, CompileRule};
pub use discover::{HeaderDiscoveryAction, HeaderUniverse};
pub use generate::{GenerateAction, GenerateRule};
pub use link::{LinkAction, LinkRule};
pub use test::{TestAction, TestRule};

use pith_core::{Action, EnvironmentVariable, Request, RuleIdentity, RuleRevision, Value};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{PureRuleFrame, PureStep, Resumption};
use pith_ids::ContentId;

use crate::toolchain::{Toolchain, Toolchains};
use crate::types;

/// The staged path of the one C source every compile-shaped action works on.
pub(crate) const SOURCE_PATH: &str = "source.c";

/// The staged path of the object a compile writes.
pub(crate) const OBJECT_PATH: &str = "source.o";

/// The staged path of the depfile a discovery pass captures.
pub(crate) const DEPFILE_PATH: &str = "deps.d";

/// The staged path of the executable a link writes.
pub(crate) const EXECUTABLE_PATH: &str = "out";

/// The staged path of the source a generator writes.
pub(crate) const GENERATED_PATH: &str = "generated.c";

/// The revision every xylem rule derives its identity from. Bumping this
/// invalidates every cached xylem result, which is what a semantic change to a
/// rule body should do.
pub(crate) fn rule_revision(label: &str) -> RuleRevision {
    let identity = RuleIdentity::of_module_declaration("xylem", label);
    RuleRevision::of_manifest(identity, b"xylem-v3")
}

pub(crate) fn diag(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(9002),
        Span::none(),
        message,
    ));
    sink
}

/// Extracts the blob identity from a nominal content value
/// (`xylem.CSource`, `xylem.Object`), or diagnoses a value that is not one.
pub(crate) fn blob_of(value: &Value, expected_name: &str) -> PithResult<ContentId> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == expected_name => match representation.as_ref() {
            Value::Blob(id) => Ok(*id),
            _ => Err(diag(&format!(
                "a {expected_name} value carried {representation:?} rather than a blob"
            ))),
        },
        _ => Err(diag(&format!(
            "expected a {expected_name} value, found {}",
            value.describe()
        ))),
    }
}

/// One header the request provided beside the source: the include spelling
/// and the content it names, which is also the staged path.
pub(crate) struct ProvidedHeader {
    pub(crate) path: Box<str>,
    pub(crate) content: ContentId,
}

/// Reads a provided-headers value into pairs, diagnosing a request that did
/// not carry the declared record shape.
pub(crate) fn provided_headers_of(value: &Value) -> PithResult<Vec<ProvidedHeader>> {
    let Value::List(entries) = value else {
        return Err(diag(&format!(
            "expected a provided header set, found {}",
            value.describe()
        )));
    };
    let mut headers = Vec::with_capacity(entries.len());
    for entry in entries.iter() {
        let Value::Record(fields) = entry else {
            return Err(diag(&format!(
                "a provided header was {}, not a path-and-content record",
                entry.describe()
            )));
        };
        let mut path = None;
        let mut content = None;
        for field in fields.iter() {
            match field.name.as_ref() {
                types::HEADER_PATH => match &field.payload {
                    Value::Text(text) => path = Some(text.clone()),
                    other => {
                        return Err(diag(&format!(
                            "a provided header path was {other:?} rather than text"
                        )));
                    }
                },
                types::HEADER_CONTENT => match &field.payload {
                    Value::Blob(id) => content = Some(*id),
                    other => {
                        return Err(diag(&format!(
                            "a provided header content was {other:?} rather than a blob"
                        )));
                    }
                },
                _ => {}
            }
        }
        match (path, content) {
            (Some(path), Some(content)) => headers.push(ProvidedHeader { path, content }),
            _ => {
                return Err(diag(
                    "a provided header record was missing its path or its content",
                ));
            }
        }
    }
    Ok(headers)
}

/// The headers one compile may see: the registered universe plus the request's
/// provided set, refusing a path the two spell with different content — the
/// same include naming two headers is a conflict to report, and agreeing
/// duplicates collapse.
pub(crate) fn effective_headers(
    universe: &HeaderUniverse,
    provided: &[ProvidedHeader],
) -> PithResult<Vec<(Box<str>, ContentId)>> {
    let mut headers: Vec<(Box<str>, ContentId)> = universe.iter().cloned().collect();
    for header in provided {
        match universe.resolve(&header.path) {
            Some(registered) if registered != header.content => {
                return Err(diag(&format!(
                    "the include path `{}` is provided as content `{}` and registered as \
                     content `{}`: one spelling cannot name two headers",
                    header.path,
                    header.content.digest(),
                    registered.digest(),
                )));
            }
            // A provided header naming the registered content collapses into
            // the entry the universe already contributed.
            Some(_) => {}
            None => headers.push((header.path.clone(), header.content)),
        }
    }
    Ok(headers)
}

/// The search paths a driver needs to find the rest of itself. `COMPILER_PATH`
/// appears only for a driver that has a separate compiler to find, since an
/// empty one would be a declaration with nothing behind it.
pub(crate) fn compiler_environment(toolchain: &Toolchain) -> Box<[EnvironmentVariable]> {
    let mut environment = Vec::with_capacity(2);
    if let Some(program_path) = &toolchain.program_path {
        environment.push(EnvironmentVariable {
            name: "COMPILER_PATH".into(),
            value: program_path.clone(),
        });
    }
    environment.push(EnvironmentVariable {
        name: "PATH".into(),
        value: toolchain.tool_directory.clone(),
    });
    environment.into_boxed_slice()
}

/// The driver path a toolchain value carries, which is its identity for dispatch.
fn driver_of(value: &Value) -> PithResult<&str> {
    match value {
        Value::Nominal {
            name,
            representation,
        } if name.as_ref() == types::TOOLCHAIN => match representation.as_ref() {
            Value::Text(driver) => Ok(driver),
            _ => Err(diag("a Toolchain value carried no driver path")),
        },
        _ => Err(diag(&format!(
            "expected a {} value, found {}",
            types::TOOLCHAIN,
            value.describe()
        ))),
    }
}

/// The registered toolchain the request's first input names.
pub(crate) fn requested_toolchain<'a>(
    toolchains: &'a Toolchains,
    inputs: &[Value],
) -> PithResult<&'a Toolchain> {
    let driver = driver_of(input(inputs, 0)?)?;
    toolchains.resolve(driver).ok_or_else(|| {
        diag(&format!(
            "the request named the toolchain `{driver}`, which this build was not registered with"
        ))
    })
}

/// Extract input `index` from `inputs`, diagnosing a request that did not
/// supply enough values.
pub(crate) fn input(inputs: &[Value], index: usize) -> PithResult<&Value> {
    inputs.get(index).ok_or_else(|| {
        diag(&format!(
            "the request supplied {len} input(s); input {index} is missing",
            len = inputs.len()
        ))
    })
}

/// A pure frame that yields one action request and completes with its result.
pub(crate) struct ActionRequestFrame {
    pub(crate) action: Option<Request<Action>>,
}

impl PureRuleFrame for ActionRequestFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        if let Some(action) = self.action.take() {
            return Ok(PureStep::NeedAction(action));
        }
        match input.and_then(Resumption::one) {
            Some(value) => Ok(PureStep::Complete(value)),
            None => Err(diag("an action completed without a value")),
        }
    }
}
