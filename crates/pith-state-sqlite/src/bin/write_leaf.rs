//! Test fixture: evaluate one pure computation into a sqlite engine-state
//! database and exit.
//!
//! Hydration across a *process* boundary cannot be observed from inside one
//! process, so the cross-process test spawns this binary to be the engine that
//! computes the result, then reads it back from a fresh engine after this
//! process has exited.
//!
//! Usage: `write_leaf <database-path> <value>`

use std::process::ExitCode;

use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{Engine, EvaluationSource, PureRule, PureRuleFrame, PureStep, Resumption};
use pith_state_sqlite::SqliteEngineStateStore;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let (Some(database), Some(value)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: write_leaf <database-path> <value>");
        return ExitCode::FAILURE;
    };
    let Ok(value) = value.parse::<i64>() else {
        eprintln!("the value must be an integer");
        return ExitCode::FAILURE;
    };

    let state = match SqliteEngineStateStore::open(&database) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("could not open {database}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut engine = Engine::with_state_store(pith_store::MemoryContentStore::default(), state);
    engine.register_rule(leaf_rule(), ConstantRule(Value::Int(value)));

    match engine.evaluate_pure(&leaf_request()) {
        Ok(evaluation) => {
            // The writer must genuinely compute: a fixture that itself hydrated
            // would make the reader's hydration meaningless.
            if evaluation.source != EvaluationSource::Computed {
                eprintln!("expected to compute the leaf, got {:?}", evaluation.source);
                return ExitCode::FAILURE;
            }
            println!("{}", evaluation.value.describe());
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            for diagnostic in diagnostics.iter() {
                eprintln!("{}", diagnostic.message.0);
            }
            ExitCode::FAILURE
        }
    }
}

/// The rule identity and revision are derived from the module and declaration
/// names, so both processes name the same rule without sharing memory. That is
/// what makes the reader's computation key equal the writer's.
pub fn leaf_rule() -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("pith-state-sqlite-fixture", "leaf");
    let revision = RuleRevision::of_manifest(identity, b"pith-state-sqlite-fixture-v1");
    Rule::<Pure>::new(revision, "leaf", leaf_interface(), Span::none())
}

pub fn leaf_request() -> Request<Pure> {
    Request::<Pure>::new("leaf", leaf_interface(), [], Span::none())
}

fn leaf_interface() -> Interface {
    Interface {
        inputs: Box::new([]),
        output: Type::Int,
    }
}

struct ConstantRule(Value);

impl PureRule for ConstantRule {
    fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(ConstantFrame(self.0.clone()))
    }
}

struct ConstantFrame(Value);

impl PureRuleFrame for ConstantFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(self.0.clone()))
    }
}
