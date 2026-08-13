use std::sync::Arc;

use pith_core::{
    ActionComputationKey, ActionOutput, ActionSpec, CapabilityRequirement, Content, Interface,
    OutputKind, PureComputationKey, Request, Rule, RuleIdentity, RuleRevision, Type, Value,
};
use pith_diag::{Diag, EngineCode, Span};
use pith_engine::state::{
    CURRENT_ENGINE_STATE_VERSIONS, CompletedAttempt, DurableActionPlan, DurableActionProvenance,
    DurableAttemptId, DurableAttemptState, DurableAttemptStatus, DurableCapturedExecutionReport,
    DurableCapturedOutput, DurableComputation, DurableDependency, DurableDiagnostic,
    DurableProvenance, DurableReuseDecision, DurableReuseReason, DurableRule, EncodedValue,
    EngineStateError, EngineStateStore, ExpectedReuseDecision, InvalidActionLifecycleReason,
    InvalidDependencyReason, MemoryEngineStateStore, StoppedAttempt,
};
use pith_engine::{
    AccessVerification, ActionAuthorization, CapturedExecutionReport, CapturedOutput,
    ExecutionPlatform, ExecutionReport, ProducedOutput,
};
use pith_ids::{ActionComputationDigest, ContentId};

fn pure_computation(declaration: &str, input: i64) -> PureComputationKey {
    let identity = RuleIdentity::of_module_declaration("engine-state-tests", declaration);
    let revision = RuleRevision::of_manifest(identity, b"engine-state-tests-v1");
    let rule = Rule::new(
        revision,
        declaration,
        Interface {
            inputs: Box::new([Type::Int]),
            output: Type::Int,
        },
        Span::none(),
    );
    let request = Request::new(
        declaration,
        rule.interface.clone(),
        [Value::Int(input)],
        Span::none(),
    );
    PureComputationKey::new(&rule, &request)
}

fn action_plan(declaration: &str) -> Result<DurableActionPlan, Diag> {
    let identity = RuleIdentity::of_module_declaration("engine-state-tests", declaration);
    let revision = RuleRevision::of_manifest(identity, b"engine-state-action-v1");
    let mut spec = ActionSpec::isolated("/bin/executable");
    spec.outputs = Box::new([ActionOutput {
        path: "result".into(),
        kind: OutputKind::Blob,
    }]);
    spec.capabilities = Box::new([network_capability()]);
    DurableActionPlan::new(DurableRule::new(revision), spec)
}

/// The digest half of the reusable-index key for the action `action_plan`
/// builds. The rule halves come from the plan, so only this varies per fixture.
fn action_digest(declaration: &str) -> ActionComputationDigest {
    ActionComputationDigest::of_manifest(declaration.as_bytes())
}

/// The whole key for one of those actions, for reading the reusable index back.
fn action_key(declaration: &str) -> Result<ActionComputationKey, Diag> {
    let plan = action_plan(declaration)?;
    Ok(ActionComputationKey {
        rule_identity: plan.rule().identity(),
        rule_revision: plan.rule().revision(),
        digest: action_digest(declaration),
    })
}

fn network_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        name: "network".into(),
        scope: "example.test:443".into(),
    }
}

fn execution_platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: "linux".into(),
        architecture: "x86_64".into(),
    }
}

fn captured_report() -> CapturedExecutionReport {
    CapturedExecutionReport {
        executor: "fixture-executor".into(),
        platform: execution_platform(),
        access: AccessVerification::Observed,
        outputs: Box::new([CapturedOutput {
            path: "result".into(),
            content: Content::Blob(b"output".to_vec().into_boxed_slice()),
        }]),
        capabilities_used: Box::new([network_capability()]),
    }
}

fn imported_report(output: ContentId) -> ExecutionReport {
    ExecutionReport {
        executor: "fixture-executor".into(),
        platform: execution_platform(),
        access: AccessVerification::Observed,
        outputs: Box::new([ProducedOutput {
            path: "result".into(),
            content: Content::Blob(output),
        }]),
        capabilities_used: Box::new([network_capability()]),
    }
}

fn pure_completion(
    value: Value,
    dependencies: Box<[DurableDependency]>,
    reuse: DurableReuseDecision,
) -> CompletedAttempt {
    CompletedAttempt {
        dependencies,
        result: EncodedValue::from_value(&value),
        provenance: DurableProvenance::Pure,
        reuse,
    }
}

fn conformance_suite(store: &mut dyn EngineStateStore) -> Result<(), Box<dyn std::error::Error>> {
    let dependency_key = pure_computation("dependency", 1);
    let dependency_attempt =
        store.create_pending_attempt(DurableComputation::Pure(dependency_key))?;
    store.publish_complete(
        dependency_attempt,
        pure_completion(Value::Int(1), Box::new([]), DurableReuseDecision::Reusable),
    )?;

    let root_key = pure_computation("root", 2);
    let first_root = store.create_pending_attempt(DurableComputation::Pure(root_key))?;
    let content = ContentId::of_blob(b"input");
    let capability = CapabilityRequirement {
        name: "clock".into(),
        scope: "monotonic".into(),
    };
    store.publish_complete(
        first_root,
        pure_completion(
            Value::Int(3),
            Box::new([
                DurableDependency::Blob { content },
                DurableDependency::Pure {
                    computation: dependency_key,
                    attempt: dependency_attempt,
                },
                DurableDependency::CapabilityUse {
                    capability: capability.clone(),
                },
            ]),
            DurableReuseDecision::Reusable,
        ),
    )?;

    let second_root = store.create_pending_attempt(DurableComputation::Pure(root_key))?;
    store.publish_complete(
        second_root,
        pure_completion(
            Value::Int(3),
            Box::new([DurableDependency::Pure {
                computation: dependency_key,
                attempt: dependency_attempt,
            }]),
            DurableReuseDecision::Reusable,
        ),
    )?;

    let failed_root = store.create_pending_attempt(DurableComputation::Pure(root_key))?;
    let diagnostic = Diag::engine(
        EngineCode::ContentUnavailable,
        Span::none(),
        "missing input",
    );
    store.publish_failed(
        failed_root,
        StoppedAttempt {
            dependencies: Box::new([DurableDependency::Blob { content }]),
            diagnostics: Box::new([DurableDiagnostic::from(&diagnostic)]),
            provenance: DurableProvenance::Pure,
        },
    )?;

    let history = store.attempt_history(root_key)?;
    assert_eq!(
        history.iter().map(|attempt| attempt.id).collect::<Vec<_>>(),
        vec![first_root, second_root, failed_root]
    );
    let Some(first_attempt) = history.first() else {
        return Err(EngineStateError::Adapter {
            message: "root attempt history is empty".into(),
        }
        .into());
    };
    let DurableAttemptState::Complete(first_completion) = &first_attempt.state else {
        return Err(EngineStateError::Adapter {
            message: "first root attempt is not complete".into(),
        }
        .into());
    };
    assert_eq!(
        first_completion.dependencies.as_ref(),
        [
            DurableDependency::Blob { content },
            DurableDependency::Pure {
                computation: dependency_key,
                attempt: dependency_attempt,
            },
            DurableDependency::CapabilityUse { capability },
        ]
    );
    assert_eq!(first_completion.result.decode(), Ok(Value::Int(3)));
    assert_eq!(
        store
            .latest_completed_reusable_attempt(root_key)?
            .map(|attempt| attempt.id),
        Some(second_root)
    );
    assert!(store.pending_attempts()?.is_empty());

    let republish = store.publish_failed(
        first_root,
        StoppedAttempt {
            dependencies: Box::new([]),
            diagnostics: Box::new([]),
            provenance: DurableProvenance::Pure,
        },
    );
    assert_eq!(
        republish,
        Err(EngineStateError::AttemptNotPending {
            attempt: first_root,
            status: DurableAttemptStatus::Complete,
        })
    );
    assert!(
        store
            .attempt(first_root)?
            .is_some_and(|attempt| matches!(attempt.state, DurableAttemptState::Complete(_)))
    );
    Ok(())
}

#[test]
fn memory_store_conforms_to_the_engine_state_contract() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = MemoryEngineStateStore::default();
    assert_eq!(store.versions(), CURRENT_ENGINE_STATE_VERSIONS);
    conformance_suite(&mut store)
}

#[test]
fn pending_action_retains_durable_plan_authorization_and_observed_report()
-> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let identity = RuleIdentity::of_module_declaration("engine-state-tests", "action");
    let revision = RuleRevision::of_manifest(identity, b"engine-state-action-v1");
    let plan = action_plan("action")?;
    let authorization = ActionAuthorization::Allowed {
        policy: "fixture-policy".into(),
    };
    let attempt = store.create_pending_attempt(DurableComputation::Action {
        computation_digest: action_digest("action"),
        plan: plan.clone(),
        authorization: authorization.clone(),
    })?;

    let pending = store.pending_attempts()?;
    assert_eq!(pending.len(), 1);
    let Some(pending_action) = pending.first() else {
        return Err(EngineStateError::Adapter {
            message: "pending action was not enumerated".into(),
        }
        .into());
    };
    assert!(matches!(
        &pending_action.computation,
        DurableComputation::Action {
            computation_digest: stored_digest,
            plan: stored_plan,
            authorization: stored_authorization,
        } if stored_digest == &action_digest("action")
            && stored_plan == &plan
            && stored_authorization == &authorization
    ));
    assert_eq!(plan.rule().identity(), identity);
    assert_eq!(plan.rule().revision(), revision);
    assert_eq!(plan.spec_digest(), plan.spec().digest()?);

    let output = ContentId::of_blob(b"output");
    let capability = network_capability();
    let imported_report = imported_report(output);
    let missing_report_completion = store.publish_complete(
        attempt,
        CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Blob(output)),
            provenance: DurableProvenance::Action(DurableActionProvenance::NotExecuted),
            reuse: DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled),
        },
    );
    assert_eq!(
        missing_report_completion,
        Err(EngineStateError::InvalidActionLifecycle {
            attempt,
            reason: InvalidActionLifecycleReason::CompletedActionMissingExecutorReport,
        })
    );

    // Capability-use edges are the only edges an action attempt carries, and
    // they never block reuse, so these dependencies support `Reusable` and
    // nothing else.
    let invalid_completion = store.publish_complete(
        attempt,
        CompletedAttempt {
            dependencies: Box::new([DurableDependency::CapabilityUse {
                capability: capability.clone(),
            }]),
            result: EncodedValue::from_value(&Value::Blob(output)),
            provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report: imported_report.clone(),
            }),
            reuse: DurableReuseDecision::NotReusable(DurableReuseReason::DependencyNotReusable {
                attempt: DurableAttemptId::from_raw(u64::MAX),
            }),
        },
    );
    assert_eq!(
        invalid_completion,
        Err(EngineStateError::InvalidReuseDecision {
            attempt,
            expected: ExpectedReuseDecision::Reusable,
        })
    );

    store.publish_complete(
        attempt,
        CompletedAttempt {
            dependencies: Box::new([DurableDependency::CapabilityUse { capability }]),
            result: EncodedValue::from_value(&Value::Blob(output)),
            provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report: imported_report.clone(),
            }),
            reuse: DurableReuseDecision::Reusable,
        },
    )?;
    // A completed reusable action is findable under its own key, and only
    // there (decision 0031).
    assert_eq!(
        store
            .latest_completed_reusable_action_attempt(action_key("action")?)?
            .map(|found| found.id),
        Some(attempt)
    );
    assert!(
        store
            .latest_completed_reusable_action_attempt(action_key("other-action")?)?
            .is_none()
    );

    let Some(completed) = store.attempt(attempt)? else {
        return Err(EngineStateError::AttemptNotFound { attempt }.into());
    };
    let DurableAttemptState::Complete(completion) = &completed.state else {
        return Err(EngineStateError::Adapter {
            message: "action attempt is not complete".into(),
        }
        .into());
    };
    assert_eq!(
        completion.provenance,
        DurableProvenance::Action(DurableActionProvenance::Imported { imported_report })
    );
    let Some(second_read) = store.attempt(attempt)? else {
        return Err(EngineStateError::AttemptNotFound { attempt }.into());
    };
    assert!(Arc::ptr_eq(&completed, &second_read));
    Ok(())
}

#[test]
fn pending_attempts_can_be_enumerated_for_crash_recovery() -> Result<(), Box<dyn std::error::Error>>
{
    let store = MemoryEngineStateStore::default();
    let key = pure_computation("interrupted", 1);
    let pending = store.create_pending_attempt(DurableComputation::Pure(key))?;

    assert_eq!(
        store
            .pending_attempts()?
            .iter()
            .map(|attempt| attempt.id)
            .collect::<Vec<_>>(),
        vec![pending]
    );
    Ok(())
}

#[test]
fn malformed_encoded_results_are_rejected() {
    assert!(EncodedValue::from_bytes([0xff]).is_err());
}

#[test]
fn mismatched_provenance_does_not_partially_publish() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let attempt =
        store.create_pending_attempt(DurableComputation::Pure(pure_computation("pure", 1)))?;

    let result = store.publish_complete(
        attempt,
        CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Int(1)),
            provenance: DurableProvenance::Action(DurableActionProvenance::NotExecuted),
            reuse: DurableReuseDecision::Reusable,
        },
    );

    assert_eq!(
        result,
        Err(EngineStateError::ProvenanceCategoryMismatch { attempt })
    );
    assert!(
        store
            .attempt(attempt)?
            .is_some_and(|attempt| matches!(attempt.state, DurableAttemptState::Pending))
    );
    Ok(())
}

#[test]
fn invalid_dependency_edges_do_not_publish() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let dependency_key = pure_computation("dependency", 1);
    let dependency_attempt =
        store.create_pending_attempt(DurableComputation::Pure(dependency_key))?;
    store.publish_complete(
        dependency_attempt,
        pure_completion(Value::Int(1), Box::new([]), DurableReuseDecision::Reusable),
    )?;

    let pending_dependency_key = pure_computation("pending-dependency", 1);
    let pending_dependency =
        store.create_pending_attempt(DurableComputation::Pure(pending_dependency_key))?;
    let failed_dependency_key = pure_computation("failed-dependency", 1);
    let failed_dependency =
        store.create_pending_attempt(DurableComputation::Pure(failed_dependency_key))?;
    store.publish_failed(
        failed_dependency,
        StoppedAttempt {
            dependencies: Box::new([]),
            diagnostics: Box::new([]),
            provenance: DurableProvenance::Pure,
        },
    )?;

    let root_key = pure_computation("root", 1);
    let root = store.create_pending_attempt(DurableComputation::Pure(root_key))?;
    let missing_attempt = DurableAttemptId::from_raw(u64::MAX);
    let wrong_key = pure_computation("different-dependency", 1);
    let invalid_dependencies = [
        (
            DurableDependency::Action {
                attempt: missing_attempt,
            },
            missing_attempt,
            InvalidDependencyReason::MissingAttempt,
        ),
        (
            DurableDependency::Action {
                attempt: dependency_attempt,
            },
            dependency_attempt,
            InvalidDependencyReason::ExpectedActionAttempt,
        ),
        (
            DurableDependency::Pure {
                computation: wrong_key,
                attempt: dependency_attempt,
            },
            dependency_attempt,
            InvalidDependencyReason::PureComputationMismatch,
        ),
        (
            DurableDependency::Pure {
                computation: pending_dependency_key,
                attempt: pending_dependency,
            },
            pending_dependency,
            InvalidDependencyReason::PendingAttempt,
        ),
        (
            DurableDependency::Pure {
                computation: failed_dependency_key,
                attempt: failed_dependency,
            },
            failed_dependency,
            InvalidDependencyReason::FailedDependencyForCompleteAttempt,
        ),
        (
            DurableDependency::Pure {
                computation: root_key,
                attempt: root,
            },
            root,
            InvalidDependencyReason::SelfReference,
        ),
    ];
    for (dependency, dependency_attempt, reason) in invalid_dependencies {
        let result = store.publish_complete(
            root,
            pure_completion(
                Value::Int(1),
                Box::new([dependency]),
                DurableReuseDecision::Reusable,
            ),
        );
        assert_eq!(
            result,
            Err(EngineStateError::InvalidDependency {
                attempt: root,
                dependency: dependency_attempt,
                reason,
            })
        );
    }
    assert!(
        store
            .attempt(root)?
            .is_some_and(|attempt| matches!(attempt.state, DurableAttemptState::Pending))
    );
    Ok(())
}

#[test]
fn pure_reuse_is_derived_from_ordered_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let action_attempt = store.create_pending_attempt(DurableComputation::Action {
        computation_digest: action_digest("reuse-dependency"),
        plan: action_plan("reuse-dependency")?,
        authorization: ActionAuthorization::Allowed {
            policy: "fixture-policy".into(),
        },
    })?;
    let output = ContentId::of_blob(b"output");
    store.publish_complete(
        action_attempt,
        CompletedAttempt {
            dependencies: Box::new([DurableDependency::CapabilityUse {
                capability: network_capability(),
            }]),
            result: EncodedValue::from_value(&Value::Blob(output)),
            provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report: imported_report(output),
            }),
            reuse: DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled),
        },
    )?;

    let pure_key = pure_computation("action-parent", 1);
    let pure_attempt = store.create_pending_attempt(DurableComputation::Pure(pure_key))?;
    let expected_reuse =
        DurableReuseDecision::NotReusable(DurableReuseReason::DependencyNotReusable {
            attempt: action_attempt,
        });
    let invalid_decisions = [
        DurableReuseDecision::Reusable,
        DurableReuseDecision::NotReusable(DurableReuseReason::DependencyNotReusable {
            attempt: DurableAttemptId::from_raw(u64::MAX),
        }),
    ];
    for reuse in invalid_decisions {
        let result = store.publish_complete(
            pure_attempt,
            pure_completion(
                Value::Int(1),
                Box::new([DurableDependency::Action {
                    attempt: action_attempt,
                }]),
                reuse,
            ),
        );
        assert_eq!(
            result,
            Err(EngineStateError::InvalidReuseDecision {
                attempt: pure_attempt,
                expected: ExpectedReuseDecision::DependencyNotReusable {
                    attempt: action_attempt,
                },
            })
        );
    }

    store.publish_complete(
        pure_attempt,
        pure_completion(
            Value::Int(1),
            Box::new([DurableDependency::Action {
                attempt: action_attempt,
            }]),
            expected_reuse,
        ),
    )?;
    assert!(store.latest_completed_reusable_attempt(pure_key)?.is_none());
    Ok(())
}

#[test]
fn denied_actions_cannot_complete_or_retain_execution_reports()
-> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let attempt = store.create_pending_attempt(DurableComputation::Action {
        computation_digest: action_digest("denied-action"),
        plan: action_plan("denied-action")?,
        authorization: ActionAuthorization::Denied {
            policy: "fixture-policy".into(),
            reason: "fixture denial".into(),
        },
    })?;
    let output = ContentId::of_blob(b"output");
    let imported_report = imported_report(output);

    let completion_result = store.publish_complete(
        attempt,
        CompletedAttempt {
            dependencies: Box::new([]),
            result: EncodedValue::from_value(&Value::Blob(output)),
            provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report: imported_report.clone(),
            }),
            reuse: DurableReuseDecision::NotReusable(DurableReuseReason::ActionCachingDisabled),
        },
    );
    assert_eq!(
        completion_result,
        Err(EngineStateError::InvalidActionLifecycle {
            attempt,
            reason: InvalidActionLifecycleReason::DeniedActionCompleted,
        })
    );

    let failure_result = store.publish_failed(
        attempt,
        StoppedAttempt {
            dependencies: Box::new([]),
            diagnostics: Box::new([]),
            provenance: DurableProvenance::Action(DurableActionProvenance::Imported {
                imported_report,
            }),
        },
    );
    assert_eq!(
        failure_result,
        Err(EngineStateError::InvalidActionLifecycle {
            attempt,
            reason: InvalidActionLifecycleReason::DeniedActionHasExecutorReport,
        })
    );

    store.publish_failed(
        attempt,
        StoppedAttempt {
            dependencies: Box::new([]),
            diagnostics: Box::new([]),
            provenance: DurableProvenance::Action(DurableActionProvenance::NotExecuted),
        },
    )?;
    Ok(())
}

#[test]
fn captured_report_metadata_survives_output_import_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryEngineStateStore::default();
    let attempt = store.create_pending_attempt(DurableComputation::Action {
        computation_digest: action_digest("failed-import"),
        plan: action_plan("failed-import")?,
        authorization: ActionAuthorization::Allowed {
            policy: "fixture-policy".into(),
        },
    })?;
    let capability = network_capability();
    let captured_report = DurableCapturedExecutionReport::from(&captured_report());
    assert_eq!(
        captured_report.outputs.as_ref(),
        [DurableCapturedOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
    );
    let diagnostic = Diag::engine(
        EngineCode::ContentUnavailable,
        Span::none(),
        "output import failed",
    );
    let missing_capability_edge = store.publish_failed(
        attempt,
        StoppedAttempt {
            dependencies: Box::new([]),
            diagnostics: Box::new([DurableDiagnostic::from(&diagnostic)]),
            provenance: DurableProvenance::Action(DurableActionProvenance::Captured {
                executor_report: captured_report.clone(),
            }),
        },
    );
    assert_eq!(
        missing_capability_edge,
        Err(EngineStateError::CapabilityDependenciesMismatch { attempt })
    );

    store.publish_failed(
        attempt,
        StoppedAttempt {
            dependencies: Box::new([DurableDependency::CapabilityUse { capability }]),
            diagnostics: Box::new([DurableDiagnostic::from(&diagnostic)]),
            provenance: DurableProvenance::Action(DurableActionProvenance::Captured {
                executor_report: captured_report.clone(),
            }),
        },
    )?;

    let Some(failed_attempt) = store.attempt(attempt)? else {
        return Err(EngineStateError::AttemptNotFound { attempt }.into());
    };
    let DurableAttemptState::Failed(failure) = &failed_attempt.state else {
        return Err(EngineStateError::Adapter {
            message: "action attempt is not failed".into(),
        }
        .into());
    };
    assert_eq!(
        failure.provenance,
        DurableProvenance::Action(DurableActionProvenance::Captured {
            executor_report: captured_report,
        })
    );
    Ok(())
}
