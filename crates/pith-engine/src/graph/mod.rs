//! The arena dependency graph and the engine's evaluation entry points
//! (decisions 0021, 0022).
//!
//! The evaluator is split across three modules along the seam 0022 describes:
//! [`scheduler`] holds the set of in-flight evaluation chains and touches no
//! arena state, [`eval`] is the synchronous core that runs a chain on the step
//! machine, and [`drive`] is the shell that serves the effects a chain stops
//! for. This module owns the graph itself and the public surface over it.

mod action_pipeline;
mod capabilities;
mod diagnostics;
mod drive;
mod eval;
pub mod ir;
pub mod query;
mod reuse;
mod scheduler;
mod state_publish;

pub(crate) use capabilities::canonical_capabilities;
pub use ir::{
    ActionPlan, ActionRecord, AttemptState, ComputationKind, ComputationNode, DependencyEdge,
    Evaluation, EvaluationSource, LiveInvalidationExplanation, LiveInvalidationReason, PureRule,
    PureRuleFrame, PureStep, Resumption, ReuseDecision, ReuseReason, RuleSelection,
};
pub use query::EngineQuery;

use indexmap::IndexMap;
use pith_core::{Action, Pure, PureComputationKey, Request, Rule, RuleArena, RuleId};
use pith_diag::PithResult;
use pith_ids::{ComputationArena, ComputationId, ContentId};
use pith_store::{ContentStore, MemoryContentStore};

use crate::action::{ActionRule, Executor};
use crate::cancel::{CancelSignal, NeverCancelled};
use crate::policy::ActionPolicy;
use crate::runtime::{Runtime, RuntimeError};
use crate::state::{DurableAttemptId, EngineStateStore, MemoryEngineStateStore};
use eval::single_evaluation;
use ir::StopReason;
use reuse::PureComputationIndex;

pub struct Engine {
    pub(crate) rules: RuleArena<Rule<Pure>>,
    pub(crate) bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    pub(crate) action_rules: RuleArena<Rule<Action>>,
    pub(crate) action_bodies: IndexMap<RuleId, Box<dyn ActionRule>>,
    pub(crate) computations: ComputationArena<ComputationNode>,
    pure_computations: PureComputationIndex,
    pub(crate) store: Box<dyn ContentStore>,
    /// Durable engine metadata. Arena handles never cross this boundary; the
    /// process-local [`durable_attempts`] side-table maps computation nodes to
    /// their durable attempt identifiers. Store calls happen only at engine
    /// scheduling boundaries (decision 0024, "adapter boundaries").
    pub(crate) state_store: Box<dyn EngineStateStore>,
    pub(crate) durable_attempts: IndexMap<ComputationId, DurableAttemptId>,
}

impl Engine {
    pub fn new() -> Self {
        Self::with_content_store(MemoryContentStore::default())
    }

    pub fn with_content_store(store: impl ContentStore + 'static) -> Self {
        Self::with_state_store(store, MemoryEngineStateStore::default())
    }

    /// Build an engine with explicit content and engine-state adapters. The
    /// state store is shared, not moved, so callers (typically tests) can read
    /// the durable records the engine publishes.
    pub fn with_state_store(
        store: impl ContentStore + 'static,
        state_store: impl EngineStateStore + 'static,
    ) -> Self {
        Self {
            rules: RuleArena::new(),
            bodies: IndexMap::new(),
            action_rules: RuleArena::new(),
            action_bodies: IndexMap::new(),
            computations: ComputationArena::new(),
            pure_computations: IndexMap::new(),
            store: Box::new(store),
            state_store: Box::new(state_store),
            durable_attempts: IndexMap::new(),
        }
    }

    /// The durable engine-state adapter (decision 0024). Writes take `&self`,
    /// so this is the only accessor the adapter needs.
    pub fn state_store(&self) -> &dyn EngineStateStore {
        self.state_store.as_ref()
    }

    /// The durable attempt identifier the engine published for `computation`,
    /// if any. `None` for computations that never left `Pending` or that this
    /// engine instance did not allocate.
    pub fn durable_attempt_for(&self, computation: ComputationId) -> Option<DurableAttemptId> {
        self.durable_attempts.get(&computation).copied()
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
        let mut plan = self.open_roots(std::slice::from_ref(request))?;
        if let Err(diagnostics) = self.drive_pure(&mut plan.scheduler) {
            self.stop_live_frames(&plan.scheduler, &diagnostics, StopReason::Failed);
            return Err(diagnostics);
        }
        single_evaluation(plan.into_evaluations()?)
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
    ) -> Result<PithResult<Evaluation>, RuntimeError> {
        self.run_cancellable(request, runtime, policy, executor, &NeverCancelled)
    }

    /// [`Engine::run`], stoppable. `cancel` is polled at scheduling boundaries;
    /// when it reports cancellation the run stops, every computation it was
    /// still working on is recorded `Cancelled`, and the result is `E-1215`.
    ///
    /// # Errors
    /// The same as [`Engine::run`], plus `E-1215` when the run was cancelled.
    pub fn run_cancellable<R: Runtime, P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        request: &Request<Pure>,
        runtime: &R,
        policy: &P,
        executor: &E,
        cancel: &C,
    ) -> Result<PithResult<Evaluation>, RuntimeError> {
        let evaluations = runtime.block_on(self.run_inner(
            std::slice::from_ref(request),
            policy,
            executor,
            cancel,
        ))?;
        Ok(evaluations.and_then(single_evaluation))
    }

    /// Evaluate several requests that do not depend on one another, driving
    /// them concurrently: their actions overlap, and the results come back in
    /// request order. This is the multi-target entry point — one root per thing
    /// the caller asked to build.
    ///
    /// A request that fails aborts the whole run, exactly as it does under
    /// [`Engine::run`]; the diagnostics say which one.
    ///
    /// # Errors
    /// The same as [`Engine::run`].
    pub fn run_many<R: Runtime, P: ActionPolicy, E: Executor>(
        &mut self,
        requests: &[Request<Pure>],
        runtime: &R,
        policy: &P,
        executor: &E,
    ) -> Result<PithResult<Box<[Evaluation]>>, RuntimeError> {
        self.run_many_cancellable(requests, runtime, policy, executor, &NeverCancelled)
    }

    /// [`Engine::run_many`], stoppable. See [`Engine::run_cancellable`].
    ///
    /// # Errors
    /// The same as [`Engine::run_many`], plus `E-1215` when the run was
    /// cancelled.
    pub fn run_many_cancellable<R: Runtime, P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        requests: &[Request<Pure>],
        runtime: &R,
        policy: &P,
        executor: &E,
        cancel: &C,
    ) -> Result<PithResult<Box<[Evaluation]>>, RuntimeError> {
        runtime.block_on(self.run_inner(requests, policy, executor, cancel))
    }

    async fn run_inner<P: ActionPolicy, E: Executor, C: CancelSignal>(
        &mut self,
        requests: &[Request<Pure>],
        policy: &P,
        executor: &E,
        cancel: &C,
    ) -> PithResult<Box<[Evaluation]>> {
        let mut plan = self.open_roots(requests)?;
        // `drive_run` records whatever it was holding before returning, so
        // there is nothing left Pending to clean up here.
        self.drive_run(&mut plan.scheduler, policy, executor, cancel)
            .await?;
        plan.into_evaluations()
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
    use pith_core::{Interface, RuleIdentity, RuleRevision, Type, Value};
    use pith_diag::{EngineCode, Span};

    struct UnitRule;

    impl PureRule for UnitRule {
        fn start(&self, _inputs: &[Value]) -> Box<dyn PureRuleFrame> {
            Box::new(UnitFrame)
        }
    }

    struct UnitFrame;

    impl PureRuleFrame for UnitFrame {
        fn step(&mut self, _input: Option<Resumption>) -> PithResult<PureStep> {
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
