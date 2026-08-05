//! Declared action contracts (requirements A-1 and A-2).
//!
//! An [`ActionSpec`] is inert data. Planning one performs no external work;
//! executors in `pith-engine` are responsible for enforcing the contract.

use pith_diag::{Diag, EngineCode, Span};
use pith_ids::{ActionSpecDigest, ContentId};

use crate::manifest::{encode_length, encode_str};

/// Immutable content made available to an action at a relative path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionInput {
    pub path: Box<str>,
    pub content: ActionInputContent,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActionInputContent {
    Blob(ContentId),
    Tree(ContentId),
}

/// One path the executor must capture after successful execution.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionOutput {
    pub path: Box<str>,
    pub kind: ActionOutputKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActionOutputKind {
    Blob,
    Tree,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EnvironmentVariable {
    pub name: Box<str>,
    pub value: Box<str>,
}

/// The platform on which an executor may run an action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlatformRequirement {
    Any,
    Exact {
        operating_system: Box<str>,
        architecture: Box<str>,
    },
}

/// A namespaced capability and the resource scope to which it grants access.
///
/// Names and scopes are semantic values so domain libraries can add
/// capabilities without patching the kernel.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityRequirement {
    pub name: Box<str>,
    pub scope: Box<str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NetworkPolicy {
    Deny,
    AllowHosts(Box<[Box<str>]>),
    AllowAll,
}

/// Complete, inspectable contract for one bounded external action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionSpec {
    /// Content identity of the executable artifact.
    pub executable: ContentId,
    /// Ordered command arguments. The executable itself is not repeated here.
    pub arguments: Box<[Box<str>]>,
    pub inputs: Box<[ActionInput]>,
    pub outputs: Box<[ActionOutput]>,
    pub environment: Box<[EnvironmentVariable]>,
    pub platform: PlatformRequirement,
    pub capabilities: Box<[CapabilityRequirement]>,
    pub network: NetworkPolicy,
}

impl ActionSpec {
    /// A minimal contract with no ambient environment, network, or authority.
    pub fn isolated(executable: ContentId) -> Self {
        Self {
            executable,
            arguments: Box::new([]),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
        }
    }

    /// Validate the complete declared contract.
    ///
    /// # Errors
    /// Returns `E-1105` when a path, argument, environment entry, platform,
    /// capability, or network host is invalid or ambiguous.
    pub fn validate(&self) -> Result<(), Diag> {
        for argument in &self.arguments {
            if argument.contains('\0') {
                return Err(invalid_action_spec("action argument contains a NUL byte"));
            }
        }

        validate_paths(self.inputs.iter().map(|input| input.path.as_ref()), "input")?;
        validate_paths(
            self.outputs.iter().map(|output| output.path.as_ref()),
            "output",
        )?;
        for input in &self.inputs {
            for output in &self.outputs {
                if paths_overlap(&input.path, &output.path) {
                    return Err(invalid_action_spec(format!(
                        "action input `{}` overlaps output `{}`",
                        input.path, output.path
                    )));
                }
            }
        }

        for (position, variable) in self.environment.iter().enumerate() {
            if variable.name.is_empty()
                || variable.name.contains('=')
                || variable.name.contains('\0')
            {
                return Err(invalid_action_spec(format!(
                    "invalid action environment variable name `{}`",
                    variable.name
                )));
            }
            if variable.value.contains('\0') {
                return Err(invalid_action_spec(format!(
                    "action environment variable `{}` contains a NUL byte",
                    variable.name
                )));
            }
            if self
                .environment
                .iter()
                .take(position)
                .any(|previous| previous.name == variable.name)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action environment variable `{}`",
                    variable.name
                )));
            }
        }

        if let PlatformRequirement::Exact {
            operating_system,
            architecture,
        } = &self.platform
            && (operating_system.is_empty() || architecture.is_empty())
        {
            return Err(invalid_action_spec(
                "exact action platform requires an operating system and architecture",
            ));
        }

        for (position, capability) in self.capabilities.iter().enumerate() {
            if capability.name.is_empty()
                || capability.scope.is_empty()
                || capability.name.contains('\0')
                || capability.scope.contains('\0')
            {
                return Err(invalid_action_spec(
                    "action capability requires non-empty NUL-free name and scope",
                ));
            }
            if self
                .capabilities
                .iter()
                .take(position)
                .any(|previous| previous == capability)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action capability `{}` scoped to `{}`",
                    capability.name, capability.scope
                )));
            }
        }

        if let NetworkPolicy::AllowHosts(hosts) = &self.network {
            for (position, host) in hosts.iter().enumerate() {
                if host.is_empty() || host.contains('\0') {
                    return Err(invalid_action_spec(
                        "allowed network host must be non-empty and NUL-free",
                    ));
                }
                if hosts.iter().take(position).any(|previous| previous == host) {
                    return Err(invalid_action_spec(format!(
                        "duplicate allowed network host `{host}`"
                    )));
                }
            }
        }

        Ok(())
    }

    /// Derive a stable digest from a valid declared contract.
    ///
    /// This digest identifies the specification, not a resolved execution or
    /// reusable cache entry.
    ///
    /// # Errors
    /// Returns `E-1105` when [`ActionSpec::validate`] rejects the contract.
    pub fn digest(&self) -> Result<ActionSpecDigest, Diag> {
        self.validate()?;
        Ok(ActionSpecDigest::of_manifest(&self.canonical_manifest()))
    }

    fn canonical_manifest(&self) -> Vec<u8> {
        let mut manifest = Vec::new();
        manifest.extend_from_slice(self.executable.digest().as_bytes());

        encode_length(&mut manifest, self.arguments.len());
        for argument in &self.arguments {
            encode_str(&mut manifest, argument);
        }

        let mut inputs: Vec<_> = self.inputs.iter().collect();
        inputs.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| {
                    action_input_content_tag(left.content)
                        .cmp(&action_input_content_tag(right.content))
                })
                .then_with(|| {
                    action_input_content_id(left.content)
                        .cmp(&action_input_content_id(right.content))
                })
        });
        encode_length(&mut manifest, inputs.len());
        for input in inputs {
            encode_str(&mut manifest, &input.path);
            manifest.push(action_input_content_tag(input.content));
            manifest.extend_from_slice(action_input_content_id(input.content).digest().as_bytes());
        }

        let mut outputs: Vec<_> = self.outputs.iter().collect();
        outputs.sort_by(|left, right| {
            left.path.cmp(&right.path).then_with(|| {
                action_output_kind_tag(left.kind).cmp(&action_output_kind_tag(right.kind))
            })
        });
        encode_length(&mut manifest, outputs.len());
        for output in outputs {
            encode_str(&mut manifest, &output.path);
            manifest.push(action_output_kind_tag(output.kind));
        }

        let mut environment: Vec<_> = self.environment.iter().collect();
        environment.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.value.cmp(&right.value))
        });
        encode_length(&mut manifest, environment.len());
        for variable in environment {
            encode_str(&mut manifest, &variable.name);
            encode_str(&mut manifest, &variable.value);
        }

        match &self.platform {
            PlatformRequirement::Any => manifest.push(0),
            PlatformRequirement::Exact {
                operating_system,
                architecture,
            } => {
                manifest.push(1);
                encode_str(&mut manifest, operating_system);
                encode_str(&mut manifest, architecture);
            }
        }

        let mut capabilities: Vec<_> = self.capabilities.iter().collect();
        capabilities.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.scope.cmp(&right.scope))
        });
        encode_length(&mut manifest, capabilities.len());
        for capability in capabilities {
            encode_str(&mut manifest, &capability.name);
            encode_str(&mut manifest, &capability.scope);
        }

        match &self.network {
            NetworkPolicy::Deny => manifest.push(0),
            NetworkPolicy::AllowHosts(hosts) => {
                manifest.push(1);
                let mut hosts: Vec<_> = hosts.iter().collect();
                hosts.sort();
                encode_length(&mut manifest, hosts.len());
                for host in hosts {
                    encode_str(&mut manifest, host);
                }
            }
            NetworkPolicy::AllowAll => manifest.push(2),
        }

        manifest
    }
}

fn validate_paths<'path>(paths: impl Iterator<Item = &'path str>, kind: &str) -> Result<(), Diag> {
    let paths: Vec<_> = paths.collect();
    for (position, path) in paths.iter().enumerate() {
        if !is_valid_action_path(path) {
            return Err(invalid_action_spec(format!(
                "invalid action {kind} path `{path}`"
            )));
        }
        if let Some(previous) = paths
            .iter()
            .take(position)
            .find(|previous| paths_overlap(previous, path))
        {
            return Err(invalid_action_spec(format!(
                "action {kind} path `{path}` overlaps `{previous}`"
            )));
        }
    }
    Ok(())
}

fn is_valid_action_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn invalid_action_spec(message: impl Into<Box<str>>) -> Diag {
    Diag::engine(EngineCode::InvalidActionSpec, Span::none(), message)
}

fn action_input_content_tag(content: ActionInputContent) -> u8 {
    match content {
        ActionInputContent::Blob(_) => 0,
        ActionInputContent::Tree(_) => 1,
    }
}

fn action_input_content_id(content: ActionInputContent) -> ContentId {
    match content {
        ActionInputContent::Blob(identity) | ActionInputContent::Tree(identity) => identity,
    }
}

fn action_output_kind_tag(kind: ActionOutputKind) -> u8 {
    match kind {
        ActionOutputKind::Blob => 0,
        ActionOutputKind::Tree => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_action_denies_ambient_authority() {
        let spec = ActionSpec::isolated(ContentId::of_blob(b"tool"));

        assert!(spec.environment.is_empty());
        assert!(spec.capabilities.is_empty());
        assert_eq!(spec.network, NetworkPolicy::Deny);
    }

    #[test]
    fn action_contract_is_structural_data() {
        let mut spec = ActionSpec::isolated(ContentId::of_blob(b"compiler"));
        spec.arguments = ["source.c".into(), "-o".into(), "source.o".into()].into();
        spec.outputs = [ActionOutput {
            path: "source.o".into(),
            kind: ActionOutputKind::Blob,
        }]
        .into();

        assert_eq!(spec.arguments.len(), 3);
        assert_eq!(
            spec.outputs.first().map(|output| output.path.as_ref()),
            Some("source.o")
        );
    }

    #[test]
    fn identity_is_stable_for_unordered_contract_fields() {
        let first_blob = ContentId::of_blob(b"first");
        let second_blob = ContentId::of_blob(b"second");
        let mut first = ActionSpec::isolated(ContentId::of_blob(b"tool"));
        first.inputs = [
            ActionInput {
                path: "b".into(),
                content: ActionInputContent::Blob(second_blob),
            },
            ActionInput {
                path: "a".into(),
                content: ActionInputContent::Blob(first_blob),
            },
        ]
        .into();
        first.environment = [
            EnvironmentVariable {
                name: "B".into(),
                value: "2".into(),
            },
            EnvironmentVariable {
                name: "A".into(),
                value: "1".into(),
            },
        ]
        .into();
        let mut second = first.clone();
        second.inputs.reverse();
        second.environment.reverse();

        assert_eq!(first.digest().unwrap(), second.digest().unwrap());
    }

    #[test]
    fn every_contract_field_participates_in_identity() {
        let executable = ContentId::of_blob(b"tool");
        let mut baseline = ActionSpec::isolated(executable);
        baseline.arguments = ["argument".into()].into();
        baseline.inputs = [ActionInput {
            path: "input".into(),
            content: ActionInputContent::Blob(ContentId::of_blob(b"input")),
        }]
        .into();
        baseline.outputs = [ActionOutput {
            path: "output".into(),
            kind: ActionOutputKind::Blob,
        }]
        .into();
        baseline.environment = [EnvironmentVariable {
            name: "MODE".into(),
            value: "release".into(),
        }]
        .into();
        baseline.platform = PlatformRequirement::Exact {
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
        };
        baseline.capabilities = [CapabilityRequirement {
            name: "filesystem.read".into(),
            scope: "input".into(),
        }]
        .into();
        baseline.network = NetworkPolicy::AllowHosts(["example.com".into()].into());

        let variants = [
            ActionSpec {
                executable: ContentId::of_blob(b"other tool"),
                ..baseline.clone()
            },
            ActionSpec {
                arguments: ["other argument".into()].into(),
                ..baseline.clone()
            },
            ActionSpec {
                inputs: [ActionInput {
                    path: "other input".into(),
                    content: ActionInputContent::Blob(ContentId::of_blob(b"input")),
                }]
                .into(),
                ..baseline.clone()
            },
            ActionSpec {
                outputs: [ActionOutput {
                    path: "other output".into(),
                    kind: ActionOutputKind::Blob,
                }]
                .into(),
                ..baseline.clone()
            },
            ActionSpec {
                environment: [EnvironmentVariable {
                    name: "MODE".into(),
                    value: "debug".into(),
                }]
                .into(),
                ..baseline.clone()
            },
            ActionSpec {
                platform: PlatformRequirement::Exact {
                    operating_system: "linux".into(),
                    architecture: "aarch64".into(),
                },
                ..baseline.clone()
            },
            ActionSpec {
                capabilities: [CapabilityRequirement {
                    name: "filesystem.read".into(),
                    scope: "other input".into(),
                }]
                .into(),
                ..baseline.clone()
            },
            ActionSpec {
                network: NetworkPolicy::AllowAll,
                ..baseline.clone()
            },
        ];

        for variant in variants {
            assert_ne!(baseline.digest().unwrap(), variant.digest().unwrap());
        }
    }

    #[test]
    fn invalid_or_ambiguous_paths_are_rejected() {
        for path in [
            "",
            "/absolute",
            "parent/../child",
            "trailing/",
            "back\\slash",
        ] {
            let mut spec = ActionSpec::isolated(ContentId::of_blob(b"tool"));
            spec.inputs = [ActionInput {
                path: path.into(),
                content: ActionInputContent::Blob(ContentId::of_blob(b"input")),
            }]
            .into();

            assert_eq!(
                spec.validate().unwrap_err().code,
                EngineCode::InvalidActionSpec.into()
            );
        }

        let mut overlapping = ActionSpec::isolated(ContentId::of_blob(b"tool"));
        overlapping.inputs = [ActionInput {
            path: "source".into(),
            content: ActionInputContent::Tree(ContentId::of_tree(b"source")),
        }]
        .into();
        overlapping.outputs = [ActionOutput {
            path: "source/generated".into(),
            kind: ActionOutputKind::Tree,
        }]
        .into();

        assert_eq!(
            overlapping.validate().unwrap_err().code,
            EngineCode::InvalidActionSpec.into()
        );
    }
}
