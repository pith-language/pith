//! Public contract tests for the first-party local executor (decision 0028).
//!
//! The tests deliberately cover the currently implemented staging, process,
//! capture, cleanup, and fail-closed behavior. Full landlock/seccomp
//! confinement, timeouts, and network-enabled policies remain unresolved by
//! the decision record and are not asserted here.

#![cfg(target_os = "linux")]

use std::path::Path;

use pith_core::{
    ActionInput, ActionOutput, ActionProgram, ActionSpec, CapabilityRequirement, Content,
    EnvironmentVariable, ExitStatusContract, NetworkPolicy, OutputKind, PlatformRequirement,
};
use pith_engine::{
    ActionExit, ActionInvocation, CapturedOutputContent, CapturedTreeEntryContent, Executor,
    MaterializedActionInput, MaterializedBlob, MaterializedContent, MaterializedFileContent,
    MaterializedTree, MaterializedTreeEntryContent,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;
use pith_store::TreeEntry;

mod support;

/// The shell, plus the two coreutils the tree-output script shells out to.
fn fixture_closure() -> Box<[Box<str>]> {
    support::closure_for(&["/bin/sh", "mkdir", "chmod", "mktemp", "ls"])
}

fn shell_present() -> bool {
    std::fs::read("/bin/sh").is_ok()
}

fn blob(bytes: &[u8]) -> MaterializedContent {
    MaterializedContent::Blob(MaterializedBlob {
        id: ContentId::of_blob(bytes),
        bytes: bytes.into(),
    })
}

fn invocation(script: &str) -> Option<ActionInvocation> {
    if !shell_present() {
        return None;
    }
    Some(ActionInvocation {
        spec: ActionSpec {
            executable: ActionProgram::HostPath("/bin/sh".into()),
            toolchain: fixture_closure(),
            arguments: ["-c".into(), script.into()].into(),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        },
        inputs: Box::new([]),
        program: None,
    })
}

fn declare_blob_output(invocation: &mut ActionInvocation, path: &str) {
    invocation.spec.outputs = [ActionOutput {
        path: path.into(),
        kind: OutputKind::Blob,
    }]
    .into();
}

fn declare_tree_output(invocation: &mut ActionInvocation, path: &str) {
    invocation.spec.outputs = [ActionOutput {
        path: path.into(),
        kind: OutputKind::Tree,
    }]
    .into();
}

fn add_blob_input(invocation: &mut ActionInvocation, path: &str, bytes: &[u8]) {
    let id = ContentId::of_blob(bytes);
    let mut declared = Vec::from(invocation.spec.inputs.clone());
    declared.push(ActionInput {
        path: path.into(),
        content: Content::Blob(id),
    });
    invocation.spec.inputs = declared.into_boxed_slice();

    let mut materialized = Vec::from(invocation.inputs.clone());
    materialized.push(MaterializedActionInput {
        path: path.into(),
        content: blob(bytes),
    });
    invocation.inputs = materialized.into_boxed_slice();
}

fn first_diagnostic_message(error: &pith_diag::DiagnosticSink) -> &str {
    error
        .iter()
        .next()
        .map(|diagnostic| diagnostic.message.0.as_ref())
        .unwrap_or("")
}

fn captured_blob(
    execution: &pith_engine::CapturedActionExecution,
    position: usize,
) -> Option<&[u8]> {
    match execution.report.outputs.get(position)?.content {
        CapturedOutputContent::Blob(ref bytes) => Some(bytes),
        CapturedOutputContent::Tree(_) => None,
    }
}

fn directory_is_empty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next().transpose().ok())
        .flatten()
        .is_none()
}

#[tokio::test]
async fn an_action_with_no_inputs_or_outputs_succeeds() {
    let Some(invocation) = invocation(":") else {
        return;
    };

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert!(execution.is_ok());
    assert!(
        execution
            .ok()
            .is_some_and(|result| result.report.outputs.is_empty())
    );
}

#[tokio::test]
async fn nested_blob_inputs_are_staged_at_the_declared_path() {
    let Some(mut invocation) =
        invocation("IFS= read -r value < nested/input; printf '%s' \"$value\" > result")
    else {
        return;
    };
    add_blob_input(&mut invocation, "nested/input", b"staged-value\n");
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"staged-value".to_vec())
    );
}

#[tokio::test]
async fn multiple_inputs_are_available_to_one_action() {
    let Some(mut invocation) = invocation(
        "IFS= read -r left < left; IFS= read -r right < data/right; printf '%s:%s' \"$left\" \"$right\" > result",
    ) else {
        return;
    };
    add_blob_input(&mut invocation, "left", b"one\n");
    add_blob_input(&mut invocation, "data/right", b"two\n");
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"one:two".to_vec())
    );
}

#[tokio::test]
async fn only_declared_environment_variables_and_tmpdir_reach_the_child() {
    // `TMPDIR` is the one variable the executor adds: a confined child cannot
    // reach the host's temp directory, so the executor gives it one inside the
    // scratch root. Everything else must come from the spec.
    let Some(mut invocation) = invocation(
        "if [ -z \"${HOME+x}\" ] && [ \"$DECLARED\" = visible ] && [ -d \"$TMPDIR\" ]; then printf yes > result; else printf no > result; fi",
    ) else {
        return;
    };
    invocation.spec.environment = [EnvironmentVariable {
        name: "DECLARED".into(),
        value: "visible".into(),
    }]
    .into();
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"yes".to_vec())
    );
}

#[tokio::test]
async fn the_temporary_directory_sits_outside_the_working_directory() {
    // A tool's temporaries must not be able to collide with a declared path or
    // be mistaken for a declared output, so `TMPDIR` is a sibling of the
    // working directory rather than a subdirectory of it.
    let Some(mut invocation) = invocation(
        "case \"$TMPDIR\" in \"$PWD\"|\"$PWD\"/*) printf inside > result;; *) printf outside > result;; esac",
    ) else {
        return;
    };
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"outside".to_vec())
    );
}

#[tokio::test]
async fn a_temporary_does_not_appear_in_the_working_directory() {
    // `mktemp` puts its file wherever `TMPDIR` says. Listing the working
    // directory afterwards is what catches a temporary directory that is really
    // the working directory under another name.
    let Some(mut invocation) = invocation("created=$(mktemp); ls > result") else {
        return;
    };
    invocation.spec.environment = [EnvironmentVariable {
        name: "PATH".into(),
        value: support::path_for(&["mktemp", "ls"]).into_boxed_str(),
    }]
    .into();
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"result\n".to_vec())
    );
}

#[tokio::test]
async fn empty_and_binary_blob_outputs_are_captured_exactly() {
    let Some(mut invocation) = invocation("printf '\\001\\000\\377' > binary; : > empty") else {
        return;
    };
    invocation.spec.outputs = [
        ActionOutput {
            path: "empty".into(),
            kind: OutputKind::Blob,
        },
        ActionOutput {
            path: "binary".into(),
            kind: OutputKind::Blob,
        },
    ]
    .into();

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .as_ref()
            .ok()
            .and_then(|value| captured_blob(value, 0)),
        Some([].as_slice())
    );
    assert_eq!(
        execution
            .as_ref()
            .ok()
            .and_then(|value| captured_blob(value, 1)),
        Some([1, 0, 255].as_slice())
    );
}

#[tokio::test]
async fn outputs_are_reported_in_declaration_order() {
    let Some(mut invocation) = invocation("printf second > second; printf first > first") else {
        return;
    };
    invocation.spec.outputs = [
        ActionOutput {
            path: "first".into(),
            kind: OutputKind::Blob,
        },
        ActionOutput {
            path: "second".into(),
            kind: OutputKind::Blob,
        },
    ]
    .into();

    let execution = LocalExecutor::new().execute(&invocation).await;
    let paths = execution.ok().map(|value| {
        value
            .report
            .outputs
            .iter()
            .map(|output| output.path.to_string())
            .collect::<Vec<_>>()
    });

    assert_eq!(paths, Some(vec!["first".to_string(), "second".to_string()]));
}

#[tokio::test]
async fn nested_tree_outputs_preserve_files_and_executable_bits() {
    let Some(mut invocation) = invocation(
        "mkdir -p result/nested; printf plain > result/plain; printf tool > result/nested/tool; chmod +x result/nested/tool",
    ) else {
        return;
    };
    invocation.spec.environment = [EnvironmentVariable {
        name: "PATH".into(),
        value: support::path_for(&["mkdir", "chmod"]).into_boxed_str(),
    }]
    .into();
    declare_tree_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;
    assert!(execution.is_ok(), "tree-producing action should succeed");
    let Some(execution) = execution.ok() else {
        return;
    };
    assert_eq!(execution.report.outputs.len(), 1);
    let Some(output) = execution.report.outputs.first() else {
        return;
    };
    assert!(matches!(output.content, CapturedOutputContent::Tree(_)));
    let CapturedOutputContent::Tree(tree) = &output.content else {
        return;
    };

    let plain = tree.entries.iter().find(|entry| entry.name() == "plain");
    assert!(plain.is_some_and(|entry| matches!(entry.content(), CapturedTreeEntryContent::File(file) if file.bytes.as_ref() == b"plain" && !file.executable)));
    let nested = tree.entries.iter().find(|entry| entry.name() == "nested");
    assert!(nested.is_some_and(|entry| matches!(entry.content(), CapturedTreeEntryContent::Tree(child) if child.entries.iter().any(|child_entry| matches!(child_entry.content(), CapturedTreeEntryContent::File(file) if child_entry.name() == "tool" && file.bytes.as_ref() == b"tool" && file.executable)))));
}

#[tokio::test]
async fn materialized_tree_inputs_preserve_nesting_executability_and_symlinks() {
    let plain_bytes = b"tree-value\n";
    let executable_bytes = b"#!/bin/sh\n";
    let child = MaterializedTree {
        id: ContentId::of_tree(b"child"),
        entries: [TreeEntry::new(
            "plain",
            MaterializedTreeEntryContent::File(MaterializedFileContent {
                content: ContentId::of_blob(plain_bytes),
                executable: false,
                bytes: plain_bytes.as_slice().into(),
            }),
        )
        .ok()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_boxed_slice(),
    };
    let tree = MaterializedTree {
        id: ContentId::of_tree(b"root"),
        entries: [
            TreeEntry::new("nested", MaterializedTreeEntryContent::Tree(child)).ok(),
            TreeEntry::new(
                "tool",
                MaterializedTreeEntryContent::File(MaterializedFileContent {
                    content: ContentId::of_blob(executable_bytes),
                    executable: true,
                    bytes: executable_bytes.as_slice().into(),
                }),
            )
            .ok(),
            TreeEntry::new(
                "alias",
                MaterializedTreeEntryContent::Symlink {
                    target: b"nested/plain".as_slice().into(),
                },
            )
            .ok(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_boxed_slice(),
    };
    let Some(mut invocation) = invocation(
        "IFS= read -r direct < tree/nested/plain; IFS= read -r linked < tree/alias; if [ -x tree/tool ]; then printf '%s:%s:yes' \"$direct\" \"$linked\" > result; fi",
    ) else {
        return;
    };
    invocation.spec.inputs = [ActionInput {
        path: "tree".into(),
        content: Content::Tree(tree.id),
    }]
    .into();
    invocation.inputs = [MaterializedActionInput {
        path: "tree".into(),
        content: MaterializedContent::Tree(tree),
    }]
    .into();
    declare_blob_output(&mut invocation, "result");

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution
            .ok()
            .and_then(|value| captured_blob(&value, 0).map(<[u8]>::to_vec)),
        Some(b"tree-value:tree-value:yes".to_vec())
    );
}

#[tokio::test]
async fn exact_host_platform_is_accepted_and_reported_literally() {
    let Some(mut invocation) = invocation(":") else {
        return;
    };
    invocation.spec.platform = PlatformRequirement::Exact {
        operating_system: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    };

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert!(execution.ok().is_some_and(|result| {
        result.report.platform.operating_system.as_ref() == std::env::consts::OS
            && result.report.platform.architecture.as_ref() == std::env::consts::ARCH
    }));
}

#[tokio::test]
async fn declared_capabilities_are_preserved_in_the_report() {
    let Some(mut invocation) = invocation(":") else {
        return;
    };
    let capabilities: Box<[CapabilityRequirement]> = [CapabilityRequirement {
        name: "filesystem.read".into(),
        scope: "input".into(),
    }]
    .into();
    invocation.spec.capabilities = capabilities.clone();

    let execution = LocalExecutor::new().execute(&invocation).await;

    assert_eq!(
        execution.ok().map(|result| result.report.capabilities_used),
        Some(capabilities)
    );
}

#[tokio::test]
async fn allow_hosts_is_refused_without_running_the_action() {
    let base = tempfile::tempdir().ok();
    let Some(base) = base else {
        return;
    };
    let marker = base.path().join("marker");
    let Some(mut invocation) = invocation(&format!("printf ran > '{}'", marker.display())) else {
        return;
    };
    invocation.spec.network = NetworkPolicy::AllowHosts(["example.test".into()].into());

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(
        error
            .as_ref()
            .is_some_and(|sink| first_diagnostic_message(sink).contains("network"))
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn a_platform_architecture_mismatch_fails_before_execution() {
    let Some(mut invocation) = invocation(":") else {
        return;
    };
    invocation.spec.platform = PlatformRequirement::Exact {
        operating_system: std::env::consts::OS.into(),
        architecture: "definitely-not-the-host".into(),
    };

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(error.as_ref().is_some_and(|sink| {
        let message = first_diagnostic_message(sink);
        message.contains(std::env::consts::ARCH) && message.contains("definitely-not-the-host")
    }));
}

#[tokio::test]
async fn a_missing_declared_output_is_a_capture_error() {
    let Some(mut invocation) = invocation(":") else {
        return;
    };
    declare_blob_output(&mut invocation, "missing");

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(error.as_ref().is_some_and(|sink| {
        let message = first_diagnostic_message(sink);
        message.contains("capturing blob output") && message.contains("No such file")
    }));
}

#[tokio::test]
async fn declaring_a_file_as_a_tree_is_a_capture_error() {
    let Some(mut invocation) = invocation("printf not-a-tree > result") else {
        return;
    };
    declare_tree_output(&mut invocation, "result");

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(
        error
            .as_ref()
            .is_some_and(|sink| first_diagnostic_message(sink).contains("tree output directory"))
    );
}

#[tokio::test]
async fn an_executable_path_that_does_not_exist_is_a_spawn_error() {
    // The executable is a host path (decision 0030). A path that does not exist
    // on the host fails at spawn with an adapter diagnostic, before any child
    // runs.
    let invocation = ActionInvocation {
        spec: ActionSpec {
            executable: ActionProgram::HostPath("/bin/definitely-not-a-real-executable".into()),
            toolchain: Box::new([]),
            arguments: Box::new([]),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        },
        inputs: Box::new([]),
        program: None,
    };

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(error.as_ref().is_some_and(|sink| {
        sink.iter()
            .next()
            .is_some_and(|diagnostic| diagnostic.code.0 == 1211)
            && !first_diagnostic_message(sink).is_empty()
    }));
}

#[tokio::test]
async fn nonzero_exit_status_is_reported_numerically() {
    let Some(invocation) = invocation("exit 37") else {
        return;
    };

    let error = LocalExecutor::new().execute(&invocation).await.err();

    assert!(
        error
            .as_ref()
            .is_some_and(|sink| first_diagnostic_message(sink).contains("status 37"))
    );
}

#[tokio::test]
async fn a_reported_contract_survives_a_nonzero_exit_and_carries_the_status() {
    // `exit 37` under `Reported`: the status is the result, so a rule can read
    // a verdict from a program that exits nonzero to express one (decision 0037).
    let Some(mut invocation) = invocation("exit 37") else {
        return;
    };
    invocation.spec.exit_status = ExitStatusContract::Reported;

    let captured = LocalExecutor::new()
        .execute(&invocation)
        .await
        .expect("a reported nonzero exit is not a failure");

    assert_eq!(captured.exit, Some(ActionExit::Code(37)));
}

/// A crashed program and a program reporting failures are different facts, and a
/// rule turning either into a verdict has to tell them apart, so the reported
/// status keeps the signal separate from the code.
///
/// The signal here is `SIGSYS` from the seccomp filter, because `kill(2)` is
/// outside the allowlist and the shell has no other way to signal itself. Under
/// `Reported` a sandbox kill also reaches the rule as a fact for it to judge, so
/// a rule reading every signal as a pass would call a confinement violation a
/// success. 0037's "unresolved" section carries that.
#[cfg(target_arch = "x86_64")]
#[tokio::test]
async fn a_reported_contract_distinguishes_a_signal_from_a_status() {
    let Some(mut invocation) = invocation("kill -0 $$") else {
        return;
    };
    invocation.spec.exit_status = ExitStatusContract::Reported;

    let captured = LocalExecutor::new()
        .execute(&invocation)
        .await
        .expect("a reported signal death is not an executor failure");

    assert_eq!(captured.exit, Some(ActionExit::Signal(libc::SIGSYS)));
}

#[tokio::test]
async fn a_custom_scratch_base_is_cleaned_after_success() {
    let Some(base) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(invocation) = invocation(":") else {
        return;
    };
    let executor = LocalExecutor::with_scratch_base(base.path().to_path_buf());

    assert!(executor.execute(&invocation).await.is_ok());
    assert!(directory_is_empty(base.path()));
}

#[tokio::test]
async fn a_custom_scratch_base_is_cleaned_after_action_failure() {
    let Some(base) = tempfile::tempdir().ok() else {
        return;
    };
    let Some(invocation) = invocation("exit 1") else {
        return;
    };
    let executor = LocalExecutor::with_scratch_base(base.path().to_path_buf());

    assert!(executor.execute(&invocation).await.is_err());
    assert!(directory_is_empty(base.path()));
}

#[tokio::test]
async fn a_missing_custom_scratch_base_fails_cleanly() {
    let Some(base) = tempfile::tempdir().ok() else {
        return;
    };
    let missing = base.path().join("does-not-exist");
    let Some(invocation) = invocation(":") else {
        return;
    };
    let executor = LocalExecutor::with_scratch_base(missing);

    let error = executor.execute(&invocation).await.err();

    assert!(error.as_ref().is_some_and(|sink| {
        first_diagnostic_message(sink).contains("could not create scratch root")
    }));
}

#[tokio::test]
async fn a_failing_action_reports_what_it_wrote_to_stderr() {
    let Some(invocation) = invocation("echo 'the compiler said no' >&2; exit 1") else {
        return;
    };

    let error = LocalExecutor::new()
        .execute(&invocation)
        .await
        .expect_err("nonzero exit");

    // An exit status alone says something went wrong and nothing about what.
    let message = first_diagnostic_message(&error);
    assert!(
        message.contains("the compiler said no"),
        "the diagnostic should carry the action's stderr, got: {message}"
    );
}

#[tokio::test]
async fn a_long_stderr_is_excerpted_rather_than_inlined_whole() {
    // 20k lines is a modest amount for a real toolchain and far more than
    // belongs in one diagnostic.
    let Some(invocation) = invocation(
        "i=0; while [ $i -lt 20000 ]; do echo padding >&2; i=$((i+1)); done; echo FINAL >&2; exit 1",
    ) else {
        return;
    };

    let error = LocalExecutor::new()
        .execute(&invocation)
        .await
        .expect_err("nonzero exit");

    let message = first_diagnostic_message(&error);
    assert!(
        message.len() < 8192,
        "the diagnostic should be bounded, got {} bytes",
        message.len()
    );
    // The tail is kept, because that is where a failing tool puts the reason.
    assert!(
        message.contains("FINAL"),
        "the excerpt should keep the end of stderr, got: {message}"
    );
    // Says it dropped something and how much, without the test restating the
    // arithmetic the excerpt already did.
    assert!(
        message.contains("stderr (last ") && message.contains(" bytes): "),
        "the excerpt should say how much it dropped, got: {message}"
    );
    // Cut at a line boundary, so the excerpt does not open mid-word.
    let (_, excerpt) = message
        .split_once("bytes): ")
        .expect("the diagnostic names its excerpt");
    assert!(
        excerpt.starts_with("padding"),
        "the excerpt should start at a line boundary, got: {excerpt}"
    );
}
