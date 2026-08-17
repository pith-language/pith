//! The link action and the pure entry that requests it.

use pith_core::{
    Action, ActionInput, ActionOutput, ActionProgram, ActionSpec, BodyRevision, Content,
    EnvironmentVariable, ExitStatusContract, NetworkPolicy, OutputKind, PlatformRequirement, Pure,
    Request, Rule, Value,
};
use pith_diag::{PithResult, Span};
use pith_engine::{ActionExecution, ActionRule, ProducedOutput, PureRule, PureRuleFrame};

use super::{ActionRequestFrame, EXECUTABLE_PATH, blob_of, diag, input, requested_toolchain};
use crate::toolchain::Toolchains;
use crate::types;

/// Links any number of objects into one executable. The objects arrive as one
/// request input, a `List<Object>` (decision 0035); each is staged at a path
/// derived from its position, which is the order the caller's list gave and is
/// the order the driver receives them in.
pub struct LinkAction {
    toolchains: Toolchains,
}

impl LinkAction {
    #[must_use]
    pub fn new(toolchains: Toolchains) -> Self {
        Self { toolchains }
    }

    #[must_use]
    pub fn rule(&self) -> Rule<Action> {
        Rule::<Action>::declared(
            types::MODULE,
            "link",
            BodyRevision(1),
            types::link_interface(),
            Span::none(),
        )
    }
}

/// The staged path of the object at `position`: `object-0.o`, `object-1.o`, ….
/// Derived from position so the contract is a function of the list and nothing
/// else; two link requests over the same objects in the same order plan the
/// same contract and share a cache entry.
fn object_path(position: usize) -> Box<str> {
    format!("object-{position}.o").into()
}

impl ActionRule for LinkAction {
    fn plan(&self, inputs: &[Value]) -> PithResult<ActionSpec> {
        let toolchain = requested_toolchain(&self.toolchains, inputs)?;
        let Value::List(objects) = input(inputs, 1)? else {
            return Err(diag(
                "the link request carried no object list as its second input",
            ));
        };
        if objects.is_empty() {
            return Err(diag("the link request carried an empty object list"));
        }
        let mut action_inputs = Vec::with_capacity(objects.len());
        let mut arguments = Vec::with_capacity(objects.len().saturating_add(2));
        for (position, value) in objects.iter().enumerate() {
            let id = blob_of(value, types::object_name())?;
            let path = object_path(position);
            arguments.push(path.clone());
            action_inputs.push(ActionInput {
                path,
                content: Content::Blob(id),
            });
        }
        arguments.push("-o".into());
        arguments.push(EXECUTABLE_PATH.into());
        Ok(ActionSpec {
            executable: ActionProgram::HostPath(toolchain.driver.clone()),
            toolchain: toolchain.closure.clone(),
            arguments: arguments.into(),
            inputs: action_inputs.into_boxed_slice(),
            outputs: [ActionOutput {
                path: EXECUTABLE_PATH.into(),
                kind: OutputKind::Blob,
            }]
            .into(),
            environment: [EnvironmentVariable {
                name: "PATH".into(),
                value: toolchain.tool_directory.clone(),
            }]
            .into(),
            platform: PlatformRequirement::Exact {
                operating_system: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
            },
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        })
    }

    fn complete(&self, _inputs: &[Value], execution: &ActionExecution) -> PithResult<Value> {
        let Some(ProducedOutput {
            content: Content::Blob(executable),
            ..
        }) = execution.report.outputs.first()
        else {
            return Err(diag("the link action captured no executable"));
        };
        Ok(types::executable(*executable))
    }
}

/// Requests a link action and hands back its executable.
pub struct LinkRule;

impl LinkRule {
    #[must_use]
    pub fn rule() -> Rule<Pure> {
        Rule::<Pure>::declared(
            types::MODULE,
            "link-entry",
            BodyRevision(1),
            types::link_interface(),
            Span::none(),
        )
    }
}

impl PureRule for LinkRule {
    fn start(&self, inputs: &[Value]) -> Box<dyn PureRuleFrame> {
        let request = Request::<Action>::new(
            "link",
            types::link_interface(),
            inputs.to_vec(),
            Span::none(),
        );
        Box::new(ActionRequestFrame {
            action: Some(request),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolchain::Toolchain;
    use pith_ids::ContentId;

    fn link_action() -> LinkAction {
        let toolchain = Toolchain {
            driver: "/bin/cc".into(),
            closure: Box::new([]),
            program_path: Some("/bin".into()),
            tool_directory: "/bin".into(),
        };
        LinkAction::new(Toolchains::one(toolchain))
    }

    #[test]
    fn a_link_plans_one_staged_path_per_object_in_list_order() {
        let first = ContentId::of_blob(b"first");
        let second = ContentId::of_blob(b"second");
        let third = ContentId::of_blob(b"third");
        let request = types::link_request(types::toolchain("/bin/cc"), [first, second, third]);
        let spec = link_action()
            .plan(request.inputs.as_ref())
            .expect("the link plans");

        let paths: Vec<&str> = spec.inputs.iter().map(|i| i.path.as_ref()).collect();
        assert_eq!(paths, ["object-0.o", "object-1.o", "object-2.o"]);
        assert_eq!(
            spec.arguments
                .iter()
                .map(|a| a.as_ref())
                .collect::<Vec<_>>(),
            ["object-0.o", "object-1.o", "object-2.o", "-o", "out"]
        );
        // Each staged input carries the content identity at its position, so
        // the contract — and the action key derived from it — is a function of
        // the list.
        let staged: Vec<ContentId> = spec
            .inputs
            .iter()
            .map(|i| match i.content {
                Content::Blob(id) => id,
                Content::Tree(_) => unreachable!("an object stages as a blob"),
            })
            .collect();
        assert_eq!(staged, [first, second, third]);
    }

    #[test]
    fn a_reordering_of_the_same_objects_plans_a_different_contract() {
        let first = ContentId::of_blob(b"first");
        let second = ContentId::of_blob(b"second");
        let forward = types::link_request(types::toolchain("/bin/cc"), [first, second]);
        let reversed = types::link_request(types::toolchain("/bin/cc"), [second, first]);
        let forward_spec = link_action()
            .plan(forward.inputs.as_ref())
            .expect("the forward link plans");
        let reversed_spec = link_action()
            .plan(reversed.inputs.as_ref())
            .expect("the reversed link plans");

        // Object order reaches the driver, and the contract says so: a
        // reordered link is a different request, not a cache hit on the same
        // set. (A linker is free to make order observable through symbol
        // resolution and layout.)
        assert_ne!(
            forward_spec.digest().expect("the forward spec digests"),
            reversed_spec.digest().expect("the reversed spec digests")
        );
    }

    #[test]
    fn an_empty_object_list_fails_the_plan() {
        let request = types::link_request(types::toolchain("/bin/cc"), std::iter::empty());
        let error = link_action()
            .plan(request.inputs.as_ref())
            .expect_err("an empty link is not a plan");
        assert!(
            error
                .iter()
                .any(|d| d.message.0.contains("empty object list")),
            "the diagnostics should name the empty list: {error:?}"
        );
    }

    #[test]
    fn a_bare_blob_in_the_object_list_fails_the_plan() {
        // The list's element type is nominal: a `Value::Blob` that skipped the
        // `xylem.Object` constructor is a type error the planner reports
        // rather than links.
        let request = Request::<Pure>::new(
            "link-entry",
            types::link_interface(),
            [
                types::toolchain("/bin/cc"),
                Value::List(
                    vec![Value::Blob(ContentId::of_blob(b"not-an-object"))].into_boxed_slice(),
                ),
            ],
            Span::none(),
        );
        let error = link_action()
            .plan(request.inputs.as_ref())
            .expect_err("a bare blob is not an object value");
        assert!(
            error.iter().any(|d| d.message.0.contains("xylem.Object")),
            "the diagnostics should name the expected nominal type: {error:?}"
        );
    }
}
