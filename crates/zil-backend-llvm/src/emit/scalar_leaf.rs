//! Compact emission for a currently verified parameter-plus-literal leaf.
//! Recognition has no effects; every other shape uses the ordinary emitter.
use super::*;

const MAX_VALUES: usize = 32;
const MAX_INSTRUCTIONS: usize = 32;

#[derive(Clone, Copy)]
enum Operand {
    Parameter,
    Literal(i128),
    Sum,
}

#[derive(Clone, Copy)]
pub(super) struct Prepared {
    source: FunctionId,
    operation: &'static str,
    bits: u32,
    result: ValueId,
    literal: i128,
    literal_first: bool,
}

pub(super) fn emit(
    source: &TypedFunction,
    intrinsics: &mut BTreeSet<(&'static str, u32)>,
) -> Option<m::Function> {
    Some(prepare(source)?.emit(intrinsics))
}

pub(super) fn prepare(source: &TypedFunction) -> Option<Prepared> {
    if source.foreign.is_some()
        || source.environment.is_some()
        || source.parameters.len() != 1
        || source.locals.len() != 1
        || source.blocks.len() != 1
        || source.values.len() > MAX_VALUES
    {
        return None;
    }
    let (operation, bits) = match &source.return_type {
        Type::I32 => ("sadd", 32),
        Type::I64 => ("sadd", 64),
        Type::U8 => ("uadd", 8),
        Type::U16 => ("uadd", 16),
        _ => return None,
    };
    let parameter = source.parameters[0];
    let local = &source.locals[0];
    let block = &source.blocks[0];
    if parameter != LocalId(0)
        || local.id != parameter
        || local.ty != source.return_type
        || block.id.0 != 0
        || source.entry != block.id
        || block.instructions.len() > MAX_INSTRUCTIONS
        || source.values.iter().any(|ty| *ty != source.return_type)
    {
        return None;
    }
    let mut values = [None; MAX_VALUES];
    let mut addition = None;
    for instruction in &block.instructions {
        let operand = match &instruction.kind {
            InstructionKind::Constant(constant) if constant.ty() == source.return_type => {
                Operand::Literal(match constant {
                    Constant::I32(n) => i128::from(*n),
                    Constant::I64(n) => i128::from(*n),
                    Constant::U8(n) => i128::from(*n),
                    Constant::U16(n) => i128::from(*n),
                    _ => return None,
                })
            }
            InstructionKind::Load(id) if *id == parameter => Operand::Parameter,
            InstructionKind::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } if addition.is_none() => {
                let left = values.get(left.0 as usize).copied().flatten()?;
                let right = values.get(right.0 as usize).copied().flatten()?;
                let (literal, literal_first) = match (left, right) {
                    (Operand::Parameter, Operand::Literal(n)) => (n, false),
                    (Operand::Literal(n), Operand::Parameter) => (n, true),
                    _ => return None,
                };
                addition = Some((instruction.result?, literal, literal_first));
                Operand::Sum
            }
            InstructionKind::StorageLive(id) | InstructionKind::StorageDead(id)
                if *id == parameter && instruction.result.is_none() =>
            {
                continue;
            }
            InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
                if instruction.result.is_none() =>
            {
                continue;
            }
            _ => return None,
        };
        let result = instruction.result?.0 as usize;
        if source.values.get(result)? != &source.return_type
            || values.get_mut(result)?.replace(operand).is_some()
        {
            return None;
        }
    }
    let (result, literal, literal_first) = addition?;
    if !matches!(&block.terminator, Terminator::Return(Some(returned)) if *returned == result)
        || values[..source.values.len()].iter().any(Option::is_none)
    {
        return None;
    }

    Some(Prepared {
        source: source.id,
        operation,
        bits,
        result,
        literal,
        literal_first,
    })
}

impl Prepared {
    pub(super) fn emit(self, intrinsics: &mut BTreeSet<(&'static str, u32)>) -> m::Function {
        let Self {
            source,
            operation,
            bits,
            result,
            literal,
            literal_first,
        } = self;
        // Qualification is complete. Keep the ordinary checked operation and its
        // failure edge, but never construct the disposable source entry/constant.
        let scalar = m::Ty::Int(bits);
        let pair = m::Ty::Struct(vec![scalar.clone(), m::Ty::Int(1)]);
        let parameter = m::Arg::new(scalar.clone(), m::local("arg.0".into()));
        let literal = m::Arg::new(scalar.clone(), m::Val::Int(literal));
        let arguments = if literal_first {
            vec![literal, parameter]
        } else {
            vec![parameter, literal]
        };
        let result = format!("v.{}", result.0);
        let function = m::Function {
            name: symbol(source),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                scalar.clone(),
                vec![m::Param::new(scalar.clone(), "arg.0")],
            ),
            attrs: vec![],
            blocks: Some(vec![
                m::Block {
                    name: "entry".into(),
                    instructions: vec![
                        m::Inst::new(
                            Some("tmp.0".into()),
                            m::Op::Call {
                                result: pair.clone(),
                                result_attrs: vec![],
                                callee: m::global(format!(
                                    "llvm.{operation}.with.overflow.i{bits}"
                                )),
                                args: arguments,
                                vararg: None,
                            },
                        ),
                        m::Inst::new(
                            Some(result.clone()),
                            m::Op::Extract {
                                arg: m::Arg::new(pair.clone(), m::local("tmp.0".into())),
                                index: 0,
                            },
                        ),
                        m::Inst::new(
                            Some("tmp.1".into()),
                            m::Op::Extract {
                                arg: m::Arg::new(pair, m::local("tmp.0".into())),
                                index: 1,
                            },
                        ),
                        m::Inst::new(
                            None,
                            m::Op::CondBranch {
                                condition: m::local("tmp.1".into()),
                                yes: "zil.trap".into(),
                                no: "checked.0".into(),
                            },
                        ),
                    ],
                },
                m::Block {
                    name: "checked.0".into(),
                    instructions: vec![m::Inst::new(
                        None,
                        m::Op::Return(Some(m::Arg::new(scalar, m::local(result)))),
                    )],
                },
                m::Block {
                    name: "zil.trap".into(),
                    instructions: vec![
                        m::Inst::new(
                            None,
                            m::Op::Call {
                                result: m::Ty::Void,
                                result_attrs: vec![],
                                callee: m::global("llvm.trap".into()),
                                args: vec![],
                                vararg: None,
                            },
                        ),
                        m::Inst::new(None, m::Op::Unreachable),
                    ],
                },
            ]),
        };
        intrinsics.insert((operation, bits));
        function
    }
}
