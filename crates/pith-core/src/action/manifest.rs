use pith_diag::Diag;
use pith_ids::{ActionSpecDigest, ContentId};

use super::{
    ActionInputContent, ActionProgram, ActionSpec, Content, ExitStatusContract, NetworkPolicy,
    OutputKind, PlatformRequirement,
};
use crate::manifest::{encode_length, encode_str};

const TAG_PROGRAM_HOST_PATH: u8 = 0;
const TAG_PROGRAM_CONTENT: u8 = 1;
const TAG_EXIT_SUCCESS_REQUIRED: u8 = 0;
const TAG_EXIT_REPORTED: u8 = 1;
const TAG_PLATFORM_ANY: u8 = 0;
const TAG_PLATFORM_EXACT: u8 = 1;
const TAG_NETWORK_DENY: u8 = 0;
const TAG_NETWORK_ALLOW_HOSTS: u8 = 1;
const TAG_NETWORK_ALLOW_ALL: u8 = 2;

impl ActionSpec {
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
        encode_program(&mut manifest, &self.executable);
        encode_toolchain(&mut manifest, &self.toolchain);
        encode_arguments(&mut manifest, &self.arguments);
        encode_inputs(&mut manifest, &self.inputs);
        encode_outputs(&mut manifest, &self.outputs);
        encode_environment(&mut manifest, &self.environment);
        encode_platform(&mut manifest, &self.platform);
        encode_capabilities(&mut manifest, &self.capabilities);
        encode_network(&mut manifest, &self.network);
        manifest.push(match self.exit_status {
            ExitStatusContract::SuccessRequired => TAG_EXIT_SUCCESS_REQUIRED,
            ExitStatusContract::Reported => TAG_EXIT_REPORTED,
        });
        manifest
    }
}

fn encode_program(manifest: &mut Vec<u8>, program: &ActionProgram) {
    match program {
        ActionProgram::HostPath(path) => {
            manifest.push(TAG_PROGRAM_HOST_PATH);
            encode_str(manifest, path);
        }
        ActionProgram::Content(id) => {
            manifest.push(TAG_PROGRAM_CONTENT);
            manifest.extend_from_slice(id.digest().as_bytes());
        }
    }
}

fn encode_toolchain(manifest: &mut Vec<u8>, toolchain: &[Box<str>]) {
    let mut toolchain: Vec<_> = toolchain.iter().collect();
    toolchain.sort();
    encode_length(manifest, toolchain.len());
    for path in toolchain {
        encode_str(manifest, path);
    }
}

fn encode_arguments(manifest: &mut Vec<u8>, arguments: &[Box<str>]) {
    encode_length(manifest, arguments.len());
    for argument in arguments {
        encode_str(manifest, argument);
    }
}

fn encode_inputs(manifest: &mut Vec<u8>, inputs: &[super::ActionInput]) {
    let mut inputs: Vec<_> = inputs.iter().collect();
    inputs.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| {
                output_kind_tag(left.content.kind()).cmp(&output_kind_tag(right.content.kind()))
            })
            .then_with(|| content_id(&left.content).cmp(&content_id(&right.content)))
    });
    encode_length(manifest, inputs.len());
    for input in inputs {
        encode_str(manifest, &input.path);
        manifest.push(output_kind_tag(input.content.kind()));
        manifest.extend_from_slice(content_id(&input.content).digest().as_bytes());
    }
}

fn encode_outputs(manifest: &mut Vec<u8>, outputs: &[super::ActionOutput]) {
    let mut outputs: Vec<_> = outputs.iter().collect();
    outputs.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| output_kind_tag(left.kind).cmp(&output_kind_tag(right.kind)))
    });
    encode_length(manifest, outputs.len());
    for output in outputs {
        encode_str(manifest, &output.path);
        manifest.push(output_kind_tag(output.kind));
    }
}

fn encode_environment(manifest: &mut Vec<u8>, environment: &[super::EnvironmentVariable]) {
    let mut environment: Vec<_> = environment.iter().collect();
    environment.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.value.cmp(&right.value))
    });
    encode_length(manifest, environment.len());
    for variable in environment {
        encode_str(manifest, &variable.name);
        encode_str(manifest, &variable.value);
    }
}

fn encode_platform(manifest: &mut Vec<u8>, platform: &PlatformRequirement) {
    match platform {
        PlatformRequirement::Any => manifest.push(TAG_PLATFORM_ANY),
        PlatformRequirement::Exact {
            operating_system,
            architecture,
        } => {
            manifest.push(TAG_PLATFORM_EXACT);
            encode_str(manifest, operating_system);
            encode_str(manifest, architecture);
        }
    }
}

fn encode_capabilities(manifest: &mut Vec<u8>, capabilities: &[super::CapabilityRequirement]) {
    let mut capabilities: Vec<_> = capabilities.iter().collect();
    capabilities.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.scope.cmp(&right.scope))
    });
    encode_length(manifest, capabilities.len());
    for capability in capabilities {
        encode_str(manifest, &capability.name);
        encode_str(manifest, &capability.scope);
    }
}

fn encode_network(manifest: &mut Vec<u8>, network: &NetworkPolicy) {
    match network {
        NetworkPolicy::Deny => manifest.push(TAG_NETWORK_DENY),
        NetworkPolicy::AllowHosts(hosts) => {
            manifest.push(TAG_NETWORK_ALLOW_HOSTS);
            let mut hosts: Vec<_> = hosts.iter().collect();
            hosts.sort();
            encode_length(manifest, hosts.len());
            for host in hosts {
                encode_str(manifest, host);
            }
        }
        NetworkPolicy::AllowAll => manifest.push(TAG_NETWORK_ALLOW_ALL),
    }
}

fn output_kind_tag(kind: OutputKind) -> u8 {
    match kind {
        OutputKind::Blob => 0,
        OutputKind::Tree => 1,
    }
}

fn content_id(content: &ActionInputContent) -> ContentId {
    match content {
        Content::Blob(identity) | Content::Tree(identity) => *identity,
    }
}
