//! The assembly action and the entry that requests it.
//!
//! Everything above the action is decided from values; the action exists
//! because content enters the store only through an executor's capture. Its
//! contract stages each file's bytes at a neutral path under `pool/`, hands
//! the three rendered texts to the script as environment, and derives the
//! script itself from the canonical file set: one `mkdir -p` for the
//! directories, one `cat` per staged file, one `printf` per text, one
//! `chmod` per executable flag, one `ln -s` per symlink. The script is a
//! derived fact of the contract — in its arguments, in its digest, and in
//! `plan_action`'s answer — rather than a program anyone writes by hand.
//!
//! The input and output paths of one contract may not overlap, which is why
//! staging and artifact live in two disjoint trees inside the scratch root:
//! the executor stages `pool/`, the child builds `system/`, and capture reads
//! `system/` back as the declared tree output.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionProgram, ActionSpec, BodyRevision, Content,
    EnvironmentVariable, ExitStatusContract, NetworkPolicy, OutputKind, PlatformRequirement, Pure,
    Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionRule, PureRule, PureRuleFrame, PureStep, Resumption};

use crate::rules::{diag, field_of, file_entries_of, representation_of, text_of, unit_parts_of};
use crate::types::{self, FileBody, MODULE};

/// Where the artifact is built inside the working directory, and the declared
/// tree output's path.
const ARTIFACT: &str = "system";

/// Where file bytes are staged, disjoint from the artifact so no declared
/// input path overlaps the declared output.
const STAGED: &str = "pool";

/// The artifact-relative paths of the three rendered texts.
const PASSWD_ENTRY: &str = "etc/passwd";
const BOOT_DIR: &str = "boot/loader/entries";
const UNIT_DIR: &str = "etc/systemd/system";

/// The environment variables the script receives the rendered texts through.
const ENV_BOOT: &str = "STELE_BOOT";
const ENV_PASSWD: &str = "STELE_PASSWD";
const ENV_UNIT: &str = "STELE_UNIT";

/// Assemble the composed system's tree artifact.
pub struct AssembleAction;

impl AssembleAction {
    #[must_use]
    pub fn rule() -> Rule<Action> {
        Rule::<Action>::declared(
            MODULE,
            types::ASSEMBLE,
            BodyRevision(1),
            types::assemble_interface(),
            Span::none(),
        )
    }
}

/// The host programs an assembly runs and the closure they need, as the
/// tools record spells them.
struct ToolPaths {
    shell: Box<str>,
    mkdir: Box<str>,
    cat: Box<str>,
    chmod: Box<str>,
    ln: Box<str>,
    closure: Vec<Box<str>>,
}

fn tools_of(value: &Value) -> PithResult<ToolPaths> {
    let record = representation_of(value, types::tools())?;
    let tool = |field: &str| -> PithResult<Box<str>> {
        let Some(path) = field_of(record, field) else {
            return Err(diag(&format!(
                "the tools record is missing its `{field}` program"
            )));
        };
        let path = text_of(path)?;
        if !path.starts_with('/') || path.contains('\0') || path.contains('\\') {
            return Err(diag(&format!(
                "the `{field}` program must be an absolute path, not `{path}`"
            )));
        }
        Ok(path.into())
    };
    let closure = match field_of(record, types::TOOL_CLOSURE) {
        Some(closure) => match closure {
            Value::List(paths) => {
                let mut closure = Vec::with_capacity(paths.len());
                for path in paths {
                    closure.push(text_of(path)?.into());
                }
                closure
            }
            other => {
                return Err(diag(&format!(
                    "the tools closure must be a list of paths, not {}",
                    other.describe()
                )));
            }
        },
        None => Vec::new(),
    };
    Ok(ToolPaths {
        shell: tool(types::TOOL_SHELL)?,
        mkdir: tool(types::TOOL_MKDIR)?,
        cat: tool(types::TOOL_CAT)?,
        chmod: tool(types::TOOL_CHMOD)?,
        ln: tool(types::TOOL_LN)?,
        closure,
    })
}

/// A single path component, as the unit file name and the machine name must
/// be to become file names inside the artifact.
fn component_of(value: &Value, what: &str) -> PithResult<Box<str>> {
    let component = text_of(value)?;
    if component.is_empty()
        || component.contains('/')
        || component.contains('\0')
        || component == "."
        || component == ".."
    {
        return Err(diag(&format!(
            "{what} must be one path component, not `{component}`"
        )));
    }
    Ok(component.into())
}

/// An artifact-relative path with no empty, `.`, or `..` components, which is
/// what both the staged input spelling and the script can carry.
fn relative_path_of(path: &str) -> PithResult<()> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\0')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if valid {
        Ok(())
    } else {
        Err(diag(&format!(
            "a file set entry's path must be a relative path with plain components, not \
             `{path}`"
        )))
    }
}

/// A rendered text carried by a nominal text value, refused when it cannot
/// cross the process boundary the script runs across.
fn rendered_text(value: &Value, declared: &types::Declared) -> PithResult<Box<str>> {
    let representation = representation_of(value, declared)?;
    let text = text_of(representation)?;
    if text.contains('\0') {
        return Err(diag(&format!(
            "a {} carries a NUL byte, which no process environment can hold",
            declared.name()
        )));
    }
    Ok(text.into())
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

impl ActionRule for AssembleAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let [
            tools,
            machine,
            unit_name,
            files,
            unit_text,
            passwd_text,
            boot_text,
        ] = inputs
        else {
            return Err(diag(&format!(
                "an assemble request supplies tools, a machine, a unit file name, a file set, \
                 and three rendered texts; this one supplied {}",
                inputs.len()
            )));
        };
        let tools = tools_of(tools)?;
        let machine = component_of(machine, "the machine a boot entry names")?;
        let unit_name = component_of(unit_name, "the unit file name")?;
        let entries = file_entries_of(files)?;
        let boot_text = rendered_text(boot_text, types::boot_text())?;
        let passwd_text = rendered_text(passwd_text, types::passwd_text())?;
        let unit_text = rendered_text(unit_text, types::unit_text())?;

        let boot_entry = format!("{BOOT_DIR}/{machine}.conf");
        let unit_entry = format!("{UNIT_DIR}/{unit_name}");

        let mut directories: Vec<String> = Vec::new();
        let mut add_parents = |path: &str| {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let directory = format!("{ARTIFACT}/{parent}");
                if !directories.contains(&directory) {
                    directories.push(directory);
                }
            }
        };
        add_parents(PASSWD_ENTRY);
        add_parents(&boot_entry);
        add_parents(&unit_entry);

        let mut staged_inputs: Vec<ActionInput> = Vec::new();
        let mut copies: Vec<String> = Vec::new();
        let mut executables: Vec<String> = Vec::new();
        let mut links: Vec<String> = Vec::new();
        for (path, body) in &entries {
            relative_path_of(path)?;
            add_parents(path);
            let staged = format!("{STAGED}/{path}");
            let artifact = format!("{ARTIFACT}/{path}");
            match body {
                FileBody::File {
                    content,
                    executable,
                } => {
                    staged_inputs.push(ActionInput {
                        path: staged.clone().into(),
                        content: Content::Blob(*content),
                    });
                    copies.push(format!(
                        "{} {} > {}",
                        shell_quote(&tools.cat),
                        shell_quote(&staged),
                        shell_quote(&artifact),
                    ));
                    if *executable {
                        executables.push(format!(
                            "{} +x {}",
                            shell_quote(&tools.chmod),
                            shell_quote(&artifact),
                        ));
                    }
                }
                FileBody::Symlink { target } => {
                    if target.contains('\0') {
                        return Err(diag(&format!(
                            "the symlink `{path}` carries a NUL byte in its target"
                        )));
                    }
                    links.push(format!(
                        "{} -s {} {}",
                        shell_quote(&tools.ln),
                        shell_quote(target),
                        shell_quote(&artifact),
                    ));
                }
            }
        }
        directories.sort();

        let mut script = String::from("set -eu\n");
        if !directories.is_empty() {
            let quoted: Vec<String> = directories.iter().map(|d| shell_quote(d)).collect();
            script.push_str(&format!(
                "{} -p {}\n",
                shell_quote(&tools.mkdir),
                quoted.join(" "),
            ));
        }
        for copy in &copies {
            script.push_str(copy);
            script.push('\n');
        }
        script.push_str(&format!(
            "printf '%s' \"${ENV_BOOT}\" > {}\n",
            shell_quote(&format!("{ARTIFACT}/{boot_entry}")),
        ));
        script.push_str(&format!(
            "printf '%s' \"${ENV_PASSWD}\" > {}\n",
            shell_quote(&format!("{ARTIFACT}/{PASSWD_ENTRY}")),
        ));
        script.push_str(&format!(
            "printf '%s' \"${ENV_UNIT}\" > {}\n",
            shell_quote(&format!("{ARTIFACT}/{unit_entry}")),
        ));
        for executable in &executables {
            script.push_str(executable);
            script.push('\n');
        }
        for link in &links {
            script.push_str(link);
            script.push('\n');
        }

        // The tools themselves plus the closure they run against, which is
        // what the loader opens: the interpreter above all. Without it the
        // confined child cannot even start (decision 0030's finding, measured
        // here a second time).
        let mut closure: Vec<Box<str>> = vec![
            tools.cat.clone(),
            tools.chmod.clone(),
            tools.ln.clone(),
            tools.mkdir.clone(),
        ];
        closure.extend(tools.closure.iter().cloned());
        closure.sort();
        closure.dedup();

        Ok(ActionSpec {
            executable: ActionProgram::HostPath(tools.shell),
            toolchain: closure.into_boxed_slice(),
            arguments: ["-c".into(), script.into()].into(),
            inputs: staged_inputs.into_boxed_slice(),
            outputs: Box::new([ActionOutput {
                path: ARTIFACT.into(),
                kind: OutputKind::Tree,
            }]),
            environment: Box::new([
                EnvironmentVariable {
                    name: ENV_BOOT.into(),
                    value: boot_text,
                },
                EnvironmentVariable {
                    name: ENV_PASSWD.into(),
                    value: passwd_text,
                },
                EnvironmentVariable {
                    name: ENV_UNIT.into(),
                    value: unit_text,
                },
            ]),
            // The artifact is plain tree content — files, modes, and link
            // targets — so where the tools ran is not part of what the result
            // means; the tool paths themselves are declared inputs already.
            platform: PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        match execution.report.outputs.first() {
            Some(output) => match &output.content {
                Content::Tree(id) => Ok(types::system_tree().content(*id)),
                Content::Blob(_) => Err(diag(
                    "the assembly wrote a blob where a tree artifact was declared",
                )),
            },
            None => Err(diag("the assembly produced no artifact")),
        }
    }
}

/// The entry a caller requests: merges each kind of contribution, renders the
/// three texts, and assembles the artifact.
pub struct ComposeSystem;

impl ComposeSystem {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            MODULE,
            types::COMPOSE_SYSTEM,
            BodyRevision(1),
            types::compose_system_interface(),
            Span::none(),
        )
    }
}

impl PureRule for ComposeSystem {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        Box::new(SystemFrame {
            inputs: inputs.to_vec(),
            merged: None,
            phase: Phase::Merging,
        })
    }
}

enum Phase {
    Merging,
    Rendering,
    Assembling,
    Finishing,
}

struct SystemFrame {
    inputs: Vec<Value>,
    merged: Option<[Value; 3]>,
    phase: Phase,
}

impl SystemFrame {
    fn tools(&self) -> PithResult<Value> {
        self.inputs.first().cloned().ok_or_else(|| {
            diag(&format!(
                "a compose-system request supplies seven inputs; this one supplied {}",
                self.inputs.len()
            ))
        })
    }

    fn boot(&self) -> PithResult<Value> {
        self.inputs.get(1).cloned().ok_or_else(|| {
            diag(&format!(
                "a compose-system request supplies seven inputs; this one supplied {}",
                self.inputs.len()
            ))
        })
    }
}

impl PureRuleFrame for SystemFrame {
    fn step(&mut self, input: Option<Resumption>) -> PithResult<PureStep> {
        match self.phase {
            Phase::Merging => {
                if input.is_some() {
                    return Err(diag("the engine resumed a step that requested nothing"));
                }
                let (Some(etc), Some(users), Some(policy), Some(units), Some(replacements)) = (
                    self.inputs.get(2),
                    self.inputs.get(3),
                    self.inputs.get(4),
                    self.inputs.get(5),
                    self.inputs.get(6),
                ) else {
                    return Err(diag(&format!(
                        "a compose-system request supplies seven inputs; this one supplied {}",
                        self.inputs.len()
                    )));
                };
                self.phase = Phase::Rendering;
                Ok(PureStep::NeedAll(Box::new([
                    types::compose_etc_request(etc.clone()),
                    types::compose_users_request(users.clone()),
                    types::compose_unit_request(
                        policy.clone(),
                        units.clone(),
                        replacements.clone(),
                    ),
                ])))
            }
            Phase::Rendering => {
                let Some(Resumption::Many(merged)) = input else {
                    return Err(diag(
                        "the engine resumed the merges without their three values",
                    ));
                };
                let [files, table, unit] = merged.as_ref() else {
                    return Err(diag("the engine resumed the merges with the wrong count"));
                };
                self.merged = Some([files.clone(), table.clone(), unit.clone()]);
                let boot = self.boot()?;
                self.phase = Phase::Assembling;
                Ok(PureStep::NeedAll(Box::new([
                    types::render_unit_request(unit.clone()),
                    types::render_passwd_request(table.clone()),
                    types::render_boot_request(boot),
                ])))
            }
            Phase::Assembling => {
                let Some(Resumption::Many(rendered)) = input else {
                    return Err(diag(
                        "the engine resumed the renders without their three texts",
                    ));
                };
                let [unit_text, passwd_text, boot_text] = rendered.as_ref() else {
                    return Err(diag("the engine resumed the renders with the wrong count"));
                };
                let Some([files, _table, unit]) = &self.merged else {
                    return Err(diag(
                        "the engine stepped the assembly without merged values",
                    ));
                };
                let parts = unit_parts_of(unit)?;
                let machine = representation_of(&self.boot()?, types::boot())
                    .ok()
                    .and_then(|record| field_of(record, types::MACHINE))
                    .cloned();
                let Some(machine) = machine else {
                    return Err(diag("the boot description is missing its machine name"));
                };
                self.phase = Phase::Finishing;
                Ok(PureStep::NeedAction(types::assemble_request(
                    self.tools()?,
                    text_of(&machine)?,
                    &parts.name,
                    files.clone(),
                    unit_text.clone(),
                    passwd_text.clone(),
                    boot_text.clone(),
                )))
            }
            Phase::Finishing => match input.and_then(Resumption::one) {
                Some(artifact) => Ok(PureStep::Complete(artifact)),
                None => Err(diag("the assembly completed without an artifact")),
            },
        }
    }
}
