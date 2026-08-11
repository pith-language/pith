//! Property tests for the canonical codecs in `pith-core`.
//!
//! Two contracts are fuzzed:
//! 1. encode/decode round-trips for `Type`, `Value`, and `ActionSpec` storage;
//! 2. decoding arbitrary bytes never panics — persistence adapters decode
//!    untrusted stored input through these readers, so malformed bytes must
//!    become an error rather than an index, allocation, or panic.
//!
//! The storage codec does not call `ActionSpec::validate`, so the round-trip
//! strategy is free to produce contracts with arbitrary (possibly invalid)
//! strings; the digest invariants over *valid* contracts are covered in
//! `fuzz_action_spec.rs`.

use pith_core::{
    ActionInput, ActionOutput, ActionSpec, CapabilityRequirement, Content, EnvironmentVariable,
    NetworkPolicy, OutputKind, PlatformRequirement, Type, Value,
};
use pith_ids::ContentId;
use proptest::prelude::*;

fn bounded_bytes(max: usize) -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..max)
}

/// Arbitrary `char`s collected into a `String` always produce valid UTF-8, so
/// `Value::Text` and the storage codec's length-prefixed strings round-trip.
fn bounded_string(max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..max).prop_map(|cs| cs.into_iter().collect())
}

fn type_strategy() -> impl Strategy<Value = Type> {
    prop_oneof![
        Just(Type::Unit),
        Just(Type::Bool),
        Just(Type::Int),
        Just(Type::Text),
        Just(Type::Bytes),
        Just(Type::Blob),
        bounded_string(16).prop_map(|name| Type::Nominal {
            name: name.into_boxed_str(),
        }),
    ]
}

fn value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Unit),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        bounded_string(24).prop_map(|s| Value::Text(s.into_boxed_str())),
        bounded_bytes(32).prop_map(|b| Value::Bytes(b.into_boxed_slice())),
        bounded_bytes(32).prop_map(|b| Value::Blob(ContentId::of_blob(&b))),
    ]
}

/// Valid absolute host paths for the `executable` field (decision 0030). Built
/// from a fixed alphabet so every generated path passes `is_valid_host_path`:
/// absolute, NUL-free, no traversal components.
fn executable_path_strategy() -> impl Strategy<Value = Box<str>> {
    let components = ["a", "bin", "tool", "nix", "store", "gcc", "x86_64"];
    proptest::collection::vec(0u8..(components.len() as u8), 1..4).prop_map(move |indices| {
        let mut path = String::from("/");
        for (i, idx) in indices.iter().enumerate() {
            if i > 0 {
                path.push('/');
            }
            path.push_str(components.get(*idx as usize).copied().unwrap_or("a"));
        }
        path.into_boxed_str()
    })
}

fn action_input_strategy() -> impl Strategy<Value = ActionInput> {
    (
        bounded_string(24),
        prop_oneof![Just(OutputKind::Blob), Just(OutputKind::Tree)],
        bounded_bytes(32),
    )
        .prop_map(|(path, kind, seed)| {
            let content = match kind {
                OutputKind::Blob => Content::Blob(ContentId::of_blob(&seed)),
                OutputKind::Tree => Content::Tree(ContentId::of_tree(&seed)),
            };
            ActionInput {
                path: path.into_boxed_str(),
                content,
            }
        })
}

fn action_output_strategy() -> impl Strategy<Value = ActionOutput> {
    (
        bounded_string(24),
        prop_oneof![Just(OutputKind::Blob), Just(OutputKind::Tree)],
    )
        .prop_map(|(path, kind)| ActionOutput {
            path: path.into_boxed_str(),
            kind,
        })
}

fn environment_strategy() -> impl Strategy<Value = EnvironmentVariable> {
    (bounded_string(16), bounded_string(16)).prop_map(|(name, value)| EnvironmentVariable {
        name: name.into_boxed_str(),
        value: value.into_boxed_str(),
    })
}

fn capability_strategy() -> impl Strategy<Value = CapabilityRequirement> {
    (bounded_string(16), bounded_string(16)).prop_map(|(name, scope)| CapabilityRequirement {
        name: name.into_boxed_str(),
        scope: scope.into_boxed_str(),
    })
}

fn platform_strategy() -> impl Strategy<Value = PlatformRequirement> {
    prop_oneof![
        Just(PlatformRequirement::Any),
        (bounded_string(16), bounded_string(16)).prop_map(|(os, arch)| {
            PlatformRequirement::Exact {
                operating_system: os.into_boxed_str(),
                architecture: arch.into_boxed_str(),
            }
        }),
    ]
}

fn network_strategy() -> impl Strategy<Value = NetworkPolicy> {
    prop_oneof![
        Just(NetworkPolicy::Deny),
        Just(NetworkPolicy::AllowAll),
        proptest::collection::vec(bounded_string(16), 0..4).prop_map(|hosts| {
            NetworkPolicy::AllowHosts(
                hosts
                    .into_iter()
                    .map(|h| h.into_boxed_str())
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            )
        }),
    ]
}

fn spec_strategy() -> impl Strategy<Value = ActionSpec> {
    (
        executable_path_strategy(),
        proptest::collection::vec(bounded_string(16), 0..4),
        proptest::collection::vec(action_input_strategy(), 0..4),
        proptest::collection::vec(action_output_strategy(), 0..4),
        proptest::collection::vec(environment_strategy(), 0..4),
        platform_strategy(),
        proptest::collection::vec(capability_strategy(), 0..4),
        network_strategy(),
    )
        .prop_map(
            |(
                executable,
                arguments,
                inputs,
                outputs,
                environment,
                platform,
                capabilities,
                network,
            )| {
                ActionSpec {
                    executable,
                    toolchain: Box::new([]),
                    arguments: arguments
                        .into_iter()
                        .map(|a| a.into_boxed_str())
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    inputs: inputs.into_boxed_slice(),
                    outputs: outputs.into_boxed_slice(),
                    environment: environment.into_boxed_slice(),
                    platform,
                    capabilities: capabilities.into_boxed_slice(),
                    network,
                }
            },
        )
}

proptest! {
    #[test]
    fn type_encode_decode_round_trips(ty in type_strategy()) {
        let encoded = ty.encode_canonical();
        prop_assert_eq!(Type::decode_canonical(&encoded), Ok(ty.clone()));
    }

    #[test]
    fn value_encode_decode_round_trips(value in value_strategy()) {
        let encoded = value.encode_canonical();
        prop_assert_eq!(Value::decode_canonical(&encoded), Ok(value.clone()));
    }

    #[test]
    fn type_and_value_encodings_are_deterministic(
        ty in type_strategy(),
        value in value_strategy(),
    ) {
        prop_assert_eq!(ty.encode_canonical(), ty.encode_canonical());
        prop_assert_eq!(value.encode_canonical(), value.encode_canonical());
    }

    #[test]
    fn action_spec_storage_round_trips(spec in spec_strategy()) {
        let encoded = spec.encode_stored();
        prop_assert_eq!(ActionSpec::decode_stored(&encoded), Ok(spec.clone()));
    }

    #[test]
    fn action_spec_storage_encoding_is_deterministic(spec in spec_strategy()) {
        prop_assert_eq!(spec.encode_stored(), spec.encode_stored());
    }

    #[test]
    fn decoding_arbitrary_bytes_as_a_type_never_panics(bytes in bounded_bytes(256)) {
        // The reader is fed untrusted stored bytes; it must return an error
        // rather than panic or index out of bounds.
        let result = Type::decode_canonical(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn decoding_arbitrary_bytes_as_a_value_never_panics(bytes in bounded_bytes(256)) {
        let result = Value::decode_canonical(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn decoding_arbitrary_bytes_as_an_action_spec_never_panics(bytes in bounded_bytes(512)) {
        let result = ActionSpec::decode_stored(&bytes);
        prop_assert!(result.is_ok() || result.is_err());
    }
}
