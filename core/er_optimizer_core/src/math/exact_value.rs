use std::cmp::Ordering;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use num_bigint::BigInt;
use num_rational::{BigRational, Ratio};
#[cfg(test)]
use num_traits::Signed;
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, One, ToPrimitive, Zero};

type SmallRatio = Ratio<i128>;

/// An exact rational that keeps ordinary values inline and promotes on overflow.
///
/// Small values always have a positive, nonzero denominator and never use
/// `i128::MIN` as a numerator. The latter keeps the fixed-width ratio's edge
/// cases out of projection and normalization helpers.
#[derive(Clone)]
pub(crate) enum ExactRational {
    Small(SmallRatio),
    Big(Box<BigRational>),
}

impl ExactRational {
    #[cfg(test)]
    pub(crate) fn new(numer: BigInt, denom: BigInt) -> Self {
        Self::from_big(BigRational::new(numer, denom))
    }

    #[cfg(test)]
    pub(crate) fn new_raw(numer: BigInt, denom: BigInt) -> Self {
        assert!(!denom.is_zero(), "zero rational denominator");
        if denom.is_positive() {
            if numer.is_zero() {
                return Self::zero();
            }
            if let (Some(numer), Some(denom)) = (numer.to_i128(), denom.to_i128())
                && small_parts_are_safe(numer, denom)
            {
                return Self::Small(Ratio::new_raw(numer, denom));
            }
            return Self::Big(Box::new(BigRational::new_raw(numer, denom)));
        }

        Self::from_big(BigRational::new(numer, denom))
    }

    pub(crate) fn from_integer(value: BigInt) -> Self {
        if let Some(value) = value.to_i128()
            && value != i128::MIN
        {
            return Self::Small(Ratio::from_integer(value));
        }
        Self::Big(Box::new(BigRational::from_integer(value)))
    }

    pub(crate) fn from_scaled_i128(value: i128, exponent: i16) -> Self {
        if value == 0 {
            return Self::zero();
        }

        if value != i128::MIN {
            let trailing_zeros = value.wrapping_abs().trailing_zeros();
            let value = value >> trailing_zeros;
            let exponent = i32::from(exponent) + i32::try_from(trailing_zeros).expect("u32 to i32");

            if exponent >= 0 {
                let shift = u32::try_from(exponent).expect("nonnegative i32 exponent");
                if shift < 127
                    && value.unsigned_abs() <= (i128::MAX as u128 >> shift)
                    && let Some(scale) = 1_i128.checked_shl(shift)
                    && let Some(scaled) = value.checked_mul(scale)
                {
                    return Self::Small(Ratio::from_integer(scaled));
                }

                let numerator = BigInt::from(value) << usize::try_from(shift).expect("i32 shift");
                return Self::from_big(BigRational::from_integer(numerator));
            }

            let shift = usize::try_from(-exponent).expect("negative i32 exponent");
            if shift < 127
                && let Some(denominator) = 1_i128.checked_shl(shift as u32)
            {
                return Self::Small(Ratio::new_raw(value, denominator));
            }

            let denominator = BigInt::one() << shift;
            return Self::from_big(BigRational::new_raw(BigInt::from(value), denominator));
        }

        if exponent >= 0 {
            let shift = exponent as u32;
            let numerator = BigInt::from(value) << usize::try_from(shift).expect("i16 shift");
            return Self::from_big(BigRational::from_integer(numerator));
        }

        let shift = usize::from(exponent.unsigned_abs());
        let denominator = BigInt::one() << shift;
        Self::from_big(BigRational::new_raw(BigInt::from(value), denominator))
    }

    pub(crate) fn small(&self) -> Option<&Ratio<i128>> {
        match self {
            Self::Small(value) => Some(value),
            Self::Big(_) => None,
        }
    }

    pub(crate) fn to_big(&self) -> BigRational {
        match self {
            Self::Small(value) => big_from_small(value),
            Self::Big(value) => value.as_ref().clone(),
        }
    }

    pub(crate) fn floor(&self) -> Self {
        match self {
            Self::Small(value) => {
                let numer = *value.numer();
                let denom = *value.denom();
                let quotient = numer / denom;
                let remainder = numer % denom;
                let quotient = if numer < 0 && remainder != 0 {
                    quotient
                        .checked_sub(1)
                        .expect("small rational floor is representable")
                } else {
                    quotient
                };
                Self::Small(Ratio::from_integer(quotient))
            }
            Self::Big(value) => Self::from_big(value.floor()),
        }
    }

    fn from_big(value: BigRational) -> Self {
        let Some(numer) = value.numer().to_i128() else {
            return Self::Big(Box::new(value));
        };
        let Some(denom) = value.denom().to_i128() else {
            return Self::Big(Box::new(value));
        };
        if small_parts_are_safe(numer, denom) {
            Self::Small(Ratio::new_raw(numer, denom))
        } else {
            Self::Big(Box::new(value))
        }
    }
}

fn small_parts_are_safe(numer: i128, denom: i128) -> bool {
    numer != i128::MIN && denom > 0
}

fn big_from_small(value: &SmallRatio) -> BigRational {
    BigRational::new(BigInt::from(*value.numer()), BigInt::from(*value.denom()))
}

fn small_float_is_safe(value: &SmallRatio) -> bool {
    // num-rational shifts the numerator to denominator_bits + 55 bits,
    // or shifts the denominator when the quotient already needs 55 bits.
    fn bits(value: i128) -> u32 {
        128 - value.wrapping_abs().leading_zeros()
    }

    let numerator_bits = bits(*value.numer());
    let denominator_bits = bits(*value.denom());
    denominator_bits <= 72 || numerator_bits >= denominator_bits + 55
}

fn add_values(left: &ExactRational, right: &ExactRational) -> ExactRational {
    if left.is_zero() {
        return right.clone();
    }
    if right.is_zero() {
        return left.clone();
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && left.denom() == right.denom()
        && let Some(numer) = left.numer().checked_add(right.numer())
        && small_parts_are_safe(numer, *left.denom())
    {
        return ExactRational::Small(Ratio::new_raw(numer, *left.denom()));
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && let Some(value) = left.checked_add(right)
        && small_parts_are_safe(*value.numer(), *value.denom())
    {
        return ExactRational::Small(value);
    }
    ExactRational::from_big(left.to_big() + right.to_big())
}

fn sub_values(left: &ExactRational, right: &ExactRational) -> ExactRational {
    if right.is_zero() {
        return left.clone();
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && left.denom() == right.denom()
        && let Some(numer) = left.numer().checked_sub(right.numer())
        && small_parts_are_safe(numer, *left.denom())
    {
        return ExactRational::Small(Ratio::new_raw(numer, *left.denom()));
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && let Some(value) = left.checked_sub(right)
        && small_parts_are_safe(*value.numer(), *value.denom())
    {
        return ExactRational::Small(value);
    }
    ExactRational::from_big(left.to_big() - right.to_big())
}

fn mul_values(left: &ExactRational, right: &ExactRational) -> ExactRational {
    if left.is_zero() || right.is_zero() {
        return ExactRational::zero();
    }
    if left.is_one() {
        return right.clone();
    }
    if right.is_one() {
        return left.clone();
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && let Some(numer) = left.numer().checked_mul(right.numer())
        && let Some(denom) = left.denom().checked_mul(right.denom())
        && small_parts_are_safe(numer, denom)
    {
        return ExactRational::Small(Ratio::new_raw(numer, denom));
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && let Some(value) = left.checked_mul(right)
        && small_parts_are_safe(*value.numer(), *value.denom())
    {
        return ExactRational::Small(value);
    }
    ExactRational::from_big(left.to_big() * right.to_big())
}

fn div_values(left: &ExactRational, right: &ExactRational) -> ExactRational {
    if right.is_zero() {
        return ExactRational::from_big(left.to_big() / right.to_big());
    }
    if left.is_zero() {
        return ExactRational::zero();
    }
    if right.is_one() {
        return left.clone();
    }
    if let (ExactRational::Small(left), ExactRational::Small(right)) = (left, right)
        && let Some(value) = left.checked_div(right)
        && small_parts_are_safe(*value.numer(), *value.denom())
    {
        return ExactRational::Small(value);
    }
    ExactRational::from_big(left.to_big() / right.to_big())
}

macro_rules! impl_binary_op {
    ($trait:ident, $method:ident, $helper:ident) => {
        impl $trait for ExactRational {
            type Output = Self;

            fn $method(self, rhs: Self) -> Self::Output {
                $helper(&self, &rhs)
            }
        }

        impl $trait<&ExactRational> for ExactRational {
            type Output = Self;

            fn $method(self, rhs: &ExactRational) -> Self::Output {
                $helper(&self, rhs)
            }
        }

        impl $trait<ExactRational> for &ExactRational {
            type Output = ExactRational;

            fn $method(self, rhs: ExactRational) -> Self::Output {
                $helper(self, &rhs)
            }
        }

        impl $trait<&ExactRational> for &ExactRational {
            type Output = ExactRational;

            fn $method(self, rhs: &ExactRational) -> Self::Output {
                $helper(self, rhs)
            }
        }
    };
}

impl_binary_op!(Add, add, add_values);
impl_binary_op!(Sub, sub, sub_values);
impl_binary_op!(Mul, mul, mul_values);
impl_binary_op!(Div, div, div_values);

macro_rules! impl_assign_op {
    ($trait:ident, $method:ident, $helper:ident) => {
        impl $trait for ExactRational {
            fn $method(&mut self, rhs: Self) {
                *self = $helper(self, &rhs);
            }
        }

        impl $trait<&ExactRational> for ExactRational {
            fn $method(&mut self, rhs: &ExactRational) {
                *self = $helper(self, rhs);
            }
        }
    };
}

impl_assign_op!(AddAssign, add_assign, add_values);
impl_assign_op!(SubAssign, sub_assign, sub_values);
impl_assign_op!(MulAssign, mul_assign, mul_values);
impl_assign_op!(DivAssign, div_assign, div_values);

impl Zero for ExactRational {
    fn zero() -> Self {
        Self::Small(Ratio::zero())
    }

    fn is_zero(&self) -> bool {
        match self {
            Self::Small(value) => value.is_zero(),
            Self::Big(value) => value.is_zero(),
        }
    }
}

impl One for ExactRational {
    fn one() -> Self {
        Self::Small(Ratio::one())
    }

    fn is_one(&self) -> bool {
        match self {
            Self::Small(value) => value.is_one(),
            Self::Big(value) => value.is_one(),
        }
    }
}

impl Default for ExactRational {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<BigRational> for ExactRational {
    fn from(value: BigRational) -> Self {
        Self::from_big(value)
    }
}

impl PartialEq for ExactRational {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ExactRational {}

impl PartialOrd for ExactRational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExactRational {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Small(left), Self::Small(right)) => left.cmp(right),
            (Self::Big(left), Self::Big(right)) => left.cmp(right),
            (Self::Small(left), Self::Big(right)) => big_from_small(left).cmp(right.as_ref()),
            (Self::Big(left), Self::Small(right)) => left.as_ref().cmp(&big_from_small(right)),
        }
    }
}

impl ToPrimitive for ExactRational {
    fn to_i64(&self) -> Option<i64> {
        match self {
            Self::Small(value) => value.to_i64(),
            Self::Big(value) => value.to_i64(),
        }
    }

    fn to_i128(&self) -> Option<i128> {
        match self {
            Self::Small(value) => value.to_i128(),
            Self::Big(value) => value.to_i128(),
        }
    }

    fn to_u64(&self) -> Option<u64> {
        match self {
            Self::Small(value) => value.to_u64(),
            Self::Big(value) => value.to_u64(),
        }
    }

    fn to_u128(&self) -> Option<u128> {
        match self {
            Self::Small(value) => value.to_u128(),
            Self::Big(value) => value.to_u128(),
        }
    }

    fn to_f64(&self) -> Option<f64> {
        match self {
            // Scaling a rounded i128 by a power of two is exact within f64's
            // normal range, which contains every Small ratio.
            Self::Small(value) if (*value.denom() as u128).is_power_of_two() => {
                Some(*value.numer() as f64 / *value.denom() as f64)
            }
            Self::Small(value) if small_float_is_safe(value) => value.to_f64(),
            Self::Small(value) => big_from_small(value).to_f64(),
            Self::Big(value) => value.to_f64(),
        }
    }
}

impl Sum for ExactRational {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), |total, value| total + value)
    }
}

impl<'a> Sum<&'a ExactRational> for ExactRational {
    fn sum<I: Iterator<Item = &'a ExactRational>>(iter: I) -> Self {
        iter.fold(Self::zero(), |total, value| total + value)
    }
}

impl fmt::Debug for ExactRational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_big().reduced(), formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_arithmetic_and_projection_match_big_rationals() {
        let mut seed = 123456789u128;
        for index in 0..512 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let numerator = (seed >> 1) as i128 * if index % 2 == 0 { 1 } else { -1 };
            let denominator = 1i128 << (index % 127);
            let left = ExactRational::new_raw(numerator.into(), denominator.into());
            let right = ExactRational::new_raw((index + 1).into(), (index % 31 + 1).into());
            let a = left.to_big();
            let b = right.to_big();
            assert_eq!((&left + &right).to_big(), &a + &b);
            assert_eq!((&left - &right).to_big(), &a - &b);
            assert_eq!((&left * &right).to_big(), &a * &b);
            assert_eq!((&left / &right).to_big(), &a / &b);
            for value in [&left, &(&left / &right)] {
                assert_eq!(value.to_f64(), value.to_big().to_f64());
                assert_eq!(value.to_f32(), value.to_big().to_f32());
            }
        }
    }

    fn big(numer: i128, denom: i128) -> BigRational {
        BigRational::new(BigInt::from(numer), BigInt::from(denom))
    }

    #[test]
    fn small_and_big_projection_agree_at_and_around_a_float_midpoint() {
        let denominator: BigInt = BigInt::one() << 50;
        let midpoint: BigInt = (BigInt::one() << 50) + (BigInt::one() << 26);
        let below = ExactRational::new_raw(&midpoint - BigInt::one(), denominator.clone());
        let tie = ExactRational::new_raw(midpoint.clone(), denominator.clone());
        let above = ExactRational::new_raw(midpoint + BigInt::one(), denominator.clone());

        for value in [below, tie, above] {
            let expected = BigRational::new(
                value.to_big().numer().clone(),
                value.to_big().denom().clone(),
            )
            .to_f32();
            assert_eq!(value.to_f32(), expected);
        }
    }

    #[test]
    fn overflowing_small_arithmetic_promotes_exactly() {
        let maximum = ExactRational::from_integer(BigInt::from(i128::MAX));
        let result = &maximum + ExactRational::one();
        assert!(result.small().is_none());
        assert_eq!(
            result.to_big(),
            BigRational::from_integer(BigInt::from(i128::MAX) + BigInt::one())
        );

        let minimum = ExactRational::from_integer(BigInt::from(i128::MIN));
        assert!(minimum.small().is_none());
        assert_eq!(
            minimum.to_big(),
            BigRational::from_integer(BigInt::from(i128::MIN))
        );
    }

    #[test]
    fn overflowing_scaled_values_promote_exactly() {
        let cases = [
            (2_i128, 126_i16, BigInt::one() << 127),
            (i128::MAX, 1_i16, BigInt::from(i128::MAX) << 1),
        ];

        for (value, exponent, expected) in cases {
            let actual = ExactRational::from_scaled_i128(value, exponent);
            assert!(actual.small().is_none());
            assert_eq!(actual.to_big(), BigRational::from_integer(expected));
        }
    }

    #[test]
    fn floor_avoids_fixed_width_negative_intermediate_overflow() {
        let value = ExactRational::new_raw(BigInt::from(i128::MIN + 1), BigInt::from(i128::MAX));
        assert_eq!(value.floor().to_big(), big(-1, 1));
    }

    #[test]
    fn large_small_operands_use_big_projection_without_changing_value() {
        let value = ExactRational::new_raw(BigInt::from(i128::MAX - 1), BigInt::from(i128::MAX));
        let expected =
            BigRational::new(BigInt::from(i128::MAX - 1), BigInt::from(i128::MAX)).to_f32();
        assert_eq!(value.to_f32(), expected);
    }
}
