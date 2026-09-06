//! The module resolver domain: a pure rule over an acquired universe,
//! registered on the same engine API a peer domain uses.
//!
//! Resolution is a computation under 0040 — constraints, universe,
//! preference, and budget in; one of four answers out — and the module
//! domain supplies its own vocabulary for it rather than linking the
//! package resolver's. The compiler and query drivers link this crate and
//! register the resolver through the public engine call, so dependency
//! selection is a computation the engine can explain, reuse, and replay
//! rather than a subroutine running beside it.
//!
//! Acquiring the universe is a caller-side effect. Nothing in this module
//! reads a file, a socket, or a clock: removing network access changes
//! whether a universe could be acquired, never a solved answer.

mod model;
mod search;
mod values;

use pith_core::manifest::encode_str;
use pith_core::{
    BODY_ENCODING_VERSION, Int, Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Value,
};
use pith_diag::{DiagnosticSink, PithResult, Severity, Span};
use pith_engine::{Engine, PureRule, PureRuleFrame, PureStep, Resumption};
use pith_hir::FrontendCode;

pub use model::{
    Derivation, ModuleCandidate, ModuleConstraint, ModuleRequirement, ModuleResolution,
    ModuleSelection, ModuleUniverse, Preference, TrailEntry, UnknownPreference,
};
pub use search::{SolveRequest, resolve as solve};
pub use values::{MODULES_MODULE, selections_from_value};

/// The resolver's semantic version: bumped when the solve's meaning
/// changes, which moves the derived rule revision and with it the
/// identity a lock records.
const RESOLVER_SEMANTIC_VERSION: u32 = 1;

/// Registers the module resolver on an engine, through the same public
/// call a peer domain's extension trait uses.
pub trait RegisterModuleResolver {
    fn register_module_resolver(&mut self);
}

impl RegisterModuleResolver for Engine {
    fn register_module_resolver(&mut self) {
        self.register_rule(resolver_rule(), ModuleSolver);
    }
}

/// The resolver rule: `modules.resolve` over the four protocol inputs,
/// ready for the engine's own `register_rule` call.
#[must_use]
pub fn resolver_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration(values::MODULES_MODULE, "resolve");
    let revision = RuleRevision::of_manifest(identity, &resolver_revision_manifest());
    Rule::new(
        values::MODULES_MODULE,
        revision,
        "resolve",
        resolve_interface(),
        Span::none(),
    )
}

/// The resolver's revision digest as lowercase hex, for the lock header
/// that records which resolver produced a selection. Read off the rule so
/// the lock records the revision the engine actually keys on; the revision
/// derives from the interface, so a change to any resolver declaration
/// moves it.
#[must_use]
pub fn resolver_revision_hex() -> Box<str> {
    resolver_rule().revision.digest().to_string().into()
}

fn resolve_interface() -> Interface {
    Interface {
        inputs: Box::new([
            values::constraint_set_type(),
            values::universe_type(),
            values::preference_type(),
            pith_core::Type::Int,
        ]),
        output: values::resolution_type(),
    }
}

/// The revision manifest: the resolver's semantic version, the crate's
/// version, the body encoding, and the canonical interface, each of which
/// moving the answer moves the revision.
fn resolver_revision_manifest() -> Vec<u8> {
    let mut manifest = RESOLVER_SEMANTIC_VERSION.to_le_bytes().to_vec();
    encode_str(&mut manifest, env!("CARGO_PKG_VERSION"));
    manifest.push(BODY_ENCODING_VERSION);
    manifest.extend_from_slice(&resolve_interface().encode_canonical());
    manifest
}

/// A resolve request over the four protocol inputs, as values.
#[must_use]
pub fn resolve_request(
    constraints: &[ModuleConstraint],
    universe: &ModuleUniverse,
    preference: Preference,
    budget: u64,
) -> Request<Pure> {
    Request::new(
        "resolve",
        resolve_interface(),
        [
            values::constraint_set_value(constraints),
            values::universe_value(universe.candidates()),
            values::preference_value(preference),
            Value::int(Int::from(budget)),
        ],
        Span::none(),
    )
}

/// The resolver's host-rule body: one step, completing with the answer
/// value. The engine never sees the search.
struct ModuleSolver;

impl PureRule for ModuleSolver {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ModuleSolveFrame {
            answer: Some(solve_from_values(inputs)),
        })
    }
}

struct ModuleSolveFrame {
    answer: Option<PithResult<Value>>,
}

impl PureRuleFrame for ModuleSolveFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        match self.answer.take() {
            Some(Ok(value)) => Ok(PureStep::Complete(value)),
            Some(Err(diagnostics)) => Err(diagnostics),
            None => Err(unreadable("the resolve step ran twice")),
        }
    }
}

/// Decodes the request's four inputs and runs the search.
fn solve_from_values(inputs: &[Value]) -> PithResult<Value> {
    let [constraints, universe, preference, budget] = inputs else {
        return Err(unreadable(&format!(
            "a resolve request supplies four inputs; found {}",
            inputs.len()
        )));
    };
    let Value::List(entries) = constraints else {
        return Err(unreadable(
            "the first resolve input is not a constraint set",
        ));
    };
    let parsed = entries
        .iter()
        .map(values::constraint_from_value)
        .collect::<PithResult<Vec<_>>>()?;
    let candidates = values::universe_from_value(universe)?;
    let preference = values::preference_from_value(preference)?;
    let Value::Int(budget) = budget else {
        return Err(unreadable(
            "the fourth resolve input is not a budget integer",
        ));
    };
    let budget = budget
        .to_i64()
        .and_then(|budget| u64::try_from(budget).ok())
        .ok_or_else(|| unreadable("the resolve budget is not a count"))?;
    let request = SolveRequest {
        constraints: parsed.into(),
        universe: ModuleUniverse::new(candidates),
        preference,
        budget,
    };
    Ok(values::resolution_value(&search::resolve(&request)))
}

fn unreadable(message: &str) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(pith_diag::Diag::new(
        Severity::Error,
        FrontendCode::MalformedResolution.stable(),
        Span::none(),
        message,
    ));
    sink
}
