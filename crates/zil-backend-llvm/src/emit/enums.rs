//! Tagged unions use one aligned max-payload area. All payload bytes start at
//! zero; recursive scalar stores preserve padding and bool's spare bits.
use super::*;

#[derive(Clone, Copy)]
struct Layout {
    size: u64,
    align: u32,
}
#[derive(Clone, Copy)]
pub(super) struct UnionLayout {
    pub words: u64,
    pub word_align: u32,
    pub align: u32,
}
fn overflow<T>() -> CompileResult<T> {
    Err(vec![Diagnostic::limit(
        Phase::Codegen,
        "enum physical layout exceeds target address range",
        None,
    )])
}
fn rounded(size: u64, align: u32) -> CompileResult<u64> {
    size.checked_add(u64::from(align) - 1)
        .map(|n| n / u64::from(align) * u64::from(align))
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "enum layout size overflow",
                None,
            )]
        })
}
fn fields_layout<'a>(fields: impl IntoIterator<Item = &'a Type>) -> CompileResult<Layout> {
    let mut result = Layout { size: 0, align: 1 };
    for field in fields {
        let layout = physical_layout(field)?;
        result.align = result.align.max(layout.align);
        result.size = rounded(result.size, layout.align)?
            .checked_add(layout.size)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Codegen,
                    "enum payload size overflow",
                    None,
                )]
            })?;
    }
    result.size = rounded(result.size, result.align)?;
    Ok(result)
}
fn physical_layout(ty: &Type) -> CompileResult<Layout> {
    Ok(match ty {
        Type::Unit => Layout { size: 0, align: 1 },
        Type::Bool | Type::U8 => Layout { size: 1, align: 1 },
        Type::U16 => Layout { size: 2, align: 2 },
        Type::I32 => Layout { size: 4, align: 4 },
        Type::I64
        | Type::FunctionPointer { .. }
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ForeignCallback { .. } => Layout { size: 8, align: 8 },
        Type::Tuple(fields) => fields_layout(fields)?,
        Type::Record { fields, .. } => fields_layout(fields.iter().map(|(_, ty)| ty))?,
        Type::ErrorUnion { error, result } => {
            fields_layout([&Type::Bool, error.as_ref(), result.as_ref()])?
        }
        Type::Array { element, length } => {
            let element = physical_layout(element)?;
            let Some(size) = element
                .size
                .checked_mul(u64::from(*length))
                .filter(|n| *n <= i64::MAX as u64)
            else {
                return overflow();
            };
            Layout {
                size,
                align: element.align,
            }
        }
        Type::Enum { .. } => {
            let layout = union_layout(ty)?;
            let payload = layout
                .words
                .checked_mul(u64::from(layout.word_align))
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Codegen,
                        "enum payload size overflow",
                        None,
                    )]
                })?;
            let Some(size) = rounded(4, layout.word_align)?.checked_add(payload) else {
                return overflow();
            };
            Layout {
                size: rounded(size, layout.align)?,
                align: layout.align,
            }
        }
        _ => return unsupported("non-Copy enum payload physical layout", None),
    })
}
pub(super) fn union_layout(ty: &Type) -> CompileResult<UnionLayout> {
    let Type::Enum { variants, .. } = ty else {
        return unsupported("union layout needs enum type", None);
    };
    let mut bytes = 0;
    let mut align = 1;
    for (_, fields) in variants {
        let layout = fields_layout(fields)?;
        bytes = bytes.max(layout.size);
        align = align.max(layout.align);
    }
    Ok(UnionLayout {
        words: rounded(bytes, align)? / u64::from(align),
        word_align: align,
        align: align.max(4),
    })
}
pub(super) fn llvm_enum_type(ty: &Type) -> CompileResult<m::Ty> {
    let layout = union_layout(ty)?;
    Ok(m::Ty::Struct(vec![
        m::Ty::Int(32),
        m::Ty::array(layout.words, m::Ty::Int(layout.word_align * 8)),
    ]))
}

impl FunctionEmitter<'_> {
    pub(super) fn enum_slot(&mut self, instruction: &Instruction) -> CompileResult<()> {
        let (ty, result) = match &instruction.kind {
            InstructionKind::MakeVariant { .. } => {
                let result = result(instruction)?;
                (&self.function.values[result.0 as usize], result)
            }
            InstructionKind::ReadVariant { value, .. } => (
                &self.function.values[value.0 as usize],
                result(instruction)?,
            ),
            _ => return Ok(()),
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("enum.slot.{}", result.0)),
            m::Op::Alloca {
                ty: (llvm_type(ty)?).clone(),
                align: (union_layout(ty)?.align) as u32,
            },
        )));
        Ok(())
    }
    pub(super) fn make_variant(
        &mut self,
        instruction: &Instruction,
        variant: u32,
        fields: &[ValueId],
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let ty = self.function.values[result.0 as usize].clone();
        let Type::Enum { variants, .. } = &ty else {
            return unsupported("verified variant lost enum type", Some(instruction.span));
        };
        let payload = Type::Tuple(variants[variant as usize].1.clone());
        let llvm = llvm_type(&ty)?;
        let layout = union_layout(&ty)?;
        let slot = format!("%enum.slot.{}", result.0);
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new((llvm).clone(), m::Val::Zero),
                ptr: m::val((slot).clone()),
                align: (layout.align) as u32,
            },
        )));
        let tag = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag).clone())),
            m::Op::Gep {
                ty: (llvm).clone(),
                ptr: m::val((slot).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(32), m::val(variant)),
                ptr: m::val((tag).clone()),
                align: 4,
            },
        )));
        let base = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((base).clone())),
            m::Op::Gep {
                ty: (llvm).clone(),
                ptr: m::val((slot).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                ],
            },
        )));
        for (index, field) in fields.iter().enumerate() {
            let field_ty = &variants[variant as usize].1[index];
            if *field_ty == Type::Unit {
                continue;
            }
            let address = self.temp();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((address).clone())),
                m::Op::Gep {
                    ty: (llvm_type(&payload)?).clone(),
                    ptr: m::val((base).clone()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::val(index)),
                    ],
                },
            )));
            self.store_payload(field_ty, &value(*field), &address)?;
        }
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result)).clone())),
            m::Op::Load {
                ty: (llvm).clone(),
                ptr: m::val((slot).clone()),
                align: (layout.align) as u32,
            },
        )));
        Ok(())
    }
    pub(super) fn variant_tag(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
    ) -> CompileResult<()> {
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result(instruction)?)).clone())),
            m::Op::Extract {
                arg: m::Arg::new(
                    (llvm_type(&self.function.values[source.0 as usize])?).clone(),
                    m::val((value(source)).clone()),
                ),
                index: 0,
            },
        )));
        Ok(())
    }
    pub(super) fn read_variant(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
        variant: u32,
        index: u32,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let ty = self.function.values[source.0 as usize].clone();
        let Type::Enum { variants, .. } = &ty else {
            return unsupported("verified projection lost enum type", Some(instruction.span));
        };
        let fields = &variants[variant as usize].1;
        let llvm = llvm_type(&ty)?;
        let tag = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag).clone())),
            m::Op::Extract {
                arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                index: 0,
            },
        )));
        let mismatch = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((mismatch).clone())),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val((tag).clone()),
                right: m::val(variant),
            },
        )));
        self.trap_if(&mismatch);
        if fields[index as usize] == Type::Unit {
            return Ok(());
        }
        let slot = format!("%enum.slot.{}", result.0);
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                ptr: m::val((slot).clone()),
                align: (union_layout(&ty)?.align) as u32,
            },
        )));
        let base = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((base).clone())),
            m::Op::Gep {
                ty: (llvm).clone(),
                ptr: m::val((slot).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                ],
            },
        )));
        let address = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((address).clone())),
            m::Op::Gep {
                ty: (llvm_type(&Type::Tuple(fields.clone()))?).clone(),
                ptr: m::val((base).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result)).clone())),
            m::Op::Load {
                ty: (llvm_type(&fields[index as usize])?).clone(),
                ptr: m::val((address).clone()),
                align: (physical_layout(&fields[index as usize])?.align) as u32,
            },
        )));
        Ok(())
    }
    fn store_payload(&mut self, ty: &Type, source: &str, address: &str) -> CompileResult<()> {
        match ty {
            Type::Unit => {}
            Type::Bool => {
                let widened = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((widened).clone())),
                    m::Op::Cast {
                        opcode: "zext".into(),
                        arg: m::Arg::new(m::Ty::Int(1), m::val(source)),
                        to: m::Ty::Int(8),
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(8), m::val((widened).clone())),
                        ptr: m::val(address),
                        align: 1,
                    },
                )));
            }
            Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::FunctionPointer { .. }
            | Type::ForeignHandle
            | Type::CWideString
            | Type::ForeignCallback { .. } => self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new((llvm_type(ty)?).clone(), m::val(source)),
                    ptr: m::val(address),
                    align: (physical_layout(ty)?.align) as u32,
                },
            ))),
            Type::Enum { .. } => {
                let llvm = llvm_type(ty)?;
                let layout = union_layout(ty)?;
                let tag = self.temp();
                let tag_address = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((tag).clone())),
                    m::Op::Extract {
                        arg: m::Arg::new((llvm).clone(), m::val(source)),
                        index: 0,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((tag_address).clone())),
                    m::Op::Gep {
                        ty: (llvm).clone(),
                        ptr: m::val(address),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(32), m::val((tag).clone())),
                        ptr: m::val((tag_address).clone()),
                        align: 4,
                    },
                )));
                if layout.words != 0 {
                    let payload = self.temp();
                    let payload_address = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((payload).clone())),
                        m::Op::Extract {
                            arg: m::Arg::new((llvm).clone(), m::val(source)),
                            index: 1,
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((payload_address).clone())),
                        m::Op::Gep {
                            ty: (llvm).clone(),
                            ptr: m::val(address),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                            ],
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(
                                m::Ty::array(
                                    (layout.words) as u64,
                                    m::Ty::Int((layout.word_align * 8) as u32),
                                ),
                                m::val((payload).clone()),
                            ),
                            ptr: m::val((payload_address).clone()),
                            align: (layout.word_align) as u32,
                        },
                    )));
                }
            }
            Type::ErrorUnion { error, result } => {
                let llvm = llvm_type(ty)?;
                for (index, field) in [&Type::Bool, error.as_ref(), result.as_ref()]
                    .into_iter()
                    .enumerate()
                {
                    if *field == Type::Unit {
                        continue;
                    }
                    let projected = self.temp();
                    let field_address = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((projected).clone())),
                        m::Op::Extract {
                            arg: m::Arg::new((llvm).clone(), m::val(source)),
                            index: (index) as u32,
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((field_address).clone())),
                        m::Op::Gep {
                            ty: (llvm).clone(),
                            ptr: m::val(address),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(index)),
                            ],
                        },
                    )));
                    self.store_payload(field, &projected, &field_address)?;
                }
            }
            _ => {
                let fields = ty.aggregate_fields().ok_or_else(|| {
                    vec![Diagnostic::unsupported(
                        Phase::Codegen,
                        "non-Copy enum field store",
                        None,
                    )]
                })?;
                let llvm = llvm_type(ty)?;
                for (index, field) in fields.iter().enumerate() {
                    if **field == Type::Unit {
                        continue;
                    }
                    let projected = self.temp();
                    let field_address = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((projected).clone())),
                        m::Op::Extract {
                            arg: m::Arg::new((llvm).clone(), m::val(source)),
                            index: (index) as u32,
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((field_address).clone())),
                        m::Op::Gep {
                            ty: (llvm).clone(),
                            ptr: m::val(address),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(index)),
                            ],
                        },
                    )));
                    self.store_payload(field, &projected, &field_address)?;
                }
            }
        }
        Ok(())
    }
}
