//! Declared action contracts (requirements A-1 and A-2).
//!
//! An [`ActionSpec`] is inert data. Planning one performs no external work;
//! executors in `pith-engine` are responsible for enforcing the contract.

use pith_ids::ContentId;

mod manifest;
mod validation;

/// The Blob/Tree discriminator on its own. Used where a phase names the kind
/// without yet carrying payload (a declared action output), and as the return
/// of [`Content::kind`]. This is the single source of truth for the two
/// top-level content variants.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OutputKind {
    Blob,
    Tree,
}

/// Top-level content: a blob or a tree. The discriminator is the single source
/// of truth for "is this a Blob or a Tree"; each phase specializes `Blob` and
/// `Tree` to the payload it carries (a `ContentId`, materialized bytes, a
/// captured tree, …). Two phases never re-spell the Blob/Tree distinction.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Content<Blob, Tree> {
    Blob(Blob),
    Tree(Tree),
}

impl<Blob, Tree> Content<Blob, Tree> {
    /// The discriminant, discarding the payload. Lets a caller compare a
    /// content-carrying value against a declared [`OutputKind`] without the
    /// kind being stored redundantly alongside the content.
    #[must_use]
    pub const fn kind(&self) -> OutputKind {
        match self {
            Content::Blob(_) => OutputKind::Blob,
            Content::Tree(_) => OutputKind::Tree,
        }
    }
}

/// Immutable content made available to an action at a relative path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionInput {
    pub path: Box<str>,
    pub content: ActionInputContent,
}

/// Declared action input content: a blob or tree identified by content
/// identity. A specialization of [`Content`] where both variants carry a
/// [`ContentId`].
pub type ActionInputContent = Content<ContentId, ContentId>;

/// One path the executor must capture after successful execution.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionOutput {
    pub path: Box<str>,
    pub kind: OutputKind,
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// How an action's exit status is read.
///
/// A compiler that exits nonzero wrote no object, so the action failed and there
/// is nothing to capture. A test that exits nonzero produced a verdict, and a
/// verdict is a result. Nothing about the two invocations tells them apart from
/// outside, and decision 0032 bars wrapping the program in something that could
/// report the difference, so the contract states it. Decision 0003 puts what an
/// action claims in the contract; this is that rule applied to how it ends.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExitStatusContract {
    /// A nonzero exit, or a death by signal, fails the action. The default, and
    /// the right reading for every tool whose output is its purpose.
    SuccessRequired,
    /// However the program ended is a fact the rule reads. The action succeeds
    /// as long as the executor ran it and captured what was declared, and
    /// `ActionRule::complete` decides what the status means.
    Reported,
}

/// The program an action runs.
///
/// The two variants are two of the identity kinds decision 0005 separates, and
/// the type is what keeps them apart. A host path is an external identity: it
/// names a thing outside the engine, and what those bytes are is a fact about
/// the host rather than something the contract states. A [`ContentId`] is a
/// content identity the engine owns. Encoding the second as a path would put a
/// content identity in a field typed for an external one, which is the
/// substitution 0005 has a type system to prevent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActionProgram {
    /// An absolute host path the executor `execve`s directly, with the rest of
    /// the program's installation declared in [`ActionSpec::toolchain`]. This is
    /// how a toolchain enters a contract (decision 0030): a compiler is not one
    /// file, so naming its bytes would be a claim the contract cannot keep.
    HostPath(Box<str>),
    /// Content the graph produced, which the executor stages inside the scratch
    /// root and runs from there. A build product is one file, and its bytes are
    /// what a contract can name. The identity reaches the contract digest, so
    /// an action that runs a rebuilt program is a different action, which is
    /// what 0031's request-side key needs to distinguish the two.
    Content(ContentId),
}

impl ActionProgram {
    /// The host path this program runs from, when it has one. `None` for a
    /// content program, whose path exists only once an executor has staged it.
    #[must_use]
    pub fn host_path(&self) -> Option<&str> {
        match self {
            Self::HostPath(path) => Some(path),
            Self::Content(_) => None,
        }
    }

    /// The content this program is, when the engine owns it. `None` for a host
    /// path, whose bytes the engine never reads.
    #[must_use]
    pub fn content(&self) -> Option<ContentId> {
        match self {
            Self::HostPath(_) => None,
            Self::Content(id) => Some(*id),
        }
    }
}

/// Complete, inspectable contract for one bounded external action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionSpec {
    /// The program the executor runs: a host path it `execve`s directly, or
    /// content it stages and runs. See [`ActionProgram`].
    pub executable: ActionProgram,
    /// Host filesystem paths the action may read to find the rest of its
    /// toolchain. For a nix toolchain these are the top-level `/nix/store/...`
    /// directories returned by `nix path-info -r` over the executable's store
    /// path. The executor adds one landlock `PathBeneath` rule per path. See
    /// decision 0030.
    pub toolchain: Box<[Box<str>]>,
    /// Ordered command arguments. The executable itself is not repeated here.
    pub arguments: Box<[Box<str>]>,
    pub inputs: Box<[ActionInput]>,
    pub outputs: Box<[ActionOutput]>,
    pub environment: Box<[EnvironmentVariable]>,
    pub platform: PlatformRequirement,
    pub capabilities: Box<[CapabilityRequirement]>,
    pub network: NetworkPolicy,
    /// How the program's exit status is read. See [`ExitStatusContract`].
    pub exit_status: ExitStatusContract,
}

impl ActionSpec {
    /// A minimal contract with no ambient environment, network, or authority,
    /// running the host program at `executable`. Use
    /// [`ActionSpec::isolated_content`] for a program the graph produced.
    pub fn isolated(executable: &str) -> Self {
        Self::isolated_program(ActionProgram::HostPath(executable.into()))
    }

    /// A minimal contract running `program`, content the engine owns.
    #[must_use]
    pub fn isolated_content(program: ContentId) -> Self {
        Self::isolated_program(ActionProgram::Content(program))
    }

    fn isolated_program(executable: ActionProgram) -> Self {
        Self {
            executable,
            toolchain: Box::new([]),
            arguments: Box::new([]),
            inputs: Box::new([]),
            outputs: Box::new([]),
            environment: Box::new([]),
            platform: PlatformRequirement::Any,
            capabilities: Box::new([]),
            network: NetworkPolicy::Deny,
            exit_status: ExitStatusContract::SuccessRequired,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pith_diag::EngineCode;

    #[test]
    fn isolated_action_denies_ambient_authority() {
        let spec = ActionSpec::isolated("/bin/tool");

        assert!(spec.environment.is_empty());
        assert!(spec.capabilities.is_empty());
        assert_eq!(spec.network, NetworkPolicy::Deny);
    }

    #[test]
    fn content_kind_reports_the_discriminant_not_the_payload() {
        // The discriminator is the single source of truth for Blob-vs-Tree
        // across every phase. Whatever the payload, kind() must report the
        // variant — this is what lets validate_execution compare a produced
        // Content against a declared OutputKind without a redundant kind field.
        let blob: ActionInputContent = Content::Blob(ContentId::of_blob(b"x"));
        let tree: ActionInputContent = Content::Tree(ContentId::of_tree(b"manifest"));
        assert_eq!(blob.kind(), OutputKind::Blob);
        assert_eq!(tree.kind(), OutputKind::Tree);
    }

    #[test]
    fn action_contract_is_structural_data() {
        let mut spec = ActionSpec::isolated("/bin/compiler");
        spec.arguments = ["source.c".into(), "-o".into(), "source.o".into()].into();
        spec.outputs = [ActionOutput {
            path: "source.o".into(),
            kind: OutputKind::Blob,
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
        let mut first = ActionSpec::isolated("/bin/tool");
        first.inputs = [
            ActionInput {
                path: "b".into(),
                content: Content::Blob(second_blob),
            },
            ActionInput {
                path: "a".into(),
                content: Content::Blob(first_blob),
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
        first.toolchain = ["/nix/store/a".into(), "/nix/store/b".into()].into();
        let mut second = first.clone();
        second.inputs.reverse();
        second.environment.reverse();
        second.toolchain = ["/nix/store/b".into(), "/nix/store/a".into()].into();

        assert_eq!(first.digest().unwrap(), second.digest().unwrap());
    }

    #[test]
    fn every_contract_field_participates_in_identity() {
        let executable = "/bin/tool";
        let mut baseline = ActionSpec::isolated(executable);
        baseline.arguments = ["argument".into()].into();
        baseline.inputs = [ActionInput {
            path: "input".into(),
            content: Content::Blob(ContentId::of_blob(b"input")),
        }]
        .into();
        baseline.outputs = [ActionOutput {
            path: "output".into(),
            kind: OutputKind::Blob,
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
                executable: ActionProgram::HostPath("/bin/other-tool".into()),
                ..baseline.clone()
            },
            ActionSpec {
                toolchain: ["/nix/store/other".into()].into(),
                ..baseline.clone()
            },
            ActionSpec {
                arguments: ["other argument".into()].into(),
                ..baseline.clone()
            },
            ActionSpec {
                inputs: [ActionInput {
                    path: "other input".into(),
                    content: Content::Blob(ContentId::of_blob(b"input")),
                }]
                .into(),
                ..baseline.clone()
            },
            ActionSpec {
                outputs: [ActionOutput {
                    path: "other output".into(),
                    kind: OutputKind::Blob,
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
            let mut spec = ActionSpec::isolated("/bin/tool");
            spec.inputs = [ActionInput {
                path: path.into(),
                content: Content::Blob(ContentId::of_blob(b"input")),
            }]
            .into();

            assert_eq!(
                spec.validate().unwrap_err().code,
                EngineCode::InvalidActionSpec.into()
            );
        }

        let mut overlapping = ActionSpec::isolated("/bin/tool");
        overlapping.inputs = [ActionInput {
            path: "source".into(),
            content: Content::Tree(ContentId::of_tree(b"source")),
        }]
        .into();
        overlapping.outputs = [ActionOutput {
            path: "source/generated".into(),
            kind: OutputKind::Tree,
        }]
        .into();

        assert_eq!(
            overlapping.validate().unwrap_err().code,
            EngineCode::InvalidActionSpec.into()
        );
    }
}
