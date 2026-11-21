//! Checked numeric conversions (`_zil_design.md` §2.4): integer targets
//! keep the mathematical value or fail; float sources must be finite,
//! integral and in range (`-0.0` → 0); float targets round once to nearest
//! even and fail when a finite source rounds to infinity.

mod common;

use common::*;
use zil_interp::Value;
use zil_ir::body::{FloatTy, IntTy, TrapKind, TyKind};

fn conv(from: TyKind, to: TyKind, v: Value) -> Result<Value, zil_interp::EvalError> {
    let (m, f) = convert_fn(from, to);
    eval(&m, f, vec![v])
}

const F64: TyKind = TyKind::Float(FloatTy::F64);
const F32: TyKind = TyKind::Float(FloatTy::F32);

#[test]
fn float_to_u32() {
    let u32t = TyKind::Int(IntTy::U32);
    assert_eq!(
        conv(F64, u32t.clone(), f64v(4_294_967_295.0)),
        Ok(iv(IntTy::U32, 4_294_967_295))
    );
    assert!(is_trap(
        &conv(F64, u32t.clone(), f64v(4_294_967_296.0)),
        TrapKind::Conversion
    ));
    assert!(is_trap(
        &conv(F64, u32t.clone(), f64v(-1.0)),
        TrapKind::Conversion
    ));
    assert_eq!(conv(F64, u32t, f64v(-0.0)), Ok(iv(IntTy::U32, 0)));
}

#[test]
fn float_to_int_requires_an_integral_finite_value() {
    let i32t = TyKind::Int(IntTy::I32);
    assert!(is_trap(
        &conv(F64, i32t.clone(), f64v(1.5)),
        TrapKind::Conversion
    ));
    assert!(is_trap(
        &conv(F64, i32t.clone(), f64v(-0.5)),
        TrapKind::Conversion
    ));
    assert!(is_trap(
        &conv(F64, i32t.clone(), f64v(f64::NAN)),
        TrapKind::Conversion
    ));
    assert!(is_trap(
        &conv(F64, TyKind::Int(IntTy::I64), f64v(f64::INFINITY)),
        TrapKind::Conversion
    ));
    assert_eq!(
        conv(F64, i32t.clone(), f64v(-2_147_483_648.0)),
        Ok(iv(IntTy::I32, -2_147_483_648))
    );
    assert!(is_trap(
        &conv(F64, i32t, f64v(2_147_483_648.0)),
        TrapKind::Conversion
    ));
    // 2^63 is outside i64; -2^63 is inside.
    assert!(is_trap(
        &conv(
            F64,
            TyKind::Int(IntTy::I64),
            f64v(9_223_372_036_854_775_808.0)
        ),
        TrapKind::Conversion
    ));
    assert_eq!(
        conv(
            F64,
            TyKind::Int(IntTy::Isize),
            f64v(-9_223_372_036_854_775_808.0)
        ),
        Ok(iv(IntTy::Isize, -9_223_372_036_854_775_808))
    );
    assert_eq!(
        conv(F32, TyKind::Int(IntTy::U8), f32v(255.0)),
        Ok(iv(IntTy::U8, 255))
    );
    assert!(is_trap(
        &conv(F32, TyKind::Int(IntTy::U8), f32v(256.0)),
        TrapKind::Conversion
    ));
}

#[test]
fn integer_to_integer_preserves_the_value() {
    assert!(is_trap(
        &conv(
            TyKind::Int(IntTy::I32),
            TyKind::Int(IntTy::U32),
            iv(IntTy::I32, -1)
        ),
        TrapKind::Conversion
    ));
    assert!(is_trap(
        &conv(
            TyKind::Int(IntTy::U32),
            TyKind::Int(IntTy::U8),
            iv(IntTy::U32, 300)
        ),
        TrapKind::Conversion
    ));
    assert_eq!(
        conv(
            TyKind::Int(IntTy::U8),
            TyKind::Int(IntTy::I16),
            iv(IntTy::U8, 255)
        ),
        Ok(iv(IntTy::I16, 255))
    );
    assert_eq!(
        conv(
            TyKind::Int(IntTy::I64),
            TyKind::Int(IntTy::Isize),
            iv(IntTy::I64, -5)
        ),
        Ok(iv(IntTy::Isize, -5))
    );
    assert_eq!(
        conv(
            TyKind::Int(IntTy::U64),
            TyKind::Int(IntTy::Usize),
            iv(IntTy::U64, 18_446_744_073_709_551_615)
        ),
        Ok(iv(IntTy::Usize, 18_446_744_073_709_551_615))
    );
    assert!(is_trap(
        &conv(
            TyKind::Int(IntTy::I8),
            TyKind::Int(IntTy::U64),
            iv(IntTy::I8, -128)
        ),
        TrapKind::Conversion
    ));
}

#[test]
fn conversions_into_floats_round_once() {
    // u64::MAX = 2^64 - 1 rounds to 2^64 in binary32 (0x5F800000).
    assert_eq!(
        conv(
            TyKind::Int(IntTy::U64),
            F32,
            iv(IntTy::U64, 18_446_744_073_709_551_615)
        ),
        Ok(Value::Float {
            ty: FloatTy::F32,
            bits: 0x5F80_0000
        })
    );
    // i64::MAX rounds to 2^63 in binary64.
    assert_eq!(
        conv(
            TyKind::Int(IntTy::I64),
            F64,
            iv(IntTy::I64, 9_223_372_036_854_775_807)
        ),
        Ok(Value::Float {
            ty: FloatTy::F64,
            bits: 0x43E0_0000_0000_0000
        })
    );
    assert_eq!(
        conv(TyKind::Int(IntTy::I8), F64, iv(IntTy::I8, -128)),
        Ok(f64v(-128.0))
    );
    // 0.1 in binary64 rounds to 0x3DCCCCCD in binary32.
    assert_eq!(
        conv(F64, F32, f64v(0.1)),
        Ok(Value::Float {
            ty: FloatTy::F32,
            bits: 0x3DCC_CCCD
        })
    );
    // A finite source rounding to infinity fails; infinity stays infinity.
    assert!(is_trap(&conv(F64, F32, f64v(1e300)), TrapKind::Conversion));
    assert_eq!(
        conv(F64, F32, f64v(f64::NEG_INFINITY)),
        Ok(Value::Float {
            ty: FloatTy::F32,
            bits: 0xFF80_0000
        })
    );
    assert_eq!(conv(F32, F64, f32v(0.5)), Ok(f64v(0.5)));
}
