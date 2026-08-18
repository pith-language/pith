//! A portable executor for the assembly action: stages the blobs, runs the
//! contract's own shell script with the declared environment, and captures
//! the declared tree back by walking what the script built.
//!
//! It runs the same derived script the confined executor runs, on any host
//! with a POSIX shell, which keeps the artifact's claims checkable where the
//! first-party executor does not build. It claims no confinement and reports
//! `Unverified`, which is what it installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use pith_core::{ActionProgram, OutputKind};
use pith_diag::{Diag, DiagnosticSink, PithResult, Severity, Span, StableCode};
use pith_engine::{
    AccessVerification, ActionExit, ActionInvocation, CapturedActionExecution,
    CapturedExecutionReport, CapturedFileContent, CapturedOutput, CapturedOutputContent,
    CapturedTree, CapturedTreeEntryContent, ExecutionPlatform, Executor, ExecutorIdentity,
    MaterializedContent,
};
use pith_store::TreeEntry;
use tempfile::TempDir;

fn diag(message: String) -> DiagnosticSink {
    let mut sink = DiagnosticSink::new();
    sink.push(Diag::new(
        Severity::Error,
        StableCode(9006),
        Span::none(),
        message,
    ));
    sink
}

fn host_platform() -> ExecutionPlatform {
    ExecutionPlatform {
        operating_system: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    }
}

pub(crate) struct AssemblerExecutor {
    executions: AtomicUsize,
}

impl AssemblerExecutor {
    pub(crate) fn new() -> Self {
        Self {
            executions: AtomicUsize::new(0),
        }
    }

    pub(crate) fn executions(&self) -> usize {
        self.executions.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl Executor for AssemblerExecutor {
    fn identity(&self) -> ExecutorIdentity {
        ExecutorIdentity {
            executor: "stele-assembler".into(),
            platform: host_platform(),
        }
    }

    async fn execute(&self, invocation: &ActionInvocation) -> PithResult<CapturedActionExecution> {
        self.executions.fetch_add(1, Ordering::Relaxed);
        let root = match TempDir::new() {
            Ok(root) => root,
            Err(error) => {
                return Err(diag(format!(
                    "the fixture could not create a root: {error}"
                )));
            }
        };
        let work = root.path().join("work");
        if let Err(error) = fs::create_dir_all(&work) {
            return Err(diag(format!(
                "the fixture could not create a working directory: {error}"
            )));
        }

        for input in &invocation.inputs {
            let destination = work.join(input.path.as_ref());
            match &input.content {
                MaterializedContent::Blob(blob) => {
                    if let Some(parent) = destination.parent()
                        && let Err(error) = fs::create_dir_all(parent)
                    {
                        return Err(diag(format!(
                            "the fixture could not stage `{}`: {error}",
                            input.path
                        )));
                    }
                    if let Err(error) = fs::write(&destination, &blob.bytes) {
                        return Err(diag(format!(
                            "the fixture could not stage `{}`: {error}",
                            input.path
                        )));
                    }
                }
                MaterializedContent::Tree(_) => {
                    return Err(diag("the fixture stages blobs, not trees".to_string()));
                }
            }
        }

        let ActionProgram::HostPath(shell) = &invocation.spec.executable else {
            return Err(diag("the fixture runs host-path programs only".to_string()));
        };
        let Some(script) = invocation.spec.arguments.get(1) else {
            return Err(diag("the contract carried no script argument".to_string()));
        };
        let mut command = Command::new(shell.as_ref());
        command
            .arg("-c")
            .arg(script.as_ref())
            .current_dir(&work)
            .env_clear();
        for variable in &invocation.spec.environment {
            command.env(variable.name.as_ref(), variable.value.as_ref());
        }
        let output = match command.output() {
            Ok(output) => output,
            Err(error) => {
                return Err(diag(format!(
                    "the fixture could not run the script: {error}"
                )));
            }
        };
        if !output.status.success() {
            return Err(diag(format!(
                "the assembly script failed: {}{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let Some(declared) = invocation
            .spec
            .outputs
            .iter()
            .find(|output| output.kind == OutputKind::Tree)
        else {
            return Err(diag("the contract declared no tree output".to_string()));
        };
        let tree = capture_tree(&work.join(declared.path.as_ref()))?;
        let identity = self.identity();
        Ok(CapturedActionExecution {
            report: CapturedExecutionReport {
                executor: identity.executor,
                platform: identity.platform,
                access: AccessVerification::Unverified,
                outputs: vec![CapturedOutput {
                    path: declared.path.clone(),
                    content: CapturedOutputContent::Tree(tree),
                }]
                .into(),
                capabilities_used: Box::new([]),
            },
            exit: Some(ActionExit::Code(0)),
        })
    }
}

fn capture_tree(path: &Path) -> PithResult<CapturedTree> {
    let reader = match fs::read_dir(path) {
        Ok(reader) => reader,
        Err(error) => {
            return Err(diag(format!(
                "the fixture could not read {path:?}: {error}"
            )));
        }
    };
    let mut names: Vec<(String, std::fs::DirEntry)> = Vec::new();
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        names.push((name, entry));
    }
    names.sort_by(|(left, _), (right, _)| left.cmp(right));

    let mut entries = Vec::with_capacity(names.len());
    for (name, entry) in names {
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => return Err(diag(format!("the fixture could not type {name}: {error}"))),
        };
        let path: PathBuf = entry.path();
        let content = if file_type.is_symlink() {
            let target = match fs::read_link(&path) {
                Ok(target) => target,
                Err(error) => {
                    return Err(diag(format!(
                        "the fixture could not read the link {name}: {error}"
                    )));
                }
            };
            CapturedTreeEntryContent::Symlink {
                target: target_bytes(&target),
            }
        } else if file_type.is_dir() {
            CapturedTreeEntryContent::Tree(capture_tree(&path)?)
        } else {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(diag(format!("the fixture could not read {name}: {error}")));
                }
            };
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                fs::metadata(&path)
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            };
            #[cfg(not(unix))]
            let executable = false;
            CapturedTreeEntryContent::File(CapturedFileContent {
                bytes: bytes.into(),
                executable,
            })
        };
        let entry = match TreeEntry::new(name.as_str(), content) {
            Ok(entry) => entry,
            Err(error) => return Err(diag(format!("a captured entry is invalid: {error}"))),
        };
        entries.push(entry);
    }
    Ok(CapturedTree {
        entries: entries.into(),
    })
}

fn target_bytes(target: &Path) -> Box<[u8]> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        target.as_os_str().as_bytes().into()
    }
    #[cfg(not(unix))]
    {
        target
            .to_string_lossy()
            .into_owned()
            .into_boxed_str()
            .into_boxed_bytes()
    }
}
