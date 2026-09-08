//! Property tests for the canonical codecs in `pith-core`: encode/decode
//! round-trips for `Type`, `Value`, and `ActionSpec` storage, and decoding
//! arbitrary bytes without panicking (persistence adapters decode untrusted
//! stored input through these readers). The storage codec does not call
//! `ActionSpec::validate`, so its strategy may produce invalid contracts and
//! the digest invariants over valid contracts are separate.

use pith_core::{
    ActionInput, ActionOutput, ActionProgram, ActionSpec, CanonicalDecodeError,
    CapabilityRequirement, Content, DeclarationTable, EnvironmentVariable, ExitStatusContract, Int,
    NetworkPolicy, OutputKind, PlatformRequirement, RecordField, SumConstructor, Type, Value,
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

fn declaration_name() -> impl Strategy<Value = String> {
    bounded_string(15).prop_map(|name| format!("n{}", name.replace('.', "_")))
}

/// Ascending, duplicate-free field names, so generated records satisfy the
/// closed-record shape without a filter that would skew generation.
const FIELD_NAMES: [&str; 3] = ["alpha", "beta", "gamma"];

fn record_fields<T>(payloads: Vec<T>) -> Vec<RecordField<T>> {
    FIELD_NAMES
        .iter()
        .zip(payloads)
        .map(|(name, payload)| RecordField {
            name: (*name).into(),
            payload,
        })
        .collect()
}

fn type_strategy() -> impl Strategy<Value = Type> {
    prop_oneof![
        Just(Type::Unit),
        Just(Type::Bool),
        Just(Type::Int),
        Just(Type::Text),
        Just(Type::Bytes),
        Just(Type::Blob),
        declaration_name().prop_map(|name| declared_nominal(&name, Type::Blob)),
        leaf_type_strategy().prop_map(|element| Type::List(Box::new(element))),
        proptest::collection::vec(leaf_type_strategy(), 0..3)
            .prop_map(|payloads| Type::Record(record_fields(payloads).into())),
        (
            declaration_name(),
            proptest::option::of(leaf_type_strategy())
        )
            .prop_map(|(name, payload)| declared_sum(
                &name,
                [SumConstructor {
                    name: "only".into(),
                    payload,
                }]
            ),),
    ]
}

/// Non-recursive types, so `type_strategy` stays finite: list elements and
/// record fields are drawn from here, never from `type_strategy`.
fn leaf_type_strategy() -> impl Strategy<Value = Type> {
    prop_oneof![
        Just(Type::Unit),
        Just(Type::Bool),
        Just(Type::Int),
        Just(Type::Text),
        Just(Type::Bytes),
        Just(Type::Blob),
        declaration_name().prop_map(|name| declared_nominal(&name, Type::Blob)),
        // The recursion cut, so generation covers the tag itself, not only the
        // hand-written fixtures below.
        Just(Type::Cut),
        // A nominal over something other than `Blob`, so the declared
        // representation varies and the codec has to carry it.
        declaration_name()
            .prop_map(|name| declared_nominal(&name, Type::List(Box::new(Type::Text)))),
        // A recursive nominal, whose body reaches its own cut.
        declaration_name()
            .prop_map(|name| declared_nominal(&name, Type::List(Box::new(Type::Cut)))),
    ]
}

/// A nominal declared in a throwaway table. The property under test is the
/// codec's round trip, so the declaration's module and representation are
/// fixed and only the declared name varies.
fn declared_nominal(name: &str, representation: Type) -> Type {
    let mut table = DeclarationTable::new("fuzz");
    match table.nominal(name, representation) {
        Ok(declared) => declared,
        Err(error) => unreachable!("a fresh table admits one name: {error}"),
    }
}

/// A declared sum in a throwaway table. See [`declared_nominal`].
fn declared_sum(name: &str, constructors: [SumConstructor; 1]) -> Type {
    let mut table = DeclarationTable::new("fuzz");
    match table.sum(name, constructors) {
        Ok(declared) => declared,
        Err(error) => unreachable!("a fresh table admits one name: {error}"),
    }
}

/// Integers across the whole range the type admits: a machine integer, and a
/// product of several of them, whose magnitude runs to hundreds of bits. This
/// population exercises the codec's minimal-magnitude rule at lengths no
/// `i64` can produce.
fn int_strategy() -> impl Strategy<Value = Int> {
    prop_oneof![
        any::<i64>().prop_map(Int::from),
        (any::<bool>(), proptest::collection::vec(any::<i64>(), 1..6)).prop_map(
            |(negative, factors)| {
                let product = factors.into_iter().fold(Int::from(1), |product, factor| {
                    product.multiplied(&Int::from(factor))
                });
                if negative { product.negated() } else { product }
            }
        ),
    ]
}

/// Scalar-only generators used both directly and as the bounded
/// representation inside a nominal value, so `value_strategy` stays finite.
fn scalar_value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Unit),
        any::<bool>().prop_map(Value::Bool),
        int_strategy().prop_map(Value::Int),
        bounded_string(24).prop_map(|s| Value::Text(s.into_boxed_str())),
        bounded_bytes(32).prop_map(|b| Value::Bytes(b.into_boxed_slice())),
        bounded_bytes(32).prop_map(|b| Value::Blob(ContentId::of_blob(&b))),
    ]
}

fn value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Unit),
        any::<bool>().prop_map(Value::Bool),
        int_strategy().prop_map(Value::Int),
        bounded_string(24).prop_map(|s| Value::Text(s.into_boxed_str())),
        bounded_bytes(32).prop_map(|b| Value::Bytes(b.into_boxed_slice())),
        bounded_bytes(32).prop_map(|b| Value::Blob(ContentId::of_blob(&b))),
        (bounded_string(16), scalar_value_strategy()).prop_map(|(name, representation)| {
            Value::Nominal {
                name: name.into_boxed_str(),
                representation: Box::new(representation),
            }
        }),
        proptest::collection::vec(scalar_value_strategy(), 0..4)
            .prop_map(|elements| Value::List(elements.into_boxed_slice())),
        proptest::collection::vec(scalar_value_strategy(), 0..3)
            .prop_map(|payloads| Value::Record(record_fields(payloads).into())),
        (
            bounded_string(16),
            bounded_string(16),
            proptest::option::of(scalar_value_strategy()),
        )
            .prop_map(|(type_name, constructor, payload)| Value::Sum {
                type_name: type_name.into_boxed_str(),
                constructor: constructor.into_boxed_str(),
                payload: payload.map(Box::new),
            }),
    ]
}

/// Valid absolute host paths, built from a fixed alphabet so every generated
/// path passes `is_valid_host_path`: absolute, NUL-free, no traversal.
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

/// Both program variants, so the round trip covers the tagged sum and not only
/// the host-path arm.
fn program_strategy() -> impl Strategy<Value = ActionProgram> {
    prop_oneof![
        executable_path_strategy().prop_map(ActionProgram::HostPath),
        bounded_bytes(32).prop_map(|bytes| ActionProgram::Content(ContentId::of_blob(&bytes))),
    ]
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
        program_strategy(),
        prop_oneof![
            Just(ExitStatusContract::SuccessRequired),
            Just(ExitStatusContract::Reported),
        ],
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
                exit_status,
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
                    exit_status,
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

/// The generated properties cap input size, which caps nesting far below the
/// depth that overflows the stack, and a stack overflow is not a panic a
/// property test can catch: the process aborts rather than unwinds. Before
/// the depth limit, a few megabytes of recursive tags aborted the process
/// instead of returning an error, so the limit is asserted directly.
#[test]
fn decoding_deeply_nested_nominal_values_fails_rather_than_overflowing() {
    // Each level is the nominal tag plus an empty name: one recursion per nine
    // bytes. Enough levels to have overflowed a real stack.
    let mut encoded = vec![1u8];
    for _ in 0..400_000 {
        encoded.push(6);
        encoded.extend_from_slice(&0u64.to_le_bytes());
    }
    encoded.push(0);

    assert_eq!(
        Value::decode_canonical(&encoded),
        Err(CanonicalDecodeError::NestingTooDeep {
            limit: pith_core::MAX_NOMINAL_NESTING,
        })
    )
}

/// The list tag is one byte per level, so it reaches the unbounded-recursion
/// shape even faster than the nominal chain above.
#[test]
fn decoding_deeply_nested_lists_fails_rather_than_overflowing() {
    let mut encoded = vec![1u8, 7];
    for _ in 0..400_000 {
        encoded.extend_from_slice(&1u64.to_le_bytes());
        encoded.push(7);
    }
    // The innermost list holds one `Unit`, closing the chain.
    encoded.extend_from_slice(&1u64.to_le_bytes());
    encoded.push(0);

    assert_eq!(
        Value::decode_canonical(&encoded),
        Err(CanonicalDecodeError::NestingTooDeep {
            limit: pith_core::MAX_NOMINAL_NESTING,
        })
    )
}

/// A chain exactly at the limit decodes; one level past it is refused.
#[test]
fn nesting_is_accepted_up_to_the_limit() {
    fn nest(levels: u32) -> Value {
        let mut value = Value::Unit;
        for _ in 0..levels {
            value = Value::Nominal {
                name: "N".into(),
                representation: Box::new(value),
            };
        }
        value
    }

    let at_limit = nest(pith_core::MAX_NOMINAL_NESTING);
    assert_eq!(
        Value::decode_canonical(&at_limit.encode_canonical()),
        Ok(at_limit)
    );

    let past_limit = nest(pith_core::MAX_NOMINAL_NESTING + 1);
    assert_eq!(
        Value::decode_canonical(&past_limit.encode_canonical()),
        Err(CanonicalDecodeError::NestingTooDeep {
            limit: pith_core::MAX_NOMINAL_NESTING,
        })
    );
}
