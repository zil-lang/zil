//! Integer and float operators. Expected values follow `_zil_design.md`
//! §2.4: checked `+ - *` and negation, explicit wrapping modulo 2^N,
//! unsigned meaning for unsigned division/right shift/comparison, shift
//! amount `0 <= n < N`, IEEE floats with canonical quiet NaN.

mod common;

use common::*;
use zil_interp::Value;
use zil_ir::body::{BinOp, FloatTy, IntTy, TrapKind, TyKind, UnOp};

fn int_op(ty: IntTy, op: BinOp, a: i128, b: i128) -> Result<Value, zil_interp::EvalError> {
    let (m, f) = binary_fn(TyKind::Int(ty), op);
    eval(&m, f, vec![iv(ty, a), iv(ty, b)])
}

const I64_MAX: i128 = 9_223_372_036_854_775_807;
const I64_MIN: i128 = -9_223_372_036_854_775_808;
const U64_MAX: i128 = 18_446_744_073_709_551_615;

#[test]
fn i8_checked_arithmetic_edges() {
    let t = IntTy::I8;
    assert_eq!(int_op(t, BinOp::Add, 100, 27), Ok(iv(t, 127)));
    assert!(is_trap(&int_op(t, BinOp::Add, 127, 1), TrapKind::Overflow));
    assert!(is_trap(
        &int_op(t, BinOp::Add, -128, -1),
        TrapKind::Overflow
    ));
    assert!(is_trap(&int_op(t, BinOp::Sub, -128, 1), TrapKind::Overflow));
    assert_eq!(int_op(t, BinOp::Sub, -127, 1), Ok(iv(t, -128)));
    assert!(is_trap(&int_op(t, BinOp::Mul, 16, 8), TrapKind::Overflow));
    assert_eq!(int_op(t, BinOp::Mul, -16, 8), Ok(iv(t, -128)));
    assert_eq!(int_op(t, BinOp::Div, 7, -2), Ok(iv(t, -3)));
    assert_eq!(int_op(t, BinOp::Rem, -7, 2), Ok(iv(t, -1)));
    assert!(is_trap(
        &int_op(t, BinOp::Div, -128, -1),
        TrapKind::Overflow
    ));
    assert_eq!(int_op(t, BinOp::Rem, -128, -1), Ok(iv(t, 0)));
    assert!(is_trap(
        &int_op(t, BinOp::Div, 5, 0),
        TrapKind::DivideByZero
    ));
    assert!(is_trap(
        &int_op(t, BinOp::Rem, 5, 0),
        TrapKind::DivideByZero
    ));
}

#[test]
fn i8_wrapping_and_bitwise() {
    let t = IntTy::I8;
    assert_eq!(int_op(t, BinOp::WrappingAdd, 127, 1), Ok(iv(t, -128)));
    assert_eq!(int_op(t, BinOp::WrappingSub, -128, 1), Ok(iv(t, 127)));
    assert_eq!(int_op(t, BinOp::WrappingMul, 16, 8), Ok(iv(t, -128)));
    assert_eq!(int_op(t, BinOp::WrappingMul, 100, 3), Ok(iv(t, 44)));
    assert_eq!(int_op(t, BinOp::BitAnd, -1, 0x35), Ok(iv(t, 0x35)));
    assert_eq!(int_op(t, BinOp::BitOr, 0x40, -128), Ok(iv(t, -64)));
    assert_eq!(int_op(t, BinOp::BitXor, -1, 0x0f), Ok(iv(t, -16)));
    assert_eq!(int_op(t, BinOp::Lt, -128, 127), Ok(Value::Bool(true)));
    assert_eq!(int_op(t, BinOp::Ge, -1, 0), Ok(Value::Bool(false)));
}

#[test]
fn i8_shifts() {
    let t = IntTy::I8;
    // The left shift keeps the low 8 bits.
    assert_eq!(int_op(t, BinOp::Shl, 1, 7), Ok(iv(t, -128)));
    assert_eq!(int_op(t, BinOp::Shl, 3, 7), Ok(iv(t, -128)));
    // Signed right shift keeps the sign.
    assert_eq!(int_op(t, BinOp::Shr, -128, 7), Ok(iv(t, -1)));
    assert_eq!(int_op(t, BinOp::Shr, -7, 1), Ok(iv(t, -4)));
    assert!(is_trap(&int_op(t, BinOp::Shl, 1, 8), TrapKind::Overflow));
    assert!(is_trap(&int_op(t, BinOp::Shr, 1, -1), TrapKind::Overflow));
}

#[test]
fn u32_unsigned_meaning() {
    let t = IntTy::U32;
    assert!(is_trap(&int_op(t, BinOp::Sub, 0, 1), TrapKind::Overflow));
    assert_eq!(
        int_op(t, BinOp::WrappingSub, 0, 1),
        Ok(iv(t, 4_294_967_295))
    );
    assert!(is_trap(
        &int_op(t, BinOp::Mul, 65536, 65536),
        TrapKind::Overflow
    ));
    assert_eq!(int_op(t, BinOp::WrappingMul, 65536, 65536), Ok(iv(t, 0)));
    assert_eq!(
        int_op(t, BinOp::WrappingAdd, 4_294_967_295, 2),
        Ok(iv(t, 1))
    );
    assert_eq!(
        int_op(t, BinOp::Add, 4_294_967_294, 1),
        Ok(iv(t, 4_294_967_295))
    );
    assert_eq!(int_op(t, BinOp::Shl, 1, 31), Ok(iv(t, 0x8000_0000)));
    assert_eq!(int_op(t, BinOp::Shl, 3, 31), Ok(iv(t, 0x8000_0000)));
    // Logical right shift on unsigned.
    assert_eq!(int_op(t, BinOp::Shr, 0x8000_0000, 31), Ok(iv(t, 1)));
    assert!(is_trap(&int_op(t, BinOp::Shr, 1, 32), TrapKind::Overflow));
    assert_eq!(
        int_op(t, BinOp::Div, 4_294_967_295, 2),
        Ok(iv(t, 2_147_483_647))
    );
    assert_eq!(int_op(t, BinOp::Rem, 4_294_967_295, 10), Ok(iv(t, 5)));
    assert_eq!(
        int_op(t, BinOp::Gt, 4_294_967_295, 0),
        Ok(Value::Bool(true))
    );
    assert_eq!(int_op(t, BinOp::Eq, 7, 7), Ok(Value::Bool(true)));
    assert_eq!(int_op(t, BinOp::Ne, 7, 7), Ok(Value::Bool(false)));
}

#[test]
fn isize_is_64_bits_on_the_target() {
    let t = IntTy::Isize;
    assert_eq!(int_op(t, BinOp::Add, I64_MAX - 1, 1), Ok(iv(t, I64_MAX)));
    assert!(is_trap(
        &int_op(t, BinOp::Add, I64_MAX, 1),
        TrapKind::Overflow
    ));
    assert!(is_trap(
        &int_op(t, BinOp::Sub, I64_MIN, 1),
        TrapKind::Overflow
    ));
    assert!(is_trap(
        &int_op(t, BinOp::Div, I64_MIN, -1),
        TrapKind::Overflow
    ));
    assert_eq!(
        int_op(t, BinOp::WrappingAdd, I64_MAX, 1),
        Ok(iv(t, I64_MIN))
    );
    assert_eq!(int_op(t, BinOp::Shr, -8, 1), Ok(iv(t, -4)));
    assert_eq!(int_op(t, BinOp::Shl, 1, 63), Ok(iv(t, I64_MIN)));
    assert!(is_trap(&int_op(t, BinOp::Shl, 1, 64), TrapKind::Overflow));
    assert!(is_trap(&int_op(t, BinOp::Shl, 1, -1), TrapKind::Overflow));
}

#[test]
fn other_widths_at_their_edges() {
    assert!(is_trap(
        &int_op(IntTy::U8, BinOp::Add, 255, 1),
        TrapKind::Overflow
    ));
    assert_eq!(
        int_op(IntTy::U8, BinOp::WrappingAdd, 255, 1),
        Ok(iv(IntTy::U8, 0))
    );
    assert_eq!(
        int_op(IntTy::U16, BinOp::WrappingMul, 300, 300),
        Ok(iv(IntTy::U16, 90000 - 65536))
    );
    assert!(is_trap(
        &int_op(IntTy::I16, BinOp::Mul, 200, 200),
        TrapKind::Overflow
    ));
    assert!(is_trap(
        &int_op(IntTy::I32, BinOp::Add, 2_147_483_647, 1),
        TrapKind::Overflow
    ));
    assert!(is_trap(
        &int_op(IntTy::I64, BinOp::Mul, 4_294_967_296, 2_147_483_648),
        TrapKind::Overflow
    ));
    assert_eq!(
        int_op(IntTy::I64, BinOp::Mul, 4_294_967_296, 2_147_483_647),
        Ok(iv(IntTy::I64, 9_223_372_032_559_808_512))
    );
    assert!(is_trap(
        &int_op(IntTy::U64, BinOp::Add, U64_MAX, 1),
        TrapKind::Overflow
    ));
    assert_eq!(
        int_op(IntTy::U64, BinOp::Div, U64_MAX, 1),
        Ok(iv(IntTy::U64, U64_MAX))
    );
    assert!(is_trap(
        &int_op(IntTy::Usize, BinOp::Add, U64_MAX, 1),
        TrapKind::Overflow
    ));
    assert!(is_trap(
        &int_op(IntTy::Usize, BinOp::Sub, 0, 1),
        TrapKind::Overflow
    ));
    assert_eq!(
        int_op(IntTy::Usize, BinOp::WrappingSub, 0, 1),
        Ok(iv(IntTy::Usize, U64_MAX))
    );
}

#[test]
fn negation_and_not() {
    let (m, f) = unary_fn(TyKind::Int(IntTy::I16), UnOp::Neg);
    assert_eq!(eval(&m, f, vec![iv(IntTy::I16, 5)]), Ok(iv(IntTy::I16, -5)));
    assert!(is_trap(
        &eval(&m, f, vec![iv(IntTy::I16, -32768)]),
        TrapKind::Overflow
    ));
    let (m, f) = unary_fn(TyKind::Int(IntTy::I32), UnOp::WrappingNeg);
    assert_eq!(
        eval(&m, f, vec![iv(IntTy::I32, -2_147_483_648)]),
        Ok(iv(IntTy::I32, -2_147_483_648))
    );
    let (m, f) = unary_fn(TyKind::Int(IntTy::U8), UnOp::Not);
    assert_eq!(
        eval(&m, f, vec![iv(IntTy::U8, 0x0f)]),
        Ok(iv(IntTy::U8, 0xf0))
    );
    let (m, f) = unary_fn(TyKind::Int(IntTy::I32), UnOp::Not);
    assert_eq!(eval(&m, f, vec![iv(IntTy::I32, 0)]), Ok(iv(IntTy::I32, -1)));
    let (m, f) = unary_fn(TyKind::Bool, UnOp::Not);
    assert_eq!(eval(&m, f, vec![Value::Bool(true)]), Ok(Value::Bool(false)));
}

#[test]
fn bool_operators() {
    let (m, f) = binary_fn(TyKind::Bool, BinOp::BitXor);
    assert_eq!(
        eval(&m, f, vec![Value::Bool(true), Value::Bool(true)]),
        Ok(Value::Bool(false))
    );
    let (m, f) = binary_fn(TyKind::Bool, BinOp::BitOr);
    assert_eq!(
        eval(&m, f, vec![Value::Bool(false), Value::Bool(true)]),
        Ok(Value::Bool(true))
    );
}

fn f64_op(op: BinOp, a: f64, b: f64) -> Result<Value, zil_interp::EvalError> {
    let (m, f) = binary_fn(TyKind::Float(FloatTy::F64), op);
    eval(&m, f, vec![f64v(a), f64v(b)])
}

#[test]
fn f64_ieee_arithmetic() {
    // 0.1 + 0.2 rounds to 0x3FD3333333333334 under nearest-even.
    assert_eq!(
        f64_op(BinOp::Add, 0.1, 0.2),
        Ok(Value::Float {
            ty: FloatTy::F64,
            bits: 0x3FD3_3333_3333_3334
        })
    );
    assert_eq!(
        f64_op(BinOp::Div, 1.0, 0.0),
        Ok(Value::Float {
            ty: FloatTy::F64,
            bits: 0x7FF0_0000_0000_0000
        })
    );
    // CT canonicalizes arithmetic NaN to the quiet NaN.
    assert_eq!(
        f64_op(BinOp::Div, 0.0, 0.0),
        Ok(Value::Float {
            ty: FloatTy::F64,
            bits: 0x7FF8_0000_0000_0000
        })
    );
    assert_eq!(f64_op(BinOp::Rem, 5.5, 2.0), Ok(f64v(1.5)));
    assert_eq!(f64_op(BinOp::Rem, -5.5, 2.0), Ok(f64v(-1.5)));
    assert_eq!(f64_op(BinOp::Mul, 1.5, -2.0), Ok(f64v(-3.0)));
    assert_eq!(f64_op(BinOp::Sub, 1.0, 1.0), Ok(f64v(0.0)));
}

#[test]
fn f64_comparisons_follow_ieee() {
    let nan = f64::from_bits(0x7FF8_0000_0000_0000);
    assert_eq!(f64_op(BinOp::Eq, nan, nan), Ok(Value::Bool(false)));
    assert_eq!(f64_op(BinOp::Ne, nan, nan), Ok(Value::Bool(true)));
    assert_eq!(f64_op(BinOp::Lt, nan, 1.0), Ok(Value::Bool(false)));
    assert_eq!(f64_op(BinOp::Ge, nan, 1.0), Ok(Value::Bool(false)));
    assert_eq!(f64_op(BinOp::Eq, -0.0, 0.0), Ok(Value::Bool(true)));
    assert_eq!(f64_op(BinOp::Le, 1.0, 2.0), Ok(Value::Bool(true)));
}

#[test]
fn f32_arithmetic_and_negation() {
    let (m, f) = binary_fn(TyKind::Float(FloatTy::F32), BinOp::Add);
    // 16777216 + 1 is a tie in binary32 and rounds to even (16777216).
    assert_eq!(
        eval(&m, f, vec![f32v(16_777_216.0), f32v(1.0)]),
        Ok(f32v(16_777_216.0))
    );
    let (m, f) = unary_fn(TyKind::Float(FloatTy::F32), UnOp::Neg);
    assert_eq!(
        eval(&m, f, vec![f32v(0.0)]),
        Ok(Value::Float {
            ty: FloatTy::F32,
            bits: 0x8000_0000
        })
    );
}
