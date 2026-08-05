//! The arena dependency graph, the synchronous pure evaluator, and the async
//! driver that crosses the sync/async boundary for blob fetches and action
//! execution (decisions 0021, 0022).

pub mod ir;
pub mod query;
mod reuse;

pub use ir::{
    ActionPlan, ActionRecord, ComputationKind, ComputationNode, DependencyEdge, Evaluation,
    EvaluationSource, PureRule, PureRuleFrame, PureStep, RuleSelection,
};
pub use query::EngineQuery;

use indexmap::IndexMap;
use pith_core::{Action, Pure, Request, Rule, RuleArena, RuleId, Type, Value, select_rule};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_ids::{ComputationArena, ComputationId, ContentId};
use pith_store::{ContentStore, MemoryContentStore};
use smallvec::SmallVec;

use crate::action::{ActionExecution, ActionRule, Executor};
use crate::runtime::Runtime;
use ir::EvalFrame;
use reuse::{ActionComputationIndex, PureComputationIndex};

pub struct Engine {
    pub(crate) rules: RuleArena<Rule<Pure>>,
    pub(crate) bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    pub(crate) action_rules: RuleArena<Rule<Action>>,
    pub(crate) action_bodies: IndexMap<RuleId, Box<dyn ActionRule>>,
    pub(crate) computations: ComputationArena<ComputationNode>,
    pure_computations: PureComputationIndex,
    action_computations: ActionComputationIndex,
    pub(crate) store: MemoryContentStore,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            rules: RuleArena::new(),
            bodies: IndexMap::new(),
            action_rules: RuleArena::new(),
            action_bodies: IndexMap::new(),
            computations: ComputationArena::new(),
            pure_computations: IndexMap::new(),
            action_computations: IndexMap::new(),
            store: MemoryContentStore::default(),
        }
    }

    /// Register a pure rule together with its executable body.
    pub fn register_rule<B>(&mut self, rule: Rule<Pure>, body: B) -> RuleId
    where
        B: PureRule + 'static,
    {
        let id = self.rules.push(rule);
        self.bodies.insert(id, Box::new(body));
        id
    }

    /// Register an action rule together with its deterministic planner.
    pub fn register_action_rule<B>(&mut self, rule: Rule<Action>, body: B) -> RuleId
    where
        B: ActionRule + 'static,
    {
        let id = self.action_rules.push(rule);
        self.action_bodies.insert(id, Box::new(body));
        id
    }

    /// Insert a blob into the engine's content store and return its identity.
    ///
    /// # Errors
    /// Returns the store's error if the adapter cannot store the bytes. The
    /// in-memory adapter is infallible; filesystem/remote adapters may fail.
    pub fn put_blob(&mut self, bytes: &[u8]) -> Result<ContentId, pith_store::StoreError> {
        self.store.put_blob(bytes)
    }

    /// Evaluate a request on the synchronous pure step machine. Rejects
    /// effectful steps (`NeedBlob`, `NeedAction`) with `E-1206`: this entry
    /// point is for pure-only computations and keeps the sync core honest.
    /// Use [`Engine::run`] to cross the sync/async boundary.
    ///
    /// # Errors
    /// `E-1101` no match, `E-1102` ambiguous, `E-1103` bad inputs, `E-1104`
    /// result type mismatch, `E-1203` cycle, `E-1206` effectful step in a
    /// pure-only evaluation, plus any diagnostic emitted by a rule body.
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
    /// let _ = engine.evaluate_pure(&request);
    /// ```
    pub fn evaluate_pure(&mut self, request: &Request<Pure>) -> PithResult<Evaluation> {
        let rule = self.resolve_pure_rule(request)?;
        if let Some(evaluation) = self.reusable_pure_evaluation(rule, request) {
            return Ok(evaluation);
        }
        let root = self.start_frame(request.clone(), rule)?;
        let mut stack = vec![root];

        loop {
            let step = self.step_top_frame(&mut stack)?;
            match step {
                PureStep::Complete(value) => {
                    let completed = self.finish_frame(&mut stack, value)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(completed.value.clone());
                    } else {
                        return Ok(completed);
                    }
                }
                PureStep::Need(child_request) => {
                    self.handle_pure_need(&mut stack, child_request)?;
                }
                PureStep::NeedBlob(_) | PureStep::NeedAction(_) => {
                    return Err(effectful_in_pure_diag());
                }
            }
        }
    }

    /// Evaluate a request, crossing the sync/async boundary as needed: pure
    /// steps run synchronously inside the loop, blob fetches hit the content
    /// store synchronously, and declared action contracts are driven through
    /// `executor` via `runtime`.
    ///
    /// # Errors
    /// `Ok(Ok(_))` on success. `Ok(Err(_))` when evaluation produced
    /// diagnostics (same codes as [`Engine::evaluate_pure`] plus `E-1205`
    /// blob-not-available and `E-1208` through `E-1210` for executor evidence
    /// outside the declared contract). `Err(_)` when the runtime could not be
    /// driven.
    pub fn run<R: Runtime, E: Executor>(
        &mut self,
        request: &Request<Pure>,
        runtime: &R,
        executor: &E,
    ) -> Result<PithResult<Evaluation>, crate::runtime::RuntimeError> {
        runtime.block_on(self.run_inner(request, executor))
    }

    async fn run_inner<E: Executor>(
        &mut self,
        request: &Request<Pure>,
        executor: &E,
    ) -> PithResult<Evaluation> {
        let rule = self.resolve_pure_rule(request)?;
        if let Some(evaluation) = self.reusable_pure_evaluation(rule, request) {
            return Ok(evaluation);
        }
        let root = self.start_frame(request.clone(), rule)?;
        let mut stack = vec![root];

        loop {
            let step = self.step_top_frame(&mut stack)?;
            match step {
                PureStep::Complete(value) => {
                    let completed = self.finish_frame(&mut stack, value)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(completed.value.clone());
                    } else {
                        return Ok(completed);
                    }
                }
                PureStep::Need(child_request) => {
                    self.handle_pure_need(&mut stack, child_request)?;
                }
                PureStep::NeedBlob(id) => {
                    let bytes = self.fetch_blob(id)?;
                    self.record_blob_edge(&stack, id);
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(Value::Bytes(bytes));
                    } else {
                        return Err(internal_diag("blob requested with no frame on the stack"));
                    }
                }
                PureStep::NeedAction(action_request) => {
                    let (value, action_computation) =
                        self.run_action(&action_request, executor).await?;
                    let Some(parent) = stack.last() else {
                        return Err(internal_diag("action requested with no frame on the stack"));
                    };
                    let Some(parent_node) = self.computations.get_mut(parent.computation) else {
                        return Err(internal_diag("pure evaluator lost a parent computation"));
                    };
                    parent_node.dependencies.push(DependencyEdge::Action {
                        computation: action_computation,
                        request: action_request,
                    });
                    let Some(parent) = stack.last_mut() else {
                        return Err(internal_diag("action requested with no frame on the stack"));
                    };
                    parent.resume_with = Some(value);
                }
            }
        }
    }

    fn step_top_frame(&self, stack: &mut [EvalFrame]) -> PithResult<PureStep> {
        match stack.last_mut() {
            Some(frame) => frame.body.step(frame.resume_with.take()),
            None => Err(internal_diag("pure evaluator lost its root frame")),
        }
    }

    fn resolve_pure_rule(&self, request: &Request<Pure>) -> PithResult<RuleId> {
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
            kind: ComputationKind::Pure(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            result: None,
            action: None,
            is_reusable: false,
        });
        self.index_pure_computation(rule, &request, computation);
        Ok(EvalFrame {
            computation,
            rule,
            request,
            body,
            resume_with: None,
        })
    }

    fn finish_frame(&mut self, stack: &mut Vec<EvalFrame>, value: Value) -> PithResult<Evaluation> {
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
        let dependencies_are_reusable = match self.computations.get(completed.computation) {
            Some(node) => self.pure_dependencies_are_reusable(&node.dependencies),
            None => return Err(internal_diag("pure evaluator lost a computation node")),
        };
        let Some(node) = self.computations.get_mut(completed.computation) else {
            return Err(internal_diag("pure evaluator lost a computation node"));
        };
        node.result = Some(value.clone());
        node.is_reusable = dependencies_are_reusable;
        Ok(Evaluation {
            value,
            computation: completed.computation,
            source: EvaluationSource::Computed,
        })
    }

    /// Handle a `PureStep::Need`: select the child rule, cycle-check, allocate
    /// the child frame, and record the dependency edge on the parent node.
    fn handle_pure_need(
        &mut self,
        stack: &mut Vec<EvalFrame>,
        child_request: Request<Pure>,
    ) -> PithResult<()> {
        let child_rule = self.resolve_pure_rule(&child_request)?;
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

        if let Some(reused) = self.reusable_pure_evaluation(child_rule, &child_request) {
            let Some(parent) = stack.last() else {
                return Err(internal_diag("pure evaluator lost a requesting frame"));
            };
            let Some(parent_node) = self.computations.get_mut(parent.computation) else {
                return Err(internal_diag("pure evaluator lost a parent computation"));
            };
            parent_node.dependencies.push(DependencyEdge::Request {
                computation: reused.computation,
                request: child_request,
            });
            let Some(parent) = stack.last_mut() else {
                return Err(internal_diag("pure evaluator lost a requesting frame"));
            };
            parent.resume_with = Some(reused.value);
            return Ok(());
        }

        let child = self.start_frame(child_request.clone(), child_rule)?;
        let Some(parent) = stack.last() else {
            return Err(internal_diag("pure evaluator lost a requesting frame"));
        };
        let Some(parent_node) = self.computations.get_mut(parent.computation) else {
            return Err(internal_diag("pure evaluator lost a parent computation"));
        };
        parent_node.dependencies.push(DependencyEdge::Request {
            computation: child.computation,
            request: child_request,
        });
        stack.push(child);
        Ok(())
    }

    fn fetch_blob(&self, id: ContentId) -> PithResult<Box<[u8]>> {
        match self.store.get_blob(id).map_err(store_error_diag)? {
            Some(blob) => Ok(blob.as_bytes().to_vec().into_boxed_slice()),
            None => Err(one_diag(Diag::new(
                Severity::Error,
                StableCode::engine(205),
                Span::none(),
                format!("content {id:?} is not available locally"),
            ))),
        }
    }

    fn record_blob_edge(&mut self, stack: &[EvalFrame], id: ContentId) {
        let Some(parent) = stack.last() else {
            return;
        };
        let Some(parent_node) = self.computations.get_mut(parent.computation) else {
            return;
        };
        parent_node.dependencies.push(DependencyEdge::Blob { id });
    }

    pub(crate) fn plan_action(&self, request: &Request<Action>) -> PithResult<ActionPlan> {
        request.validate_inputs().map_err(one_diag)?;
        let rule = select_rule(request, &self.action_rules)
            .into_result(request, &self.action_rules)
            .map_err(one_diag)?;
        let Some(body) = self.action_bodies.get(&rule) else {
            return Err(internal_diag("selected action rule has no body"));
        };
        let spec = body.plan(&request.inputs)?;
        let spec_digest = spec.digest().map_err(one_diag)?;
        Ok(ActionPlan {
            rule,
            spec_digest,
            spec,
        })
    }

    async fn run_action<E: Executor>(
        &mut self,
        request: &Request<Action>,
        executor: &E,
    ) -> PithResult<(Value, ComputationId)> {
        let plan = self.plan_action(request)?;
        let rule = plan.rule;

        let Some(action_rule) = self.action_rules.get(rule) else {
            return Err(internal_diag("selected action rule has no metadata"));
        };
        let declared_output = action_rule.interface.output.clone();
        let rule_span = action_rule.span;
        let rule_label = action_rule.label.clone();

        if let Some((value, computation)) = self.reusable_action_result(plan.spec_digest) {
            validate_action_result(&value, &declared_output, &rule_label, rule_span)?;
            return Ok((value, computation));
        }

        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Action(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            result: None,
            action: Some(ActionRecord {
                spec_digest: plan.spec_digest,
                spec: plan.spec.clone(),
                evidence: None,
            }),
            is_reusable: false,
        });

        let execution = executor.execute(&plan.spec).await?;
        self.validate_execution(&plan.spec, &execution)?;
        let Some(body) = self.action_bodies.get(&rule) else {
            return Err(internal_diag("selected action rule has no body"));
        };
        let value = body.complete(&request.inputs, &execution)?;
        validate_action_result(&value, &declared_output, &rule_label, rule_span)?;
        let is_reusable = execution.evidence.contract == crate::ContractVerification::Enforced;

        let Some(node) = self.computations.get_mut(computation) else {
            return Err(internal_diag("action evaluator lost its computation node"));
        };
        node.result = Some(value.clone());
        node.is_reusable = is_reusable;
        let Some(action) = node.action.as_mut() else {
            return Err(internal_diag("action evaluator lost its action record"));
        };
        action.evidence = Some(execution.evidence);
        if is_reusable {
            self.index_action_computation(plan.spec_digest, computation);
        }

        Ok((value, computation))
    }

    fn validate_execution(
        &self,
        spec: &pith_core::ActionSpec,
        execution: &ActionExecution,
    ) -> PithResult<()> {
        for used in &execution.evidence.capabilities_used {
            if !spec.capabilities.contains(used) {
                return Err(one_diag(Diag::new(
                    Severity::Error,
                    StableCode::engine(208),
                    Span::none(),
                    format!(
                        "executor reported undeclared capability `{}` scoped to `{}`",
                        used.name, used.scope
                    ),
                )));
            }
        }

        for produced in &execution.evidence.outputs {
            let declared = spec
                .outputs
                .iter()
                .any(|output| output.path == produced.path && output.kind == produced.kind);
            if !declared {
                return Err(one_diag(Diag::new(
                    Severity::Error,
                    StableCode::engine(209),
                    Span::none(),
                    format!("executor reported undeclared output `{}`", produced.path),
                )));
            }
        }

        for declared in &spec.outputs {
            let produced = execution
                .evidence
                .outputs
                .iter()
                .any(|output| output.path == declared.path && output.kind == declared.kind);
            if !produced {
                return Err(one_diag(Diag::new(
                    Severity::Error,
                    StableCode::engine(210),
                    Span::none(),
                    format!(
                        "executor did not produce declared output `{}`",
                        declared.path
                    ),
                )));
            }
        }

        Ok(())
    }
}

fn validate_action_result(
    value: &Value,
    declared_output: &Type,
    rule_label: &str,
    rule_span: Span,
) -> PithResult<()> {
    let actual = value.value_type();
    if actual != *declared_output {
        return Err(one_diag(Diag::new(
            Severity::Error,
            StableCode::engine(104),
            rule_span,
            format!("action `{rule_label}` returned {actual}, expected {declared_output}"),
        )));
    }
    Ok(())
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

fn effectful_in_pure_diag() -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(206),
        Span::none(),
        "effectful step (NeedBlob/NeedAction) in a pure-only evaluation; use Engine::run",
    ))
}

fn one_diag(diag: Diag) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(diag);
    sink
}

fn store_error_diag(error: pith_store::StoreError) -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(207),
        Span::none(),
        format!("content store error: {error}"),
    ))
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
        assert!(engine.evaluate_pure(&request("missing")).is_err());
    }

    #[test]
    fn two_matches_is_ambiguous() {
        let mut engine = Engine::new();
        engine.register_rule(rule("thing"), UnitRule);
        engine.register_rule(rule("thing"), UnitRule);
        let err = engine.evaluate_pure(&request("thing")).unwrap_err();
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

        let err = engine.evaluate_pure(&request).unwrap_err();
        let diagnostics: Vec<_> = err.iter().collect();
        assert_eq!(diagnostics.first().unwrap().code, StableCode::engine(103));
    }

    #[test]
    fn result_must_match_the_rule_interface() {
        let mut engine = Engine::new();
        engine.register_rule(rule("thing"), UnitRule);

        let err = engine.evaluate_pure(&request("thing")).unwrap_err();
        let diagnostics: Vec<_> = err.iter().collect();
        assert_eq!(diagnostics.first().unwrap().code, StableCode::engine(104));
    }
}
