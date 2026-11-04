//! Floating source typing; callers reserve decimal work before parsing.
use super::*;
use zil_base::numeric::{FloatFormat, FloatValue};

pub(super) fn ty(text: &str, expected: Option<&Type>, span: Span) -> R<Type> {
    let suffix = if text.ends_with("f32") {
        Some(Type::F32)
    } else if text.ends_with("f64") {
        Some(Type::F64)
    } else {
        None
    };
    let context = expected.filter(|ty| ty.is_float());
    if let (Some(suffix), Some(context)) = (&suffix, context)
        && suffix != context
    {
        return Err(error(
            "E-TYPE-MISMATCH",
            span,
            "floating literal suffix differs from expected type",
        ));
    }
    Ok(suffix.or_else(|| context.cloned()).unwrap_or(Type::F64))
}
pub(super) fn literal(text: &str, expected: Option<&Type>, span: Span) -> R<Constant> {
    let ty = ty(text, expected, span)?;
    let spelling = text
        .strip_suffix("f32")
        .or_else(|| text.strip_suffix("f64"))
        .unwrap_or(text);
    let format = if ty == Type::F32 {
        FloatFormat::F32
    } else {
        FloatFormat::F64
    };
    match FloatValue::from_decimal(spelling, format) {
        Ok(FloatValue::F32(bits)) => Ok(Constant::F32(bits)),
        Ok(FloatValue::F64(bits)) => Ok(Constant::F64(bits)),
        Err(_) => Err(error(
            "E-FLOAT-LITERAL",
            span,
            "invalid floating literal or finite literal overflows its format",
        )),
    }
}
pub(super) fn binary(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    )
}
