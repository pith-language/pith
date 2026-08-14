//! Versioned storage encoding for [`ActionSpec`].
//!
//! Distinct from the action-spec *manifest* in [`crate::action`], which exists
//! to derive an [`ActionSpecDigest`] and whose byte layout is frozen by every
//! persisted digest (decision 0023). This encoding exists to store and restore
//! a contract, and is versioned independently so the storage contract can
//! evolve without invalidating identities (decision 0024).
//!
//! [`ActionSpecDigest`]: pith_ids::ActionSpecDigest

use crate::action::{
    ActionInput, ActionOutput, ActionProgram, ActionSpec, EnvironmentVariable, ExitStatusContract,
    NetworkPolicy, PlatformRequirement,
};
use crate::codec::{
    CanonicalDecodeError, CanonicalReader, encode_capabilities, encode_content, encode_sequence,
    encode_str, output_kind_tag, read_capabilities, read_content, read_output_kind,
};

/// Version 2 carries the program as a tagged sum so a contract can name content
/// the graph produced as well as a host path (decision 0036), and the exit-status
/// contract a rule reads a verdict from (decision 0037). The storage format is
/// versioned independently of the digest manifest for this reason (0024).
const ENCODING_VERSION: u8 = 2;

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
    /// Encode this contract in the current version of the storage format.
    #[must_use]
    pub fn encode_stored(&self) -> Vec<u8> {
        let mut encoded = vec![ENCODING_VERSION];
        match &self.executable {
            ActionProgram::HostPath(path) => {
                encoded.push(TAG_PROGRAM_HOST_PATH);
                encode_str(&mut encoded, path);
            }
            ActionProgram::Content(id) => {
                encoded.push(TAG_PROGRAM_CONTENT);
                encoded.extend_from_slice(id.digest().as_bytes());
            }
        }
        encode_sequence(&mut encoded, &self.toolchain, |out, path| {
            encode_str(out, path);
        });
        encode_sequence(&mut encoded, &self.arguments, |out, argument| {
            encode_str(out, argument);
        });
        encode_sequence(&mut encoded, &self.inputs, |out, input| {
            encode_str(out, &input.path);
            encode_content(out, &input.content);
        });
        encode_sequence(&mut encoded, &self.outputs, |out, output| {
            encode_str(out, &output.path);
            out.push(output_kind_tag(output.kind));
        });
        encode_sequence(&mut encoded, &self.environment, |out, variable| {
            encode_str(out, &variable.name);
            encode_str(out, &variable.value);
        });
        encode_platform(&mut encoded, &self.platform);
        encode_capabilities(&mut encoded, &self.capabilities);
        encode_network(&mut encoded, &self.network);
        encoded.push(match self.exit_status {
            ExitStatusContract::SuccessRequired => TAG_EXIT_SUCCESS_REQUIRED,
            ExitStatusContract::Reported => TAG_EXIT_REPORTED,
        });
        encoded
    }

    /// Decode a contract from the versioned storage format.
    ///
    /// # Errors
    /// Returns a canonical decoding error for an unsupported version, an
    /// unknown discriminant tag, truncated or trailing data, invalid UTF-8, or
    /// a length not representable on this platform.
    pub fn decode_stored(encoded: &[u8]) -> Result<Self, CanonicalDecodeError> {
        let mut reader = CanonicalReader::new(encoded);
        reader.read_version(ENCODING_VERSION)?;
        let spec = read_spec(&mut reader)?;
        reader.finish()?;
        Ok(spec)
    }
}

/// Decode the tagged program a contract runs.
fn read_program(reader: &mut CanonicalReader<'_>) -> Result<ActionProgram, CanonicalDecodeError> {
    match reader.read_byte()? {
        TAG_PROGRAM_HOST_PATH => Ok(ActionProgram::HostPath(reader.read_text()?.into())),
        TAG_PROGRAM_CONTENT => Ok(ActionProgram::Content(reader.read_content_id()?)),
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

/// Decode a contract body after its version byte has been read.
fn read_spec(reader: &mut CanonicalReader<'_>) -> Result<ActionSpec, CanonicalDecodeError> {
    let executable = read_program(reader)?;
    let toolchain = reader.read_sequence(|reader| reader.read_text().map(Box::<str>::from))?;
    let arguments = reader.read_sequence(|reader| reader.read_text().map(Box::<str>::from))?;
    let inputs = reader.read_sequence(|reader| {
        let path = Box::<str>::from(reader.read_text()?);
        let content = read_content(reader)?;
        Ok(ActionInput { path, content })
    })?;
    let outputs = reader.read_sequence(|reader| {
        let path = Box::<str>::from(reader.read_text()?);
        let kind = read_output_kind(reader)?;
        Ok(ActionOutput { path, kind })
    })?;
    let environment = reader.read_sequence(|reader| {
        let name = Box::<str>::from(reader.read_text()?);
        let value = Box::<str>::from(reader.read_text()?);
        Ok(EnvironmentVariable { name, value })
    })?;
    let platform = read_platform(reader)?;
    let capabilities = read_capabilities(reader)?;
    let network = read_network(reader)?;
    let exit_status = match reader.read_byte()? {
        TAG_EXIT_SUCCESS_REQUIRED => ExitStatusContract::SuccessRequired,
        TAG_EXIT_REPORTED => ExitStatusContract::Reported,
        tag => return Err(CanonicalDecodeError::UnknownValueTag { tag }),
    };
    Ok(ActionSpec {
        executable,
        toolchain,
        arguments,
        inputs,
        outputs,
        environment,
        platform,
        capabilities,
        network,
        exit_status,
    })
}

fn encode_platform(encoded: &mut Vec<u8>, platform: &PlatformRequirement) {
    match platform {
        PlatformRequirement::Any => encoded.push(TAG_PLATFORM_ANY),
        PlatformRequirement::Exact {
            operating_system,
            architecture,
        } => {
            encoded.push(TAG_PLATFORM_EXACT);
            encode_str(encoded, operating_system);
            encode_str(encoded, architecture);
        }
    }
}

fn read_platform(
    reader: &mut CanonicalReader<'_>,
) -> Result<PlatformRequirement, CanonicalDecodeError> {
    match reader.read_byte()? {
        TAG_PLATFORM_ANY => Ok(PlatformRequirement::Any),
        TAG_PLATFORM_EXACT => Ok(PlatformRequirement::Exact {
            operating_system: reader.read_text()?.into(),
            architecture: reader.read_text()?.into(),
        }),
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

fn encode_network(encoded: &mut Vec<u8>, network: &NetworkPolicy) {
    match network {
        NetworkPolicy::Deny => encoded.push(TAG_NETWORK_DENY),
        NetworkPolicy::AllowHosts(hosts) => {
            encoded.push(TAG_NETWORK_ALLOW_HOSTS);
            encode_sequence(encoded, hosts, |out, host| encode_str(out, host));
        }
        NetworkPolicy::AllowAll => encoded.push(TAG_NETWORK_ALLOW_ALL),
    }
}

fn read_network(reader: &mut CanonicalReader<'_>) -> Result<NetworkPolicy, CanonicalDecodeError> {
    match reader.read_byte()? {
        TAG_NETWORK_DENY => Ok(NetworkPolicy::Deny),
        TAG_NETWORK_ALLOW_HOSTS => Ok(NetworkPolicy::AllowHosts(
            reader.read_sequence(|reader| reader.read_text().map(Box::<str>::from))?,
        )),
        TAG_NETWORK_ALLOW_ALL => Ok(NetworkPolicy::AllowAll),
        tag => Err(CanonicalDecodeError::UnknownValueTag { tag }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{CapabilityRequirement, Content, OutputKind};
    use pith_ids::{ContentDigest, ContentId};

    fn content_id(seed: u8) -> ContentId {
        ContentId::from_digest(ContentDigest::from_bytes([seed; pith_ids::DIGEST_LEN]))
    }

    fn populated_spec() -> ActionSpec {
        ActionSpec {
            executable: ActionProgram::HostPath("/bin/tool".into()),
            toolchain: ["/nix/store/gcc".into(), "/nix/store/glibc".into()].into(),
            arguments: ["--flag".into(), "".into()].into(),
            inputs: [
                ActionInput {
                    path: "in/blob".into(),
                    content: Content::Blob(content_id(2)),
                },
                ActionInput {
                    path: "in/tree".into(),
                    content: Content::Tree(content_id(3)),
                },
            ]
            .into(),
            outputs: [
                ActionOutput {
                    path: "out/blob".into(),
                    kind: OutputKind::Blob,
                },
                ActionOutput {
                    path: "out/tree".into(),
                    kind: OutputKind::Tree,
                },
            ]
            .into(),
            environment: [EnvironmentVariable {
                name: "NAME".into(),
                value: "value".into(),
            }]
            .into(),
            platform: PlatformRequirement::Exact {
                operating_system: "linux".into(),
                architecture: "aarch64".into(),
            },
            capabilities: [CapabilityRequirement {
                name: "net".into(),
                scope: "example.test".into(),
            }]
            .into(),
            network: NetworkPolicy::AllowHosts(["example.test".into()].into()),
            exit_status: ExitStatusContract::SuccessRequired,
        }
    }

    #[test]
    fn a_populated_contract_round_trips() {
        let spec = populated_spec();
        let decoded =
            ActionSpec::decode_stored(&spec.encode_stored()).expect("the contract decodes");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn an_isolated_contract_round_trips() {
        let spec = ActionSpec::isolated("/bin/tool");
        let decoded =
            ActionSpec::decode_stored(&spec.encode_stored()).expect("the contract decodes");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn every_platform_and_network_variant_round_trips() {
        for platform in [
            PlatformRequirement::Any,
            PlatformRequirement::Exact {
                operating_system: "darwin".into(),
                architecture: "x86_64".into(),
            },
        ] {
            for network in [
                NetworkPolicy::Deny,
                NetworkPolicy::AllowAll,
                NetworkPolicy::AllowHosts(Box::new([])),
                NetworkPolicy::AllowHosts(["a".into(), "b".into()].into()),
            ] {
                let mut spec = ActionSpec::isolated("/bin/tool");
                spec.platform = platform.clone();
                spec.network = network.clone();
                let decoded =
                    ActionSpec::decode_stored(&spec.encode_stored()).expect("the contract decodes");
                assert_eq!(decoded, spec);
            }
        }
    }

    #[test]
    fn round_tripping_preserves_the_contract_digest() {
        // The storage encoding is a separate contract from the digest manifest,
        // but restoring a contract must not change the identity it had.
        let spec = populated_spec();
        let decoded =
            ActionSpec::decode_stored(&spec.encode_stored()).expect("the contract decodes");
        assert_eq!(decoded.digest().ok(), spec.digest().ok());
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let spec = ActionSpec::isolated("/bin/tool");
        let mut encoded = spec.encode_stored();
        if let Some(version) = encoded.first_mut() {
            *version = ENCODING_VERSION.wrapping_add(1);
        }
        assert_eq!(
            ActionSpec::decode_stored(&encoded).err(),
            Some(CanonicalDecodeError::UnsupportedVersion {
                version: ENCODING_VERSION.wrapping_add(1),
            })
        );
    }

    #[test]
    fn a_truncated_encoding_is_rejected() {
        let spec = populated_spec();
        let encoded = spec.encode_stored();
        for length in 0..encoded.len() {
            let prefix = encoded.get(..length).unwrap_or_default();
            assert!(
                ActionSpec::decode_stored(prefix).is_err(),
                "a {length}-byte prefix decoded as a whole contract"
            );
        }
    }
}
