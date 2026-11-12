//! Value computation only. Original scopes and witness dependencies are checked
//! separately; no scalar result can supply a type, source, or effect premise.
use super::*;
use zil_base::numeric::{FloatFormat, FloatOp, FloatValue, IntegerValue};

#[derive(Clone, Copy)]
struct Int {
    signed: bool,
    width: u32,
}
fn integer(ty: &Type, target: u16) -> Check<Int> {
    let (signed, width) = match ty {
        Type::I8 => (true, 8),
        Type::I16 => (true, 16),
        Type::I32 => (true, 32),
        Type::I64 => (true, 64),
        Type::U8 => (false, 8),
        Type::U16 => (false, 16),
        Type::U32 => (false, 32),
        Type::U64 => (false, 64),
        Type::Isize if matches!(target, 32 | 64) => (true, u32::from(target)),
        Type::Usize if matches!(target, 32 | 64) => (false, u32::from(target)),
        _ => {
            return Err(bad(
                "constant carrier is not an actual supported integer domain",
            ));
        }
    };
    Ok(Int { signed, width })
}
fn number(value: &Constant, target: u16) -> Check<i128> {
    let result = match value {
        Constant::I8(v) => i128::from(*v),
        Constant::I16(v) => i128::from(*v),
        Constant::I32(v) => i128::from(*v),
        Constant::I64(v) | Constant::Isize(v) => i128::from(*v),
        Constant::U8(v) => i128::from(*v),
        Constant::U16(v) => i128::from(*v),
        Constant::U32(v) => i128::from(*v),
        Constant::U64(v) | Constant::Usize(v) => i128::from(*v),
        _ => return Err(bad("constant operand is not an actual integer")),
    };
    let domain = integer(&value.ty(), target)?;
    if !domain.contains(result) {
        return Err(bad("integer constant is outside its exact target domain"));
    }
    Ok(result)
}
impl Int {
    fn contains(self, n: i128) -> bool {
        if self.signed {
            let bound = 1i128 << (self.width - 1);
            (-bound..bound).contains(&n)
        } else {
            (0..(1i128 << self.width)).contains(&n)
        }
    }
    fn mask(self) -> u128 {
        (1u128 << self.width) - 1
    }
    fn wrap(self, bits: u128) -> i128 {
        let bits = bits & self.mask();
        if self.signed && bits & (1u128 << (self.width - 1)) != 0 {
            bits as i128 - (1i128 << self.width)
        } else {
            bits as i128
        }
    }
}
fn make(ty: &Type, n: i128, target: u16) -> Check<Constant> {
    if !integer(ty, target)?.contains(n) {
        return Err(bad(
            "constant arithmetic or conversion overflows its actual carrier",
        ));
    }
    Ok(match ty {
        Type::I8 => Constant::I8(i8::try_from(n).map_err(|_| bad("i8 constant range"))?),
        Type::I16 => Constant::I16(i16::try_from(n).map_err(|_| bad("i16 constant range"))?),
        Type::I32 => Constant::I32(i32::try_from(n).map_err(|_| bad("i32 constant range"))?),
        Type::I64 => Constant::I64(i64::try_from(n).map_err(|_| bad("i64 constant range"))?),
        Type::U8 => Constant::U8(u8::try_from(n).map_err(|_| bad("u8 constant range"))?),
        Type::U16 => Constant::U16(u16::try_from(n).map_err(|_| bad("u16 constant range"))?),
        Type::U32 => Constant::U32(u32::try_from(n).map_err(|_| bad("u32 constant range"))?),
        Type::U64 => Constant::U64(u64::try_from(n).map_err(|_| bad("u64 constant range"))?),
        Type::Isize => Constant::Isize(i64::try_from(n).map_err(|_| bad("isize constant range"))?),
        Type::Usize => Constant::Usize(u64::try_from(n).map_err(|_| bad("usize constant range"))?),
        _ => return Err(bad("constant result is not an integer carrier")),
    })
}
fn float(value: &Constant) -> Check<FloatValue> {
    match value {
        Constant::F32(v) => Ok(FloatValue::F32(*v)),
        Constant::F64(v) => Ok(FloatValue::F64(*v)),
        _ => Err(bad("constant operand is not IEEE floating point")),
    }
}
fn floating(value: FloatValue) -> Constant {
    match value {
        FloatValue::F32(v) => Constant::F32(v),
        FloatValue::F64(v) => Constant::F64(v),
    }
}
fn format(ty: &Type) -> Check<FloatFormat> {
    match ty {
        Type::F32 => Ok(FloatFormat::F32),
        Type::F64 => Ok(FloatFormat::F64),
        _ => Err(bad("constant carrier is not an IEEE format")),
    }
}
fn parse_integer(
    text: &str,
    ty: &Type,
    target: u16,
    b: &mut Budget,
    negative: bool,
) -> Check<Constant> {
    b.spend(text.len().saturating_mul(2).saturating_add(1))?;
    let mut digits = text;
    for suffix in [
        "usize", "isize", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
    ] {
        if let Some(prefix) = text.strip_suffix(suffix) {
            if suffix != ty.name() {
                return Err(bad(
                    "original integer suffix differs from its actual carrier",
                ));
            }
            digits = prefix;
            break;
        }
    }
    let normalized = digits.replace('_', "");
    let (digits, radix) = if let Some(d) = normalized.strip_prefix("0x") {
        (d, 16)
    } else if let Some(d) = normalized.strip_prefix("0o") {
        (d, 8)
    } else if let Some(d) = normalized.strip_prefix("0b") {
        (d, 2)
    } else {
        (normalized.as_str(), 10)
    };
    let magnitude =
        u128::from_str_radix(digits, radix).map_err(|_| bad("invalid original integer literal"))?;
    let magnitude = i128::try_from(magnitude)
        .map_err(|_| bad("original integer magnitude exceeds every supported carrier"))?;
    make(ty, if negative { -magnitude } else { magnitude }, target)
}
fn convert(value: &Constant, ty: &Type, target: u16) -> Check<Constant> {
    if value.ty().is_integer() {
        let n = number(value, target)?;
        if ty.is_integer() {
            return make(ty, n, target);
        }
        let domain = integer(&value.ty(), target)?;
        return Ok(floating(
            if domain.signed {
                IntegerValue::Signed(n)
            } else {
                IntegerValue::Unsigned(n as u128)
            }
            .to_float(format(ty)?),
        ));
    }
    let source = float(value)?;
    if ty.is_float() {
        return source
            .convert(format(ty)?)
            .map(floating)
            .map_err(|_| bad("floating constant conversion is outside its exact carrier"));
    }
    let domain = integer(ty, target)?;
    let n = source
        .to_integer(domain.signed, domain.width as usize)
        .map_err(|_| bad("floating constant cannot convert into the actual integer carrier"))?;
    let n = match n {
        IntegerValue::Signed(v) => v,
        IntegerValue::Unsigned(v) => {
            i128::try_from(v).map_err(|_| bad("converted constant exceeds the integer domain"))?
        }
    };
    make(ty, n, target)
}
fn unary(op: UnaryOp, value: &Constant, target: u16) -> Check<Constant> {
    match (op, value) {
        (UnaryOp::Not, Constant::Bool(value)) => Ok(Constant::Bool(!value)),
        (UnaryOp::Negate, Constant::F32(_) | Constant::F64(_)) => {
            Ok(floating(float(value)?.negate()))
        }
        (UnaryOp::Negate, _) => make(
            &value.ty(),
            number(value, target)?
                .checked_neg()
                .ok_or_else(|| bad("constant negate overflow"))?,
            target,
        ),
        (UnaryOp::BitNot, _) => {
            let domain = integer(&value.ty(), target)?;
            make(
                &value.ty(),
                domain.wrap(!(number(value, target)? as u128)),
                target,
            )
        }
        _ => Err(bad(
            "original unary operator is not defined for its constant operand",
        )),
    }
}
fn binary(op: BinaryOp, left: &Constant, right: &Constant, target: u16) -> Check<Constant> {
    if left.ty() != right.ty() {
        return Err(bad("constant operands have different original carriers"));
    }
    if matches!((left, right), (Constant::Unit, Constant::Unit)) {
        return match op {
            BinaryOp::Equal => Ok(Constant::Bool(true)),
            BinaryOp::NotEqual => Ok(Constant::Bool(false)),
            _ => Err(bad("original Unit constant operator is invalid")),
        };
    }
    if let (Constant::Bool(a), Constant::Bool(b)) = (left, right) {
        return Ok(Constant::Bool(match op {
            BinaryOp::And => *a && *b,
            BinaryOp::Or => *a || *b,
            BinaryOp::Equal => a == b,
            BinaryOp::NotEqual => a != b,
            _ => return Err(bad("original bool constant operator is invalid")),
        }));
    }
    if left.ty().is_float() {
        let a = float(left)?;
        let b = float(right)?;
        let arithmetic = match op {
            BinaryOp::Add => Some(FloatOp::Add),
            BinaryOp::Subtract => Some(FloatOp::Subtract),
            BinaryOp::Multiply => Some(FloatOp::Multiply),
            BinaryOp::Divide => Some(FloatOp::Divide),
            _ => None,
        };
        if let Some(op) = arithmetic {
            return a
                .binary(b, op)
                .map(floating)
                .map_err(|_| bad("IEEE constant operands differ"));
        }
        let order = a
            .partial_compare(b)
            .map_err(|_| bad("IEEE constant operands differ"))?;
        use std::cmp::Ordering::{Equal, Greater, Less};
        return Ok(Constant::Bool(match op {
            BinaryOp::Equal => order == Some(Equal),
            BinaryOp::NotEqual => order != Some(Equal),
            BinaryOp::Less => order == Some(Less),
            BinaryOp::LessEqual => matches!(order, Some(Less | Equal)),
            BinaryOp::Greater => order == Some(Greater),
            BinaryOp::GreaterEqual => matches!(order, Some(Greater | Equal)),
            _ => return Err(bad("original float constant operator is invalid")),
        }));
    }
    let ty = left.ty();
    let domain = integer(&ty, target)?;
    let a = number(left, target)?;
    let c = number(right, target)?;
    let compared = match op {
        BinaryOp::Equal => Some(a == c),
        BinaryOp::NotEqual => Some(a != c),
        BinaryOp::Less => Some(a < c),
        BinaryOp::LessEqual => Some(a <= c),
        BinaryOp::Greater => Some(a > c),
        BinaryOp::GreaterEqual => Some(a >= c),
        _ => None,
    };
    if let Some(value) = compared {
        return Ok(Constant::Bool(value));
    }
    if domain.signed
        && a == -(1i128 << (domain.width - 1))
        && c == -1
        && matches!(op, BinaryOp::Divide | BinaryOp::Remainder)
    {
        return Err(bad(
            "signed minimum division or remainder traps in its exact carrier",
        ));
    }
    let result = match op {
        BinaryOp::Add => a.checked_add(c),
        BinaryOp::Subtract => a.checked_sub(c),
        BinaryOp::Multiply => {
            if domain.signed {
                a.checked_mul(c)
            } else {
                (a as u128)
                    .checked_mul(c as u128)
                    .and_then(|v| i128::try_from(v).ok())
            }
        }
        BinaryOp::Divide => a.checked_div(c),
        BinaryOp::Remainder => a.checked_rem(c),
        BinaryOp::AddWrapping => Some(domain.wrap((a as u128).wrapping_add(c as u128))),
        BinaryOp::SubtractWrapping => Some(domain.wrap((a as u128).wrapping_sub(c as u128))),
        BinaryOp::MultiplyWrapping => Some(domain.wrap((a as u128).wrapping_mul(c as u128))),
        BinaryOp::BitAnd => Some(domain.wrap((a as u128) & (c as u128))),
        BinaryOp::BitOr => Some(domain.wrap((a as u128) | (c as u128))),
        BinaryOp::BitXor => Some(domain.wrap((a as u128) ^ (c as u128))),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            let shift = u32::try_from(c)
                .map_err(|_| bad("constant shift count is negative or too large"))?;
            if shift >= domain.width {
                return Err(bad("constant shift count exceeds its exact carrier width"));
            }
            Some(if op == BinaryOp::ShiftLeft {
                domain.wrap((a as u128) << shift)
            } else {
                a >> shift
            })
        }
        _ => return Err(bad("original integer constant operator is invalid")),
    }
    .ok_or_else(|| bad("constant arithmetic traps or exceeds its supported exact carrier"))?;
    make(&ty, result, target)
}
fn literal_type(pattern: &DynConstPattern) -> Option<Type> {
    match pattern {
        DynConstPattern::Value(value) => Some(value.ty()),
        DynConstPattern::IntegerLiteral(text) => Some(
            [
                ("usize", Type::Usize),
                ("isize", Type::Isize),
                ("u64", Type::U64),
                ("i64", Type::I64),
                ("u32", Type::U32),
                ("i32", Type::I32),
                ("u16", Type::U16),
                ("i16", Type::I16),
                ("u8", Type::U8),
                ("i8", Type::I8),
            ]
            .into_iter()
            .find_map(|(suffix, ty)| text.ends_with(suffix).then_some(ty))
            .unwrap_or(Type::I32),
        ),
        DynConstPattern::FloatLiteral(text) => Some(if text.ends_with("f32") {
            Type::F32
        } else {
            Type::F64
        }),
        DynConstPattern::Unary { ty, .. } | DynConstPattern::Convert { ty, .. } => {
            match ty.as_ref() {
                DynTypePattern::Concrete(ty) => Some(ty.clone()),
                _ => None,
            }
        }
        DynConstPattern::Binary { op, ty, .. } => {
            if matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
            ) {
                Some(Type::Bool)
            } else {
                match ty.as_ref() {
                    DynTypePattern::Concrete(ty) => Some(ty.clone()),
                    _ => None,
                }
            }
        }
        _ => None,
    }
}
/// Resolve only fully closed scalar subexpressions. None preserves an open
/// parameter/projection for the complete witness interpreter; it never proves
/// an unknown constant, carrier, or equality to be empty or true.
pub(super) fn closed(
    pattern: &DynConstPattern,
    expected: &Type,
    target: u16,
    b: &mut Budget,
) -> Check<Option<Constant>> {
    dynamic_metadata::recipe(pattern, b)?;
    eval(pattern, expected, target, b, 0)
}
fn eval(
    pattern: &DynConstPattern,
    expected: &Type,
    target: u16,
    b: &mut Budget,
    depth: usize,
) -> Check<Option<Constant>> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("dynamic scalar expression depth exceeded"));
    }
    let result = match pattern {
        DynConstPattern::Value(value) => {
            if value.ty() != *expected {
                return Err(bad("typed original constant has a different carrier"));
            }
            if expected.is_integer() {
                number(value, target)?;
            }
            if !expected.is_integer()
                && !expected.is_float()
                && !matches!(expected, Type::Unit | Type::Bool)
            {
                return Err(bad("original constant is outside supported scalar values"));
            }
            value.clone()
        }
        DynConstPattern::IntegerLiteral(text) => parse_integer(text, expected, target, b, false)?,
        DynConstPattern::FloatLiteral(text) => {
            b.spend(zil_base::numeric::decimal_work(text.len()))?;
            let digits = if let Some(digits) = text.strip_suffix("f32") {
                if *expected != Type::F32 {
                    return Err(bad("original float suffix differs from its actual carrier"));
                }
                digits
            } else if let Some(digits) = text.strip_suffix("f64") {
                if *expected != Type::F64 {
                    return Err(bad("original float suffix differs from its actual carrier"));
                }
                digits
            } else {
                text
            };
            floating(
                FloatValue::from_decimal(digits, format(expected)?)
                    .map_err(|_| bad("original floating literal is invalid or out of range"))?,
            )
        }
        DynConstPattern::Unary { op, value, ty } => {
            let DynTypePattern::Concrete(carrier) = ty.as_ref() else {
                return Ok(None);
            };
            if *op == UnaryOp::Negate && carrier.is_integer() {
                if let DynConstPattern::IntegerLiteral(text) = value.as_ref() {
                    return parse_integer(text, carrier, target, b, true).and_then(|result| {
                        if result.ty() == *expected {
                            Ok(Some(result))
                        } else {
                            Err(bad(
                                "negate expression result differs from original carrier",
                            ))
                        }
                    });
                }
            }
            let Some(value) = eval(value, carrier, target, b, depth + 1)? else {
                return Ok(None);
            };
            unary(*op, &value, target)?
        }
        DynConstPattern::Binary {
            op,
            left,
            right,
            ty,
        } => {
            let DynTypePattern::Concrete(carrier) = ty.as_ref() else {
                return Ok(None);
            };
            let Some(left) = eval(left, carrier, target, b, depth + 1)? else {
                return Ok(None);
            };
            if matches!(
                (op, &left),
                (BinaryOp::And, Constant::Bool(false)) | (BinaryOp::Or, Constant::Bool(true))
            ) {
                left
            } else {
                let Some(right) = eval(right, carrier, target, b, depth + 1)? else {
                    return Ok(None);
                };
                binary(*op, &left, &right, target)?
            }
        }
        DynConstPattern::Convert { value, ty } => {
            let DynTypePattern::Concrete(carrier) = ty.as_ref() else {
                return Ok(None);
            };
            let Some(source) = literal_type(value) else {
                return Ok(None);
            };
            let Some(value) = eval(value, &source, target, b, depth + 1)? else {
                return Ok(None);
            };
            convert(&value, carrier, target)?
        }
        _ => return Ok(None),
    };
    if result.ty() != *expected {
        return Err(bad(
            "constant expression result differs from its declared exact carrier",
        ));
    }
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eval(pattern: &DynConstPattern, ty: &Type, width: u16) -> Check<Constant> {
        closed(pattern, ty, width, &mut Budget::new(&Limits::default())).map(|value| value.unwrap())
    }
    fn lit(text: &str) -> DynConstPattern {
        DynConstPattern::IntegerLiteral(text.into())
    }
    fn bin(
        op: BinaryOp,
        left: DynConstPattern,
        right: DynConstPattern,
        ty: Type,
    ) -> DynConstPattern {
        DynConstPattern::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
            ty: Box::new(DynTypePattern::Concrete(ty)),
        }
    }
    #[test]
    fn all_integer_carriers_keep_exact_boundaries_wrap_and_target_pointer_width() {
        for ty in [
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::Isize,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::Usize,
        ] {
            for width in [32, 64] {
                let domain = integer(&ty, width).unwrap();
                let max = if domain.signed {
                    (1i128 << (domain.width - 1)) - 1
                } else {
                    (1i128 << domain.width) - 1
                };
                assert_eq!(
                    number(&eval(&lit(&max.to_string()), &ty, width).unwrap(), width).unwrap(),
                    max
                );
                assert!(eval(&lit(&(max + 1).to_string()), &ty, width).is_err());
                let wrapped = bin(
                    BinaryOp::AddWrapping,
                    lit(&max.to_string()),
                    lit("1"),
                    ty.clone(),
                );
                let expected = if domain.signed {
                    -(1i128 << (domain.width - 1))
                } else {
                    0
                };
                assert_eq!(
                    number(&eval(&wrapped, &ty, width).unwrap(), width).unwrap(),
                    expected
                );
                let checked = bin(BinaryOp::Add, lit(&max.to_string()), lit("1"), ty.clone());
                assert!(eval(&checked, &ty, width).is_err());
                let shift = bin(
                    BinaryOp::ShiftLeft,
                    lit("1"),
                    lit(&domain.width.to_string()),
                    ty.clone(),
                );
                assert!(eval(&shift, &ty, width).is_err());
            }
        }
        assert!(
            eval(
                &DynConstPattern::Value(Constant::Isize(i64::MAX)),
                &Type::Isize,
                32
            )
            .is_err()
        );
        assert!(
            eval(
                &DynConstPattern::Value(Constant::Usize(u64::MAX)),
                &Type::Usize,
                32
            )
            .is_err()
        );
    }
    #[test]
    fn signed_minimum_literal_and_checked_remainder_are_not_evaluated_in_a_wider_domain() {
        for ty in [Type::I8, Type::I16, Type::I32, Type::I64, Type::Isize] {
            for width in [32, 64] {
                let domain = integer(&ty, width).unwrap();
                let magnitude = 1i128 << (domain.width - 1);
                let min = DynConstPattern::Unary {
                    op: UnaryOp::Negate,
                    value: Box::new(lit(&magnitude.to_string())),
                    ty: Box::new(DynTypePattern::Concrete(ty.clone())),
                };
                assert_eq!(
                    number(&eval(&min, &ty, width).unwrap(), width).unwrap(),
                    -magnitude
                );
                let minus_one = DynConstPattern::Unary {
                    op: UnaryOp::Negate,
                    value: Box::new(lit("1")),
                    ty: Box::new(DynTypePattern::Concrete(ty.clone())),
                };
                for op in [BinaryOp::Divide, BinaryOp::Remainder] {
                    assert!(
                        eval(
                            &bin(op, min.clone(), minus_one.clone(), ty.clone()),
                            &ty,
                            width
                        )
                        .is_err()
                    );
                }
            }
        }
        assert!(eval(&lit("255u8"), &Type::U16, 64).is_err());
    }
    #[test]
    fn generic_literal_carrier_controls_ieee_rounding_and_comparison_operand_types() {
        let literal = DynConstPattern::FloatLiteral("0.1".into());
        assert_eq!(
            eval(&literal, &Type::F32, 64).unwrap(),
            Constant::F32(0x3dcccccd)
        );
        assert_eq!(
            eval(&literal, &Type::F64, 64).unwrap(),
            Constant::F64(0x3fb999999999999a)
        );
        let compare = bin(BinaryOp::Less, lit("127"), lit("1"), Type::I8);
        assert_eq!(
            eval(&compare, &Type::Bool, 64).unwrap(),
            Constant::Bool(false)
        );
        let compare = bin(BinaryOp::Less, lit("128"), lit("1"), Type::I8);
        assert!(eval(&compare, &Type::Bool, 64).is_err());
        let rounded = bin(
            BinaryOp::Add,
            DynConstPattern::FloatLiteral("16777216.0".into()),
            DynConstPattern::FloatLiteral("1.0".into()),
            Type::F32,
        );
        assert_eq!(
            eval(&rounded, &Type::F32, 64).unwrap(),
            Constant::F32(0x4b800000)
        );
        let negative_zero = DynConstPattern::Unary {
            op: UnaryOp::Negate,
            value: Box::new(DynConstPattern::FloatLiteral("0.0".into())),
            ty: Box::new(DynTypePattern::Concrete(Type::F64)),
        };
        assert_eq!(
            eval(&negative_zero, &Type::F64, 64).unwrap(),
            Constant::F64(0x8000000000000000)
        );
    }
    #[test]
    fn explicit_conversion_and_original_carrier_mismatch_never_reinterpret_bits() {
        let convert = DynConstPattern::Convert {
            value: Box::new(lit("256u16")),
            ty: Box::new(DynTypePattern::Concrete(Type::U8)),
        };
        assert!(eval(&convert, &Type::U8, 64).is_err());
        let convert = DynConstPattern::Convert {
            value: Box::new(DynConstPattern::FloatLiteral("1.75f32".into())),
            ty: Box::new(DynTypePattern::Concrete(Type::I16)),
        };
        assert_eq!(eval(&convert, &Type::I16, 64).unwrap(), Constant::I16(1));
        assert!(eval(&DynConstPattern::Value(Constant::U8(1)), &Type::I8, 64).is_err());
        assert!(
            closed(
                &DynConstPattern::Parameter(0),
                &Type::I32,
                64,
                &mut Budget::new(&Limits::default())
            )
            .unwrap()
            .is_none()
        );
    }
}
