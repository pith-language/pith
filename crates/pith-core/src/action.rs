//! Declared action contracts (requirements A-1 and A-2).
//!
//! An [`ActionSpec`] is inert data. Planning one performs no external work;
//! executors in `pith-engine` are responsible for enforcing the contract.

use pith_diag::{Diag, EngineCode, Span};
use pith_ids::{ActionSpecDigest, ContentId};

use crate::manifest::{encode_length, encode_str};

/// Discriminant tags for `ActionProgram`, `PlatformRequirement`, and
/// `NetworkPolicy` in the action-spec manifest. Stable digest format:
/// renumbering invalidates every persisted `ActionSpecDigest`.
const TAG_PROGRAM_HOST_PATH: u8 = 0;
const TAG_PROGRAM_CONTENT: u8 = 1;
const TAG_EXIT_SUCCESS_REQUIRED: u8 = 0;
const TAG_EXIT_REPORTED: u8 = 1;
const TAG_PLATFORM_ANY: u8 = 0;
const TAG_PLATFORM_EXACT: u8 = 1;
const TAG_NETWORK_DENY: u8 = 0;
const TAG_NETWORK_ALLOW_HOSTS: u8 = 1;
const TAG_NETWORK_ALLOW_ALL: u8 = 2;

/// The Blob/Tree discriminator on its own. Used where a phase names the kind
/// without yet carrying payload (a declared action output), and as the return
/// of [`Content::kind`]. This is the single source of truth for the two
/// top-level content variants.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

/// How an action's exit status is read.
///
/// A compiler that exits nonzero wrote no object, so the action failed and there
/// is nothing worth keeping. A test that exits nonzero produced a verdict, and a
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
    /// root and runs from there. A build product is one file, so naming its
    /// bytes is what is true of it. The identity reaches the contract digest, so
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

        // A content program carries a `ContentId`, which is well-formed by
        // construction; only the host-path variant can be spelled wrongly.
        if let ActionProgram::HostPath(path) = &self.executable
            && !is_valid_host_path(path)
        {
            return Err(invalid_action_spec(format!(
                "invalid action executable path `{path}`"
            )));
        }

        for (position, path) in self.toolchain.iter().enumerate() {
            if !is_valid_host_path(path) {
                return Err(invalid_action_spec(format!(
                    "invalid action toolchain path `{path}`"
                )));
            }
            if self
                .toolchain
                .iter()
                .take(position)
                .any(|previous| previous == path)
            {
                return Err(invalid_action_spec(format!(
                    "duplicate action toolchain path `{path}`"
                )));
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
        match &self.executable {
            ActionProgram::HostPath(path) => {
                manifest.push(TAG_PROGRAM_HOST_PATH);
                encode_str(&mut manifest, path);
            }
            ActionProgram::Content(id) => {
                manifest.push(TAG_PROGRAM_CONTENT);
                manifest.extend_from_slice(id.digest().as_bytes());
            }
        }

        let mut toolchain: Vec<_> = self.toolchain.iter().collect();
        toolchain.sort();
        encode_length(&mut manifest, toolchain.len());
        for path in toolchain {
            encode_str(&mut manifest, path);
        }

        encode_length(&mut manifest, self.arguments.len());
        for argument in &self.arguments {
            encode_str(&mut manifest, argument);
        }

        let mut inputs: Vec<_> = self.inputs.iter().collect();
        inputs.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| {
                    output_kind_tag(left.content.kind()).cmp(&output_kind_tag(right.content.kind()))
                })
                .then_with(|| content_id(&left.content).cmp(&content_id(&right.content)))
        });
        encode_length(&mut manifest, inputs.len());
        for input in inputs {
            encode_str(&mut manifest, &input.path);
            manifest.push(output_kind_tag(input.content.kind()));
            manifest.extend_from_slice(content_id(&input.content).digest().as_bytes());
        }

        let mut outputs: Vec<_> = self.outputs.iter().collect();
        outputs.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| output_kind_tag(left.kind).cmp(&output_kind_tag(right.kind)))
        });
        encode_length(&mut manifest, outputs.len());
        for output in outputs {
            encode_str(&mut manifest, &output.path);
            manifest.push(output_kind_tag(output.kind));
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
            PlatformRequirement::Any => manifest.push(TAG_PLATFORM_ANY),
            PlatformRequirement::Exact {
                operating_system,
                architecture,
            } => {
                manifest.push(TAG_PLATFORM_EXACT);
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
            NetworkPolicy::Deny => manifest.push(TAG_NETWORK_DENY),
            NetworkPolicy::AllowHosts(hosts) => {
                manifest.push(TAG_NETWORK_ALLOW_HOSTS);
                let mut hosts: Vec<_> = hosts.iter().collect();
                hosts.sort();
                encode_length(&mut manifest, hosts.len());
                for host in hosts {
                    encode_str(&mut manifest, host);
                }
            }
            NetworkPolicy::AllowAll => manifest.push(TAG_NETWORK_ALLOW_ALL),
        }

        manifest.push(match self.exit_status {
            ExitStatusContract::SuccessRequired => TAG_EXIT_SUCCESS_REQUIRED,
            ExitStatusContract::Reported => TAG_EXIT_REPORTED,
        });

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

/// Validation for an absolute host path (`executable`, `toolchain`). These name
/// where the executor finds the program and its closure on the host filesystem,
/// so they are absolute, NUL-free, and reject `..` so a declared path cannot
/// escape its directory by traversal. A trailing slash would name a directory,
/// which is valid for a `toolchain` closure entry but not for an `executable`,
/// so callers that need a file reject the trailing-slash case themselves.
fn is_valid_host_path(path: &str) -> bool {
    !path.is_empty()
        && path.starts_with('/')
        && !path.contains('\0')
        && !path.contains('\\')
        && path
            .split('/')
            .skip(1)
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

/// The canonical-manifest tag for an [`OutputKind`]. Blob/Tree inputs and
/// outputs share this single encoding: the discriminator is one value, not
/// two parallel tag functions that must agree.
fn output_kind_tag(kind: OutputKind) -> u8 {
    match kind {
        OutputKind::Blob => 0,
        OutputKind::Tree => 1,
    }
}

/// The content identity carried by a declared action input, regardless of
/// whether it is a blob or a tree.
fn content_id(content: &ActionInputContent) -> ContentId {
    match content {
        Content::Blob(identity) | Content::Tree(identity) => *identity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
