//! Declared action contracts (requirements A-1 and A-2).
//!
//! An [`ActionSpec`] is inert data. Planning one performs no external work;
//! executors in `pith-engine` are responsible for enforcing the contract.

use pith_ids::ContentId;

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
}
