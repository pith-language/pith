//! Property tests for the arbitrary-precision integer: arithmetic agrees with
//! `i128` wherever `i128` can answer, the ring laws hold past the machine
//! range where no oracle exists, and one integer has one canonical encoding,
//! so equality, hashing, and the computation digest agree.

use pith_core::{Int, Value};
use proptest::prelude::*;

/// Machine integers, and products of several of them, which reach a few
/// hundred bits.
fn int_strategy() -> impl Strategy<Value = Int> {
    (any::<bool>(), proptest::collection::vec(any::<i64>(), 1..5)).prop_map(
        |(negative, factors)| {
            let product = factors.into_iter().fold(Int::from(1), |product, factor| {
                product.multiplied(&Int::from(factor))
            });
            if negative { product.negated() } else { product }
        },
    )
}

proptest! {
    /// Where both can answer, they answer the same. `i64` operands and an
    /// `i128` result mean no case is skipped for overflowing the oracle.
    #[test]
    fn arithmetic_agrees_with_a_wider_machine_integer(left: i64, right: i64) {
        let (wide_left, wide_right) = (i128::from(left), i128::from(right));
        let (small_left, small_right) = (Int::from(left), Int::from(right));

        // The workspace bans bare arithmetic operators, hence `checked_`; two
        // `i64` operands cannot overflow an `i128`, so it never returns
        // `None`.
        let oracle = |value: Option<i128>| value.unwrap_or_default().to_string();
        prop_assert_eq!(small_left.added(&small_right).to_string(), oracle(wide_left.checked_add(wide_right)));
        prop_assert_eq!(small_left.subtracted(&small_right).to_string(), oracle(wide_left.checked_sub(wide_right)));
        prop_assert_eq!(small_left.multiplied(&small_right).to_string(), oracle(wide_left.checked_mul(wide_right)));
        prop_assert_eq!(small_left.negated().to_string(), oracle(wide_left.checked_neg()));
        prop_assert_eq!(small_left.cmp(&small_right), left.cmp(&right));
        prop_assert_eq!(small_left.to_i64(), Some(left));
    }

    /// Beyond the machine range there is no oracle, so the algebra is the
    /// check.
    #[test]
    fn the_ring_laws_hold_past_the_machine_range(
        first in int_strategy(),
        second in int_strategy(),
        third in int_strategy(),
    ) {
        prop_assert_eq!(first.added(&second), second.added(&first));
        prop_assert_eq!(first.multiplied(&second), second.multiplied(&first));
        prop_assert_eq!(
            first.added(&second).added(&third),
            first.added(&second.added(&third))
        );
        prop_assert_eq!(
            first.multiplied(&second).multiplied(&third),
            first.multiplied(&second.multiplied(&third))
        );
        prop_assert_eq!(
            first.multiplied(&second.added(&third)),
            first.multiplied(&second).added(&first.multiplied(&third))
        );
        prop_assert_eq!(first.added(&second).subtracted(&second), first.clone());
        prop_assert_eq!(first.added(&Int::zero()), first.clone());
        prop_assert_eq!(first.multiplied(&Int::from(1)), first.clone());
        prop_assert_eq!(first.multiplied(&Int::zero()), Int::zero());
        prop_assert_eq!(first.added(&first.negated()), Int::zero());
    }

    /// Equal integers encode identically however they were computed, and
    /// unequal ones differ: the property the computation digest rests on.
    #[test]
    fn one_integer_has_one_encoding(left in int_strategy(), right in int_strategy()) {
        let (encoded_left, encoded_right) = (
            Value::Int(left.clone()).encode_canonical(),
            Value::Int(right.clone()).encode_canonical(),
        );
        prop_assert_eq!(encoded_left == encoded_right, left == right);
        prop_assert_eq!(
            Value::decode_canonical(&encoded_left),
            Ok(Value::Int(left.clone()))
        );

        // The same value reached by a different route is the same bytes.
        let zero = Value::Int(left.multiplied(&Int::zero())).encode_canonical();
        prop_assert_eq!(zero, Value::int(0).encode_canonical());
        prop_assert_eq!(
            Value::Int(left.negated()).encode_canonical(),
            Value::Int(left.multiplied(&Int::from(-1))).encode_canonical()
        );
    }

    /// Decimal rendering is the projection a person and a JSON reader both
    /// see, so it must name the value exactly.
    #[test]
    fn the_decimal_rendering_matches_the_machine_one(value: i64) {
        prop_assert_eq!(Int::from(value).to_string(), value.to_string());
        prop_assert_eq!(Value::int(value).describe(), value.to_string());
    }
}
