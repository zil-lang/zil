//! Deterministic IEEE arithmetic for literal conversion and CT execution.
//! These routines compute values; they establish no source/type/loan authority.
use rustc_apfloat::{
    Float, FloatConvert, Round, Status,
    ieee::{Double, Single},
};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatFormat {
    F32,
    F64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatValue {
    F32(u32),
    F64(u64),
}
/// Finite decimal spelling, or an IEEE exceptional value for source reification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloatSource {
    Finite(String),
    Infinity { negative: bool },
    Nan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerValue {
    Signed(i128),
    Unsigned(u128),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericError {
    Spelling,
    Range,
    Type,
}
const ROUND: Round = Round::NearestTiesToEven;

/// Decimal conversion can allocate temporary multiprecision digits. Callers
/// debit this conservative quadratic work charge before normalizing/allocating
/// the spelling or entering APFloat. Saturation fails an ordinary finite budget.
pub fn decimal_work(bytes: usize) -> usize {
    bytes.saturating_mul(bytes).saturating_add(64)
}

fn canonical<F: Float>(value: F) -> F {
    if value.is_nan() { F::NAN } else { value }
}
fn binary<F: Float>(left: F, right: F, op: FloatOp) -> F {
    canonical(match op {
        FloatOp::Add => left.add_r(right, ROUND).value,
        FloatOp::Subtract => left.sub_r(right, ROUND).value,
        FloatOp::Multiply => left.mul_r(right, ROUND).value,
        FloatOp::Divide => left.div_r(right, ROUND).value,
    })
}
fn integer<F: Float>(value: F, signed: bool, width: usize) -> Result<IntegerValue, NumericError> {
    if !value.is_finite() || !(1..=64).contains(&width) {
        return Err(NumericError::Range);
    }
    // IEEE -0 has mathematical integer value zero.
    if value.is_zero() {
        return Ok(if signed {
            IntegerValue::Signed(0)
        } else {
            IntegerValue::Unsigned(0)
        });
    }
    let mut exact = false;
    let (status, result) = if signed {
        let result = value.to_i128_r(width, Round::TowardZero, &mut exact);
        (result.status, IntegerValue::Signed(result.value))
    } else {
        let result = value.to_u128_r(width, Round::TowardZero, &mut exact);
        (result.status, IntegerValue::Unsigned(result.value))
    };
    if !exact || status.intersects(Status::INVALID_OP | Status::INEXACT) {
        Err(NumericError::Range)
    } else {
        Ok(result)
    }
}
impl FloatValue {
    /// `text` is a validated source spelling without a suffix. Work must be
    /// reserved using decimal_work before this method, including underscores.
    pub fn from_decimal(text: &str, format: FloatFormat) -> Result<Self, NumericError> {
        let text = text.replace('_', "");
        match format {
            FloatFormat::F32 => {
                let result =
                    Single::from_str_r(&text, ROUND).map_err(|_| NumericError::Spelling)?;
                if !result.value.is_finite() {
                    return Err(NumericError::Range);
                }
                Ok(Self::F32(result.value.to_bits() as u32))
            }
            FloatFormat::F64 => {
                let result =
                    Double::from_str_r(&text, ROUND).map_err(|_| NumericError::Spelling)?;
                if !result.value.is_finite() {
                    return Err(NumericError::Range);
                }
                Ok(Self::F64(result.value.to_bits() as u64))
            }
        }
    }
    /// Fixed IEEE formats bound software decimal formatting work independently
    /// of source input. Callers reserve 8192 work units and 64 output bytes.
    pub fn source(self) -> FloatSource {
        fn source<F: Float + std::fmt::Display>(value: F) -> FloatSource {
            if value.is_nan() { FloatSource::Nan }
            else if value.is_infinite() { FloatSource::Infinity { negative: value.is_negative() } }
            else { FloatSource::Finite(value.to_string()) }
        }
        match self {
            Self::F32(bits) => source(Single::from_bits(bits.into())),
            Self::F64(bits) => source(Double::from_bits(bits.into())),
        }
    }
    pub fn format(self) -> FloatFormat {
        match self {
            Self::F32(_) => FloatFormat::F32,
            Self::F64(_) => FloatFormat::F64,
        }
    }
    pub fn negate(self) -> Self {
        match self {
            Self::F32(bits) => {
                Self::F32(canonical(-Single::from_bits(bits.into())).to_bits() as u32)
            }
            Self::F64(bits) => {
                Self::F64(canonical(-Double::from_bits(bits.into())).to_bits() as u64)
            }
        }
    }
    pub fn binary(self, right: Self, op: FloatOp) -> Result<Self, NumericError> {
        match (self, right) {
            (Self::F32(a), Self::F32(b)) => Ok(Self::F32(
                binary(Single::from_bits(a.into()), Single::from_bits(b.into()), op).to_bits()
                    as u32,
            )),
            (Self::F64(a), Self::F64(b)) => Ok(Self::F64(
                binary(Double::from_bits(a.into()), Double::from_bits(b.into()), op).to_bits()
                    as u64,
            )),
            _ => Err(NumericError::Type),
        }
    }
    /// None means IEEE unordered (NaN); mismatched widths are an error.
    pub fn partial_compare(self, right: Self) -> Result<Option<Ordering>, NumericError> {
        match (self, right) {
            (Self::F32(a), Self::F32(b)) => {
                Ok(Single::from_bits(a.into()).partial_cmp(&Single::from_bits(b.into())))
            }
            (Self::F64(a), Self::F64(b)) => {
                Ok(Double::from_bits(a.into()).partial_cmp(&Double::from_bits(b.into())))
            }
            _ => Err(NumericError::Type),
        }
    }
    pub fn convert(self, format: FloatFormat) -> Result<Self, NumericError> {
        match (self, format) {
            (Self::F32(bits), FloatFormat::F64) => {
                let input = Single::from_bits(bits.into());
                let output: Double = input.convert_r(ROUND, &mut false).value;
                Ok(Self::F64(canonical(output).to_bits() as u64))
            }
            (Self::F64(bits), FloatFormat::F32) => {
                let input = Double::from_bits(bits.into());
                let output: Single = input.convert_r(ROUND, &mut false).value;
                if input.is_finite() && output.is_infinite() {
                    return Err(NumericError::Range);
                }
                Ok(Self::F32(canonical(output).to_bits() as u32))
            }
            (Self::F32(bits), FloatFormat::F32) => Ok(Self::F32(
                canonical(Single::from_bits(bits.into())).to_bits() as u32,
            )),
            (Self::F64(bits), FloatFormat::F64) => Ok(Self::F64(
                canonical(Double::from_bits(bits.into())).to_bits() as u64,
            )),
        }
    }
    pub fn to_integer(self, signed: bool, width: usize) -> Result<IntegerValue, NumericError> {
        match self {
            Self::F32(bits) => integer(Single::from_bits(bits.into()), signed, width),
            Self::F64(bits) => integer(Double::from_bits(bits.into()), signed, width),
        }
    }
}
impl IntegerValue {
    pub fn to_float(self, format: FloatFormat) -> FloatValue {
        match format {
            FloatFormat::F32 => FloatValue::F32(match self {
                Self::Signed(value) => Single::from_i128_r(value, ROUND).value.to_bits() as u32,
                Self::Unsigned(value) => Single::from_u128_r(value, ROUND).value.to_bits() as u32,
            }),
            FloatFormat::F64 => FloatValue::F64(match self {
                Self::Signed(value) => Double::from_i128_r(value, ROUND).value.to_bits() as u64,
                Self::Unsigned(value) => Double::from_u128_r(value, ROUND).value.to_bits() as u64,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_ties_subnormals_and_checked_overflow_are_software_ieee() {
        assert_eq!(
            FloatValue::from_decimal("16777217.0", FloatFormat::F32),
            Ok(FloatValue::F32(0x4b800000))
        );
        assert_eq!(
            FloatValue::from_decimal("-0.0", FloatFormat::F32),
            Ok(FloatValue::F32(0x80000000))
        );
        assert_eq!(
            FloatValue::from_decimal(
                "1.401298464324817070923729583289916131280e-45",
                FloatFormat::F32
            ),
            Ok(FloatValue::F32(1))
        );
        assert_eq!(
            FloatValue::from_decimal("1e-60", FloatFormat::F32),
            Ok(FloatValue::F32(0))
        );
        assert_eq!(
            FloatValue::from_decimal("1e40", FloatFormat::F32),
            Err(NumericError::Range)
        );
        assert_eq!(
            FloatValue::from_decimal("1e400", FloatFormat::F64),
            Err(NumericError::Range)
        );
        assert_eq!(
            FloatValue::F64(0x7fefffffffffffff).convert(FloatFormat::F32),
            Err(NumericError::Range)
        );
    }
    #[test]
    fn arithmetic_nan_zero_and_integer_boundary_are_exact() {
        let zero = FloatValue::F32(0);
        assert_eq!(
            zero.binary(zero, FloatOp::Divide),
            Ok(FloatValue::F32(0x7fc00000))
        );
        assert_eq!(
            FloatValue::F32(0x3f800000).binary(zero, FloatOp::Divide),
            Ok(FloatValue::F32(0x7f800000))
        );
        assert_eq!(
            FloatValue::F64(0x8000000000000000).to_integer(true, 64),
            Ok(IntegerValue::Signed(0))
        );
        assert_eq!(
            FloatValue::F64(0x43e0000000000000).to_integer(true, 64),
            Err(NumericError::Range)
        );
        assert_eq!(
            FloatValue::F64(0x43f0000000000000).to_integer(false, 64),
            Err(NumericError::Range)
        );
        assert_eq!(
            FloatValue::F64(0xbff8000000000000).to_integer(true, 32),
            Err(NumericError::Range)
        );
        assert_eq!(
            FloatValue::F64(0x7ff8000000000000).to_integer(false, 64),
            Err(NumericError::Range)
        );
        assert_eq!(FloatValue::F32(0x7fc00000).partial_compare(zero), Ok(None));
    }
}
