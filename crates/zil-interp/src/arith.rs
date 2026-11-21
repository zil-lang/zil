//! Scalar semantics: checked and wrapping integer arithmetic, IEEE float
//! operations and checked numeric conversions.
//!
//! Sources (all in `_zil_design.md` §2.4 “不可变的基本行为”):
//! - integer widths and ranges, `usize`/`isize` follow the target pointer
//!   width (currently 64 bits on the supported native x64 target), never the
//!   host's;
//! - `+ - *` and negation are range-checked; explicit wrapping is modulo
//!   `2^N`; unsigned comparison, division, remainder and right shift use the
//!   unsigned meaning; division by zero fails;
//! - the shift amount has the left operand's type and must satisfy
//!   `0 <= amount < N`; a left shift keeps the low `N` bits;
//! - `f32`/`f64` are IEEE binary32/binary64 with nearest-ties-to-even, NaN
//!   comparisons are unordered, `+0 == -0`, float remainder is the `fmod` of
//!   the truncated quotient, and CT canonicalizes arithmetic NaN to quiet NaN;
//! - conversions: see [`convert`].

use std::cmp::Ordering;
use zil_base::numeric::{FloatFormat, FloatOp, FloatValue, IntegerValue};
use zil_ir::body::{BinOp, ConstValue, FloatTy, IntTy, TrapKind, UnOp};

/// Pointer width of the CT target. `_zil_design.md` §2.4: “当前已支持原生x64目标的
/// usize和isize为64位，CT、Wire、布局和缓存按目标配置，不取宿主usize/isize”.
pub const TARGET_POINTER_BITS: u32 = 64;

/// Failure of a scalar operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArithError {
    /// The checked contract failed; the program traps.
    Trap(TrapKind),
    /// Operand types the verified IR cannot produce.
    IllTyped(&'static str),
}

/// Numeric target of a conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConvTarget {
    Int(IntTy),
    Float(FloatTy),
}

pub(crate) fn width(ty: IntTy) -> u32 {
    ty.bits(TARGET_POINTER_BITS)
}

pub(crate) fn mask(ty: IntTy) -> u128 {
    (1u128 << width(ty)) - 1
}

fn min(ty: IntTy) -> i128 {
    if ty.signed() {
        -(1i128 << (width(ty) - 1))
    } else {
        0
    }
}

fn max(ty: IntTy) -> i128 {
    if ty.signed() {
        (1i128 << (width(ty) - 1)) - 1
    } else {
        (1i128 << width(ty)) - 1
    }
}

/// Mathematical value of a two's complement bit pattern.
pub(crate) fn math(ty: IntTy, bits: u128) -> i128 {
    let w = width(ty);
    let b = bits & mask(ty);
    if ty.signed() && (b >> (w - 1)) & 1 == 1 {
        b as i128 - (1i128 << w)
    } else {
        b as i128
    }
}

/// Bit pattern of `value` if it lies in the range of `ty`.
pub(crate) fn from_math(ty: IntTy, value: i128) -> Option<u128> {
    (min(ty)..=max(ty))
        .contains(&value)
        .then_some(value as u128 & mask(ty))
}

fn wrap(ty: IntTy, value: i128) -> ConstValue {
    ConstValue::Int {
        ty,
        bits: value as u128 & mask(ty),
    }
}

fn checked(ty: IntTy, value: Option<i128>) -> Result<ConstValue, ArithError> {
    value
        .and_then(|v| from_math(ty, v))
        .map(|bits| ConstValue::Int { ty, bits })
        .ok_or(ArithError::Trap(TrapKind::Overflow))
}

fn compare(op: BinOp, ordering: Option<Ordering>) -> Result<ConstValue, ArithError> {
    let result = match op {
        BinOp::Eq => ordering == Some(Ordering::Equal),
        BinOp::Ne => ordering != Some(Ordering::Equal),
        BinOp::Lt => ordering == Some(Ordering::Less),
        BinOp::Le => matches!(ordering, Some(Ordering::Less | Ordering::Equal)),
        BinOp::Gt => ordering == Some(Ordering::Greater),
        BinOp::Ge => matches!(ordering, Some(Ordering::Greater | Ordering::Equal)),
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Rem
        | BinOp::WrappingAdd
        | BinOp::WrappingSub
        | BinOp::WrappingMul
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor
        | BinOp::Shl
        | BinOp::Shr => return Err(ArithError::IllTyped("non-comparison in compare")),
    };
    Ok(ConstValue::Bool(result))
}

fn shift_amount(ty: IntTy, amount: i128) -> Result<u32, ArithError> {
    if (0..i128::from(width(ty))).contains(&amount) {
        Ok(amount as u32)
    } else {
        Err(ArithError::Trap(TrapKind::Overflow))
    }
}

fn int_binary(op: BinOp, ty: IntTy, x: u128, y: u128) -> Result<ConstValue, ArithError> {
    let (a, b) = (math(ty, x), math(ty, y));
    let (x, y) = (x & mask(ty), y & mask(ty));
    let bits = |bits: u128| ConstValue::Int {
        ty,
        bits: bits & mask(ty),
    };
    match op {
        BinOp::Add => checked(ty, a.checked_add(b)),
        BinOp::Sub => checked(ty, a.checked_sub(b)),
        BinOp::Mul => checked(ty, a.checked_mul(b)),
        BinOp::Div => {
            if b == 0 {
                return Err(ArithError::Trap(TrapKind::DivideByZero));
            }
            // Truncating division; `MIN / -1` leaves the range and traps.
            checked(ty, a.checked_div(b))
        }
        BinOp::Rem => {
            if b == 0 {
                return Err(ArithError::Trap(TrapKind::DivideByZero));
            }
            // Remainder of the truncated quotient. Its mathematical value
            // (`MIN % -1 == 0`) is always in range; only a zero divisor
            // fails (§2.4 “除零失败”).
            checked(ty, a.checked_rem(b))
        }
        BinOp::WrappingAdd => Ok(wrap(ty, a.wrapping_add(b))),
        BinOp::WrappingSub => Ok(wrap(ty, a.wrapping_sub(b))),
        BinOp::WrappingMul => Ok(wrap(ty, a.wrapping_mul(b))),
        BinOp::BitAnd => Ok(bits(x & y)),
        BinOp::BitOr => Ok(bits(x | y)),
        BinOp::BitXor => Ok(bits(x ^ y)),
        BinOp::Shl => Ok(bits(x << shift_amount(ty, b)?)),
        BinOp::Shr => {
            let n = shift_amount(ty, b)?;
            if ty.signed() {
                Ok(wrap(ty, a >> n))
            } else {
                Ok(bits(x >> n))
            }
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            compare(op, Some(a.cmp(&b)))
        }
    }
}

pub(crate) fn float_value(ty: FloatTy, bits: u64) -> FloatValue {
    match ty {
        FloatTy::F32 => FloatValue::F32(bits as u32),
        FloatTy::F64 => FloatValue::F64(bits),
    }
}

fn float_const(value: FloatValue) -> ConstValue {
    match value {
        FloatValue::F32(bits) => ConstValue::Float {
            ty: FloatTy::F32,
            bits: u64::from(bits),
        },
        FloatValue::F64(bits) => ConstValue::Float {
            ty: FloatTy::F64,
            bits,
        },
    }
}

fn format(ty: FloatTy) -> FloatFormat {
    match ty {
        FloatTy::F32 => FloatFormat::F32,
        FloatTy::F64 => FloatFormat::F64,
    }
}

/// `fmod` of the truncated quotient. The exact remainder is always
/// representable, so the host operation is exact and needs no rounding mode;
/// NaN results are canonicalized like every CT arithmetic NaN.
fn float_rem(ty: FloatTy, x: u64, y: u64) -> ConstValue {
    match ty {
        FloatTy::F32 => {
            let r = f32::from_bits(x as u32) % f32::from_bits(y as u32);
            let bits = if r.is_nan() { 0x7fc0_0000 } else { r.to_bits() };
            ConstValue::Float {
                ty,
                bits: u64::from(bits),
            }
        }
        FloatTy::F64 => {
            let r = f64::from_bits(x) % f64::from_bits(y);
            let bits = if r.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                r.to_bits()
            };
            ConstValue::Float { ty, bits }
        }
    }
}

fn float_binary(op: BinOp, ty: FloatTy, x: u64, y: u64) -> Result<ConstValue, ArithError> {
    let (a, b) = (float_value(ty, x), float_value(ty, y));
    let arith = |op: FloatOp| {
        a.binary(b, op)
            .map(float_const)
            .map_err(|_| ArithError::IllTyped("float width mismatch"))
    };
    match op {
        BinOp::Add => arith(FloatOp::Add),
        BinOp::Sub => arith(FloatOp::Subtract),
        BinOp::Mul => arith(FloatOp::Multiply),
        BinOp::Div => arith(FloatOp::Divide),
        BinOp::Rem => Ok(float_rem(ty, x, y)),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ordering = a
                .partial_compare(b)
                .map_err(|_| ArithError::IllTyped("float width mismatch"))?;
            compare(op, ordering)
        }
        BinOp::WrappingAdd
        | BinOp::WrappingSub
        | BinOp::WrappingMul
        | BinOp::BitAnd
        | BinOp::BitOr
        | BinOp::BitXor
        | BinOp::Shl
        | BinOp::Shr => Err(ArithError::IllTyped("integer-only operator on floats")),
    }
}

fn bool_binary(op: BinOp, a: bool, b: bool) -> Result<ConstValue, ArithError> {
    Ok(ConstValue::Bool(match op {
        BinOp::BitAnd => a & b,
        BinOp::BitOr => a | b,
        BinOp::BitXor => a ^ b,
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::Div
        | BinOp::Rem
        | BinOp::WrappingAdd
        | BinOp::WrappingSub
        | BinOp::WrappingMul
        | BinOp::Shl
        | BinOp::Shr
        | BinOp::Lt
        | BinOp::Le
        | BinOp::Gt
        | BinOp::Ge => return Err(ArithError::IllTyped("operator on bool")),
    }))
}

pub(crate) fn binary(
    op: BinOp,
    lhs: ConstValue,
    rhs: ConstValue,
) -> Result<ConstValue, ArithError> {
    let mismatch = Err(ArithError::IllTyped("binary operand types differ"));
    match lhs {
        ConstValue::Int { ty, bits: x } => match rhs {
            ConstValue::Int { ty: t, bits: y } if t == ty => int_binary(op, ty, x, y),
            ConstValue::Int { .. }
            | ConstValue::Unit
            | ConstValue::Bool(_)
            | ConstValue::Float { .. } => mismatch,
        },
        ConstValue::Float { ty, bits: x } => match rhs {
            ConstValue::Float { ty: t, bits: y } if t == ty => float_binary(op, ty, x, y),
            ConstValue::Float { .. }
            | ConstValue::Unit
            | ConstValue::Bool(_)
            | ConstValue::Int { .. } => mismatch,
        },
        ConstValue::Bool(a) => match rhs {
            ConstValue::Bool(b) => bool_binary(op, a, b),
            ConstValue::Unit | ConstValue::Int { .. } | ConstValue::Float { .. } => mismatch,
        },
        ConstValue::Unit => match rhs {
            ConstValue::Unit => match op {
                BinOp::Eq => Ok(ConstValue::Bool(true)),
                BinOp::Ne => Ok(ConstValue::Bool(false)),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Rem
                | BinOp::WrappingAdd
                | BinOp::WrappingSub
                | BinOp::WrappingMul
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::Shl
                | BinOp::Shr
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge => Err(ArithError::IllTyped("operator on unit")),
            },
            ConstValue::Bool(_) | ConstValue::Int { .. } | ConstValue::Float { .. } => mismatch,
        },
    }
}

pub(crate) fn unary(op: UnOp, operand: ConstValue) -> Result<ConstValue, ArithError> {
    match (op, operand) {
        (UnOp::Not, ConstValue::Bool(b)) => Ok(ConstValue::Bool(!b)),
        (UnOp::Not, ConstValue::Int { ty, bits }) => Ok(ConstValue::Int {
            ty,
            bits: !bits & mask(ty),
        }),
        (UnOp::Neg, ConstValue::Int { ty, bits }) => checked(ty, math(ty, bits).checked_neg()),
        (UnOp::Neg, ConstValue::Float { ty, bits }) => {
            Ok(float_const(float_value(ty, bits).negate()))
        }
        (UnOp::WrappingNeg, ConstValue::Int { ty, bits }) => {
            Ok(wrap(ty, math(ty, bits).wrapping_neg()))
        }
        (UnOp::Not, ConstValue::Unit | ConstValue::Float { .. })
        | (UnOp::Neg, ConstValue::Unit | ConstValue::Bool(_))
        | (UnOp::WrappingNeg, ConstValue::Unit | ConstValue::Bool(_) | ConstValue::Float { .. }) => {
            Err(ArithError::IllTyped("unary operand type"))
        }
    }
}

fn is_infinite(value: FloatValue) -> bool {
    match value {
        FloatValue::F32(bits) => bits & 0x7fff_ffff == 0x7f80_0000,
        FloatValue::F64(bits) => bits & 0x7fff_ffff_ffff_ffff == 0x7ff0_0000_0000_0000,
    }
}

/// Checked numeric conversion preserving the mathematical value.
///
/// Rule (`_zil_design.md` §2.4, the paragraph beginning
/// “`f32(numeric)`/`f64(numeric)`明确允许向目标精度舍入一次”, and the integer
/// paragraph “十种整数之间只允许保留数学值的显式检查转换；负值转无符号或超出目标范围失败”):
/// - integer → integer: the mathematical value must lie in the target range;
/// - integer/float → float: rounds once to nearest-ties-to-even; a finite
///   source that rounds to infinity fails; NaN/infinity keep their category;
/// - float → integer: the source must be finite, have an *integral*
///   mathematical value (no truncation: `1.5` fails) and lie in the target
///   range; `-0.0` converts to `0`.
///
/// The frozen reference `crates/zil-ct/src/lib.rs` (`convert_integer_target`,
/// `convert_number`) implements the same rule over `zil_base::numeric`, which
/// is reused here so CT and literal conversion share one IEEE model.
pub(crate) fn convert(value: ConstValue, to: ConvTarget) -> Result<ConstValue, ArithError> {
    let fail = Err(ArithError::Trap(TrapKind::Conversion));
    match (value, to) {
        (ConstValue::Int { ty, bits }, ConvTarget::Int(target)) => {
            match from_math(target, math(ty, bits)) {
                Some(bits) => Ok(ConstValue::Int { ty: target, bits }),
                None => fail,
            }
        }
        (ConstValue::Int { ty, bits }, ConvTarget::Float(target)) => {
            let v = math(ty, bits);
            let integer = if ty.signed() {
                IntegerValue::Signed(v)
            } else {
                IntegerValue::Unsigned(v as u128)
            };
            let result = integer.to_float(format(target));
            if is_infinite(result) {
                return fail;
            }
            Ok(float_const(result))
        }
        (ConstValue::Float { ty, bits }, ConvTarget::Float(target)) => {
            match float_value(ty, bits).convert(format(target)) {
                Ok(v) => Ok(float_const(v)),
                Err(_) => fail,
            }
        }
        (ConstValue::Float { ty, bits }, ConvTarget::Int(target)) => {
            let integer =
                match float_value(ty, bits).to_integer(target.signed(), width(target) as usize) {
                    Ok(v) => v,
                    Err(_) => return fail,
                };
            let v = match integer {
                IntegerValue::Signed(v) => v,
                IntegerValue::Unsigned(v) => match i128::try_from(v) {
                    Ok(v) => v,
                    Err(_) => return fail,
                },
            };
            match from_math(target, v) {
                Some(bits) => Ok(ConstValue::Int { ty: target, bits }),
                None => fail,
            }
        }
        (ConstValue::Unit | ConstValue::Bool(_), ConvTarget::Int(_) | ConvTarget::Float(_)) => {
            Err(ArithError::IllTyped("conversion of a non-numeric value"))
        }
    }
}
