//! IEEE scalar operations and checked conversions. No fast-math flags.
use super::*;
use zil_base::numeric::{FloatFormat, FloatValue, IntegerValue};

fn constant(value: FloatValue) -> m::Val {
    match value {
        FloatValue::F32(bits) => m::Val::F32(bits),
        FloatValue::F64(bits) => m::Val::F64(bits),
    }
}
impl FunctionEmitter<'_> {
    pub(super) fn floating_binary(
        &mut self,
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
        output: ValueId,
    ) -> CompileResult<()> {
        let ty = llvm_type(self.value_type(left)?)?;
        let left = m::val(value(left));
        let right = m::val(value(right));
        let operation = match op {
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                m::Op::Binary {
                    opcode: match op {
                        BinaryOp::Add => "fadd",
                        BinaryOp::Subtract => "fsub",
                        BinaryOp::Multiply => "fmul",
                        _ => "fdiv",
                    }
                    .into(),
                    ty,
                    left,
                    right,
                }
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => m::Op::Compare {
                predicate: match op {
                    BinaryOp::Equal => "oeq",
                    BinaryOp::NotEqual => "une",
                    BinaryOp::Less => "olt",
                    BinaryOp::LessEqual => "ole",
                    BinaryOp::Greater => "ogt",
                    _ => "oge",
                }
                .into(),
                ty,
                left,
                right,
            },
            _ => {
                return invalid(
                    "E-CODEGEN-CFG",
                    "invalid floating operator",
                    Some(self.function.span),
                );
            }
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", output.0)),
            operation,
        )));
        Ok(())
    }
    fn float_compare(
        &mut self,
        predicate: &str,
        ty: &m::Ty,
        left: m::Val,
        right: m::Val,
    ) -> String {
        let name = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(name.clone())),
            m::Op::Compare {
                predicate: predicate.into(),
                ty: ty.clone(),
                left,
                right,
            },
        )));
        name
    }
    fn float_logic(&mut self, opcode: &str, left: m::Val, right: m::Val) -> String {
        let name = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(name.clone())),
            m::Op::Binary {
                opcode: opcode.into(),
                ty: m::Ty::Int(1),
                left,
                right,
            },
        )));
        name
    }
    fn float_cast(&mut self, opcode: &str, value: m::Val, from: &m::Ty, to: &m::Ty) -> String {
        let name = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(name.clone())),
            m::Op::Cast {
                opcode: opcode.into(),
                arg: m::Arg::new(from.clone(), value),
                to: to.clone(),
            },
        )));
        name
    }
    pub(super) fn convert_number(
        &mut self,
        input: ValueId,
        target: &Type,
        output: ValueId,
    ) -> CompileResult<()> {
        let source = self.value_type(input)?.clone();
        if !source.is_numeric() || !target.is_numeric() || !(source.is_float() || target.is_float())
        {
            return invalid(
                "E-CODEGEN-CFG",
                "invalid numeric conversion",
                Some(self.function.span),
            );
        }
        let from = llvm_type(&source)?;
        let to = llvm_type(target)?;
        let input = m::val(value(input));
        let converted = if target.is_float() {
            if source.is_integer() {
                self.float_cast(
                    if matches!(source, Type::U8 | Type::U16) {
                        "uitofp"
                    } else {
                        "sitofp"
                    },
                    input,
                    &from,
                    &to,
                )
            } else if source == *target {
                // A same-format conversion preserves the actual runtime bits.
                let result = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name(result.clone())),
                    m::Op::Freeze(m::Arg::new(from.clone(), input)),
                )));
                result
            } else {
                let converted = self.float_cast(
                    if source == Type::F32 {
                        "fpext"
                    } else {
                        "fptrunc"
                    },
                    input.clone(),
                    &from,
                    &to,
                );
                if source == Type::F64 {
                    let a = self.float_compare(
                        "ogt",
                        &from,
                        input.clone(),
                        m::Val::F64(0xfff0000000000000),
                    );
                    let b =
                        self.float_compare("olt", &from, input, m::Val::F64(0x7ff0000000000000));
                    let finite = self.float_logic("and", m::val(a), m::val(b));
                    let a = self.float_compare(
                        "oeq",
                        &to,
                        m::val(converted.clone()),
                        m::Val::F32(0x7f800000),
                    );
                    let b = self.float_compare(
                        "oeq",
                        &to,
                        m::val(converted.clone()),
                        m::Val::F32(0xff800000),
                    );
                    let infinite = self.float_logic("or", m::val(a), m::val(b));
                    let overflow = self.float_logic("and", m::val(finite), m::val(infinite));
                    self.trap_if(&overflow);
                }
                converted
            }
        } else {
            let (signed, bits) = match target {
                Type::U8 => (false, 8),
                Type::U16 => (false, 16),
                Type::I32 => (true, 32),
                Type::I64 => (true, 64),
                _ => {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "invalid numeric integer target",
                        Some(self.function.span),
                    );
                }
            };
            let format = if source == Type::F32 {
                FloatFormat::F32
            } else {
                FloatFormat::F64
            };
            let low = if signed {
                IntegerValue::Signed(-(1i128 << (bits - 1)))
            } else {
                IntegerValue::Unsigned(0)
            }
            .to_float(format);
            let high = IntegerValue::Unsigned(1u128 << if signed { bits - 1 } else { bits })
                .to_float(format);
            // Powers-of-two bounds are exact. This rejects NaN, infinity and
            // 2^63/2^64 before any potentially poison-producing LLVM cast.
            let a = self.float_compare("oge", &from, input.clone(), constant(low));
            let b = self.float_compare("olt", &from, input.clone(), constant(high));
            let valid = self.float_logic("and", m::val(a), m::val(b));
            let invalid = self.float_logic("xor", m::val(valid), m::Val::Bool(true));
            self.trap_if(&invalid);
            let converted = self.float_cast(
                if signed { "fptosi" } else { "fptoui" },
                input.clone(),
                &from,
                &to,
            );
            let roundtrip = self.float_cast(
                if signed { "sitofp" } else { "uitofp" },
                m::val(converted.clone()),
                &to,
                &from,
            );
            let integral = self.float_compare("oeq", &from, input, m::val(roundtrip));
            let invalid = self.float_logic("xor", m::val(integral), m::Val::Bool(true));
            self.trap_if(&invalid);
            converted
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", output.0)),
            m::Op::Freeze(m::Arg::new(to, m::val(converted))),
        )));
        Ok(())
    }
}
