//! End-to-end tests for the local executor (decision 0028).
//!
//! These run real child processes through [`LocalExecutor`], proving the
//! stage → fork/exec → capture path works. The executable is `/bin/sh` bytes
//! staged from a materialized blob, so the tests exercise the same byte-staging
//! path the engine's `materialize_action` produces without depending on the
//! engine.

#![cfg(target_os = "linux")]

use pith_core::{
    ActionInput, ActionOutput, ActionSpec, Content, EnvironmentVariable, NetworkPolicy, OutputKind,
    PlatformRequirement,
};
use pith_engine::{
    AccessVerification, ActionInvocation, Executor, MaterializedBlob, MaterializedContent,
};
use pith_executor_local::LocalExecutor;
use pith_ids::ContentId;

/// Read `/bin/sh` once; it is the executable the action runs. Staged as a blob
/// the way the engine would stage it. Returns `None` if `/bin/sh` is missing so
/// each `#[test]` can fail loudly via its own assertion rather than a helper
/// panic; tests skip with a clear message in that case.
fn shell_blob() -> Option<MaterializedContent> {
    let bytes = std::fs::read("/bin/sh").ok()?;
    let id = ContentId::of_blob(&bytes);
    Some(MaterializedContent::Blob(MaterializedBlob {
        id,
        bytes: bytes.into(),
    }))
}

/// Build an invocation that runs `/bin/sh -c <script>` with `operand` staged as
/// an input and `result` declared as a blob output.
fn invocation(shell: MaterializedContent, script: &str, operand: &str) -> ActionInvocation {
    let executable = ContentId::of_blob(b"shell");
    let spec = ActionSpec {
        executable,
        arguments: ["-c".into(), script.into()].into(),
        inputs: [ActionInput {
            path: "operand".into(),
            content: Content::Blob(ContentId::of_blob(operand.as_bytes())),
        }]
        .into(),
        outputs: [ActionOutput {
            path: "result".into(),
            kind: OutputKind::Blob,
        }]
        .into(),
        environment: [EnvironmentVariable {
            name: "PATH".into(),
            value: "/usr/bin:/bin".into(),
        }]
        .into(),
        platform: PlatformRequirement::Any,
        capabilities: [].into(),
        network: NetworkPolicy::Deny,
    };
    ActionInvocation {
        spec,
        executable: shell,
        inputs: [pith_engine::MaterializedActionInput {
            path: "operand".into(),
            content: MaterializedContent::Blob(MaterializedBlob {
                id: ContentId::of_blob(operand.as_bytes()),
                bytes: operand.as_bytes().to_vec().into_boxed_slice(),
            }),
        }]
        .into(),
    }
}

#[tokio::test]
async fn runs_a_real_child_and_captures_declared_output() {
    let executor = LocalExecutor::new();
    // Double the input's length and write it to the declared output path.
    let Some(shell) = shell_blob() else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
    let invocation = invocation(shell, "wc -c < operand | tr -d ' ' > result", "hello");
    let captured = executor
        .execute(&invocation)
        .await
        .expect("execution succeeds");

    assert_eq!(captured.report.executor.as_ref(), "pith-executor-local");
    assert_eq!(captured.report.platform.operating_system.as_ref(), "linux");
    assert_eq!(captured.report.outputs.len(), 1);
    let output = captured.report.outputs.first().expect("one output");
    assert_eq!(output.path.as_ref(), "result");
    match &output.content {
        pith_engine::CapturedOutputContent::Blob(bytes) => {
            // `wc -c < "hello"` is 5 bytes.
            let text = std::str::from_utf8(bytes).expect("utf-8 output");
            assert_eq!(text.trim(), "5");
        }
        other => unreachable!("expected a blob output, got {other:?}"),
    }
}

#[tokio::test]
async fn reports_unverified_until_both_layers_are_installed() {
    // The first build installs no_new_privs only (see sys_seccomp /
    // sys_landlock), so the honest report is Unverified. When landlock and the
    // full seccomp filter land, this assertion flips to Prevented.
    let executor = LocalExecutor::new();
    let Some(shell) = shell_blob() else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
    let invocation = invocation(shell, "true > result", "x");
    let captured = executor
        .execute(&invocation)
        .await
        .expect("execution succeeds");
    assert_eq!(captured.report.access, AccessVerification::Unverified);
}

#[tokio::test]
async fn refuses_specs_that_request_network_access() {
    let executor = LocalExecutor::new();
    let Some(shell) = shell_blob() else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
    let mut invocation = invocation(shell, "true > result", "x");
    invocation.spec.network = NetworkPolicy::AllowAll;
    let error = executor
        .execute(&invocation)
        .await
        .expect_err("network refused");
    let message = error
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains("network"),
        "the error should name network access, got: {message}"
    );
}

#[tokio::test]
async fn refuses_specs_that_require_a_different_platform() {
    let executor = LocalExecutor::new();
    let Some(shell) = shell_blob() else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
    let mut invocation = invocation(shell, "true > result", "x");
    invocation.spec.platform = PlatformRequirement::Exact {
        operating_system: "plan9".into(),
        architecture: "x86_64".into(),
    };
    let error = executor
        .execute(&invocation)
        .await
        .expect_err("platform refused");
    let message = error
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains("plan9"),
        "the error should name the required platform, got: {message}"
    );
}

#[tokio::test]
async fn action_failure_surfaces_as_a_diagnostic() {
    let executor = LocalExecutor::new();
    // `false` exits nonzero and produces no output.
    let Some(shell) = shell_blob() else {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    };
    let invocation = invocation(shell, "false", "x");
    let error = executor
        .execute(&invocation)
        .await
        .expect_err("nonzero exit");
    let message = error
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains("exit status 1"),
        "the error should name the exit status, got: {message}"
    );
    // `false` is silent, and saying so is worth a few words: it tells the
    // reader the executor looked rather than that it did not bother.
    assert!(
        message.contains("nothing to stderr"),
        "the error should say the action was silent, got: {message}"
    );
}
