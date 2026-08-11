//! The process driver and `Executor` implementation (decision 0028).
//!
//! Orchestrates [`crate::stage`] → fork/exec → [`crate::capture`], installing
//! the sandbox filters in the child between `fork` and `execve`. The sandbox is
//! installed via a `pre_exec` hook on `tokio::process::Command`, which is the
//! async-safe place to run `prctl`/`seccomp`/`landlock` setup.

use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use pith_core::{NetworkPolicy, PlatformRequirement};
use pith_engine::{
    AccessVerification, ActionInvocation, CapturedActionExecution, CapturedExecutionReport,
    ExecutionPlatform, Executor,
};
use tokio::process::Command;

use crate::capture::capture;
use crate::stage;
use crate::sys_landlock::landlock_installed;
use crate::sys_seccomp::{register_sandbox_hook, seccomp_filter_installed};

/// Configuration for the local executor.
#[derive(Clone, Debug)]
pub struct LocalExecutor {
    /// The base directory under which per-action scratch roots are created. If
    /// `None`, the platform temp directory is used.
    scratch_base: Option<PathBuf>,
}

impl LocalExecutor {
    /// Construct an executor that creates scratch roots under the platform temp
    /// directory.
    #[must_use]
    pub fn new() -> Self {
        Self { scratch_base: None }
    }

    /// Construct an executor that creates scratch roots under `base`.
    #[must_use]
    pub fn with_scratch_base(base: PathBuf) -> Self {
        Self {
            scratch_base: Some(base),
        }
    }

    /// The [`AccessVerification`] this build reports, given which confinement
    /// layers it actually installed. `Prevented` requires both landlock and
    /// seccomp; `Observed` is seccomp-only; `Unverified` is neither. Because
    /// neither layer is fully installed in this build (see
    /// [`crate::sys_landlock`] and [`crate::sys_seccomp`]), this is currently
    /// `Unverified`.
    fn access_verification() -> AccessVerification {
        match (landlock_installed(), seccomp_filter_installed()) {
            (true, true) => AccessVerification::Prevented,
            (false, true) => AccessVerification::Observed,
            (true, false) => AccessVerification::Observed,
            (false, false) => AccessVerification::Unverified,
        }
    }

    /// The platform this executor runs on. Detected from `std::env::consts` so
    /// it matches the host the executor binary runs on.
    fn platform() -> ExecutionPlatform {
        ExecutionPlatform {
            operating_system: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        }
    }
}

impl Default for LocalExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn execute(
        &self,
        invocation: &ActionInvocation,
    ) -> pith_diag::PithResult<CapturedActionExecution> {
        // The first version of this executor does not honor network access
        // (decision 0028 "unresolved"). Refuse rather than silently relaxing
        // the seccomp filter, so the declared contract is never quietly
        // violated.
        if !matches!(invocation.spec.network, NetworkPolicy::Deny) {
            return Err(crate::executor_diag(
                "the local executor does not honor network access in this build; \
                 a spec requesting network is refused rather than silently permitted",
            ));
        }

        // If the spec pins an exact platform, it must match the host this
        // executor runs on. The engine validates this again on the report; do
        // it here too so a mismatch fails fast before staging a scratch root.
        if let PlatformRequirement::Exact {
            operating_system,
            architecture,
        } = &invocation.spec.platform
            && (operating_system.as_ref() != std::env::consts::OS
                || architecture.as_ref() != std::env::consts::ARCH)
        {
            return Err(crate::executor_diag(format!(
                "the local executor runs on {}-{}, but the spec requires {}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                operating_system,
                architecture
            )));
        }

        let scratch = create_scratch_root(&self.scratch_base)?;
        let result = run_in_scratch(invocation, scratch.path()).await;
        // The scratch root is cleaned up when `scratch` drops, regardless of
        // outcome. Outputs were already captured into memory by `run_in_scratch`.
        drop(scratch);
        result
    }
}

fn create_scratch_root(base: &Option<PathBuf>) -> pith_diag::PithResult<tempfile::TempDir> {
    match base {
        Some(base) => tempfile::Builder::new()
            .prefix("pith-action-")
            .tempdir_in(base),
        None => tempfile::Builder::new().prefix("pith-action-").tempdir(),
    }
    .map_err(|error| crate::executor_diag(format!("could not create scratch root: {error}")))
}

/// How much of a failed action's stderr the diagnostic carries. A build tool
/// can produce megabytes; the tail is where the error that stopped it is, and
/// the rest belongs in a log the executor does not yet keep.
const STDERR_EXCERPT: usize = 4096;

/// Why the child stopped, in terms the reader can act on. A signal is not an
/// exit code, and reporting one as the other is how "killed by the OOM killer"
/// gets mistaken for "the compiler rejected the input".
fn exit_description(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;

    if let Some(code) = status.code() {
        format!("exit status {code}")
    } else if let Some(signal) = status.signal() {
        format!("signal {signal}")
    } else {
        "no exit status".to_string()
    }
}

/// The tail of the child's stderr, if it wrote any. Without this a failed
/// action reports only its exit status, which says that something went wrong
/// and nothing about what.
fn stderr_note(stderr: &[u8]) -> String {
    let excerpt = stderr_excerpt(stderr);
    let text = String::from_utf8_lossy(excerpt);
    let text = text.trim();
    if text.is_empty() {
        return "; it wrote nothing to stderr".to_string();
    }
    if excerpt.len() < stderr.len() {
        return format!(
            "; stderr (last {} of {} bytes): {text}",
            excerpt.len(),
            stderr.len()
        );
    }
    format!("; stderr: {text}")
}

/// The tail of `stderr`, cut at a line boundary so the excerpt does not begin
/// mid-word. Falls back to the byte cut when the tail holds no newline, which
/// is the case for a tool that wrote one very long line.
fn stderr_excerpt(stderr: &[u8]) -> &[u8] {
    let Some(from) = stderr.len().checked_sub(STDERR_EXCERPT) else {
        return stderr;
    };
    let Some(tail) = stderr.get(from..) else {
        return stderr;
    };
    match tail.iter().position(|byte| *byte == b'\n') {
        Some(newline) => tail.get(newline.saturating_add(1)..).unwrap_or(tail),
        None => tail,
    }
}

/// Fork the child and wait for its output.
async fn run_child(command: &mut Command) -> pith_diag::PithResult<std::process::Output> {
    let child = command
        .spawn()
        .map_err(|error| crate::executor_diag(format!("spawning the action failed: {error}")))?;
    child
        .wait_with_output()
        .await
        .map_err(|error| crate::executor_diag(format!("waiting for the action failed: {error}")))
}

async fn run_in_scratch(
    invocation: &ActionInvocation,
    scratch_root: &std::path::Path,
) -> pith_diag::PithResult<CapturedActionExecution> {
    let staged = stage::stage(invocation, scratch_root).await?;
    let declared_outputs = stage::declared_outputs(invocation);

    let mut command = Command::new(staged.executable.as_ref());
    command
        .args(invocation.spec.arguments.iter().map(|arg| arg.as_ref()))
        .current_dir(&staged.working_dir)
        // A clean environment: only the variables the spec declared, with no
        // ambient inheritance. This is the least-authority default.
        .env_clear()
        .envs(
            invocation
                .spec
                .environment
                .iter()
                .map(|var| (var.name.as_ref(), var.value.as_ref())),
        )
        .stdin(Stdio::null())
        // Piped explicitly because the child is spawned rather than run through
        // `output()`, which would set these itself.
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The engine drops this future to cancel an action, so the child has to
        // die with it. Without this a cancelled action leaves a process running
        // against a scratch root that is about to be deleted.
        .kill_on_drop(true);
    register_sandbox_hook(&mut command);

    let output = run_child(&mut command).await?;
    if !output.status.success() {
        return Err(crate::executor_diag(format!(
            "the action failed with {}{}",
            exit_description(&output.status),
            stderr_note(&output.stderr)
        )));
    }

    let captured_outputs = capture(&staged.working_dir, &declared_outputs).await?;

    Ok(CapturedActionExecution {
        report: CapturedExecutionReport {
            executor: "pith-executor-local".into(),
            platform: LocalExecutor::platform(),
            access: LocalExecutor::access_verification(),
            outputs: captured_outputs.into_boxed_slice(),
            // The executor enforced whatever capabilities the spec declared via
            // the sandbox; it adds none of its own. The engine's
            // `validate_execution` confirms the reported set is a subset of the
            // declared set.
            capabilities_used: invocation.spec.capabilities.clone(),
        },
    })
}
