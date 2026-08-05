//! The arena dependency graph and synchronous pure evaluator (0021, 0022).

use indexmap::IndexMap;
use pith_core::{Pure, Request, Rule, RuleArena, RuleId, Value, select_rule};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_ids::{ComputationArena, ComputationId};
use smallvec::SmallVec;

/// One bounded transition made by a pure rule body.
///
/// A body can only finish with a value or yield a tracked request to the
/// engine. It cannot call the engine or the async runtime directly.
#[derive(Clone, Debug)]
pub enum PureStep {
    Need(Request<Pure>),
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
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame>;
}

/// A dependency recorded while evaluating a computation.
#[derive(Clone, Debug)]
pub enum DependencyEdge {
    Request {
        request: Request<Pure>,
        computation: ComputationId,
    },
}

impl DependencyEdge {
    pub fn computation(&self) -> ComputationId {
        match self {
            Self::Request { computation, .. } => *computation,
        }
    }
}

/// One rule application in the in-memory graph.
pub struct ComputationNode {
    pub request: Request<Pure>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSelection {
    pub rule: RuleId,
    pub interface: pith_core::Interface,
}

struct EvalFrame {
    computation: ComputationId,
    rule: RuleId,
    request: Request<Pure>,
    body: Box<dyn PureRuleFrame>,
    resume_with: Option<Value>,
}

pub struct Engine {
    rules: RuleArena<Rule<Pure>>,
    bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    computations: ComputationArena<ComputationNode>,
}

pub struct EngineQuery<'engine> {
    engine: &'engine Engine,
}

impl<'engine> EngineQuery<'engine> {
    /// # Errors
    /// Returns `E-1101`, `E-1102`, or `E-1103` when the request cannot select
    /// exactly one rule.
    pub fn select(&self, request: &Request<Pure>) -> Result<RuleSelection, Diag> {
        request.validate_inputs()?;
        let rule =
            select_rule(request, &self.engine.rules).into_result(request, &self.engine.rules)?;
        Ok(RuleSelection {
            rule,
            interface: request.interface.clone(),
        })
    }

    pub fn rules(&self) -> impl Iterator<Item = (RuleId, &'engine Rule<Pure>)> + 'engine {
        self.engine.rules.iter()
    }

    pub fn rule(&self, id: RuleId) -> Option<&'engine Rule<Pure>> {
        self.engine.rules.get(id)
    }

    pub fn computations(
        &self,
    ) -> impl Iterator<Item = (ComputationId, &'engine ComputationNode)> + 'engine {
        self.engine.computations.iter()
    }

    pub fn computation(&self, id: ComputationId) -> Option<&'engine ComputationNode> {
        self.engine.computations.get(id)
    }

    pub fn dependencies_of(&self, id: ComputationId) -> Option<&'engine [DependencyEdge]> {
        self.engine
            .computations
            .get(id)
            .map(|node| node.dependencies.as_slice())
    }

    pub fn dependents_of(
        &self,
        dependency: ComputationId,
    ) -> impl Iterator<Item = (ComputationId, &'engine DependencyEdge)> + 'engine {
        self.engine
            .computations
            .iter()
            .flat_map(move |(computation, node)| {
                node.dependencies
                    .iter()
                    .filter(move |edge| edge.computation() == dependency)
                    .map(move |edge| (computation, edge))
            })
    }
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
    pub fn register_rule<B>(&mut self, rule: Rule<Pure>, body: B) -> RuleId
    where
        B: PureRule + 'static,
    {
        let id = self.rules.push(rule);
        self.bodies.insert(id, Box::new(body));
        id
    }

    pub fn query(&self) -> EngineQuery<'_> {
        EngineQuery { engine: self }
    }

    /// Evaluate a request on the synchronous pure step machine.
    ///
    /// # Errors
    /// Returns a `DiagnosticSink` with stable code `E-1101` when no rule
    /// matches, `E-1102` (naming every candidate) when more than one matches,
    /// `E-1103` or `E-1104` when values violate the selected interface,
    /// `E-1203` when evaluation detects a dependency cycle, or diagnostics
    /// emitted by a rule body.
    ///
    /// ```compile_fail
    /// use pith_core::{Interface, Mutation, Request, Type};
    /// use pith_diag::Span;
    /// use pith_engine::Engine;
    ///
    /// let request = Request::<Mutation>::new(
    ///     "write",
    ///     Interface { inputs: Box::new([]), output: Type::Unit },
    ///     [],
    ///     Span::none(),
    /// );
    /// let mut engine = Engine::new();
    /// let _ = engine.evaluate(&request);
    /// ```
    pub fn evaluate(&mut self, request: &Request<Pure>) -> PithResult<Evaluation> {
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
                    let Some(rule) = self.rules.get(completed.rule) else {
                        return Err(internal_diag("pure evaluator lost selected rule metadata"));
                    };
                    let actual = value.value_type();
                    if actual != completed.request.interface.output {
                        return Err(one_diag(Diag::new(
                            Severity::Error,
                            StableCode::engine(104),
                            rule.span,
                            format!(
                                "rule `{}` returned {}, expected {}",
                                rule.label, actual, completed.request.interface.output
                            ),
                        )));
                    }
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
                    if stack.iter().any(|frame| {
                        frame.rule == child_rule
                            && frame.request.interface == child_request.interface
                            && frame.request.inputs == child_request.inputs
                    }) {
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

    fn resolve_rule(&self, request: &Request<Pure>) -> PithResult<RuleId> {
        request.validate_inputs().map_err(one_diag)?;
        select_rule(request, &self.rules)
            .into_result(request, &self.rules)
            .map_err(one_diag)
    }

    fn start_frame(&mut self, request: Request<Pure>, rule: RuleId) -> PithResult<EvalFrame> {
        let Some(body) = self.bodies.get(&rule) else {
            return Err(internal_diag("selected rule has no executable body"));
        };
        let body = body.start(&request.inputs);
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
        fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
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
        Rule::new(
            label,
            Interface {
                inputs: Box::new([]),
                output: Type::Int,
            },
            Span::none(),
        )
    }

    fn request(label: &str) -> Request {
        Request::new(
            label,
            Interface {
                inputs: Box::new([]),
                output: Type::Int,
            },
            [],
            Span::none(),
        )
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

    #[test]
    fn invalid_request_inputs_are_rejected_before_selection() {
        let mut engine = Engine::new();
        let request = Request::new(
            "thing",
            Interface {
                inputs: Box::new([Type::Int]),
                output: Type::Int,
            },
            [Value::Bool(true)],
            Span::none(),
        );

        let err = engine.evaluate(&request).unwrap_err();
        let diagnostics: Vec<_> = err.iter().collect();
        assert_eq!(diagnostics.first().unwrap().code, StableCode::engine(103));
    }

    #[test]
    fn result_must_match_the_rule_interface() {
        let mut engine = Engine::new();
        engine.register_rule(rule("thing"), UnitRule);

        let err = engine.evaluate(&request("thing")).unwrap_err();
        let diagnostics: Vec<_> = err.iter().collect();
        assert_eq!(diagnostics.first().unwrap().code, StableCode::engine(104));
    }
}
