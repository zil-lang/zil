#![forbid(unsafe_code)]
//! A bounded, pure scalar VM. It only executes independently verified CTIR.
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::{BinaryOp, Constant, CtInstructionKind, Type, UnaryOp};
use zil_verify::VerifiedCTIR;

mod aggregate_metadata;
mod ct_artifact;
mod materialize;
pub use aggregate_metadata::preflight_aggregate_metadata;
pub use ct_artifact::{CtExecutionBudget, CtReadView, evaluate_ct_artifact};
mod program;
mod source;
pub use program::{CompileTimeValue, ProgramEvaluation, evaluate_program};
pub use source::{
    SourceCompiler, SourceEvaluationUsage, SourceProgramContext, evaluate_source_constants,
    evaluate_source_constants_with_compiler, evaluate_source_constants_with_context,
    evaluate_source_constants_with_usage, validate_unchanged_source,
};

pub fn evaluate(program: &VerifiedCTIR) -> CompileResult<Constant> {
    let m = program.module();
    let mut values = Vec::with_capacity(m.instructions.len());
    let mut steps = 0u64;
    for instruction in &m.instructions {
        steps += 1;
        if steps > m.max_steps {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "execution exhausted declared CT steps",
                None,
            )]);
        }
        let value = match &instruction.kind {
            CtInstructionKind::Constant(v) => v.clone(),
            CtInstructionKind::ConvertInteger { value, target } => {
                convert_integer(&values[value.0 as usize], target)?
            }
            CtInstructionKind::ConvertNumber { value, target } => {
                convert_number(&values[value.0 as usize], target)?
            }
            CtInstructionKind::Unary { op, operand } => unary(*op, &values[operand.0 as usize])?,
            CtInstructionKind::Binary { op, left, right } => {
                binary(*op, &values[left.0 as usize], &values[right.0 as usize])?
            }
            CtInstructionKind::Forbidden(name) => {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-FORBIDDEN",
                    None,
                    name,
                )]);
            }
        };
        values.push(value)
    }
    Ok(values[m.result.0 as usize].clone())
}
fn fail() -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-ARITHMETIC",
        None,
        "checked compile-time arithmetic failed",
    )]
}
pub(crate) fn convert_integer(value: &Constant, target: &Type) -> CompileResult<Constant> {
    let value = match value {
        Constant::U8(v) => i128::from(*v),
        Constant::U16(v) => i128::from(*v),
        Constant::I32(v) => i128::from(*v),
        Constant::I64(v) => i128::from(*v),
        _ => return Err(fail()),
    };
    match target {
        Type::U8 => u8::try_from(value).map(Constant::U8).map_err(|_| fail()),
        Type::U16 => u16::try_from(value).map(Constant::U16).map_err(|_| fail()),
        Type::I32 => i32::try_from(value).map(Constant::I32).map_err(|_| fail()),
        Type::I64 => i64::try_from(value).map(Constant::I64).map_err(|_| fail()),
        _ => Err(fail()),
    }
}

fn float_value(value: &Constant) -> Option<zil_base::numeric::FloatValue> {
    use zil_base::numeric::FloatValue;
    match value {
        Constant::F32(bits) => Some(FloatValue::F32(*bits)),
        Constant::F64(bits) => Some(FloatValue::F64(*bits)),
        _ => None,
    }
}
fn float_constant(value: zil_base::numeric::FloatValue) -> Constant {
    use zil_base::numeric::FloatValue;
    match value {
        FloatValue::F32(bits) => Constant::F32(bits),
        FloatValue::F64(bits) => Constant::F64(bits),
    }
}
pub(crate) fn convert_number(value: &Constant, target: &Type) -> CompileResult<Constant> {
    use zil_base::numeric::{FloatFormat, IntegerValue};
    let invalid = || vec![Diagnostic::error(Phase::Ct, "E-NUMERIC-CONVERSION", None,
        "numeric conversion requires a finite integral in-range integer result or a representable floating result")];
    if target.is_float() {
        let format = if *target == Type::F32 { FloatFormat::F32 } else { FloatFormat::F64 };
        if let Some(value) = float_value(value) {
            return value.convert(format).map(float_constant).map_err(|_| invalid());
        }
        let value = match value {
            Constant::U8(n) => IntegerValue::Unsigned((*n).into()),
            Constant::U16(n) => IntegerValue::Unsigned((*n).into()),
            Constant::I32(n) => IntegerValue::Signed((*n).into()),
            Constant::I64(n) => IntegerValue::Signed((*n).into()),
            _ => return Err(invalid()),
        };
        return Ok(float_constant(value.to_float(format)));
    }
    let value = float_value(value).ok_or_else(invalid)?;
    let (signed, width) = match target {
        Type::U8 => (false, 8), Type::U16 => (false, 16),
        Type::I32 => (true, 32), Type::I64 => (true, 64),
        _ => return Err(invalid()),
    };
    match value.to_integer(signed, width).map_err(|_| invalid())? {
        IntegerValue::Unsigned(n) if *target == Type::U8 => Ok(Constant::U8(n as u8)),
        IntegerValue::Unsigned(n) => Ok(Constant::U16(n as u16)),
        IntegerValue::Signed(n) if *target == Type::I32 => Ok(Constant::I32(n as i32)),
        IntegerValue::Signed(n) => Ok(Constant::I64(n as i64)),
    }
}
fn float_binary(op: BinaryOp, left: &Constant, right: &Constant) -> CompileResult<Constant> {
    use zil_base::numeric::FloatOp;
    use std::cmp::Ordering::{Less, Equal, Greater};
    let left = float_value(left).ok_or_else(fail)?;
    let right = float_value(right).ok_or_else(fail)?;
    let arithmetic = match op {
        BinaryOp::Add => Some(FloatOp::Add), BinaryOp::Subtract => Some(FloatOp::Subtract),
        BinaryOp::Multiply => Some(FloatOp::Multiply), BinaryOp::Divide => Some(FloatOp::Divide),
        _ => None,
    };
    if let Some(op) = arithmetic {
        return left.binary(right, op).map(float_constant).map_err(|_| fail());
    }
    let compare = left.partial_compare(right).map_err(|_| fail())?;
    Ok(Constant::Bool(match op {
        BinaryOp::Equal => compare == Some(Equal),
        BinaryOp::NotEqual => compare != Some(Equal),
        BinaryOp::Less => compare == Some(Less),
        BinaryOp::LessEqual => matches!(compare, Some(Less | Equal)),
        BinaryOp::Greater => compare == Some(Greater),
        BinaryOp::GreaterEqual => matches!(compare, Some(Greater | Equal)),
        _ => return Err(fail()),
    }))
}

pub(crate) fn unary(op: UnaryOp, value: &Constant) -> CompileResult<Constant> {
    match (op, value) {
        (UnaryOp::Negate, value @ (Constant::F32(_) | Constant::F64(_))) =>
            Ok(float_constant(float_value(value).expect("floating variant").negate())),
        (UnaryOp::Not, Constant::Bool(v)) => Ok(Constant::Bool(!v)),
        (UnaryOp::Negate, Constant::U8(v)) => {
            0u8.checked_sub(*v).map(Constant::U8).ok_or_else(fail)
        }
        (UnaryOp::BitNot, Constant::U8(v)) => Ok(Constant::U8(!v)),
        (UnaryOp::Negate, Constant::U16(v)) => {
            0u16.checked_sub(*v).map(Constant::U16).ok_or_else(fail)
        }
        (UnaryOp::BitNot, Constant::U16(v)) => Ok(Constant::U16(!v)),
        (UnaryOp::Negate, Constant::I32(v)) => v.checked_neg().map(Constant::I32).ok_or_else(fail),
        (UnaryOp::Negate, Constant::I64(v)) => v.checked_neg().map(Constant::I64).ok_or_else(fail),
        (UnaryOp::BitNot, Constant::I32(v)) => Ok(Constant::I32(!v)),
        (UnaryOp::BitNot, Constant::I64(v)) => Ok(Constant::I64(!v)),
        _ => Err(fail()),
    }
}
macro_rules! integer_binary {
    ($op:expr,$a:expr,$b:expr,$variant:ident,$bits:expr) => {{
        let (a, b) = (*$a, *$b);
        let value = match $op {
            BinaryOp::Add => a.checked_add(b),
            BinaryOp::Subtract => a.checked_sub(b),
            BinaryOp::Multiply => a.checked_mul(b),
            BinaryOp::Divide => a.checked_div(b),
            BinaryOp::Remainder => a.checked_rem(b),
            BinaryOp::AddWrapping => Some(a.wrapping_add(b)),
            BinaryOp::SubtractWrapping => Some(a.wrapping_sub(b)),
            BinaryOp::MultiplyWrapping => Some(a.wrapping_mul(b)),
            BinaryOp::BitAnd => Some(a & b),
            BinaryOp::BitOr => Some(a | b),
            BinaryOp::BitXor => Some(a ^ b),
            BinaryOp::ShiftLeft => {
                let shift = i128::from(b);
                if (0..$bits).contains(&shift) {
                    a.checked_shl(shift as u32)
                } else {
                    None
                }
            }
            BinaryOp::ShiftRight => {
                let shift = i128::from(b);
                if (0..$bits).contains(&shift) {
                    a.checked_shr(shift as u32)
                } else {
                    None
                }
            }
            BinaryOp::Equal => return Ok(Constant::Bool(a == b)),
            BinaryOp::NotEqual => return Ok(Constant::Bool(a != b)),
            BinaryOp::Less => return Ok(Constant::Bool(a < b)),
            BinaryOp::LessEqual => return Ok(Constant::Bool(a <= b)),
            BinaryOp::Greater => return Ok(Constant::Bool(a > b)),
            BinaryOp::GreaterEqual => return Ok(Constant::Bool(a >= b)),
            _ => None,
        };
        value.map(Constant::$variant).ok_or_else(fail)
    }};
}
pub(crate) fn binary(op: BinaryOp, left: &Constant, right: &Constant) -> CompileResult<Constant> {
    match (left, right) {
        (Constant::Unit, Constant::Unit) => match op {
            BinaryOp::Equal => Ok(Constant::Bool(true)),
            BinaryOp::NotEqual => Ok(Constant::Bool(false)),
            _ => Err(fail()),
        },
        (Constant::F32(_), Constant::F32(_)) | (Constant::F64(_), Constant::F64(_)) => float_binary(op, left, right),
        (Constant::U8(a), Constant::U8(b)) => integer_binary!(op, a, b, U8, 8),
        (Constant::U16(a), Constant::U16(b)) => integer_binary!(op, a, b, U16, 16),
        (Constant::I32(a), Constant::I32(b)) => integer_binary!(op, a, b, I32, 32),
        (Constant::I64(a), Constant::I64(b)) => integer_binary!(op, a, b, I64, 64),
        (Constant::Bool(a), Constant::Bool(b)) => match op {
            BinaryOp::Equal => Ok(Constant::Bool(a == b)),
            BinaryOp::NotEqual => Ok(Constant::Bool(a != b)),
            BinaryOp::And => Ok(Constant::Bool(*a && *b)),
            BinaryOp::Or => Ok(Constant::Bool(*a || *b)),
            _ => Err(fail()),
        },
        _ => Err(fail()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, TargetProfile};
    use zil_ir::{CtInstruction, CtModule, Type, ValueId};
    fn program(op: BinaryOp, a: i32, b: i32) -> CtModule {
        CtModule {
            schema_version: zil_base::SCHEMA_VERSION,
            input_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            instructions: vec![
                CtInstruction {
                    result: ValueId(0),
                    ty: Type::I32,
                    kind: CtInstructionKind::Constant(Constant::I32(a)),
                },
                CtInstruction {
                    result: ValueId(1),
                    ty: Type::I32,
                    kind: CtInstructionKind::Constant(Constant::I32(b)),
                },
                CtInstruction {
                    result: ValueId(2),
                    ty: Type::I32,
                    kind: CtInstructionKind::Binary {
                        op,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                },
            ],
            result: ValueId(2),
            max_steps: 3,
            dependencies: vec![],
        }
    }
    #[test]
    fn executes_verified_arithmetic() {
        let p = program(BinaryOp::Multiply, 6, 7);
        let v = zil_verify::verify_ct(p, &TargetProfile::default(), &Limits::default()).unwrap();
        assert_eq!(evaluate(&v).unwrap(), Constant::I32(42));
    }
    #[test]
    fn checked_division_reports_error() {
        let p = program(BinaryOp::Divide, 1, 0);
        let v = zil_verify::verify_ct(p, &TargetProfile::default(), &Limits::default()).unwrap();
        assert_eq!(evaluate(&v).unwrap_err()[0].code, "E-CT-ARITHMETIC");
    }
    #[test]
    fn overflow_and_wrapping_differ() {
        let target = TargetProfile::default();
        let limit = Limits::default();
        let p =
            zil_verify::verify_ct(program(BinaryOp::Add, i32::MAX, 1), &target, &limit).unwrap();
        assert!(evaluate(&p).is_err());
        let p = zil_verify::verify_ct(program(BinaryOp::AddWrapping, i32::MAX, 1), &target, &limit)
            .unwrap();
        assert_eq!(evaluate(&p).unwrap(), Constant::I32(i32::MIN));
    }
}
