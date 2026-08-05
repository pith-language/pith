//! The arena dependency graph, the synchronous pure evaluator, and the async
//! driver that crosses the sync/async boundary for blob fetches and action
//! execution (decisions 0021, 0022).

mod capabilities;
pub mod ir;
pub mod query;
mod reuse;

pub use ir::{
    ActionPlan, ActionRecord, ComputationKind, ComputationNode, DependencyEdge, Evaluation,
    EvaluationSource, PureRule, PureRuleFrame, PureStep, ReuseDecision, ReuseReason, RuleSelection,
};
pub use query::EngineQuery;

use indexmap::IndexMap;
use pith_core::{
    Action, ActionInputContent, ActionOutputKind, Pure, Request, Rule, RuleArena, RuleId, Type,
    Value, select_rule,
};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_ids::{ComputationArena, ComputationId, ContentId};
use pith_store::{ContentStore, MemoryContentStore, Tree, TreeEntry, TreeEntryContent};
use smallvec::SmallVec;

use crate::action::{
    ActionExecution, ActionInvocation, ActionRule, CapturedActionExecution, CapturedOutput,
    CapturedOutputContent, CapturedTree, CapturedTreeEntryContent, ExecutionReport, Executor,
    MaterializedActionInput, MaterializedContent, MaterializedTree, MaterializedTreeEntry,
    MaterializedTreeEntryContent, ProducedOutput,
};
use crate::policy::{ActionAuthorization, ActionPolicy};
use crate::runtime::Runtime;
use capabilities::canonical_capabilities;
use ir::EvalFrame;
use reuse::PureComputationIndex;

pub struct Engine {
    pub(crate) rules: RuleArena<Rule<Pure>>,
    pub(crate) bodies: IndexMap<RuleId, Box<dyn PureRule>>,
    pub(crate) action_rules: RuleArena<Rule<Action>>,
    pub(crate) action_bodies: IndexMap<RuleId, Box<dyn ActionRule>>,
    pub(crate) computations: ComputationArena<ComputationNode>,
    pure_computations: PureComputationIndex,
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
                        self.run_action(&action_request, policy, executor).await?;
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
            capabilities: Box::new([]),
            reuse: ReuseDecision::Pending,
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
        let (reuse, capabilities) = match self.computations.get(completed.computation) {
            Some(node) => (
                self.pure_reuse_decision(&node.dependencies),
                self.effective_capabilities(&node.dependencies),
            ),
            None => return Err(internal_diag("pure evaluator lost a computation node")),
        };
        let Some(capabilities) = capabilities else {
            return Err(internal_diag(
                "pure evaluator lost a capability dependency computation",
            ));
        };
        let Some(node) = self.computations.get_mut(completed.computation) else {
            return Err(internal_diag("pure evaluator lost a computation node"));
        };
        node.result = Some(value.clone());
        node.capabilities = capabilities;
        node.reuse = reuse;
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

    async fn run_action<P: ActionPolicy, E: Executor>(
        &mut self,
        request: &Request<Action>,
        policy: &P,
        executor: &E,
    ) -> PithResult<(Value, ComputationId)> {
        let plan = self.plan_action(request)?;
        let rule = plan.rule;
        let authorization = policy.authorize(&plan);
        let denial = match &authorization {
            ActionAuthorization::Allowed { .. } => None,
            ActionAuthorization::Denied { policy, reason } => Some(one_diag(Diag::new(
                Severity::Error,
                StableCode::engine(213),
                request.span,
                format!("action denied by policy `{policy}`: {reason}"),
            ))),
        };

        let Some(action_rule) = self.action_rules.get(rule) else {
            return Err(internal_diag("selected action rule has no metadata"));
        };
        let declared_output = action_rule.interface.output.clone();
        let rule_span = action_rule.span;
        let rule_label = action_rule.label.clone();

        let computation = self.computations.push(ComputationNode {
            kind: ComputationKind::Action(request.clone()),
            rule,
            dependencies: SmallVec::new(),
            result: None,
            action: Some(ActionRecord {
                spec_digest: plan.spec_digest,
                spec: plan.spec.clone(),
                authorization,
                report: None,
            }),
            capabilities: canonical_capabilities(&plan.spec.capabilities),
            reuse: ReuseDecision::Pending,
        });

        if let Some(diagnostics) = denial {
            let Some(node) = self.computations.get_mut(computation) else {
                return Err(internal_diag(
                    "policy evaluator lost its action computation",
                ));
            };
            node.reuse = ReuseDecision::NotReusable(ReuseReason::PolicyDenied);
            return Err(diagnostics);
        }

        let invocation = self.materialize_action(&plan.spec)?;
        let captured = match executor.execute(&invocation).await {
            Ok(execution) => execution,
            Err(diagnostics) => {
                self.mark_action_failed(computation, None);
                return Err(diagnostics);
            }
        };
        let execution = match self.import_execution(captured) {
            Ok(execution) => execution,
            Err(diagnostics) => {
                self.mark_action_failed(computation, None);
                return Err(diagnostics);
            }
        };
        if let Err(diagnostics) = self.validate_execution(&plan.spec, &execution) {
            self.mark_action_failed(computation, Some(execution.report));
            return Err(diagnostics);
        }
        let Some(body) = self.action_bodies.get(&rule) else {
            self.mark_action_failed(computation, Some(execution.report));
            return Err(internal_diag("selected action rule has no body"));
        };
        let value = match body.complete(&request.inputs, &execution) {
            Ok(value) => value,
            Err(diagnostics) => {
                self.mark_action_failed(computation, Some(execution.report));
                return Err(diagnostics);
            }
        };
        if let Err(diagnostics) =
            validate_action_result(&value, &declared_output, &rule_label, rule_span)
        {
            self.mark_action_failed(computation, Some(execution.report));
            return Err(diagnostics);
        }

        let Some(node) = self.computations.get_mut(computation) else {
            return Err(internal_diag("action evaluator lost its computation node"));
        };
        node.result = Some(value.clone());
        node.reuse = ReuseDecision::NotReusable(ReuseReason::ActionCachingDisabled);
        let Some(action) = node.action.as_mut() else {
            return Err(internal_diag("action evaluator lost its action record"));
        };
        action.report = Some(execution.report);

        Ok((value, computation))
    }

    fn materialize_action(&self, spec: &pith_core::ActionSpec) -> PithResult<ActionInvocation> {
        let executable = self.materialize_content_by_id(spec.executable)?;
        let mut inputs = Vec::with_capacity(spec.inputs.len());
        for input in &spec.inputs {
            let content = match input.content {
                ActionInputContent::Blob(id) => self.materialize_blob(id)?,
                ActionInputContent::Tree(id) => {
                    MaterializedContent::Tree(self.materialize_tree(id)?)
                }
            };
            inputs.push(MaterializedActionInput {
                path: input.path.clone(),
                content,
            });
        }
        Ok(ActionInvocation {
            spec: spec.clone(),
            executable,
            inputs: inputs.into_boxed_slice(),
        })
    }

    fn materialize_content_by_id(&self, id: ContentId) -> PithResult<MaterializedContent> {
        if let Some(blob) = self.store.get_blob(id).map_err(store_error_diag)? {
            return Ok(MaterializedContent::Blob {
                id,
                bytes: blob.as_bytes().to_vec().into_boxed_slice(),
            });
        }
        if self.store.get_tree(id).map_err(store_error_diag)?.is_some() {
            return Ok(MaterializedContent::Tree(self.materialize_tree(id)?));
        }
        Err(content_unavailable_diag(id))
    }

    fn materialize_blob(&self, id: ContentId) -> PithResult<MaterializedContent> {
        match self.store.get_blob(id).map_err(store_error_diag)? {
            Some(blob) => Ok(MaterializedContent::Blob {
                id,
                bytes: blob.as_bytes().to_vec().into_boxed_slice(),
            }),
            None => Err(content_unavailable_diag(id)),
        }
    }

    fn materialize_tree(&self, id: ContentId) -> PithResult<MaterializedTree> {
        let tree = match self.store.get_tree(id).map_err(store_error_diag)? {
            Some(tree) => tree,
            None => return Err(content_unavailable_diag(id)),
        };
        let mut entries = Vec::with_capacity(tree.entries().len());
        for entry in tree.entries() {
            let content = match entry.content() {
                TreeEntryContent::File {
                    content,
                    executable,
                } => {
                    let MaterializedContent::Blob { bytes, .. } =
                        self.materialize_blob(*content)?
                    else {
                        return Err(internal_diag("tree file content materialized as a tree"));
                    };
                    MaterializedTreeEntryContent::File {
                        content: *content,
                        executable: *executable,
                        bytes,
                    }
                }
                TreeEntryContent::Tree(child) => {
                    MaterializedTreeEntryContent::Tree(self.materialize_tree(*child)?)
                }
                TreeEntryContent::Symlink { target } => MaterializedTreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(MaterializedTreeEntry {
                name: entry.name().into(),
                content,
            });
        }
        Ok(MaterializedTree {
            id,
            entries: entries.into_boxed_slice(),
        })
    }

    fn import_execution(
        &mut self,
        captured: CapturedActionExecution,
    ) -> PithResult<ActionExecution> {
        let mut outputs = Vec::with_capacity(captured.report.outputs.len());
        for output in &captured.report.outputs {
            outputs.push(ProducedOutput {
                path: output.path.clone(),
                kind: output.kind,
                content: self.import_output(output)?,
            });
        }
        Ok(ActionExecution {
            report: ExecutionReport {
                executor: captured.report.executor,
                platform: captured.report.platform,
                access: captured.report.access,
                outputs: outputs.into_boxed_slice(),
                capabilities_used: captured.report.capabilities_used,
            },
        })
    }

    fn import_output(&mut self, output: &CapturedOutput) -> PithResult<ContentId> {
        match (&output.kind, &output.content) {
            (ActionOutputKind::Blob, CapturedOutputContent::Blob(bytes)) => {
                self.store.put_blob(bytes).map_err(store_error_diag)
            }
            (ActionOutputKind::Tree, CapturedOutputContent::Tree(tree)) => self.import_tree(tree),
            (ActionOutputKind::Blob, CapturedOutputContent::Tree(_))
            | (ActionOutputKind::Tree, CapturedOutputContent::Blob(_)) => {
                Err(wrong_output_kind_diag(&output.path))
            }
        }
    }

    fn import_tree(&mut self, tree: &CapturedTree) -> PithResult<ContentId> {
        let mut entries = Vec::with_capacity(tree.entries.len());
        for entry in &tree.entries {
            let content = match &entry.content {
                CapturedTreeEntryContent::File { bytes, executable } => {
                    let content = self.store.put_blob(bytes).map_err(store_error_diag)?;
                    TreeEntryContent::File {
                        content,
                        executable: *executable,
                    }
                }
                CapturedTreeEntryContent::Tree(tree) => {
                    TreeEntryContent::Tree(self.import_tree(tree)?)
                }
                CapturedTreeEntryContent::Symlink { target } => TreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(TreeEntry::new(entry.name.clone(), content).map_err(store_error_diag)?);
        }
        let tree = Tree::new(entries).map_err(store_error_diag)?;
        self.store.put_tree(tree).map_err(store_error_diag)
    }

    fn validate_execution(
        &self,
        spec: &pith_core::ActionSpec,
        execution: &ActionExecution,
    ) -> PithResult<()> {
        validate_execution_platform(spec, &execution.report.platform)?;

        for used in &execution.report.capabilities_used {
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

        for produced in &execution.report.outputs {
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
                .report
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

    fn mark_action_failed(
        &mut self,
        computation: ComputationId,
        report: Option<crate::ExecutionReport>,
    ) {
        if let Some(node) = self.computations.get_mut(computation) {
            node.reuse = ReuseDecision::NotReusable(ReuseReason::FailedExecution);
            if let Some(action) = node.action.as_mut() {
                action.report = report;
            }
        }
    }
}

fn validate_execution_platform(
    spec: &pith_core::ActionSpec,
    actual: &crate::ExecutionPlatform,
) -> PithResult<()> {
    if actual.operating_system.is_empty() || actual.architecture.is_empty() {
        return Err(one_diag(Diag::new(
            Severity::Error,
            StableCode::engine(212),
            Span::none(),
            "executor did not report a concrete execution platform",
        )));
    }

    match &spec.platform {
        pith_core::PlatformRequirement::Exact {
            operating_system,
            architecture,
        } if operating_system != &actual.operating_system
            || architecture != &actual.architecture =>
        {
            Err(one_diag(Diag::new(
                Severity::Error,
                StableCode::engine(212),
                Span::none(),
                format!(
                    "executor selected platform `{}-{}`, expected `{}-{}`",
                    actual.operating_system, actual.architecture, operating_system, architecture
                ),
            )))
        }
        _ => Ok(()),
    }
}

fn wrong_output_kind_diag(path: &str) -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(209),
        Span::none(),
        format!("executor reported output `{path}` with the wrong kind"),
    ))
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

fn content_unavailable_diag(id: ContentId) -> DiagnosticSink {
    one_diag(Diag::new(
        Severity::Error,
        StableCode::engine(205),
        Span::none(),
        format!("content {id:?} is not available locally"),
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
