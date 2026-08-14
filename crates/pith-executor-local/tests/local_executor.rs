//! End-to-end tests for the local executor (decision 0028).
//!
//! These run real child processes through [`LocalExecutor`], proving the
//! stage → fork/exec → capture path works. The executable is the host path
//! `/bin/sh` (decision 0030), so the tests exercise the same execve-a-path
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

mod support;

/// Whether `/bin/sh` exists on the host, so each `#[test]` can fail loudly via
/// its own assertion rather than a helper panic; tests skip with a clear message
/// otherwise.
fn shell_present() -> bool {
    std::fs::read("/bin/sh").is_ok()
}

/// Build an invocation that runs `/bin/sh -c <script>` with `operand` staged as
/// an input and `result` declared as a blob output.
fn invocation(script: &str, operand: &str) -> ActionInvocation {
    let spec = ActionSpec {
        executable: "/bin/sh".into(),
        toolchain: support::closure_for(&["/bin/sh", "wc", "tr"]),
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
            value: support::path_for(&["wc", "tr"]).into_boxed_str(),
        }]
        .into(),
        platform: PlatformRequirement::Any,
        capabilities: [].into(),
        network: NetworkPolicy::Deny,
    };
    ActionInvocation {
        spec,
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
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    let invocation = invocation("wc -c < operand | tr -d ' ' > result", "hello");
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
async fn reports_prevented_when_both_layers_are_installed() {
    // This build installs the landlock path-confinement ruleset and the seccomp
    // syscall allowlist (decisions 0028, 0030), so the report is Prevented,
    // which 0028 reserves for both layers. On an architecture the seccomp
    // filter does not target, the honest report would still be Observed.
    let executor = LocalExecutor::new();
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    let invocation = invocation("true > result", "x");
    let captured = executor
        .execute(&invocation)
        .await
        .expect("execution succeeds");
    #[cfg(target_arch = "x86_64")]
    assert_eq!(captured.report.access, AccessVerification::Prevented);
    #[cfg(not(target_arch = "x86_64"))]
    assert_eq!(captured.report.access, AccessVerification::Observed);
}

/// `kill(2)` is absent from the seccomp allowlist, and the shell's `kill` is a
/// builtin, so the script issues the syscall in its own process. Unfiltered,
/// the same script exits zero, which is what makes the death here evidence.
///
/// x86_64 only, since that is where the filter is installed at all.
#[cfg(target_arch = "x86_64")]
#[tokio::test]
async fn a_forbidden_syscall_kills_the_child() {
    let executor = LocalExecutor::new();
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    // Signal zero: the kernel checks permission and delivers nothing, so an
    // unfiltered run of this script succeeds.
    let invocation = invocation("kill -0 $$ > result", "x");
    let error = executor
        .execute(&invocation)
        .await
        .expect_err("a forbidden syscall kills the child");
    let message = error
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains(&format!("signal {}", libc::SIGSYS)),
        "the child should have died on SIGSYS, got: {message}"
    );
}

#[tokio::test]
async fn an_undeclared_path_is_denied_by_the_ruleset() {
    let executor = LocalExecutor::new();
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    // The file is world-readable and outside the declared closure, so without
    // the ruleset the child reads it and the action succeeds. The shell says
    // "Permission denied" for a denied open and "No such file" for a missing
    // one, so the message tells the two apart.
    const UNDECLARED: &str = "/etc/hostname";
    assert!(
        std::fs::read(UNDECLARED).is_ok(),
        "{UNDECLARED} must be readable for this test to mean anything"
    );
    let invocation = invocation(&format!("read -r line < {UNDECLARED}"), "x");
    let error = executor
        .execute(&invocation)
        .await
        .expect_err("an undeclared read is denied");
    let message = error
        .iter()
        .next()
        .expect("one diagnostic")
        .message
        .0
        .as_ref();
    assert!(
        message.contains("Permission denied"),
        "the child should have been denied the undeclared path, got: {message}"
    );
}

#[tokio::test]
async fn refuses_specs_that_request_network_access() {
    let executor = LocalExecutor::new();
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    let mut invocation = invocation("true > result", "x");
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
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    let mut invocation = invocation("true > result", "x");
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
    if !shell_present() {
        eprintln!("skipping: /bin/sh is not readable");
        return;
    }
    let invocation = invocation("false", "x");
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
