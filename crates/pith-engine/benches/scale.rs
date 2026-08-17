//! Wall-clock cost of pure evaluation at graph sizes the test suite does not
//! reach. Four shapes, because they stress different parts of the engine:
//! a deep chain puts N frames on one stack, a wide sequence puts N requests
//! through a stack two deep, a fan-out opens N chains, and a reused chain
//! revalidates a recorded subtree N times.
//!
//! `harness = false`, so this is a plain binary and needs no benchmark
//! framework. The numbers are coarse by design — what they exist for is
//! answering whether a change moved the shape of the curve, which is the
//! question no measurement in the repository could answer before decision 0050.
//!
//! `cargo bench -p pith-engine`, or `just bench`.

// A benchmark that cannot construct its own fixture has nothing to report, and
// aborting loudly at setup is the honest failure. Neither appears in a
// measured path.
#![allow(
    clippy::expect_used,
    reason = "benchmark setup; a failure here means there is nothing to measure"
)]

use std::time::{Duration, Instant};

use pith_core::{Interface, Pure, Request, Rule, RuleIdentity, RuleRevision, Type, Value};
use pith_diag::{PithResult, Span};
use pith_engine::{Engine, PureRule, PureRuleFrame, PureStep, Resumption};

/// Sizes each shape is measured at. Doubling makes a superlinear curve visible
/// without a fit: a linear shape doubles its time, a quadratic one quadruples.
const SIZES: [u64; 5] = [1_000, 2_000, 4_000, 8_000, 16_000];

fn main() {
    println!("shape                    n      ms   ms/n");
    for size in SIZES {
        report("deep-chain", size, deep_chain(size));
    }
    for size in SIZES {
        report("wide-sequence", size, wide_sequence(size));
    }
    // Fan-out opens one chain per request and each chain holds a stack, so it is
    // measured over a smaller range than the other two.
    for size in SIZES.iter().take(3) {
        report("wide-fanout", *size, wide_fanout(*size));
    }
    // Revalidation walks the recorded subtree beneath a reused result (decision
    // 0051), and this shape is the worst case for it: a reuse at every depth of
    // a chain, so the walks sum to the chain's triangle. Measured over the same
    // smaller range as fan-out.
    for size in SIZES.iter().take(3) {
        report("reused-chain", *size, reused_chain(*size));
    }
}

fn report(shape: &str, size: u64, elapsed: Duration) {
    let millis = elapsed.as_secs_f64() * 1000.0;
    let per = millis / (size as f64);
    println!("{shape:<20} {size:>6} {millis:>7.1} {per:>6.3}");
}

/// `rule(n)` needs `rule(n - 1)` down to zero, so the chain holds `n` frames at
/// once. This is the shape whose cycle check was quadratic: every request
/// compared itself against every frame already on the stack.
fn deep_chain(depth: u64) -> Duration {
    let mut engine = Engine::new();
    let signature = interface();
    engine.register_rule(rule("descend", signature.clone()), DescendRule);
    let request = request("descend", signature, [Value::Int(as_int(depth))]);
    let start = Instant::now();
    engine
        .evaluate_pure(&request)
        .expect("the descending chain evaluates");
    start.elapsed()
}

/// One root requests `n` distinct children one after another. Each child
/// completes and leaves the stack before the next is requested, so the stack
/// stays two deep and what this measures is per-request overhead without depth:
/// rule selection, key derivation, arena allocation, and publication.
fn wide_sequence(width: u64) -> Duration {
    let mut engine = Engine::new();
    let signature = interface();
    engine.register_rule(rule("leaf", signature.clone()), LeafRule);
    let root = root_interface();
    engine.register_rule(
        rule("sequence", root.clone()),
        SequenceRule {
            child: "leaf",
            signature: signature.clone(),
        },
    );
    let request = request(
        "sequence",
        root,
        [Value::Int(as_int(width)), Value::Bool(true)],
    );
    let start = Instant::now();
    engine
        .evaluate_pure(&request)
        .expect("the sequential root evaluates");
    start.elapsed()
}

/// The first request builds a chain `n` deep; each of the `n - 1` that follow
/// finds its result already in the arena and revalidates it, which walks that
/// result's whole recorded subtree. So the walk runs at every depth and this is
/// where a per-pass memo would show up as the wrong bound if the walks were not
/// each other's prefixes.
fn reused_chain(depth: u64) -> Duration {
    let mut engine = Engine::new();
    let signature = interface();
    engine.register_rule(rule("descend", signature.clone()), DescendRule);
    let root = root_interface();
    engine.register_rule(
        rule("revisit", root.clone()),
        SequenceRule {
            child: "descend",
            signature,
        },
    );
    let request = request(
        "revisit",
        root,
        [Value::Int(as_int(depth)), Value::Bool(true)],
    );
    let start = Instant::now();
    engine
        .evaluate_pure(&request)
        .expect("the revisited chain evaluates");
    start.elapsed()
}

/// One root declares `n` independent requests in a single `NeedAll`, so the
/// scheduler opens `n` chains and the root parks until every slot is filled.
fn wide_fanout(width: u64) -> Duration {
    let mut engine = Engine::new();
    let signature = interface();
    engine.register_rule(rule("leaf", signature.clone()), LeafRule);
    let root = root_interface();
    engine.register_rule(
        rule("fanout", root.clone()),
        FanOutRule {
            leaf: signature.clone(),
        },
    );
    let request = request(
        "fanout",
        root,
        [Value::Int(as_int(width)), Value::Bool(true)],
    );
    let start = Instant::now();
    engine
        .evaluate_pure(&request)
        .expect("the fan-out root evaluates");
    start.elapsed()
}

struct DescendRule;

impl PureRule for DescendRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(DescendFrame {
            depth: first_int(inputs),
            requested: false,
        })
    }
}

struct DescendFrame {
    depth: i64,
    requested: bool,
}

impl PureRuleFrame for DescendFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        if self.depth <= 0 || self.requested {
            return Ok(PureStep::Complete(Value::Int(self.depth)));
        }
        self.requested = true;
        Ok(PureStep::Need(request(
            "descend",
            interface(),
            [Value::Int(self.depth.saturating_sub(1))],
        )))
    }
}

struct LeafRule;

impl PureRule for LeafRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(LeafFrame {
            value: first_int(inputs),
        })
    }
}

struct LeafFrame {
    value: i64,
}

impl PureRuleFrame for LeafFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        Ok(PureStep::Complete(Value::Int(self.value)))
    }
}

/// Requests `child(n - 1)` down to `child(0)`, one at a time. What the child
/// is decides what the shape measures: a leaf makes every request fresh work,
/// and the descending chain makes every request but the first a reuse.
struct SequenceRule {
    child: &'static str,
    signature: Interface,
}

impl PureRule for SequenceRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(SequenceFrame {
            child: self.child,
            signature: self.signature.clone(),
            remaining: first_int(inputs),
        })
    }
}

struct SequenceFrame {
    child: &'static str,
    signature: Interface,
    remaining: i64,
}

impl PureRuleFrame for SequenceFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        if self.remaining <= 0 {
            return Ok(PureStep::Complete(Value::Int(0)));
        }
        self.remaining = self.remaining.saturating_sub(1);
        Ok(PureStep::Need(request(
            self.child,
            self.signature.clone(),
            [Value::Int(self.remaining)],
        )))
    }
}

struct FanOutRule {
    leaf: Interface,
}

impl PureRule for FanOutRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(FanOutFrame {
            leaf: self.leaf.clone(),
            width: first_int(inputs),
            requested: false,
        })
    }
}

struct FanOutFrame {
    leaf: Interface,
    width: i64,
    requested: bool,
}

impl PureRuleFrame for FanOutFrame {
    fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
        if self.requested {
            return Ok(PureStep::Complete(Value::Int(self.width)));
        }
        self.requested = true;
        let requests: Box<[Request<Pure>]> = (0..self.width)
            .map(|index| request("leaf", self.leaf.clone(), [Value::Int(index)]))
            .collect();
        Ok(PureStep::NeedAll(requests))
    }
}

/// The leaf signature, `(Int) -> Int`.
fn interface() -> Interface {
    Interface {
        inputs: vec![Type::Int].into_boxed_slice(),
        output: Type::Int,
    }
}

/// A root signature distinct from the leaf's, `(Int, Bool) -> Int`. The `Bool` is
/// never read; it exists so selection can tell the two rules apart.
fn root_interface() -> Interface {
    Interface {
        inputs: vec![Type::Int, Type::Bool].into_boxed_slice(),
        output: Type::Int,
    }
}

fn rule(label: &str, signature: Interface) -> Rule<Pure> {
    let identity = RuleIdentity::of_module_declaration("pith-engine.scale-bench", label);
    let revision = RuleRevision::of_manifest(identity, b"scale-bench-v1");
    Rule::<Pure>::new(revision, label, signature, Span::none())
}

fn request(label: &str, signature: Interface, inputs: impl Into<Box<[Value]>>) -> Request<Pure> {
    Request::<Pure>::new(label, signature, inputs, Span::none())
}

fn first_int(inputs: &[Value]) -> i64 {
    match inputs.first() {
        Some(Value::Int(value)) => *value,
        _ => 0,
    }
}

/// The sizes are `u64` so the table formats without casts; the calculus's `Int`
/// is `i64` today, and every size here is far inside it.
fn as_int(size: u64) -> i64 {
    i64::try_from(size).unwrap_or(i64::MAX)
}
