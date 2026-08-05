//! The arena dependency graph and synchronous pure evaluator (0021, 0022).

use indexmap::IndexMap;
use pith_core::{Request, Rule, RuleArena, RuleId, Value, select_rule};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_ids::{ComputationArena, ComputationId};
use smallvec::SmallVec;

/// One bounded transition made by a pure rule body.
///
/// A body can only finish with a value or yield a tracked request to the
/// engine. It cannot call the engine or the async runtime directly.
#[derive(Clone, Debug)]
pub enum PureStep {
    Need(Request),
    Complete(Value),
}

/// Suspended state for one pure rule application.
pub trait PureRuleFrame {
    /// Advance the rule by one bounded step. `input` is the value returned by
    /// the request yielded by the preceding step, or `None` for the first step.
    ///
    /// # Errors
    /// Returns structured diagnostics when the rule cannot produce its value.
    fn step(&mut self, input: Option<Value>) -> PithResult<PureStep>;
}

/// Executable implementation associated with semantic [`Rule`] metadata.
///
/// The implementation lives in `pith-engine`, leaving `pith-core` as pure IR.
/// A fresh frame is created for every rule application.
pub trait PureRule {
    fn start(&self) -> Box<dyn PureRuleFrame>;
}

/// A dependency recorded while evaluating a computation.
#[derive(Clone, Debug)]
pub enum DependencyEdge {
    Request {
        request: Request,
        computation: ComputationId,
    },
}

/// One rule application in the in-memory graph.
pub struct ComputationNode {
    pub request: Request,
    pub rule: RuleId,
    pub dependencies: SmallVec<[DependencyEdge; 4]>,
    pub result: Option<Value>,
}

/// A completed evaluation and the graph node that produced it.
#[derive(Clone, Debug)]
pub struct Evaluation {
    pub value: Value,
    pub computation: ComputationId,
}

struct EvalFrame {
    computation: ComputationId,
    rule: RuleId,
    request: Request,
    body: Box<dyn PureRuleFrame>,
    resume_with: Option<Value>,
}

pub struct Engine {
    rules: RuleArena<Rule>,
    bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    computations: ComputationArena<ComputationNode>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            rules: RuleArena::new(),
            bodies: IndexMap::new(),
            computations: ComputationArena::new(),
        }
    }

    /// Register semantic rule metadata together with its executable pure body.
    pub fn register_rule<B>(&mut self, rule: Rule, body: B) -> RuleId
    where
        B: PureRule + 'static,
    {
        let id = self.rules.push(rule);
        self.bodies.insert(id, Box::new(body));
        id
    }

    pub fn rules_iter(&self) -> impl Iterator<Item = (RuleId, &Rule)> {
        self.rules.iter()
    }

    /// Evaluate a request on the synchronous pure step machine.
    ///
    /// # Errors
    /// Returns a `DiagnosticSink` with stable code `E-1101` when no rule
    /// matches, `E-1102` (naming every candidate) when more than one matches,
    /// `E-1203` when evaluation detects a dependency cycle, or diagnostics
    /// emitted by a rule body.
    pub fn evaluate(&mut self, request: &Request) -> PithResult<Evaluation> {
        let rule = self.resolve_rule(request)?;
        let root = self.start_frame(request.clone(), rule)?;
        let mut stack = vec![root];

        loop {
            let step = match stack.last_mut() {
                Some(frame) => frame.body.step(frame.resume_with.take())?,
                None => return Err(internal_diag("pure evaluator lost its root frame")),
            };

            match step {
                PureStep::Complete(value) => {
                    let Some(completed) = stack.pop() else {
                        return Err(internal_diag("pure evaluator completed without a frame"));
                    };
                    let Some(node) = self.computations.get_mut(completed.computation) else {
                        return Err(internal_diag("pure evaluator lost a computation node"));
                    };
                    node.result = Some(value.clone());

                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(value);
                    } else {
                        return Ok(Evaluation {
                            value,
                            computation: completed.computation,
                        });
                    }
                }
                PureStep::Need(child_request) => {
                    let child_rule = self.resolve_rule(&child_request)?;
                    if stack.iter().any(|frame| frame.rule == child_rule) {
                        let mut chain: Vec<&str> = stack
                            .iter()
                            .map(|frame| frame.request.label.as_ref())
                            .collect();
                        chain.push(child_request.label.as_ref());
                        return Err(cycle_diag(&chain, child_request.span));
                    }

                    let child = self.start_frame(child_request.clone(), child_rule)?;
                    let Some(parent) = stack.last() else {
                        return Err(internal_diag("pure evaluator lost a requesting frame"));
                    };
                    let Some(parent_node) = self.computations.get_mut(parent.computation) else {
                        return Err(internal_diag("pure evaluator lost a parent computation"));
                    };
                    parent_node.dependencies.push(DependencyEdge::Request {
                        request: child_request,
                        computation: child.computation,
                    });
                    stack.push(child);
                }
            }
        }
    }

    /// Return one computation node from the current in-memory graph.
    pub fn computation(&self, id: ComputationId) -> Option<&ComputationNode> {
        self.computations.get(id)
    }

    /// Return the tracked dependencies of one rule application (K-12).
    pub fn dependencies_of(&self, id: ComputationId) -> Option<&[DependencyEdge]> {
        self.computations
            .get(id)
            .map(|node| node.dependencies.as_slice())
    }

    fn resolve_rule(&self, request: &Request) -> PithResult<RuleId> {
        select_rule(request, &self.rules)
            .into_result(request, &self.rules)
            .map_err(one_diag)
    }

    fn start_frame(&mut self, request: Request, rule: RuleId) -> PithResult<EvalFrame> {
        let Some(body) = self.bodies.get(&rule) else {
            return Err(internal_diag("selected rule has no executable body"));
        };
        let body = body.start();
        let computation = self.computations.push(ComputationNode {
            request: request.clone(),
            rule,
            dependencies: SmallVec::new(),
            result: None,
        });
        Ok(EvalFrame {
            computation,
            rule,
            request,
            body,
            resume_with: None,
        })
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

fn cycle_diag(chain: &[&str], span: Span) -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(203),
        span,
        format!("dependency cycle: {}", chain.join(" -> ")),
    ))
}

fn internal_diag(message: &str) -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(204),
        Span::none(),
        message,
    ))
}

fn one_diag(diag: Diag) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_core::{Interface, Type};

    struct UnitRule;

    impl PureRule for UnitRule {
        fn start(&self) -> Box<dyn PureRuleFrame> {
            Box::new(UnitFrame)
        }
    }

    struct UnitFrame;

    impl PureRuleFrame for UnitFrame {
        fn step(&mut self, _input: Option<Value>) -> PithResult<PureStep> {
            Ok(PureStep::Complete(Value::Unit))
        }
    }

    fn rule(label: &str) -> Rule {
        Rule {
            label: label.into(),
            interface: Interface {
                inputs: Box::new([]),
                output: Type::Int,
            },
            span: Span::none(),
        }
    }

    fn request(label: &str) -> Request {
        Request {
            label: label.into(),
            interface: Interface {
                inputs: Box::new([]),
                output: Type::Int,
            },
            span: Span::none(),
        }
    }

    #[test]
    fn no_match_when_no_rule_present() {
        let mut engine = Engine::new();
        assert!(engine.evaluate(&request("missing")).is_err());
    }

    #[test]
    fn two_matches_is_ambiguous() {
        let mut engine = Engine::new();
        engine.register_rule(rule("thing"), UnitRule);
        engine.register_rule(rule("thing"), UnitRule);
        let err = engine.evaluate(&request("thing")).unwrap_err();
        let diags: Vec<_> = err.iter().collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags.first().unwrap().code, StableCode::engine(102));
    }
}
