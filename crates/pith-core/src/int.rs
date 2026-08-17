//! The arbitrary-precision integer behind [`Value::Int`](crate::Value::Int)
//! (decision 0055).
//!
//! Sign and magnitude, in little-endian 32-bit limbs, kept normalized: the most
//! significant limb is never zero, and zero carries an empty magnitude with
//! `negative` false. One value has one representation, which is what lets the
//! derived `PartialEq` and `Hash` agree with numeric equality and what makes the
//! canonical encoding injective.
//!
//! Addition, subtraction, negation, and multiplication are total: every pair of
//! integers has a sum, a difference, and a product, and the type has room for
//! all of them. Division is absent, not deferred by oversight — see the record.

use std::cmp::Ordering;

use smallvec::SmallVec;

type Limb = u32;
const LIMB_BITS: u32 = 32;

/// Two limbs inline: the 64-bit range that used to be the whole type costs no
/// allocation, and the type stays smaller than `Value::Sum`, so `Value` is the
/// size it was before the integer grew.
type Magnitude = SmallVec<[Limb; 2]>;

/// Chunk size for decimal rendering: the largest power of ten that fits in a
/// limb, so one division per nine digits.
const DECIMAL_CHUNK: Limb = 1_000_000_000;
const DECIMAL_CHUNK_DIGITS: usize = DECIMAL_CHUNK.ilog10() as usize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Int {
    negative: bool,
    magnitude: Magnitude,
}

impl Int {
    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.magnitude.is_empty()
    }

    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    /// This integer as an `i64`, or `None` when it does not fit.
    ///
    /// The kernel's own counters and lengths stay fixed-width; this is the
    /// boundary between them and a value.
    #[must_use]
    pub fn to_i64(&self) -> Option<i64> {
        let magnitude = self.to_u64()?;
        let signed = if self.negative {
            0i128.wrapping_sub(i128::from(magnitude))
        } else {
            i128::from(magnitude)
        };
        i64::try_from(signed).ok()
    }

    fn to_u64(&self) -> Option<u64> {
        let mut value: u64 = 0;
        for (index, limb) in self.magnitude.iter().enumerate() {
            let shift = u32::try_from(index).ok()?.checked_mul(LIMB_BITS)?;
            if shift >= u64::BITS {
                return None;
            }
            value |= u64::from(*limb).wrapping_shl(shift);
        }
        Some(value)
    }

    /// This integer with the opposite sign.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self::normalized(!self.negative, self.magnitude.clone())
    }

    /// The sum, which always exists.
    #[must_use]
    pub fn added(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            return Self::normalized(
                self.negative,
                add_magnitudes(&self.magnitude, &other.magnitude),
            );
        }
        match compare_magnitudes(&self.magnitude, &other.magnitude) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => Self::normalized(
                self.negative,
                subtract_magnitudes(&self.magnitude, &other.magnitude),
            ),
            Ordering::Less => Self::normalized(
                other.negative,
                subtract_magnitudes(&other.magnitude, &self.magnitude),
            ),
        }
    }

    /// The difference, which always exists.
    #[must_use]
    pub fn subtracted(&self, other: &Self) -> Self {
        self.added(&other.negated())
    }

    /// The product, which always exists.
    #[must_use]
    pub fn multiplied(&self, other: &Self) -> Self {
        Self::normalized(
            self.negative != other.negative,
            multiply_magnitudes(&self.magnitude, &other.magnitude),
        )
    }

    /// The magnitude in big-endian bytes with no leading zero, which is the
    /// canonical encoding's payload. Zero yields an empty slice.
    #[must_use]
    pub(crate) fn magnitude_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.magnitude.len().saturating_mul(4));
        for limb in self.magnitude.iter().rev() {
            bytes.extend_from_slice(&limb.to_be_bytes());
        }
        let leading_zeroes = bytes.iter().take_while(|byte| **byte == 0).count();
        bytes.drain(..leading_zeroes);
        bytes
    }

    /// Rebuild from a sign and big-endian magnitude bytes, refusing anything
    /// that is not the one encoding of its value: a leading zero byte, or a
    /// negative zero.
    pub(crate) fn from_sign_and_magnitude(negative: bool, bytes: &[u8]) -> Option<Self> {
        if bytes.first() == Some(&0) {
            return None;
        }
        if negative && bytes.is_empty() {
            return None;
        }
        let mut magnitude = Magnitude::new();
        let mut limb: Limb = 0;
        let mut filled: u32 = 0;
        for byte in bytes.iter().rev() {
            limb |= Limb::from(*byte).wrapping_shl(filled);
            filled = filled.wrapping_add(8);
            if filled == LIMB_BITS {
                magnitude.push(limb);
                limb = 0;
                filled = 0;
            }
        }
        if filled != 0 {
            magnitude.push(limb);
        }
        Some(Self::normalized(negative, magnitude))
    }

    fn normalized(negative: bool, mut magnitude: Magnitude) -> Self {
        while magnitude.last() == Some(&0) {
            let _ = magnitude.pop();
        }
        Self {
            negative: negative && !magnitude.is_empty(),
            magnitude,
        }
    }

    fn from_sign_and_u64(negative: bool, value: u64) -> Self {
        let low = low_limb(value);
        let high = low_limb(high_bits(value));
        Self::normalized(negative, Magnitude::from_slice(&[low, high]))
    }
}

impl PartialOrd for Int {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Int {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => compare_magnitudes(&self.magnitude, &other.magnitude),
            (true, true) => compare_magnitudes(&other.magnitude, &self.magnitude),
        }
    }
}

impl std::fmt::Display for Int {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_zero() {
            return formatter.write_str("0");
        }
        let mut remaining = self.magnitude.clone();
        let mut chunks: Vec<Limb> = Vec::new();
        while !remaining.is_empty() {
            chunks.push(divide_by_decimal_chunk(&mut remaining));
        }
        if self.negative {
            formatter.write_str("-")?;
        }
        let mut chunks = chunks.into_iter().rev();
        if let Some(leading) = chunks.next() {
            write!(formatter, "{leading}")?;
        }
        for chunk in chunks {
            write!(formatter, "{chunk:0>width$}", width = DECIMAL_CHUNK_DIGITS)?;
        }
        Ok(())
    }
}

macro_rules! int_from_signed {
    ($($signed:ty),*) => {
        $(impl From<$signed> for Int {
            fn from(value: $signed) -> Self {
                Self::from_sign_and_u64(value < 0, u64::from(value.unsigned_abs()))
            }
        })*
    };
}

macro_rules! int_from_unsigned {
    ($($unsigned:ty),*) => {
        $(impl From<$unsigned> for Int {
            fn from(value: $unsigned) -> Self {
                Self::from_sign_and_u64(false, u64::from(value))
            }
        })*
    };
}

int_from_signed!(i8, i16, i32, i64);
int_from_unsigned!(u8, u16, u32, u64);

impl TryFrom<&Int> for i64 {
    type Error = OutOfRangeError;

    fn try_from(value: &Int) -> Result<Self, Self::Error> {
        value.to_i64().ok_or(OutOfRangeError)
    }
}

/// An [`Int`] was asked for as a fixed-width integer it does not fit in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfRangeError;

impl std::fmt::Display for OutOfRangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("integer does not fit in the requested width")
    }
}

impl std::error::Error for OutOfRangeError {}

/// The limb at `index`, or zero past the end. Magnitudes of different lengths
/// are added and compared limb by limb, and the shorter one's missing limbs are
/// zeroes.
fn limb_at(limbs: &[Limb], index: usize) -> Limb {
    limbs.get(index).copied().unwrap_or(0)
}

fn low_limb(value: u64) -> Limb {
    value as Limb
}

fn high_bits(value: u64) -> u64 {
    value.wrapping_shr(LIMB_BITS)
}

fn compare_magnitudes(left: &[Limb], right: &[Limb]) -> Ordering {
    // Normalized magnitudes carry no leading zero limb, so the longer one is
    // the larger one and equal lengths decide from the top down.
    match left.len().cmp(&right.len()) {
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
        ordering => ordering,
    }
}

/// Every limb product below fits in a `u64` — `(2^32 - 1)^2` plus two limbs is
/// `2^64 - 1` — so the `wrapping_` operations here cannot wrap. They are spelled
/// as methods because the workspace denies the arithmetic operators, and the
/// widening is the argument that no operation needs an escape hatch.
fn add_magnitudes(left: &[Limb], right: &[Limb]) -> Magnitude {
    let width = left.len().max(right.len());
    let mut sum = Magnitude::with_capacity(width.saturating_add(1));
    let mut carry: u64 = 0;
    for index in 0..width {
        let total = carry
            .wrapping_add(u64::from(limb_at(left, index)))
            .wrapping_add(u64::from(limb_at(right, index)));
        sum.push(low_limb(total));
        carry = high_bits(total);
    }
    if carry != 0 {
        sum.push(low_limb(carry));
    }
    sum
}

/// `left` must be the larger magnitude, which the caller decides by comparing
/// first.
fn subtract_magnitudes(left: &[Limb], right: &[Limb]) -> Magnitude {
    let mut difference = Magnitude::with_capacity(left.len());
    let mut borrow: u64 = 0;
    for index in 0..left.len() {
        let subtrahend = u64::from(limb_at(right, index)).wrapping_add(borrow);
        let (remainder, underflowed) = u64::from(limb_at(left, index)).overflowing_sub(subtrahend);
        difference.push(low_limb(remainder));
        borrow = u64::from(underflowed);
    }
    difference
}

fn multiply_magnitudes(left: &[Limb], right: &[Limb]) -> Magnitude {
    if left.is_empty() || right.is_empty() {
        return Magnitude::new();
    }
    let mut product = Magnitude::from_elem(0, left.len().saturating_add(right.len()));
    for (left_index, left_limb) in left.iter().enumerate() {
        let mut carry: u64 = 0;
        for (right_index, right_limb) in right.iter().enumerate() {
            let position = left_index.saturating_add(right_index);
            let total = u64::from(*left_limb)
                .wrapping_mul(u64::from(*right_limb))
                .wrapping_add(u64::from(limb_at(&product, position)))
                .wrapping_add(carry);
            if let Some(slot) = product.get_mut(position) {
                *slot = low_limb(total);
            }
            carry = high_bits(total);
        }
        if let Some(slot) = product.get_mut(left_index.saturating_add(right.len())) {
            *slot = low_limb(carry);
        }
    }
    product
}

/// Divide a magnitude in place by [`DECIMAL_CHUNK`], returning the remainder.
///
/// Decimal rendering is the only division in the type, and its divisor is that
/// constant, so the zero-divisor case the record leaves open does not arise
/// here: `checked_div` states it and the fallback is unreachable.
fn divide_by_decimal_chunk(magnitude: &mut Magnitude) -> Limb {
    let divisor = u64::from(DECIMAL_CHUNK);
    let mut remainder: u64 = 0;
    for index in (0..magnitude.len()).rev() {
        let current = remainder.wrapping_shl(LIMB_BITS) | u64::from(limb_at(magnitude, index));
        if let Some(slot) = magnitude.get_mut(index) {
            *slot = low_limb(current.checked_div(divisor).unwrap_or(0));
        }
        remainder = current.checked_rem(divisor).unwrap_or(0);
    }
    while magnitude.last() == Some(&0) {
        let _ = magnitude.pop();
    }
    low_limb(remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 64-bit range the type used to be, plus the limb boundaries the
    /// representation is built out of.
    const EDGES: [i64; 11] = [
        0,
        1,
        -1,
        2_147_483_647,
        -2_147_483_648,
        4_294_967_295,
        4_294_967_296,
        -4_294_967_296,
        i64::MAX,
        i64::MIN,
        -9_007_199_254_740_993,
    ];

    fn int(value: i64) -> Int {
        Int::from(value)
    }

    #[test]
    fn the_sixty_four_bit_range_survives_the_round_trip() {
        for value in EDGES {
            assert_eq!(int(value).to_i64(), Some(value));
            assert_eq!(int(value).to_string(), value.to_string());
        }
    }

    #[test]
    fn arithmetic_agrees_with_a_wider_machine_integer() {
        // i128 is the oracle wherever both can answer, which is what makes the
        // hand-rolled limb arithmetic checkable rather than self-consistent.
        for left in EDGES {
            for right in EDGES {
                let (left_wide, right_wide) = (i128::from(left), i128::from(right));
                let widened = |value: Option<i128>| value.unwrap_or_default();
                if let Ok(sum) = i64::try_from(widened(left_wide.checked_add(right_wide))) {
                    assert_eq!(int(left).added(&int(right)), int(sum), "{left} + {right}");
                }
                if let Ok(difference) = i64::try_from(widened(left_wide.checked_sub(right_wide))) {
                    assert_eq!(
                        int(left).subtracted(&int(right)),
                        int(difference),
                        "{left} - {right}"
                    );
                }
                if let Ok(product) = i64::try_from(widened(left_wide.checked_mul(right_wide))) {
                    assert_eq!(
                        int(left).multiplied(&int(right)),
                        int(product),
                        "{left} * {right}"
                    );
                }
                assert_eq!(int(left).cmp(&int(right)), left.cmp(&right));
            }
        }
    }

    #[test]
    fn a_product_beyond_the_machine_range_is_exact() {
        // 2^64 has no i64, and the square of i64::MAX has no i128 either, so
        // this is the case a fixed-width type answers with a failure.
        let two = int(2);
        let mut power = int(1);
        for _ in 0..64u32 {
            power = power.multiplied(&two);
        }
        assert_eq!(power.to_string(), "18446744073709551616");
        assert_eq!(power.to_i64(), None);
        assert_eq!(
            power.subtracted(&int(1)).to_string(),
            "18446744073709551615"
        );

        let largest = int(i64::MAX);
        assert_eq!(
            largest.multiplied(&largest).to_string(),
            "85070591730234615847396907784232501249"
        );
    }

    #[test]
    fn zero_has_one_representation() {
        // Sign travels beside the magnitude, so a negative zero is constructible
        // in principle. Normalization is what keeps equality and hashing right.
        let negative_zero = int(-5).added(&int(5));
        assert_eq!(negative_zero, Int::zero());
        assert!(!negative_zero.is_negative());
        assert_eq!(int(0).negated(), Int::zero());
        assert_eq!(int(7).multiplied(&int(0)), Int::zero());
        assert_eq!(Int::zero().to_string(), "0");
    }

    #[test]
    fn the_magnitude_encoding_is_minimal_and_refuses_anything_else() {
        assert!(Int::zero().magnitude_bytes().is_empty());
        assert_eq!(int(1).magnitude_bytes(), vec![1]);
        assert_eq!(int(-256).magnitude_bytes(), vec![1, 0]);
        assert_eq!(
            int(i64::MIN).magnitude_bytes(),
            vec![0x80, 0, 0, 0, 0, 0, 0, 0]
        );

        for value in EDGES {
            let integer = int(value);
            assert_eq!(
                Int::from_sign_and_magnitude(integer.is_negative(), &integer.magnitude_bytes()),
                Some(integer)
            );
        }

        // A leading zero byte spells a value that already has an encoding, and
        // so does a negative zero. Both are refused rather than normalized, so
        // one value keeps one digest.
        assert_eq!(Int::from_sign_and_magnitude(false, &[0, 1]), None);
        assert_eq!(Int::from_sign_and_magnitude(false, &[0]), None);
        assert_eq!(Int::from_sign_and_magnitude(true, &[]), None);
        assert_eq!(Int::from_sign_and_magnitude(false, &[]), Some(Int::zero()));
    }
}
