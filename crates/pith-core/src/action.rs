//! Declared action contracts (requirements A-1 and A-2).
//!
//! An [`ActionSpec`] is inert data. Planning one performs no external work;
//! executors in `pith-engine` are responsible for enforcing the contract.

use pith_ids::{ActionDigest, ContentId};

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

    /// Derive persistent computation identity from the complete contract.
    pub fn identity(&self) -> ActionDigest {
        ActionDigest::of_manifest(&self.canonical_manifest())
    }

    fn canonical_manifest(&self) -> Vec<u8> {
        let mut manifest = Vec::new();
        manifest.extend_from_slice(self.executable.digest().as_bytes());

        encode_length(&mut manifest, self.arguments.len());
        for argument in &self.arguments {
            encode_bytes(&mut manifest, argument.as_bytes());
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
            encode_bytes(&mut manifest, input.path.as_bytes());
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
            encode_bytes(&mut manifest, output.path.as_bytes());
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
            encode_bytes(&mut manifest, variable.name.as_bytes());
            encode_bytes(&mut manifest, variable.value.as_bytes());
        }

        match &self.platform {
            PlatformRequirement::Any => manifest.push(0),
            PlatformRequirement::Exact {
                operating_system,
                architecture,
            } => {
                manifest.push(1);
                encode_bytes(&mut manifest, operating_system.as_bytes());
                encode_bytes(&mut manifest, architecture.as_bytes());
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
            encode_bytes(&mut manifest, capability.name.as_bytes());
            encode_bytes(&mut manifest, capability.scope.as_bytes());
        }

        match &self.network {
            NetworkPolicy::Deny => manifest.push(0),
            NetworkPolicy::AllowHosts(hosts) => {
                manifest.push(1);
                let mut hosts: Vec<_> = hosts.iter().collect();
                hosts.sort();
                encode_length(&mut manifest, hosts.len());
                for host in hosts {
                    encode_bytes(&mut manifest, host.as_bytes());
                }
            }
            NetworkPolicy::AllowAll => manifest.push(2),
        }

        manifest
    }
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

fn encode_length(manifest: &mut Vec<u8>, length: usize) {
    manifest.extend_from_slice(&(length as u64).to_le_bytes());
}

fn encode_bytes(manifest: &mut Vec<u8>, bytes: &[u8]) {
    encode_length(manifest, bytes.len());
    manifest.extend_from_slice(bytes);
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

        assert_eq!(first.identity(), second.identity());
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
            assert_ne!(baseline.identity(), variant.identity());
        }
    }
}
