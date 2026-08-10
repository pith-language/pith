//! The generated grammar of store operations.
//!
//! Steps refer to attempts by selector rather than by identifier, so generation
//! stays stateless and a step resolves against whatever exists when it runs.

use proptest::prelude::*;

/// How a generated step refers to an attempt. Resolved against what exists when
/// the step runs, so generation stays stateless.
pub(super) type Selector = u16;

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
        result: i64,
        /// Publish a reuse decision the dependencies do not justify.
        corrupt_reuse: bool,
    },
    Fail {
        attempt: Selector,
        dependencies: Box<[GeneratedDependency]>,
        message_len: u8,
        notes: u8,
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
        3 => (any::<Selector>(), generated_dependencies(), any::<i64>(), one_in(8))
            .prop_map(|(attempt, dependencies, result, corrupt_reuse)| Step::Complete {
                attempt,
                dependencies,
                result,
                corrupt_reuse,
            }),
        2 => (any::<Selector>(), generated_dependencies(), 0u8..24, 0u8..3).prop_map(
            |(attempt, dependencies, message_len, notes)| Step::Fail {
                attempt,
                dependencies,
                message_len,
                notes,
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
