//! The generated grammar of store operations.
//!
//! Steps refer to attempts by selector rather than by identifier, so generation
//! stays stateless and a step resolves against whatever exists when it runs.

use pith_core::{RecordField, Value};
use proptest::prelude::*;

/// How a generated step refers to an attempt. Resolved against what exists when
/// the step runs, so generation stays stateless.
pub(super) type Selector = u16;

/// Ascending, duplicate-free field names, so a generated record satisfies the
/// closed-record shape without a filter that would skew generation.
const RESULT_FIELD_NAMES: [&str; 2] = ["name", "value"];

/// The retained result of a generated completion. Covers the recursive value
/// constructors — list, record — beside the scalars, so an adapter's treatment
/// of every landed constructor is compared, not just the ones the fixtures
/// happened to use first.
fn result_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        2 => any::<i64>().prop_map(Value::Int),
        1 => any::<bool>().prop_map(Value::Bool),
        1 => proptest::collection::vec(any::<i64>(), 0..3)
            .prop_map(|elements| Value::List(elements.into_iter().map(Value::Int).collect())),
        1 => proptest::collection::vec(any::<bool>(), 0..2).prop_map(|payloads| {
            Value::Record(
                RESULT_FIELD_NAMES
                    .iter()
                    .zip(payloads)
                    .map(|(name, payload)| RecordField {
                        name: (*name).into(),
                        payload: Value::Bool(payload),
                    })
                    .collect(),
            )
        }),
    ]
}

/// A dependency edge a generated step declares. Capability-use edges are absent
/// because [`validate`](super::validate) derives them from the executor report.
#[derive(Clone, Copy, Debug)]
pub enum GeneratedDependency {
    Pure(Selector),
    Action(Selector),
    Blob(u8),
}

#[derive(Clone, Debug)]
pub enum Step {
    CreatePure {
        rule: u8,
        input: u8,
    },
    CreateAction {
        rule: u8,
        executable: u8,
        capabilities: Box<[u8]>,
        denied: bool,
    },
    Complete {
        attempt: Selector,
        dependencies: Box<[GeneratedDependency]>,
        result: Value,
        /// Publish a reuse decision the dependencies do not justify.
        corrupt_reuse: bool,
    },
    /// Stop an attempt without a result. `cancelled` picks which terminal
    /// state it is published as; the record carried is the same either way, so
    /// one step covers both and the two cannot drift apart in what they test.
    Stop {
        attempt: Selector,
        dependencies: Box<[GeneratedDependency]>,
        message_len: u8,
        notes: u8,
        cancelled: bool,
    },
    /// Publish over an attempt that already reached a terminal state.
    RepublishTerminal {
        attempt: Selector,
    },
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub steps: Box<[Step]>,
}

pub fn scenario() -> impl Strategy<Value = Scenario> {
    proptest::collection::vec(step(), 1..24).prop_map(|steps| Scenario {
        steps: steps.into_boxed_slice(),
    })
}

fn generated_dependencies() -> impl Strategy<Value = Box<[GeneratedDependency]>> {
    proptest::collection::vec(generated_dependency(), 0..4).prop_map(Vec::into_boxed_slice)
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        // Weighted toward creation so later steps have attempts to work with.
        3 => (0u8..3, 0u8..4).prop_map(|(rule, input)| Step::CreatePure { rule, input }),
        2 => (
            0u8..3,
            0u8..4,
            proptest::collection::vec(0u8..3, 0..3).prop_map(Vec::into_boxed_slice),
            any::<bool>(),
        )
            .prop_map(|(rule, executable, capabilities, denied)| Step::CreateAction {
                rule,
                executable,
                capabilities,
                denied,
            }),
        3 => (any::<Selector>(), generated_dependencies(), result_value(), one_in(8))
            .prop_map(|(attempt, dependencies, result, corrupt_reuse)| Step::Complete {
                attempt,
                dependencies,
                result,
                corrupt_reuse,
            }),
        2 => (
            any::<Selector>(),
            generated_dependencies(),
            0u8..24,
            0u8..3,
            any::<bool>(),
        )
            .prop_map(
                |(attempt, dependencies, message_len, notes, cancelled)| Step::Stop {
                    attempt,
                    dependencies,
                    message_len,
                    notes,
                    cancelled,
                }
            ),
        1 => any::<Selector>().prop_map(|attempt| Step::RepublishTerminal { attempt }),
    ]
}

/// Keeps deliberate corruption rare enough that the valid paths still run.
fn one_in(denominator: u32) -> impl Strategy<Value = bool> {
    (0u32..denominator).prop_map(|drawn| drawn == 0)
}

fn generated_dependency() -> impl Strategy<Value = GeneratedDependency> {
    prop_oneof![
        3 => any::<Selector>().prop_map(GeneratedDependency::Pure),
        2 => any::<Selector>().prop_map(GeneratedDependency::Action),
        1 => any::<u8>().prop_map(GeneratedDependency::Blob),
    ]
}
