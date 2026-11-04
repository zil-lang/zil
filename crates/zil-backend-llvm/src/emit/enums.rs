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
    fields_layout_with_offsets(fields, |_| {})
}
fn fields_layout_with_offsets<'a>(
    fields: impl IntoIterator<Item = &'a Type>,
    mut record_offset: impl FnMut(u64),
) -> CompileResult<Layout> {
    let mut result = Layout { size: 0, align: 1 };
    for field in fields {
        let layout = physical_layout(field)?;
        result.align = result.align.max(layout.align);
        let offset = rounded(result.size, layout.align)?;
        record_offset(offset);
        result.size = offset
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
        Type::I32 | Type::F32 => Layout { size: 4, align: 4 },
        Type::I64
        | Type::U64
        | Type::Usize
        | Type::F64
        | Type::FunctionPointer { .. }
        | Type::BorrowedFunctionPointer { .. }
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ForeignCallback { .. } => Layout { size: 8, align: 8 },
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }) => {
            Layout { size: 16, align: 8 }
        }
        Type::Address { inner, .. }
            if inner.is_copy_value()
                || matches!(inner.as_ref(), Type::Closure(_))
                || resource_head_address(ty)
                || ty.is_exclusive_task_head_address()
                || ty.is_package_address() =>
        {
            Layout { size: 8, align: 8 }
        }
        Type::Desc(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer
                || matches!(payload.as_ref(), Type::TaskI32 | Type::Task { .. })
                || matches!(payload.as_ref(), Type::Registration { .. })
                || matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
        {
            Layout { size: 8, align: 8 }
        }
        Type::Tuple(fields) => fields_layout(fields)?,
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields_layout(fields.iter().map(|(_, ty)| ty))?
        }
        Type::Managed(payload) => physical_layout(payload)?,
        Type::Registration { .. } | Type::Task { .. } | Type::TaskI32 => Layout { size: 8, align: 8 },
        Type::ByteBuffer | Type::OwnedStore { .. } => Layout { size: 24, align: 8 },
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
/// Read the existing x64 LLVM value layout of a complete, driver-checked shape.
/// This grants no nominal, visibility, ownership or address permission. Closures
/// need an environment table and unresolved/opaque types must be resolved first.
/// Offsets describe only Tuple/Record/Package source fields, including Unit.
/// The preflight borrows all input. Node/name work and output storage are paid
/// before use. The recursive backend supports the full default depth (256);
/// raising Limits beyond that ceiling yields an explicit ResourceLimit rather
/// than risking an unbounded compiler stack.
pub fn ct_layout_of_checked_type(
    ty: &Type,
    target: &TargetProfile,
    limits: &zil_base::Limits,
    charge: &mut impl FnMut(usize) -> CompileResult<()>,
) -> CompileResult<(u64, u64, Vec<u64>)> {
    charge(1)?;
    if !target.is_supported_x64_llvm() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Ct,
            "CT layout supports the existing checked x64 LLVM targets",
            None,
        )]);
    }
    let mut walk = CtLayoutPreflight {
        limits,
        charge,
        nodes: 0,
        name_bytes: 0,
    };
    walk.ty(ty, 0, false)?;
    let fields = match ty {
        Type::Tuple(fields) => fields.len(),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
        _ => 0,
    };
    let bytes = fields
        .checked_mul(std::mem::size_of::<u64>())
        .filter(|bytes| *bytes <= limits.max_source_bytes)
        .ok_or_else(ct_layout_limit)?;
    (walk.charge)(bytes)?;
    let mut offsets = Vec::new();
    offsets
        .try_reserve_exact(fields)
        .map_err(|_| ct_layout_limit())?;
    let layout = match ty {
        Type::Tuple(fields) => fields_layout_with_offsets(fields, |n| offsets.push(n))?,
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields_layout_with_offsets(fields.iter().map(|(_, ty)| ty), |n| offsets.push(n))?
        }
        _ => physical_layout(ty)?,
    };
    Ok((layout.size, u64::from(layout.align), offsets))
}

fn ct_layout_limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT layout node/depth/name/output budget exhausted",
        None,
    )]
}

struct CtLayoutPreflight<'a, C> {
    limits: &'a zil_base::Limits,
    charge: &'a mut C,
    nodes: usize,
    name_bytes: usize,
}
impl<C: FnMut(usize) -> CompileResult<()>> CtLayoutPreflight<'_, C> {
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            return Err(ct_layout_limit());
        }
        if depth > zil_base::Limits::default().max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT layout exceeds the recursive backend stack ceiling (default max_call_depth)",
                None,
            )]);
        }
        // Covers this borrowed visit, bounded recursion and all later ancestor
        // shape predicates/layout visits. No type/name is cloned or formatted.
        (self.charge)(4 * (depth + 1) + std::mem::size_of::<(&Type, usize)>())?;
        self.nodes += 1;
        Ok(())
    }
    fn text(&mut self, text: &str) -> CompileResult<()> {
        let bytes = self
            .name_bytes
            .checked_add(text.len())
            .filter(|n| *n <= self.limits.max_source_bytes)
            .ok_or_else(ct_layout_limit)?;
        (self.charge)(text.len())?;
        self.name_bytes = bytes;
        Ok(())
    }
    fn ty(&mut self, ty: &Type, depth: usize, slice_pointee: bool) -> CompileResult<()> {
        self.node(depth)?;
        match ty {
            Type::Named(name) | Type::Unsupported(name) => {
                self.text(name)?;
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT layout needs a resolved ordinary type",
                    None,
                )]);
            }
            Type::Registration { .. } | Type::Closure(_) | Type::Opaque { .. } => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT layout needs a resolved shape and closure environment",
                    None,
                )]);
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.ty(field, depth + 1, false)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.text(name)?;
                for (name, field) in fields {
                    self.text(name)?;
                    self.ty(field, depth + 1, false)?;
                }
            }
            Type::Enum { name, variants } => {
                self.text(name)?;
                for (name, fields) in variants {
                    self.node(depth + 1)?;
                    self.text(name)?;
                    for field in fields {
                        self.ty(field, depth + 1, false)?;
                    }
                }
            }
            Type::Array { element, .. } | Type::OwnedStore { element } => {
                self.ty(element, depth + 1, false)?;
            }
            Type::Slice { element } => {
                if !slice_pointee {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "CT layout has no standalone unsized Slice value",
                        None,
                    )]);
                }
                self.ty(element, depth + 1, false)?;
            }
            Type::Address { inner, .. } => {
                self.ty(
                    inner,
                    depth + 1,
                    matches!(inner.as_ref(), Type::Slice { .. }),
                )?;
            }
            Type::Managed(inner) | Type::Desc(inner) => {
                self.ty(inner, depth + 1, false)?;
                if !((matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value())
                    || inner.as_ref() == &Type::ByteBuffer
                    || matches!(inner.as_ref(), Type::OwnedStore { .. }))
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Ct,
                        "CT layout needs an existing LLVM resource payload",
                        None,
                    )]);
                }
            }
            Type::ErrorUnion { error, result } => {
                self.ty(error, depth + 1, false)?;
                self.ty(result, depth + 1, false)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.ty(parameter, depth + 1, false)?;
                }
                self.ty(result, depth + 1, false)?;
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::ByteBuffer
            | Type::Task { .. }
                        | Type::TaskI32
            | Type::SyncCell(_)
            | Type::ForeignHandle
            | Type::CWideString
            | Type::U64
            | Type::Usize => {}
        }
        Ok(())
    }
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
        if match &instruction.kind {
            InstructionKind::MakeVariant { .. } => instruction
                .result
                .is_some_and(|id| self.copy_values.contains_key(&id)),
            InstructionKind::ReadVariant { value, .. } => self.copy_values.contains_key(value),
            _ => false,
        } {
            return Ok(());
        }
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
        let memory = self.copy_values.contains_key(&result);
        let slot = if memory {
            self.copy_zero(result);
            format!("%copy.value.{}", result.0)
        } else {
            let slot = format!("%enum.slot.{}", result.0);
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new((llvm).clone(), m::Val::Zero),
                    ptr: m::val((slot).clone()),
                    align: (layout.align) as u32,
                },
            )));
            slot
        };
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
            if memory && self.copy_values.contains_key(field) {
                self.copy_write(m::val(address), *field)?;
            } else {
                self.store_payload(field_ty, &value(*field), &address)?;
            }
        }
        if memory {
            return Ok(());
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
        if self.copy_values.contains_key(&source) {
            let address = self.copy_field(source, m::Arg::new(m::Ty::Int(32), 0));
            self.event(m::Event::Instruction(m::Inst::new(
                Some(format!("v.{}", result(instruction)?.0)),
                m::Op::Load {
                    ty: m::Ty::Int(32),
                    ptr: address,
                    align: 4,
                },
            )));
            return Ok(());
        }
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
        let memory = self.copy_values.contains_key(&source);
        let tag_read = if memory {
            let address = self.copy_field(source, m::Arg::new(m::Ty::Int(32), 0));
            m::Op::Load {
                ty: m::Ty::Int(32),
                ptr: address,
                align: 4,
            }
        } else {
            m::Op::Extract {
                arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                index: 0,
            }
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag).clone())),
            tag_read,
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
        let slot = if memory {
            format!("%copy.value.{}", source.0)
        } else {
            let slot = format!("%enum.slot.{}", result.0);
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                    ptr: m::val((slot).clone()),
                    align: (union_layout(&ty)?.align) as u32,
                },
            )));
            slot
        };
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
        self.copy_read(result, m::val(address))?;
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
            Type::Registration { .. }
            | Type::Task { .. }
                        | Type::TaskI32
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::F32
            | Type::F64
            | Type::U64
            | Type::Usize
            | Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. }
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
            Type::Managed(payload) => self.store_payload(payload, source, address)?,
            Type::ByteBuffer | Type::OwnedStore { .. } => {
                let llvm = llvm_type(ty)?;
                for (index, field_ty) in [m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]
                    .into_iter()
                    .enumerate()
                {
                    let projected = self.temp();
                    let target = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name(projected.clone())),
                        m::Op::Extract {
                            arg: m::Arg::new(llvm.clone(), m::val(source)),
                            index: index as u32,
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name(target.clone())),
                        m::Op::Gep {
                            ty: llvm.clone(),
                            ptr: m::val(address),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), 0),
                                m::Arg::new(m::Ty::Int(32), index as u32),
                            ],
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(field_ty, m::val(projected)),
                            ptr: m::val(target),
                            align: 8,
                        },
                    )));
                }
            }
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
                let fields = match ty {
                    Type::Package { fields, .. } => Some(fields.iter().map(|(_, t)| t).collect()),
                    _ => ty.aggregate_fields(),
                }
                .ok_or_else(|| {
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

#[cfg(test)]
mod impl243_layout_tests {
    use super::*;
    fn buffer() -> Type {
        Type::Managed(Box::new(Type::ByteBuffer))
    }
    fn store() -> Type {
        Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(Type::Package {
                name: "Element".into(),
                fields: vec![("bytes".into(), buffer())],
            }),
        }))
    }
    #[test]
    fn impl243_enum_managed_headers_keep_aligned_max_payload_abi() {
        let ty = Type::Enum {
            name: "Headers".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                ("Pair".into(), vec![buffer(), store()]),
            ],
        };
        let layout = union_layout(&ty).unwrap();
        assert_eq!((layout.words, layout.word_align, layout.align), (6, 8, 8));
        assert_eq!(
            llvm_enum_type(&ty).unwrap().to_string(),
            "{ i32, [6 x i64] }"
        );
        assert_eq!(
            (
                physical_layout(&ty).unwrap().size,
                physical_layout(&ty).unwrap().align
            ),
            (56, 8)
        );
        // Headers are three actual words; treating Managed as ptr gives 24, not56.
        let error = Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(ty),
        };
        assert_eq!(
            llvm_type(&error).unwrap().to_string(),
            "{ i1, i32, { i32, [6 x i64] } }"
        );
        assert_eq!(physical_layout(&error).unwrap().size, 64);
    }
    #[test]
    fn impl243_product_sum_nested_layout_preserves_field_offsets_and_bool_storage() {
        let inner = Type::Enum {
            name: "Inner".into(),
            variants: vec![("None".into(), vec![]), ("Some".into(), vec![buffer()])],
        };
        let product = Type::Package {
            name: "Product".into(),
            fields: vec![("bool".into(), Type::Bool), ("sum".into(), inner)],
        };
        let outer = Type::Enum {
            name: "Outer".into(),
            variants: vec![("Value".into(), vec![product])],
        };
        assert_eq!(
            (
                union_layout(&outer).unwrap().words,
                physical_layout(&outer).unwrap().size
            ),
            (5, 48)
        );
    }
}

#[cfg(test)]
mod ct_layout_tests {
    use super::*;
    use zil_base::{Limits, ResultCategory};

    fn inspect(ty: &Type) -> (u64, u64, Vec<u64>) {
        ct_layout_of_checked_type(
            ty,
            &TargetProfile::windows_x64_llvm(),
            &Limits::default(),
            &mut |_| Ok(()),
        )
        .unwrap()
    }

    #[test]
    fn ct_layout_uses_actual_pointer_slice_and_resource_representations() {
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let slice = Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::I32),
            }),
        };
        let desc = Type::Desc(Box::new(Type::ByteBuffer));
        assert_eq!(llvm_type(&address).unwrap().to_string(), "ptr");
        assert_eq!(llvm_type(&slice).unwrap().to_string(), "{ ptr, i64 }");
        assert_eq!(llvm_type(&desc).unwrap().to_string(), "ptr");
        let package = Type::Package {
            name: "Headers".into(),
            fields: vec![
                ("tag".into(), Type::U8),
                ("p".into(), address),
                ("slice".into(), slice),
                ("desc".into(), desc),
                ("owned".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ],
        };
        assert_eq!(inspect(&package), (64, 8, vec![0, 8, 16, 32, 40]));
        let payload = Type::Record {
            name: "WideHeader".into(),
            fields: vec![("a".into(), Type::I64), ("b".into(), Type::I64)],
        };
        assert_eq!(inspect(&payload), (16, 8, vec![0, 8]));
        assert_eq!(inspect(&Type::Desc(Box::new(payload))), (8, 8, vec![]));
    }

    #[test]
    fn ct_layout_offsets_include_unit_but_do_not_flatten_sum_alternatives() {
        assert_eq!(
            inspect(&Type::Tuple(vec![
                Type::Unit,
                Type::Bool,
                Type::I64,
                Type::U16
            ])),
            (24, 8, vec![0, 0, 8, 16])
        );
        let sum = Type::Enum {
            name: "Choice".into(),
            variants: vec![
                ("Wide".into(), vec![Type::I64]),
                ("Pair".into(), vec![Type::I32, Type::I32]),
            ],
        };
        assert_eq!(llvm_type(&sum).unwrap().to_string(), "{ i32, [1 x i64] }");
        assert_eq!(inspect(&sum), (16, 8, vec![]));
        assert_eq!(
            inspect(&Type::ErrorUnion {
                error: Box::new(Type::I64),
                result: Box::new(Type::Bool)
            }),
            (24, 8, vec![])
        );
        assert_eq!(
            inspect(&Type::Array {
                element: Box::new(Type::I64),
                length: 3
            }),
            (24, 8, vec![])
        );
    }

    #[test]
    fn ct_layout_rejects_unresolved_environment_and_unsupported_target() {
        let incomplete = [
            Type::Named("Unresolved".into()),
            Type::Closure(ClosureId(0)),
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Named("Inner".into())),
            },
            Type::Slice {
                element: Box::new(Type::I32),
            },
            Type::Managed(Box::new(Type::I64)),
        ];
        for ty in incomplete {
            let error = ct_layout_of_checked_type(
                &ty,
                &TargetProfile::windows_x64_llvm(),
                &Limits::default(),
                &mut |_| Ok(()),
            )
            .unwrap_err();
            assert_eq!(error[0].category, ResultCategory::Unsupported);
        }
        let mut target = TargetProfile::windows_x64_llvm();
        target.pointer_width = 32;
        assert_eq!(
            ct_layout_of_checked_type(&Type::I32, &target, &Limits::default(), &mut |_| Ok(()))
                .unwrap_err()[0]
                .category,
            ResultCategory::Unsupported
        );
    }

    #[test]
    fn ct_layout_prepays_exact_budget_and_bounds_structure_and_names() {
        let target = TargetProfile::windows_x64_llvm();
        let ty = Type::Record {
            name: "R".into(),
            fields: vec![("flag".into(), Type::Bool), ("value".into(), Type::I64)],
        };
        let mut exact = 0;
        let expected = ct_layout_of_checked_type(&ty, &target, &Limits::default(), &mut |cost| {
            exact += cost;
            Ok(())
        })
        .unwrap();
        for (budget, pass) in [(exact, true), (exact - 1, false), (exact, true)] {
            let mut remaining = budget;
            let result = ct_layout_of_checked_type(&ty, &target, &Limits::default(), &mut |cost| {
                remaining = remaining.checked_sub(cost).ok_or_else(ct_layout_limit)?;
                Ok(())
            });
            if pass {
                assert_eq!(result.unwrap(), expected);
                assert_eq!(remaining, 0);
            } else {
                assert_eq!(
                    result.unwrap_err()[0].category,
                    ResultCategory::ResourceLimit
                );
            }
        }
        for limits in [
            Limits {
                max_nodes: 2,
                ..Limits::default()
            },
            Limits {
                max_call_depth: 0,
                ..Limits::default()
            },
            Limits {
                max_source_bytes: 4,
                ..Limits::default()
            },
        ] {
            assert_eq!(
                ct_layout_of_checked_type(&ty, &target, &limits, &mut |_| Ok(())).unwrap_err()[0]
                    .category,
                ResultCategory::ResourceLimit
            );
        }
        let mut deep = Type::I32;
        for _ in 0..Limits::default().max_call_depth {
            deep = Type::Array {
                element: Box::new(deep),
                length: 1,
            };
        }
        assert_eq!(inspect(&deep), (4, 4, vec![]));
        deep = Type::Array {
            element: Box::new(deep),
            length: 1,
        };
        assert_eq!(
            ct_layout_of_checked_type(
                &deep,
                &target,
                &Limits {
                    max_call_depth: usize::MAX,
                    ..Limits::default()
                },
                &mut |_| Ok(())
            )
            .unwrap_err()[0]
                .category,
            ResultCategory::ResourceLimit
        );
    }
}
