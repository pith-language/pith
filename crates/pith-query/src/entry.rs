use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use pith_core::{PureComputationKey, RecordField, Rule, Value};
use pith_diag::DiagnosticSink;
use pith_engine::state::{
    DurableAttempt, DurableAttemptState, DurableAttemptStatus, DurableComputation,
    DurableDependency, DurableReuseReason, EngineStateReader, InvalidationExplanation,
    InvalidationReason,
};
use pith_engine::{
    Engine, Evaluation, EvaluationSource, LiveInvalidationExplanation, LiveInvalidationReason,
    ReuseReason,
};
use pith_output::ExplainStep;
use pith_output::dto::{
    ActionPlanView, AttemptStatusRepr, DependenciesView, DependencyKindRepr, DependencyNodeRepr,
    EvaluationSourceRepr, RunView, SelectionView,
};

use crate::error::QueryError;
use crate::program::{Program, teach_entry_collision};
use crate::session::{ReadOnly, Session, Writable, read_failure};

pub struct ExecInvocation {
    pub program: Box<str>,
    pub arguments: Box<[Box<str>]>,
}

struct EntryRequest {
    name: Box<str>,
    coordinate: Box<str>,
    interface: pith_core::Interface,
    rule: Rule<pith_core::Pure>,
    request: pith_core::Request<pith_core::Pure>,
}

impl EntryRequest {
    fn of(program: &Program, entry: &str) -> Result<Self, QueryError> {
        let declaration = program.entry(entry)?;
        Ok(Self {
            name: entry.into(),
            coordinate: format!("{}::{}", program.module(), declaration.rule_label()).into(),
            interface: declaration.interface().clone(),
            rule: declaration.rule(),
            request: declaration.request(),
        })
    }

    fn key(&self) -> PureComputationKey {
        PureComputationKey::new(&self.rule, &self.request)
    }
}

impl Session<Writable> {
    /// Evaluate a named entry through the writable engine. The method is absent
    /// from [`Session<ReadOnly>`], so a query-only client cannot publish an
    /// attempt accidentally.
    ///
    /// ```compile_fail
    /// use std::path::Path;
    /// use pith_query::{ReadOnly, Roots, Session};
    ///
    /// let session = Session::<ReadOnly>::open(Roots::under(Path::new("scratch")))?;
    /// let _ = session.run_entry(Path::new("module.pi"), "build");
    /// # Ok::<(), pith_query::QueryError>(())
    /// ```
    ///
    /// # Errors
    /// Returns source, registration, selection, evaluation, runtime, content,
    /// or durable-state failures.
    pub fn run_entry(self, path: &Path, entry: &str) -> Result<RunView, QueryError> {
        let Evaluated {
            request,
            evaluation,
            ..
        } = evaluate(self, path, entry)?;
        Ok(run_view(&request, &evaluation))
    }

    /// Evaluate an entry and explain the invalidation chain of its latest
    /// durable or live computation.
    ///
    /// # Errors
    /// Returns the same failures as [`Self::run_entry`], plus a durable-state
    /// read failure.
    pub fn explain_entry(self, path: &Path, entry: &str) -> Result<Box<[ExplainStep]>, QueryError> {
        let Evaluated {
            request,
            engine,
            evaluation,
        } = evaluate(self, path, entry)?;
        if let Some(explanation) = engine
            .state_store()
            .explain_invalidation(request.key())
            .map_err(read_failure)?
        {
            let mut steps = Vec::new();
            durable_explanation(&explanation, &mut steps);
            return Ok(steps.into());
        }
        if let Some(explanation) = engine.query().explain_invalidation(evaluation.computation) {
            let mut steps = Vec::new();
            live_explanation(&explanation, &mut steps);
            return Ok(steps.into());
        }
        Ok(Box::new([ExplainStep {
            label: request.coordinate,
            detail: "the latest result is reusable; there is no invalidation chain".into(),
        }]))
    }

    /// Drive an entry's pure prefix to its first action and return the planned
    /// contract without executing it.
    ///
    /// # Errors
    /// Returns source, registration, selection, content, or planner failures.
    pub fn plan_entry(self, path: &Path, entry: &str) -> Result<ActionPlanView, QueryError> {
        let (request, mut engine) = prepare_entry(path, entry, self.into_engine()?)?;
        let planned = engine
            .plan_entry(&request.request)
            .map_err(|diagnostics| evaluation_failure(entry, "cannot plan", diagnostics))?;
        let rule = engine
            .query()
            .rule(planned.action.rule)
            .ok_or_else(|| QueryError::internal("the selected action rule disappeared"))?;
        Ok(ActionPlanView {
            entry: request.name,
            rule: coordinate(rule),
            spec_digest: planned.action.spec_digest.digest().to_string().into(),
            contract: (&planned.action.spec).into(),
        })
    }

    /// Evaluate an entry and require the builtin `pith.Exec` value.
    ///
    /// # Errors
    /// Returns evaluation failures or a user failure when the value is not the
    /// agreed exec shape.
    pub fn prepare_exec(self, path: &Path, entry: &str) -> Result<ExecInvocation, QueryError> {
        let Evaluated { evaluation, .. } = evaluate(self, path, entry)?;
        exec_invocation(evaluation.value)
    }
}

impl Session<ReadOnly> {
    /// Select the synthetic entry rule without evaluating or publishing it.
    ///
    /// # Errors
    /// Returns source, registration, or selection failures.
    pub fn select_entry(self, path: &Path, entry: &str) -> Result<SelectionView, QueryError> {
        let (request, engine) =
            prepare_entry(path, entry, Engine::<dyn EngineStateReader>::query_only())?;
        let selection = preflight(&engine, &request.request)?;
        let rule = engine
            .query()
            .rule(selection.rule)
            .ok_or_else(|| QueryError::internal("the selected entry rule disappeared"))?;
        Ok(SelectionView {
            entry: request.name,
            rule: coordinate(rule),
            tier: rule.tier.into(),
            interface: (&selection.interface).into(),
        })
    }

    /// Read the dependency subtree of the most recent recorded entry attempt.
    ///
    /// # Errors
    /// Returns source or durable-state read failures.
    pub fn entry_dependencies(
        &self,
        path: &Path,
        entry: &str,
    ) -> Result<DependenciesView, QueryError> {
        let program = Program::load(path)?;
        let request = EntryRequest::of(&program, entry)?;
        let state = self.state()?;
        let Some(attempt) = state.latest_attempt(request.key()).map_err(read_failure)? else {
            return Ok(DependenciesView {
                entry: request.name,
                root: None,
            });
        };
        let mut rendered = BTreeMap::new();
        let root = Some(Box::new(attempt_node(
            state,
            attempt,
            request.coordinate.clone(),
            &mut Vec::new(),
            &mut rendered,
        )?));
        Ok(DependenciesView {
            entry: request.name,
            root,
        })
    }
}

/// Everything an entry query shares before its own step: the module loaded,
/// its declarations bound, the entry registered, and its rule selected. The
/// engine arrives with the entry on it; only its origin differs per query.
fn prepare_entry<S>(
    path: &Path,
    entry: &str,
    mut engine: Engine<S>,
) -> Result<(EntryRequest, Engine<S>), QueryError>
where
    S: EngineStateReader + ?Sized,
{
    let program = Program::load(path)?;
    let request = EntryRequest::of(&program, entry)?;
    program.bind(&mut engine)?;
    program.register_entry(entry, &mut engine)?;
    preflight(&engine, &request.request)?;
    Ok((request, engine))
}

/// A completed entry evaluation: the request it ran under, the engine that
/// ran it, and the result.
struct Evaluated {
    request: EntryRequest,
    engine: Engine,
    evaluation: Evaluation,
}

fn evaluate(session: Session<Writable>, path: &Path, entry: &str) -> Result<Evaluated, QueryError> {
    let (request, mut engine) = prepare_entry(path, entry, session.into_engine()?)?;
    let runtime = pith_engine::TokioRuntime::new()
        .map_err(|error| QueryError::internal(format!("cannot start the runtime: {}", error.0)))?;
    let evaluation = engine
        .run(
            &request.request,
            &runtime,
            &pith_engine::AllowAllActions,
            &pith_executor_local::LocalExecutor::new(),
        )
        .map_err(|error| QueryError::internal(format!("cannot drive the runtime: {}", error.0)))?
        .map_err(|diagnostics| evaluation_failure(entry, "did not evaluate", diagnostics))?;
    Ok(Evaluated {
        request,
        engine,
        evaluation,
    })
}

fn preflight<S: EngineStateReader + ?Sized>(
    engine: &Engine<S>,
    request: &pith_core::Request<pith_core::Pure>,
) -> Result<pith_engine::RuleSelection, QueryError> {
    engine
        .query()
        .select(request)
        .map_err(teach_entry_collision)
        .map_err(|diagnostic| {
            QueryError::user("entry selection was refused").with_diagnostics([diagnostic])
        })
}

fn evaluation_failure(entry: &str, verdict: &str, diagnostics: DiagnosticSink) -> QueryError {
    QueryError::user(format!("entry `{entry}` {verdict}"))
        .with_diagnostics(diagnostics.into_inner())
}

fn run_view(request: &EntryRequest, evaluation: &Evaluation) -> RunView {
    RunView {
        entry: request.name.clone(),
        coordinate: request.coordinate.clone(),
        interface: (&request.interface).into(),
        source: match evaluation.source {
            EvaluationSource::Computed => EvaluationSourceRepr::Computed,
            EvaluationSource::Reused => EvaluationSourceRepr::Reused,
            EvaluationSource::Hydrated => EvaluationSourceRepr::Hydrated,
        },
        value: (&evaluation.value).into(),
    }
}

fn coordinate<K: pith_core::EffectCategory>(rule: &Rule<K>) -> Box<str> {
    format!("{}::{}", rule.coordinate.module, rule.coordinate.name).into()
}

fn exec_invocation(value: Value) -> Result<ExecInvocation, QueryError> {
    let Value::Nominal {
        name,
        representation,
    } = value
    else {
        return Err(QueryError::user("`pith exec` requires a `pith.Exec` value"));
    };
    if name.as_ref() != "pith.Exec" {
        return Err(QueryError::user(format!(
            "`pith exec` requires `pith.Exec`, found `{name}`"
        )));
    }
    let Value::Record(fields) = *representation else {
        return Err(QueryError::internal(
            "a `pith.Exec` value has a non-record representation",
        ));
    };
    let program = text_field(&fields, "program")?;
    let arguments = list_text_field(&fields, "arguments")?;
    Ok(ExecInvocation { program, arguments })
}

fn text_field(fields: &[RecordField<Value>], name: &str) -> Result<Box<str>, QueryError> {
    let Some(field) = fields.iter().find(|field| field.name.as_ref() == name) else {
        return Err(QueryError::internal(format!(
            "`pith.Exec` has no `{name}` field"
        )));
    };
    let Value::Text(value) = &field.payload else {
        return Err(QueryError::internal(format!(
            "`pith.Exec.{name}` is not text"
        )));
    };
    Ok(value.clone())
}

fn list_text_field(
    fields: &[RecordField<Value>],
    name: &str,
) -> Result<Box<[Box<str>]>, QueryError> {
    let Some(field) = fields.iter().find(|field| field.name.as_ref() == name) else {
        return Err(QueryError::internal(format!(
            "`pith.Exec` has no `{name}` field"
        )));
    };
    let Value::List(values) = &field.payload else {
        return Err(QueryError::internal(format!(
            "`pith.Exec.{name}` is not a list"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            Value::Text(text) => Ok(text.clone()),
            _ => Err(QueryError::internal(format!(
                "`pith.Exec.{name}` contains a non-text value"
            ))),
        })
        .collect()
}

/// Expand one attempt into its dependency node. `rendered` memoizes finished
/// nodes by attempt, so a shared dependency renders once and reads once no
/// matter how many paths reach it — without it, a dense recorded graph walks
/// one path per route through it. `visiting` is the current path only, and
/// still names the cycle case, where a node repeats inside its own subtree.
fn attempt_node(
    reader: &dyn EngineStateReader,
    attempt: Arc<DurableAttempt>,
    label: Box<str>,
    visiting: &mut Vec<pith_engine::state::DurableAttemptId>,
    rendered: &mut BTreeMap<pith_engine::state::DurableAttemptId, DependencyNodeRepr>,
) -> Result<DependencyNodeRepr, QueryError> {
    if let Some(cached) = rendered.get(&attempt.id) {
        return Ok(cached.clone());
    }
    let kind = match &attempt.computation {
        DurableComputation::Pure(key) => DependencyKindRepr::Pure {
            digest: key.digest.digest().to_string().into(),
        },
        DurableComputation::Action { .. } => DependencyKindRepr::Action,
        DurableComputation::Observation { .. } => DependencyKindRepr::Observation,
    };
    if visiting.contains(&attempt.id) {
        return Ok(DependencyNodeRepr {
            label,
            attempt: Some(attempt.id.to_raw()),
            status: Some(status(attempt.state.status())),
            dependency: kind,
            children: Box::new([]),
        });
    }
    visiting.push(attempt.id);
    let dependencies = match &attempt.state {
        DurableAttemptState::Pending => &[][..],
        DurableAttemptState::Complete(completed) => completed.dependencies.as_ref(),
        DurableAttemptState::Failed(stopped) | DurableAttemptState::Cancelled(stopped) => {
            stopped.dependencies.as_ref()
        }
    };
    let mut children = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        children.push(dependency_node(reader, dependency, visiting, rendered)?);
    }
    let _ = visiting.pop();
    let node = DependencyNodeRepr {
        label,
        attempt: Some(attempt.id.to_raw()),
        status: Some(status(attempt.state.status())),
        dependency: kind,
        children: children.into(),
    };
    rendered.insert(attempt.id, node.clone());
    Ok(node)
}

fn dependency_node(
    reader: &dyn EngineStateReader,
    dependency: &DurableDependency,
    visiting: &mut Vec<pith_engine::state::DurableAttemptId>,
    rendered: &mut BTreeMap<pith_engine::state::DurableAttemptId, DependencyNodeRepr>,
) -> Result<DependencyNodeRepr, QueryError> {
    match dependency {
        DurableDependency::Pure {
            computation,
            attempt,
        } => attempt_child(
            reader,
            *attempt,
            computation.digest.digest().to_string().into(),
            DependencyKindRepr::Pure {
                digest: computation.digest.digest().to_string().into(),
            },
            visiting,
            rendered,
        ),
        DurableDependency::Action { attempt } => attempt_child(
            reader,
            *attempt,
            format!("attempt {attempt}").into(),
            DependencyKindRepr::Action,
            visiting,
            rendered,
        ),
        DurableDependency::Observation { attempt } => attempt_child(
            reader,
            *attempt,
            format!("attempt {attempt}").into(),
            DependencyKindRepr::Observation,
            visiting,
            rendered,
        ),
        DurableDependency::Blob { content } => Ok(DependencyNodeRepr {
            label: content.digest().to_string().into(),
            attempt: None,
            status: None,
            dependency: DependencyKindRepr::Blob {
                digest: content.digest().to_string().into(),
            },
            children: Box::new([]),
        }),
        DurableDependency::CapabilityUse { capability } => Ok(DependencyNodeRepr {
            label: format!("{}:{}", capability.name, capability.scope).into(),
            attempt: None,
            status: None,
            dependency: DependencyKindRepr::Capability {
                name: capability.name.clone(),
                scope: capability.scope.clone(),
            },
            children: Box::new([]),
        }),
    }
}

fn attempt_child(
    reader: &dyn EngineStateReader,
    id: pith_engine::state::DurableAttemptId,
    label: Box<str>,
    missing_kind: DependencyKindRepr,
    visiting: &mut Vec<pith_engine::state::DurableAttemptId>,
    rendered: &mut BTreeMap<pith_engine::state::DurableAttemptId, DependencyNodeRepr>,
) -> Result<DependencyNodeRepr, QueryError> {
    let Some(attempt) = reader.attempt(id).map_err(read_failure)? else {
        return Ok(DependencyNodeRepr {
            label,
            attempt: Some(id.to_raw()),
            status: None,
            dependency: missing_kind,
            children: Box::new([]),
        });
    };
    attempt_node(reader, attempt, label, visiting, rendered)
}

const fn status(status: DurableAttemptStatus) -> AttemptStatusRepr {
    match status {
        DurableAttemptStatus::Pending => AttemptStatusRepr::Pending,
        DurableAttemptStatus::Complete => AttemptStatusRepr::Complete,
        DurableAttemptStatus::Failed => AttemptStatusRepr::Failed,
        DurableAttemptStatus::Cancelled => AttemptStatusRepr::Cancelled,
    }
}

fn durable_explanation(explanation: &InvalidationExplanation, steps: &mut Vec<ExplainStep>) {
    steps.push(ExplainStep {
        label: format!("attempt {}", explanation.attempt).into(),
        detail: durable_reason(&explanation.reason).into(),
    });
    if let InvalidationReason::DependencyInvalidated { child, .. } = &explanation.reason {
        durable_explanation(child, steps);
    }
}

fn durable_reason(reason: &InvalidationReason) -> &'static str {
    match reason {
        InvalidationReason::Leaf(reason) => reuse_reason(reason),
        InvalidationReason::DependencyInvalidated { .. } => "a recorded dependency was invalidated",
    }
}

const fn reuse_reason(reason: &DurableReuseReason) -> &'static str {
    match reason {
        DurableReuseReason::ActionCachingDisabled => "action caching was disabled",
        DurableReuseReason::EffectfulDependency { .. } => {
            "an effectful dependency was not reusable"
        }
        DurableReuseReason::DependencyPending { .. } => "a dependency was still pending",
        DurableReuseReason::DependencyNotReusable { .. } => "a dependency was not reusable",
        DurableReuseReason::DependencyMissing { .. } => "a recorded dependency was missing",
    }
}

fn live_explanation(explanation: &LiveInvalidationExplanation, steps: &mut Vec<ExplainStep>) {
    steps.push(ExplainStep {
        label: format!("computation {:?}", explanation.computation).into(),
        detail: live_reason(&explanation.reason).into(),
    });
    if let LiveInvalidationReason::DependencyInvalidated { child, .. } = &explanation.reason {
        live_explanation(child, steps);
    }
}

fn live_reason(reason: &LiveInvalidationReason) -> &'static str {
    match reason {
        LiveInvalidationReason::Leaf(reason) => match reason {
            ReuseReason::ActionCachingDisabled => "action caching was disabled",
            ReuseReason::EffectfulDependency { .. } => "an effectful dependency was not reusable",
            ReuseReason::DependencyPending { .. } => "a dependency was still pending",
            ReuseReason::DependencyNotReusable { .. } => "a dependency was not reusable",
            ReuseReason::DependencyMissing { .. } => "a dependency was missing",
        },
        LiveInvalidationReason::DependencyInvalidated { .. } => "a live dependency was invalidated",
    }
}
