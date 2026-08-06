//! The action lifecycle: plan, authorize, materialize, execute, import,
//! validate, and record provenance for declared actions.
//!
//! These methods live on [`Engine`] but are separated from the pure step
//! machine for readability. They cross the sync/async boundary at the executor
//! call (decision 0022): planning, materialization, import, and validation are
//! synchronous; only `executor.execute` is awaited.

use pith_core::{Action, ActionSpec, Content, Request, RuleId, Type, Value, select_rule};
use pith_diag::{Diag, DiagnosticSink, EngineCode, PithResult, Span};
use pith_ids::{ComputationId, ContentId};
use pith_store::{Tree, TreeEntry, TreeEntryContent};
use smallvec::SmallVec;

use super::ir::{
    ActionPlan, ActionRecord, ComputationKind, ComputationNode, DependencyEdge, ReuseDecision,
    ReuseReason,
};
use super::{ActionRunOutcome, Engine};
use crate::action::{
    ActionExecution, ActionInvocation, CapturedActionExecution, CapturedFileContent,
    CapturedOutputContent, CapturedTreeEntryContent, ExecutionReport, Executor,
    MaterializedActionInput, MaterializedBlob, MaterializedContent, MaterializedFileContent,
    MaterializedTree, MaterializedTreeEntryContent, ProducedOutput,
};
use crate::graph::capabilities::canonical_capabilities;
use crate::graph::diagnostics::{
    InternalInvariant, content_unavailable_diag, internal_diag, one_diag, store_error_diag,
    validate_action_result, validate_execution_platform,
};
use crate::policy::{ActionAuthorization, ActionPolicy};

/// Metadata copied out of the selected action rule for the duration of one
/// execution. Bundling these keeps `run_action_body`'s argument list small.
struct ActionRuleMeta {
    rule: RuleId,
    declared_output: Type,
    span: Span,
    label: Box<str>,
}

impl Engine {
    pub(super) fn plan_action(&self, request: &Request<Action>) -> PithResult<ActionPlan> {
        request.validate_inputs().map_err(one_diag)?;
        let rule = select_rule(request, &self.action_rules)
            .into_result(request, &self.action_rules)
            .map_err(one_diag)?;
        let Some(body) = self.action_bodies.get(&rule) else {
            return Err(internal_diag(
                InternalInvariant::SelectedActionRuleHasNoBody,
            ));
        };
        let spec = body.plan(&request.inputs)?;
        let spec_digest = spec.digest().map_err(one_diag)?;
        Ok(ActionPlan {
            rule,
            spec_digest,
            spec,
        })
    }

    pub(super) async fn run_action<P: ActionPolicy, E: Executor>(
        &mut self,
        request: &Request<Action>,
        policy: &P,
        executor: &E,
    ) -> ActionRunOutcome {
        let plan = match self.plan_action(request) {
            Ok(plan) => plan,
            Err(diagnostics) => return ActionRunOutcome::PlanningFailed(diagnostics),
        };
        let rule = plan.rule;
        let authorization = policy.authorize(&plan);
        let denial = match &authorization {
            ActionAuthorization::Allowed { .. } => None,
            ActionAuthorization::Denied { policy, reason } => Some(one_diag(Diag::engine(
                EngineCode::PolicyDenied,
                request.span,
                format!("action denied by policy `{policy}`: {reason}"),
            ))),
        };

        let Some(action_rule) = self.action_rules.get(rule) else {
            return ActionRunOutcome::PlanningFailed(internal_diag(
                InternalInvariant::SelectedActionRuleHasNoMetadata,
            ));
        };
        let rule_meta = ActionRuleMeta {
            rule,
            declared_output: action_rule.interface.output.clone(),
            span: action_rule.span,
            label: action_rule.label.clone(),
        };

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

        let result = self
            .run_action_body(
                computation,
                request,
                &plan.spec,
                &rule_meta,
                denial,
                executor,
            )
            .await;

        ActionRunOutcome::Started {
            computation,
            result,
        }
    }

    /// Drive one action from authorization through result validation, after the
    /// computation node has been allocated. Every failure path applies the same
    /// cleanup via [`Engine::fail_action`]: mark the computation failed and, if
    /// the execution report exists, retain it as provenance.
    async fn run_action_body<E: Executor>(
        &mut self,
        computation: ComputationId,
        request: &Request<Action>,
        spec: &ActionSpec,
        rule_meta: &ActionRuleMeta,
        denial: Option<DiagnosticSink>,
        executor: &E,
    ) -> PithResult<Value> {
        // Up to and including execution: no report exists on failure yet.
        if let Some(diagnostics) = denial {
            self.set_action_reuse(computation, ReuseReason::PolicyDenied);
            return Err(diagnostics);
        }

        let invocation = self.materialize_action(spec)?;
        let captured = match executor.execute(&invocation).await {
            Ok(execution) => execution,
            Err(diagnostics) => return Err(self.fail_action(computation, None, diagnostics)),
        };
        let execution = match self.import_execution(captured) {
            Ok(execution) => execution,
            Err(diagnostics) => return Err(self.fail_action(computation, None, diagnostics)),
        };

        // From here every failure retains the execution report.
        let capabilities_used = canonical_capabilities(&execution.report.capabilities_used);
        let Some(node) = self.computations.get_mut(computation) else {
            return Err(internal_diag(InternalInvariant::ActionLostComputationNode));
        };
        node.dependencies.extend(
            capabilities_used
                .into_iter()
                .map(|capability| DependencyEdge::CapabilityUse { capability }),
        );
        if let Err(diagnostics) = self.validate_execution(spec, &execution) {
            return Err(self.fail_action(computation, Some(execution.report), diagnostics));
        }
        let Some(body) = self.action_bodies.get(&rule_meta.rule) else {
            return Err(self.fail_action(
                computation,
                Some(execution.report),
                internal_diag(InternalInvariant::SelectedActionRuleHasNoBody),
            ));
        };
        let value = match body.complete(&request.inputs, &execution) {
            Ok(value) => value,
            Err(diagnostics) => {
                return Err(self.fail_action(computation, Some(execution.report), diagnostics));
            }
        };
        if let Err(diagnostics) = validate_action_result(
            &value,
            &rule_meta.declared_output,
            &rule_meta.label,
            rule_meta.span,
        ) {
            return Err(self.fail_action(computation, Some(execution.report), diagnostics));
        }

        let Some(node) = self.computations.get_mut(computation) else {
            return Err(internal_diag(InternalInvariant::ActionLostComputationNode));
        };
        node.result = Some(value.clone());
        node.reuse = ReuseDecision::NotReusable(ReuseReason::ActionCachingDisabled);
        let Some(action) = node.action.as_mut() else {
            return Err(internal_diag(InternalInvariant::ActionLostActionRecord));
        };
        action.report = Some(execution.report);

        Ok(value)
    }

    /// Record a failed action computation and return the diagnostics unchanged.
    /// Centralizing the cleanup here keeps the failure tail at one line per site
    /// instead of repeating `mark_action_failed` + `return Err` six times.
    fn fail_action(
        &mut self,
        computation: ComputationId,
        report: Option<ExecutionReport>,
        diagnostics: DiagnosticSink,
    ) -> DiagnosticSink {
        self.mark_action_failed(computation, report);
        diagnostics
    }

    fn set_action_reuse(&mut self, computation: ComputationId, reason: ReuseReason) {
        if let Some(node) = self.computations.get_mut(computation) {
            node.reuse = ReuseDecision::NotReusable(reason);
        }
    }

    fn materialize_action(&self, spec: &ActionSpec) -> PithResult<ActionInvocation> {
        let executable = self.materialize_content_by_id(spec.executable)?;
        let mut inputs = Vec::with_capacity(spec.inputs.len());
        for input in &spec.inputs {
            let content = match &input.content {
                Content::Blob(id) => self.materialize_blob(*id)?,
                Content::Tree(id) => MaterializedContent::Tree(self.materialize_tree(*id)?),
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
            return Ok(MaterializedContent::Blob(MaterializedBlob {
                id,
                bytes: blob.as_bytes().to_vec().into_boxed_slice(),
            }));
        }
        if self.store.get_tree(id).map_err(store_error_diag)?.is_some() {
            return Ok(MaterializedContent::Tree(self.materialize_tree(id)?));
        }
        Err(content_unavailable_diag(id))
    }

    fn materialize_blob(&self, id: ContentId) -> PithResult<MaterializedContent> {
        match self.store.get_blob(id).map_err(store_error_diag)? {
            Some(blob) => Ok(MaterializedContent::Blob(MaterializedBlob {
                id,
                bytes: blob.as_bytes().to_vec().into_boxed_slice(),
            })),
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
                TreeEntryContent::File(pith_store::FileContent {
                    content,
                    executable,
                }) => {
                    let MaterializedContent::Blob(materialized) =
                        self.materialize_blob(*content)?
                    else {
                        return Err(internal_diag(InternalInvariant::TreeFileMaterializedAsTree));
                    };
                    MaterializedTreeEntryContent::File(MaterializedFileContent {
                        content: *content,
                        executable: *executable,
                        bytes: materialized.bytes,
                    })
                }
                TreeEntryContent::Tree(child) => {
                    MaterializedTreeEntryContent::Tree(self.materialize_tree(*child)?)
                }
                TreeEntryContent::Symlink { target } => MaterializedTreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(
                TreeEntry::new(entry.name(), content)
                    .map_err(|_| internal_diag(InternalInvariant::TreeFileMaterializedAsTree))?,
            );
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
            let content = self.import_output(&output.content)?;
            outputs.push(ProducedOutput {
                path: output.path.clone(),
                content,
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

    /// Content-address a captured output into the store, returning the typed
    /// content the engine retains. The Blob/Tree discriminator travels in the
    /// `Content` payload, so there is no separate `kind` to disagree with it:
    /// the previous 2×2 mismatch match and its diagnostic are gone.
    fn import_output(
        &mut self,
        content: &CapturedOutputContent,
    ) -> PithResult<Content<ContentId, ContentId>> {
        match content {
            Content::Blob(bytes) => Ok(Content::Blob(
                self.store.put_blob(bytes).map_err(store_error_diag)?,
            )),
            Content::Tree(tree) => Ok(Content::Tree(self.import_tree(tree)?)),
        }
    }

    fn import_tree(&mut self, tree: &crate::action::CapturedTree) -> PithResult<ContentId> {
        let mut entries = Vec::with_capacity(tree.entries.len());
        for entry in tree.entries.iter() {
            let content = match entry.content() {
                CapturedTreeEntryContent::File(CapturedFileContent { bytes, executable }) => {
                    let content = self.store.put_blob(bytes).map_err(store_error_diag)?;
                    TreeEntryContent::File(pith_store::FileContent {
                        content,
                        executable: *executable,
                    })
                }
                CapturedTreeEntryContent::Tree(tree) => {
                    TreeEntryContent::Tree(self.import_tree(tree)?)
                }
                CapturedTreeEntryContent::Symlink { target } => TreeEntryContent::Symlink {
                    target: target.clone(),
                },
            };
            entries.push(
                TreeEntry::new(entry.name(), content)
                    .map_err(|_| internal_diag(InternalInvariant::TreeFileMaterializedAsTree))?,
            );
        }
        let tree = Tree::new(entries).map_err(store_error_diag)?;
        self.store.put_tree(tree).map_err(store_error_diag)
    }

    fn validate_execution(&self, spec: &ActionSpec, execution: &ActionExecution) -> PithResult<()> {
        validate_execution_platform(spec, &execution.report.platform)?;

        for used in &execution.report.capabilities_used {
            if !spec.capabilities.contains(used) {
                return Err(one_diag(Diag::engine(
                    EngineCode::UndeclaredCapabilityUse,
                    Span::none(),
                    format!(
                        "executor reported undeclared capability `{}` scoped to `{}`",
                        used.name, used.scope
                    ),
                )));
            }
        }

        for produced in &execution.report.outputs {
            let declared = spec.outputs.iter().any(|output| {
                output.path == produced.path && output.kind == produced.content.kind()
            });
            if !declared {
                return Err(one_diag(Diag::engine(
                    EngineCode::UndeclaredOutput,
                    Span::none(),
                    format!("executor reported undeclared output `{}`", produced.path),
                )));
            }
        }

        for declared in &spec.outputs {
            let produced = execution.report.outputs.iter().any(|output| {
                output.path == declared.path && output.content.kind() == declared.kind
            });
            if !produced {
                return Err(one_diag(Diag::engine(
                    EngineCode::MissingDeclaredOutput,
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
