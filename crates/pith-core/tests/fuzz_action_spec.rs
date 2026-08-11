//! Property tests for declared action-contract identity (`ActionSpec::digest`).
//!
//! The digest is the persistent identity of a contract (decision 0023). Its
//! invariants, fuzzed here over many generated *valid* contracts:
//!
//! - it is deterministic;
//! - it ignores the order of set-like fields (inputs, outputs, environment,
//!   capabilities, allowed hosts) because the canonical manifest sorts them;
//! - it preserves argument order (arguments are a sequence, not a set);
//! - storage round-trip does not change it.
//!
//! The strategy builds contracts from disjoint alphabets of single-component
//! paths and NUL-free strings so every generated contract passes `validate()`
//! by construction; a guard test fails the suite if that ever stops holding.

use pith_core::{
    ActionInput, ActionOutput, ActionSpec, CapabilityRequirement, Content, EnvironmentVariable,
    NetworkPolicy, OutputKind, PlatformRequirement,
};
use pith_ids::ContentId;
use proptest::prelude::*;

// Disjoint, single-component path alphabets so no input overlaps another input,
// no output overlaps another output, and no input overlaps any output.
const INPUT_PATHS: &[&str] = &["ia", "ib", "ic", "id"];
const OUTPUT_PATHS: &[&str] = &["oa", "ob", "oc"];
const ARGS: &[&str] = &["", "--flag", "--mode", "release", "verbose"];
const ENV_NAMES: &[&str] = &["A", "B", "C", "LANG"];
const ENV_VALUES: &[&str] = &["", "x", "y", "C.UTF-8"];
const CAP_NAMES: &[&str] = &["fs.read", "fs.write", "net"];
const CAP_SCOPES: &[&str] = &["s1", "s2", "host"];
const HOSTS: &[&str] = &["h1.example", "h2.example", "h3.example"];
const PLATFORM_OS: &[&str] = &["linux", "darwin"];
const PLATFORM_ARCH: &[&str] = &["x86_64", "aarch64"];
const EXECUTABLES: &[&str] = &["/bin/a", "/bin/b", "/bin/c", "/bin/d", "/bin/e"];

fn input_decisions() -> impl Strategy<Value = Vec<(bool, bool, u8)>> {
    proptest::collection::vec(
        (any::<bool>(), any::<bool>(), 0u8..5),
        INPUT_PATHS.len()..=INPUT_PATHS.len(),
    )
}

fn output_decisions() -> impl Strategy<Value = Vec<(bool, bool)>> {
    proptest::collection::vec(
        (any::<bool>(), any::<bool>()),
        OUTPUT_PATHS.len()..=OUTPUT_PATHS.len(),
    )
}

fn env_decisions() -> impl Strategy<Value = Vec<(bool, u8)>> {
    proptest::collection::vec(
        (any::<bool>(), 0u8..(ENV_VALUES.len() as u8)),
        ENV_NAMES.len()..=ENV_NAMES.len(),
    )
}

fn cap_decisions() -> impl Strategy<Value = Vec<(u8, u8, bool)>> {
    proptest::collection::vec(
        (
            0u8..(CAP_NAMES.len() as u8),
            0u8..(CAP_SCOPES.len() as u8),
            any::<bool>(),
        ),
        0..6,
    )
}

fn host_decisions() -> impl Strategy<Value = Vec<bool>> {
    proptest::collection::vec(any::<bool>(), HOSTS.len()..=HOSTS.len())
}

#[allow(
    clippy::too_many_arguments,
    clippy::unwrap_used,
    reason = "the strategy tuple decomposes into the contract fields directly, and indices stay in range of their alphabets"
)]
fn build_spec(
    executable_seed: u8,
    arg_indices: Vec<u8>,
    inputs: Vec<(bool, bool, u8)>,
    outputs: Vec<(bool, bool)>,
    envs: Vec<(bool, u8)>,
    caps: Vec<(u8, u8, bool)>,
    hosts: Vec<bool>,
    is_any: bool,
    os_idx: u8,
    arch_idx: u8,
    network: u8,
) -> ActionSpec {
    let mut input_vec: Vec<ActionInput> = Vec::new();
    for (idx, (include, is_tree, seed)) in inputs.iter().enumerate() {
        if *include {
            let path = INPUT_PATHS.get(idx).copied().unwrap();
            let content = if *is_tree {
                Content::Tree(ContentId::of_tree(&[*seed]))
            } else {
                Content::Blob(ContentId::of_blob(&[*seed]))
            };
            input_vec.push(ActionInput {
                path: path.into(),
                content,
            });
        }
    }

    let mut output_vec: Vec<ActionOutput> = Vec::new();
    for (idx, (include, is_tree)) in outputs.iter().enumerate() {
        if *include {
            let path = OUTPUT_PATHS.get(idx).copied().unwrap();
            let kind = if *is_tree {
                OutputKind::Tree
            } else {
                OutputKind::Blob
            };
            output_vec.push(ActionOutput {
                path: path.into(),
                kind,
            });
        }
    }

    let mut env_vec: Vec<EnvironmentVariable> = Vec::new();
    for (idx, (include, value_idx)) in envs.iter().enumerate() {
        if *include {
            let name = ENV_NAMES.get(idx).copied().unwrap();
            let value = ENV_VALUES.get((*value_idx) as usize).copied().unwrap();
            env_vec.push(EnvironmentVariable {
                name: name.into(),
                value: value.into(),
            });
        }
    }

    // Dedup exact (name, scope) pairs: `validate` rejects exact duplicates but
    // permits the same name under distinct scopes.
    let mut cap_vec: Vec<CapabilityRequirement> = Vec::new();
    let mut seen_caps: Vec<(u8, u8)> = Vec::new();
    for (name_idx, scope_idx, include) in caps.iter() {
        if *include {
            let pair = (*name_idx, *scope_idx);
            if seen_caps.contains(&pair) {
                continue;
            }
            seen_caps.push(pair);
            let name = CAP_NAMES.get((*name_idx) as usize).copied().unwrap();
            let scope = CAP_SCOPES.get((*scope_idx) as usize).copied().unwrap();
            cap_vec.push(CapabilityRequirement {
                name: name.into(),
                scope: scope.into(),
            });
        }
    }

    let mut host_vec: Vec<Box<str>> = Vec::new();
    for (idx, include) in hosts.iter().enumerate() {
        if *include {
            let host = HOSTS.get(idx).copied().unwrap();
            host_vec.push(host.into());
        }
    }

    let platform = if is_any {
        PlatformRequirement::Any
    } else {
        let os = PLATFORM_OS.get(os_idx as usize).copied().unwrap();
        let arch = PLATFORM_ARCH.get(arch_idx as usize).copied().unwrap();
        PlatformRequirement::Exact {
            operating_system: os.into(),
            architecture: arch.into(),
        }
    };

    let network = match network {
        0 => NetworkPolicy::Deny,
        1 => NetworkPolicy::AllowAll,
        _ => NetworkPolicy::AllowHosts(host_vec.into_boxed_slice()),
    };

    let arguments: Vec<Box<str>> = arg_indices
        .iter()
        .map(|i| ARGS.get((*i) as usize).copied().unwrap().into())
        .collect();

    ActionSpec {
        executable: EXECUTABLES
            .get(executable_seed as usize)
            .copied()
            .unwrap_or(EXECUTABLES.first().copied().unwrap())
            .into(),
        toolchain: Box::new([]),
        arguments: arguments.into_boxed_slice(),
        inputs: input_vec.into_boxed_slice(),
        outputs: output_vec.into_boxed_slice(),
        environment: env_vec.into_boxed_slice(),
        platform,
        capabilities: cap_vec.into_boxed_slice(),
        network,
    }
}

fn valid_spec() -> impl Strategy<Value = ActionSpec> {
    (
        0u8..5,
        proptest::collection::vec(0u8..(ARGS.len() as u8), 0..6),
        input_decisions(),
        output_decisions(),
        env_decisions(),
        cap_decisions(),
        host_decisions(),
        any::<bool>(),
        0u8..(PLATFORM_OS.len() as u8),
        0u8..(PLATFORM_ARCH.len() as u8),
        0u8..3,
    )
        .prop_map(
            |(
                executable_seed,
                arg_indices,
                inputs,
                outputs,
                envs,
                caps,
                hosts,
                is_any,
                os_idx,
                arch_idx,
                network,
            )| {
                build_spec(
                    executable_seed,
                    arg_indices,
                    inputs,
                    outputs,
                    envs,
                    caps,
                    hosts,
                    is_any,
                    os_idx,
                    arch_idx,
                    network,
                )
            },
        )
}

proptest! {
    #[test]
    fn generated_contracts_validate(spec in valid_spec()) {
        // Guards the rest of the suite: if the strategy drifts and emits an
        // invalid contract, the digest properties below would be vacuous.
        let validation = spec.validate();
        prop_assert!(
            validation.is_ok(),
            "generated contract failed validation: {:?}",
            validation.err()
        );
    }

    #[test]
    fn digest_is_deterministic(spec in valid_spec()) {
        prop_assert_eq!(spec.digest().ok(), spec.digest().ok());
    }

    #[test]
    fn digest_ignores_set_like_field_order(spec in valid_spec()) {
        let mut reordered = spec.clone();
        reordered.inputs = spec.inputs.iter().rev().cloned().collect::<Vec<_>>().into_boxed_slice();
        reordered.outputs = spec.outputs.iter().rev().cloned().collect::<Vec<_>>().into_boxed_slice();
        reordered.environment = spec
            .environment
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        reordered.capabilities = spec
            .capabilities
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        if let NetworkPolicy::AllowHosts(hosts) = &spec.network {
            let reversed: Vec<Box<str>> = hosts.iter().rev().cloned().collect();
            reordered.network = NetworkPolicy::AllowHosts(reversed.into_boxed_slice());
        }

        // The canonical manifest sorts these fields, so reordering is invisible
        // to identity.
        prop_assert_eq!(spec.digest().ok(), reordered.digest().ok());
    }

    #[test]
    fn digest_preserves_argument_order(spec in valid_spec()) {
        // Arguments are a sequence, not a set: reversing them must change the
        // digest whenever the reversal actually changes the sequence.
        let reversed_args: Vec<Box<str>> = spec.arguments.iter().rev().cloned().collect();
        let unchanged = spec.arguments.iter().eq(reversed_args.iter());
        prop_assume!(!unchanged);

        let mut reordered = spec.clone();
        reordered.arguments = reversed_args.into_boxed_slice();
        prop_assert_ne!(spec.digest().ok(), reordered.digest().ok());
    }

    #[test]
    fn storage_round_trip_preserves_the_digest(spec in valid_spec()) {
        let encoded = spec.encode_stored();
        let decoded = match ActionSpec::decode_stored(&encoded) {
            Ok(decoded) => decoded,
            Err(_) => unreachable!("a valid contract round-trips through its storage encoding"),
        };
        let original_digest = spec.digest().ok();
        let restored_digest = decoded.digest().ok();
        prop_assert_eq!(decoded, spec);
        prop_assert_eq!(restored_digest, original_digest);
    }
}
