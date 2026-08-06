//! The arena dependency graph, the synchronous pure evaluator, and the async
//! driver that crosses the sync/async boundary for blob fetches and action
//! execution (decisions 0021, 0022).

mod action_pipeline;
mod capabilities;
mod diagnostics;
pub mod ir;
pub mod query;
mod reuse;

pub use ir::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, DependencyEdge,
    Evaluation, EvaluationSource, PureRule, PureRuleFrame, PureStep, ReuseDecision, ReuseReason,
    RuleSelection,
};
pub use query::EngineQuery;

use indexmap::IndexMap;
use pith_core::{
    Action, Pure, PureComputationKey, Request, Rule, RuleArena, RuleId, Value, select_rule,
};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult};
use pith_ids::{ComputationArena, ComputationId, ContentId};
use pith_store::{ContentStore, MemoryContentStore};
use smallvec::SmallVec;

use crate::action::{ActionRule, Executor};
use crate::policy::ActionPolicy;
use crate::runtime::Runtime;
use diagnostics::{
    InternalInvariant, content_unavailable_diag, cycle_diag, effectful_in_pure_diag, internal_diag,
    one_diag, store_error_diag,
};
use ir::EvalFrame;
use reuse::PureComputationIndex;

pub(super) enum ActionRunOutcome {
    PlanningFailed(DiagnosticSink),
    Started {
        computation: ComputationId,
        result: PithResult<Value>,
    },
}

pub struct Engine {
    pub(crate) rules: RuleArena<Rule<Pure>>,
    pub(crate) bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    pub(crate) action_rules: RuleArena<Rule<Action>>,
    pub(crate) action_bodies: IndexMap<RuleId, Box<dyn ActionRule>>,
    pub(crate) computations: ComputationArena<ComputationNode>,
    pure_computations: PureComputationIndex,
    pub(crate) store: Box<dyn ContentStore>,
}

impl Engine {
    pub fn new() -> Self {
        Self::with_content_store(MemoryContentStore::default())
    }

    pub fn with_content_store(store: impl ContentStore + 'static) -> Self {
        Self {
            rules: RuleArena::new(),
            bodies: IndexMap::new(),
            action_rules: RuleArena::new(),
            action_bodies: IndexMap::new(),
            computations: ComputationArena::new(),
            pure_computations: IndexMap::new(),
            store: Box::new(store),
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

        let result = self.drive_pure(&mut stack);
        if let Err(diagnostics) = &result {
            self.fail_pending_frames(&stack, diagnostics);
        }
        result
    }

    fn drive_pure(&mut self, stack: &mut Vec<EvalFrame>) -> PithResult<Evaluation> {
        loop {
            let step = self.step_top_frame(stack)?;
            match step {
                PureStep::Complete(value) => {
                    let completed = self.finish_frame(stack, value)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(completed.value.clone());
                    } else {
                        return Ok(completed);
                    }
                }
                PureStep::Need(child_request) => {
                    self.handle_pure_need(stack, child_request)?;
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
    /// `executor` via `runtime` after `policy` authorizes the action plan.
    ///
    /// # Errors
    /// `Ok(Ok(_))` on success. `Ok(Err(_))` when evaluation produced
    /// diagnostics (same codes as [`Engine::evaluate_pure`] plus `E-1205`
    /// blob-not-available, `E-1208` through `E-1210` for executor reports
    /// outside the declared contract, and `E-1212` for a missing or mismatched
    /// execution platform, and `E-1213` for policy denial). `Err(_)` when the
    /// runtime could not be driven.
    pub fn run<R: Runtime, P: ActionPolicy, E: Executor>(
        &mut self,
        request: &Request<Pure>,
        runtime: &R,
        policy: &P,
        executor: &E,
    ) -> Result<PithResult<Evaluation>, crate::runtime::RuntimeError> {
        runtime.block_on(self.run_inner(request, policy, executor))
    }

    async fn run_inner<P: ActionPolicy, E: Executor>(
        &mut self,
        request: &Request<Pure>,
        policy: &P,
        executor: &E,
    ) -> PithResult<Evaluation> {
        let rule = self.resolve_pure_rule(request)?;
        if let Some(evaluation) = self.reusable_pure_evaluation(rule, request) {
            return Ok(evaluation);
        }
        let root = self.start_frame(request.clone(), rule)?;
        let mut stack = vec![root];

        let result = self.drive_run(&mut stack, policy, executor).await;
        if let Err(diagnostics) = &result {
            self.fail_pending_frames(&stack, diagnostics);
        }
        result
    }

    async fn drive_run<P: ActionPolicy, E: Executor>(
        &mut self,
        stack: &mut Vec<EvalFrame>,
        policy: &P,
        executor: &E,
    ) -> PithResult<Evaluation> {
        loop {
            let step = self.step_top_frame(stack)?;
            match step {
                PureStep::Complete(value) => {
                    let completed = self.finish_frame(stack, value)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(completed.value.clone());
                    } else {
                        return Ok(completed);
                    }
                }
                PureStep::Need(child_request) => {
                    self.handle_pure_need(stack, child_request)?;
                }
                PureStep::NeedBlob(id) => {
                    let bytes = self.fetch_blob(id)?;
                    self.record_blob_edge(stack, id);
                    if let Some(parent) = stack.last_mut() {
                        parent.resume_with = Some(Value::Bytes(bytes));
                    } else {
                        return Err(internal_diag(InternalInvariant::EffectfulStepWithNoFrame(
                            "blob",
                        )));
                    }
                }
                PureStep::NeedAction(action_request) => {
                    let outcome = self.run_action(&action_request, policy, executor).await;
                    let (action_computation, result) = match outcome {
                        ActionRunOutcome::PlanningFailed(diagnostics) => return Err(diagnostics),
                        ActionRunOutcome::Started {
                            computation,
                            result,
                        } => (computation, result),
                    };
                    let Some(parent) = stack.last() else {
                        return Err(internal_diag(InternalInvariant::EffectfulStepWithNoFrame(
                            "action",
                        )));
                    };
                    let Some(parent_node) = self.computations.get_mut(parent.computation) else {
                        return Err(internal_diag(InternalInvariant::PureLostParentComputation));
                    };
                    parent_node.dependencies.push(DependencyEdge::Action {
                        computation: action_computation,
                        request: action_request,
                    });
                    let value = result?;
                    let Some(parent) = stack.last_mut() else {
                        return Err(internal_diag(InternalInvariant::EffectfulStepWithNoFrame(
                            "action",
                        )));
                    };
                    parent.resume_with = Some(value);
                }
            }
        }
    }

    fn step_top_frame(&self, stack: &mut [EvalFrame]) -> PithResult<PureStep> {
        match stack.last_mut() {
            Some(frame) => frame.body.step(frame.resume_with.take()),
            None => Err(internal_diag(InternalInvariant::PureLostRootFrame)),
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
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoBody));
        };
        let body = body.start(&request.inputs);
        let Some(rule_metadata) = self.rules.get(rule) else {
            return Err(internal_diag(InternalInvariant::SelectedRuleHasNoMetadata));
        };
        let key = PureComputationKey::new(rule_metadata, &request);
        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Pure(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            state: AttemptState::Pending,
            action: None,
            capabilities: Box::new([]),
        });
        self.index_pure_computation(key, computation);
        Ok(EvalFrame {
            computation,
            rule,
            request,
            body,
            resume_with: None,
        })
    }

    fn finish_frame(&mut self, stack: &mut Vec<EvalFrame>, value: Value) -> PithResult<Evaluation> {
        let Some(completed) = stack.last() else {
            return Err(internal_diag(InternalInvariant::PureCompletedWithoutFrame));
        };
        let Some(rule) = self.rules.get(completed.rule) else {
            return Err(internal_diag(
                InternalInvariant::PureLostSelectedRuleMetadata,
            ));
        };
        if !value.is_type(&completed.request.interface.output) {
            let actual = value.value_type();
            return Err(one_diag(Diag::engine(
                EngineCode::ResultTypeMismatch,
                rule.span,
                format!(
                    "rule `{}` returned {}, expected {}",
                    rule.label, actual, completed.request.interface.output
                ),
            )));
        }
        let (reuse, capabilities) = match self.computations.get(completed.computation) {
            Some(node) => (
                self.pure_reuse_decision(&node.dependencies),
                self.effective_capabilities(&node.dependencies),
            ),
            None => return Err(internal_diag(InternalInvariant::PureLostComputationNode)),
        };
        let Some(capabilities) = capabilities else {
            return Err(internal_diag(
                InternalInvariant::PureLostCapabilityComputation,
            ));
        };
        let Some(node) = self.computations.get_mut(completed.computation) else {
            return Err(internal_diag(InternalInvariant::PureLostComputationNode));
        };
        node.state = AttemptState::Complete {
            result: value.clone(),
            reuse,
        };
        node.capabilities = capabilities;
        let computation = completed.computation;
        stack.pop();
        Ok(Evaluation {
            value,
            computation,
            source: EvaluationSource::Computed,
        })
    }

    fn fail_pending_frames(&mut self, stack: &[EvalFrame], diagnostics: &DiagnosticSink) {
        for frame in stack {
            let Some(node) = self.computations.get_mut(frame.computation) else {
                continue;
            };
            if matches!(node.state, AttemptState::Pending) {
                node.state = AttemptState::Failed {
                    diagnostics: diagnostics.iter().cloned().collect(),
                };
            }
        }
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
                return Err(internal_diag(InternalInvariant::PureLostRequestingFrame));
            };
            let Some(parent_node) = self.computations.get_mut(parent.computation) else {
                return Err(internal_diag(InternalInvariant::PureLostParentComputation));
            };
            parent_node.dependencies.push(DependencyEdge::Request {
                computation: reused.computation,
                request: child_request,
            });
            let Some(parent) = stack.last_mut() else {
                return Err(internal_diag(InternalInvariant::PureLostRequestingFrame));
            };
            parent.resume_with = Some(reused.value);
            return Ok(());
        }

        let child = self.start_frame(child_request.clone(), child_rule)?;
        let Some(parent) = stack.last() else {
            return Err(internal_diag(InternalInvariant::PureLostRequestingFrame));
        };
        let Some(parent_node) = self.computations.get_mut(parent.computation) else {
            return Err(internal_diag(InternalInvariant::PureLostParentComputation));
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
            None => Err(content_unavailable_diag(id)),
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
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_core::{Interface, RuleIdentity, RuleRevision, Type};
    use pith_diag::Span;

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
        let identity = RuleIdentity::of_module_declaration("engine-graph-tests", label);
        let revision = RuleRevision::of_manifest(identity, b"engine-graph-tests-v1");
        Rule::new(
            revision,
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
        assert_eq!(
            diags.first().unwrap().code,
            pith_diag::StableCode::from(EngineCode::AmbiguousRule)
        );
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
        assert_eq!(
            diagnostics.first().unwrap().code,
            pith_diag::StableCode::from(EngineCode::RequestInputsMismatch)
        );
    }

    #[test]
    fn result_must_match_the_rule_interface() {
        let mut engine = Engine::new();
        engine.register_rule(rule("thing"), UnitRule);

        let err = engine.evaluate_pure(&request("thing")).unwrap_err();
        let diagnostics: Vec<_> = err.iter().collect();
        assert_eq!(
            diagnostics.first().unwrap().code,
            pith_diag::StableCode::from(EngineCode::ResultTypeMismatch)
        );
    }
}
