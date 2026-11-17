//! Tagged unions use one aligned max-payload area. All payload bytes start at
//! zero; recursive scalar stores preserve padding and bool's spare bits.
use super::*;

/// Borrow only the machine storage view. The verified function/value types
/// retain their complete qualified metadata throughout lowering.
pub(super) fn storage_shape(ty: &Type) -> &Type {
    match ty {
        Type::BorrowedStorage { storage, .. } => storage,
        _ => ty,
    }
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
    let recipe=layout::checked_structural(ty,target,limits,&mut *walk.charge)?;
    if fields!=0{offsets.extend(recipe.fields.iter().map(|field|field.offset));}
    Ok((recipe.size, u64::from(recipe.align), offsets))
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
            Type::Mutex { .. } | Type::MutexGuard { .. } => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "mutex CT layout is not implemented",
                    None,
                )]);
            }
            Type::AsyncRegistration { .. } => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "pollable registration layout consumer is not implemented",
                    None,
                )]);
            }
            Type::BorrowedStorage { .. } => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "stored-borrow CT layout consumer is not yet implemented",
                    None,
                )]);
            }
            Type::Named(name) | Type::Unsupported(name) => {
                self.text(name)?;
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT layout needs a resolved ordinary type",
                    None,
                )]);
            }
            Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Task { .. }
            | Type::Registration { .. }
            | Type::Closure(_)
            | Type::Opaque { .. } => {
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
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::ByteBuffer
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




pub(super) fn zero_declaration()->m::Event {
    m::Event::Declare(m::Function{name:"llvm.memset.p0.i64".into(),linkage:m::Linkage::External,
        signature:m::Signature::new(m::Ty::Void,vec![m::Param::unnamed(m::Ty::Ptr),m::Param::unnamed(m::Ty::Int(8)),m::Param::unnamed(m::Ty::Int(64)),m::Param::unnamed(m::Ty::Int(1))]),attrs:vec![],blocks:None})
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
                ty: (self.layouts.value_type(ty)?).clone(),
                align: (self.layouts.union(ty)?.align) as u32,
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
        let Type::Enum { variants, .. } = storage_shape(&ty) else {
            return unsupported("verified variant lost enum type", Some(instruction.span));
        };
        let payload = Type::Tuple(variants[variant as usize].1.clone());
        let llvm = self.layouts.value_type(&ty)?;
        let layout = self.layouts.union(&ty)?;
        let memory = self.copy_values.contains_key(&result);
        let slot = if memory {
            format!("%copy.value.{}", result.0)
        } else {
            let slot = format!("%enum.slot.{}", result.0);
            slot
        };
        let recipe=self.layouts.recipe(&ty,layout::Position::Field)?;
        self.zero_payload(&recipe,&slot)?;
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
                    ty: (self.layouts.value_type(&payload)?).clone(),
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
                    (self.layouts.value_type(&self.function.values[source.0 as usize])?).clone(),
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
        let Type::Enum { variants, .. } = storage_shape(&ty) else {
            return unsupported("verified projection lost enum type", Some(instruction.span));
        };
        let fields = &variants[variant as usize].1;
        let llvm = self.layouts.value_type(&ty)?;
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
                    align: (self.layouts.union(&ty)?.align) as u32,
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
                ty: (self.layouts.value_type(&Type::Tuple(fields.clone()))?).clone(),
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
    /// A source value may contain undefined inactive fields or struct padding.
    /// Only active fields are copied into the already-zeroed payload words.
    /// The recipe is shared with sizing, capture flags, GEP and drop planning.
    fn store_payload(&mut self,ty:&Type,source:&str,address:&str)->CompileResult<()> {
        let recipe=self.layouts.recipe(ty,layout::Position::Field)?;
        self.store_recipe(&recipe,source,address)
    }
    fn payload_field(&mut self,machine:&m::Ty,address:&str,index:u32)->String {
        let out=self.temp();
        self.event(m::Event::Instruction(m::Inst::new(Some(m::name(out.clone())),m::Op::Gep{
            ty:machine.clone(),ptr:m::val(address),indices:vec![m::Arg::new(m::Ty::Int(32),0),m::Arg::new(m::Ty::Int(32),index)]
        })));out
    }
    fn payload_extract(&mut self,machine:&m::Ty,source:&str,index:u32)->String {
        let out=self.temp();self.event(m::Event::Instruction(m::Inst::new(Some(m::name(out.clone())),m::Op::Extract{arg:m::Arg::new(machine.clone(),m::val(source)),index})));out
    }
    fn payload_scratch(&mut self,recipe:&layout::Recipe)->CompileResult<String> {
        // Place scratch in the real entry block, even when the triggering
        // operation lives in a loop. No iteration grows the native stack.
        self.layouts.meter().spend(self.text.0.len().saturating_add(1))?;
        let out=self.temp();let at=self.text.0.iter().position(|event|matches!(event,m::Event::Label(name)if name=="entry")).ok_or_else(||vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-CFG",None,"payload scratch has no actual entry block")])?+1;
        self.text.0.insert(at,m::Event::Instruction(m::Inst::new(Some(m::name(out.clone())),m::Op::Alloca{ty:recipe.machine.clone(),align:recipe.align})));Ok(out)
    }
    fn zero_payload(&mut self,recipe:&layout::Recipe,address:&str)->CompileResult<()> {
        self.layouts.meter().spend(8)?;
        self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Call{result:m::Ty::Void,result_attrs:vec![],callee:m::global("llvm.memset.p0.i64".into()),args:vec![m::Arg::new(m::Ty::Ptr,m::val(address)),m::Arg::new(m::Ty::Int(8),0),m::Arg::new(m::Ty::Int(64),recipe.size),m::Arg::new(m::Ty::Int(1),false)],vararg:None})));Ok(())
    }
    fn store_recipe(&mut self,recipe:&layout::Recipe,source:&str,address:&str)->CompileResult<()> {
        self.layouts.meter().spend(4)?;
        if recipe.size==0{return Ok(());}
        let machine=&recipe.machine;
        if let Some(enumeration)=&recipe.enumeration {
            let scratch=self.payload_scratch(recipe)?;
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Store{arg:m::Arg::new(machine.clone(),m::val(source)),ptr:m::val(&scratch),align:recipe.align})));
            self.zero_payload(recipe,address)?;
            let tag=self.payload_extract(machine,source,0);let tag_address=self.payload_field(machine,address,0);
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Store{arg:m::Arg::new(m::Ty::Int(32),m::val(&tag)),ptr:m::val(tag_address),align:4})));
            let from=self.payload_field(machine,&scratch,1);let to=self.payload_field(machine,address,1);
            let id=self.continuation;self.continuation+=1;let done=format!("payload.done.{id}");
            for(index,variant)in enumeration.variants.iter().enumerate() {
                self.layouts.meter().spend(variant.fields.len().saturating_add(8))?;
                let matched=self.temp();let yes=format!("payload.case.{id}.{index}");let no=format!("payload.next.{id}.{index}");
                self.event(m::Event::Instruction(m::Inst::new(Some(m::name(matched.clone())),m::Op::Compare{predicate:"eq".into(),ty:m::Ty::Int(32),left:m::val(&tag),right:m::val(index as u32)})));
                self.event(m::Event::Instruction(m::Inst::new(None,m::Op::CondBranch{condition:m::val(matched),yes:yes.clone(),no:no.clone()})));
                self.event(m::Event::Label(yes));
                let payload=m::Ty::Struct(variant.fields.iter().map(|field|field.recipe.machine.clone()).collect());
                for field in &variant.fields {
                    if field.recipe.size==0{continue;}
                    let source_address=self.payload_field(&payload,&from,field.index);let target_address=self.payload_field(&payload,&to,field.index);let loaded=self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(Some(m::name(loaded.clone())),m::Op::Load{ty:field.recipe.machine.clone(),ptr:m::val(source_address),align:field.recipe.align})));
                    self.store_recipe(&field.recipe,&loaded,&target_address)?;
                }
                self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Branch(done.clone()))));self.event(m::Event::Label(no));
            }
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Call{result:m::Ty::Void,result_attrs:vec![],callee:m::global("llvm.trap".into()),args:vec![],vararg:None})));
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Unreachable)));self.event(m::Event::Label(done));return Ok(());
        }
        if recipe.error_union {
            self.zero_payload(recipe,address)?;
            let tag=self.payload_extract(machine,source,0);let target=self.payload_field(machine,address,0);self.store_recipe(&recipe.fields[0].recipe,&tag,&target)?;
            let id=self.continuation;self.continuation+=1;let yes=format!("payload.error.{id}");let no=format!("payload.ok.{id}");let done=format!("payload.join.{id}");
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::CondBranch{condition:m::val(tag),yes:yes.clone(),no:no.clone()})));
            for(index,label)in [(1,yes),(2,no)] {
                self.event(m::Event::Label(label));let field=&recipe.fields[index];if field.recipe.size!=0 {let value=self.payload_extract(machine,source,index as u32);let target=self.payload_field(machine,address,index as u32);self.store_recipe(&field.recipe,&value,&target)?;}
                self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Branch(done.clone()))));
            }
            self.event(m::Event::Label(done));return Ok(());
        }
        if let m::Ty::Array(length,_)=machine {
            self.layouts.meter().spend(usize::try_from(*length).unwrap_or(usize::MAX).saturating_mul(8))?;
            let child=&recipe.fields[0].recipe;
            for index in 0..*length {
                let index=u32::try_from(index).map_err(|_|vec![Diagnostic::limit(Phase::Codegen,"array payload index exceeds machine extract range",None)])?;
                let value=self.payload_extract(machine,source,index);let target=self.payload_field(machine,address,index);self.store_recipe(child,&value,&target)?;
            }
        } else if !recipe.fields.is_empty() {
            for field in &recipe.fields {if field.recipe.size==0{continue;}let value=self.payload_extract(machine,source,field.index);let target=self.payload_field(machine,address,field.index);self.store_recipe(&field.recipe,&value,&target)?;}
        } else {
            let (ty,value)=if *machine==m::Ty::Int(1) {
                let wide=self.temp();self.event(m::Event::Instruction(m::Inst::new(Some(m::name(wide.clone())),m::Op::Cast{opcode:"zext".into(),arg:m::Arg::new(m::Ty::Int(1),m::val(source)),to:m::Ty::Int(8)})));(m::Ty::Int(8),m::val(wide))
            } else {(machine.clone(),m::val(source))};
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Store{arg:m::Arg::new(ty,value),ptr:m::val(address),align:recipe.align})));
        }
        Ok(())
    }

}

#[cfg(test)]
mod owned_sum_layout_tests {
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
    fn enum_managed_headers_keep_aligned_max_payload_abi() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        let ty = Type::Enum {
            name: "Headers".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                ("Pair".into(), vec![buffer(), store()]),
            ],
        };
        let layout = layouts.union(&ty).unwrap();
        assert_eq!((layout.words, layout.word_align, layout.align), (6, 8, 8));
        assert_eq!(
            layouts.value_type(&ty).unwrap().to_string(),
            "{ i32, [6 x i64] }"
        );
        assert_eq!(
            (
                layouts.recipe(&ty, layout::Position::Field).unwrap().size,
                layouts.recipe(&ty, layout::Position::Field).unwrap().align
            ),
            (56, 8)
        );
        // Headers are three actual words; treating Managed as ptr gives 24, not56.
        let error = Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(ty),
        };
        assert_eq!(
            layouts.value_type(&error).unwrap().to_string(),
            "{ i1, i32, { i32, [6 x i64] } }"
        );
        assert_eq!(layouts.recipe(&error, layout::Position::Field).unwrap().size, 64);
    }
    #[test]
    fn product_sum_nested_layout_preserves_field_offsets_and_bool_storage() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

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
                layouts.union(&outer).unwrap().words,
                layouts.recipe(&outer, layout::Position::Field).unwrap().size
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
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

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
        assert_eq!(layouts.value_type(&address).unwrap().to_string(), "ptr");
        assert_eq!(layouts.value_type(&slice).unwrap().to_string(), "{ ptr, i64 }");
        assert_eq!(layouts.value_type(&desc).unwrap().to_string(), "ptr");
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
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

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
        assert_eq!(layouts.value_type(&sum).unwrap().to_string(), "{ i32, [1 x i64] }");
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

#[cfg(test)]
mod qualified_pure_layout_tests {
    use super::*;

    fn qualified(storage: Type) -> Type {
        Type::BorrowedStorage {
            storage: Box::new(storage),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        }
    }

    #[test]
    fn qualified_enum_payload_keeps_nested_array_layout_and_ct_boundary() {
        let layout_fixture=layout::scalar_test_context();
        let layouts=&layout_fixture;

        // Synthetic layout probe only; this does not establish nominal/source
        // authority. The Source/native test must traverse the independent kernels.
        let leaf = qualified(Type::Record {
            name: "Leaf".into(),
            fields: vec![("small".into(), Type::U8), ("bits".into(), Type::U64)],
        });
        let storage = Type::Enum {
            name: "Choice".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                (
                    "Value".into(),
                    vec![
                        leaf.clone(),
                        Type::Array {
                            element: Box::new(leaf),
                            length: 2,
                        },
                        Type::Bool,
                    ],
                ),
            ],
        };
        let ty = qualified(storage.clone());
        let layout = layouts.recipe(&ty, layout::Position::Field).unwrap();
        assert_eq!((layout.size, layout.align), (64, 8));
        assert_eq!(layouts.value_type(&ty).unwrap(), layouts.value_type(&storage).unwrap());
        let union = layouts.union(&ty).unwrap();
        assert_eq!((union.words, union.word_align, union.align), (7, 8, 8));
        assert!(matches!(ty, Type::BorrowedStorage { .. }));
        // Runtime representation support does not widen the separate CT query.
        let errors = ct_layout_of_checked_type(
            &ty,
            &TargetProfile::host_llvm(),
            &zil_base::Limits::default(),
            &mut |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::Unsupported);
    }
}

#[cfg(test)]
mod actual_payload_tests {
    use super::*;
    #[test]
    fn enum_environment_and_dynamic_payload_store_read_and_drop_share_actual_fields() {
        // A private operation/model control, not a substitute for Verify or
        // Source/native admission. Closure7 has a complete actual definition.
        let mut captures=vec![vec![];8];
        captures[7]=vec![Type::Managed(Box::new(Type::ByteBuffer)),Type::Unit];
        let mut module=layout::test_environments(captures);
        module.resources=vec![ResourceDefinition{name:zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),payload:Type::ByteBuffer,teardown:None}];
        let interface=zil_ir::DynInterfaceId{module:zil_base::ModuleId(0),index:9};
        let ty=Type::Enum{name:"ActualClosureAndDyn".into(),variants:vec![("Empty".into(),vec![]),("Owners".into(),vec![Type::Closure(ClosureId(7)),Type::OwnedDyn{interface}])]};
        let limits=zil_base::Limits::default();
        let layouts=&LayoutContext::for_test_module(&module,&limits).unwrap();
        let machine_type=layouts.value_type(&ty).unwrap();let recipe=layouts.recipe(&ty,layout::Position::Field).unwrap();
        let functions=module.functions.iter().map(|function|(function.id,function)).collect();
        let closures=module.closures.iter().map(|closure|(closure.id,closure)).collect();
        let mut intrinsics=BTreeSet::new();let mut adapters=BTreeSet::new();
        let abi=large_copy_abi::Plans::default();let snapshots=BTreeMap::new();
        let mut emitter=FunctionEmitter{dyn_module:&module,task_ownership:None,current_instruction:None,vacated_pollable_drops:BTreeSet::new(),
            function:&module.functions[0],target:&module.target,functions:&functions,scalar_summaries:None,closures:&closures,resources:&module.resources,
            intrinsics:&mut intrinsics,foreign_adapters:&mut adapters,machine_meter:layouts.meter(),layouts,text:m::Code::new(),temporary:0,continuation:0,
            partial_packages:BTreeSet::new(),replacements:BTreeMap::new(),partial_sums:BTreeMap::new(),safe_divisors:None,copy_values:&snapshots,abi_plans:&abi,unsupported_large_copy:false};
        emitter.event(m::Event::Begin(m::Function{name:"payload_roundtrip".into(),linkage:m::Linkage::Internal,signature:m::Signature::new(m::Ty::Void,vec![m::Param::new(machine_type.clone(),"actual")]),attrs:vec![],blocks:None}));
        emitter.event(m::Event::Label("entry".into()));
        emitter.event(m::Event::Instruction(m::Inst::new(Some("destination".into()),m::Op::Alloca{ty:machine_type.clone(),align:recipe.align})));
        emitter.store_payload(&ty,"%actual","%destination").unwrap();
        // Read uses the same prepared aggregate type; cleanup selects the
        // active tag and then the true closure flags/dynamic owner helper.
        emitter.event(m::Event::Instruction(m::Inst::new(Some("roundtrip".into()),m::Op::Load{ty:machine_type,ptr:m::local("destination".into()),align:recipe.align})));
        emitter.event(m::Event::Instruction(m::Inst::new(None,m::Op::Call{result:m::Ty::Void,result_attrs:vec![],callee:m::global(drop_plan::symbol(&ty)),args:vec![m::Arg::new(m::Ty::Ptr,m::local("destination".into()))],vararg:None})));
        emitter.event(m::Event::Instruction(m::Inst::new(None,m::Op::Return(None))));emitter.event(m::Event::End);
        assert!(emitter.text.0.iter().any(|event|matches!(event,m::Event::Instruction(m::Inst{op:m::Op::Extract{arg,index:2},..})if arg.ty==environment_type(ClosureId(7)))),"the actual environment flag must be transported");
        let view=zil_ir::LirView{schema_version:SCHEMA_VERSION,module_id:module.module_id,verified_fingerprint:zil_base::Fingerprint::ZERO,target:&module.target,functions:&module.functions,closures:&module.closures,resources:&module.resources,layouts:std::borrow::Cow::Borrowed(&[])};
        let helpers=drop_plan::functions(layouts,&view,&limits,true,&[&ty],EntryPoint::None).unwrap();
        let mut code=m::Code::new();for definition in &module.closures{code.push(m::Event::NamedType(m::type_name(&environment_type(definition.id)),layouts.environment_fields(definition.id).unwrap()));}
        code.push(zero_declaration());
        for name in ["llvm.trap".to_string(),"zil.bytes.drop".to_string(),dyn_dispatch::owner_drop_symbol(interface)] {
            code.push(m::Event::Declare(m::Function{name:name.clone(),linkage:m::Linkage::External,signature:m::Signature::new(m::Ty::Void,if name=="llvm.trap"{vec![]}else{vec![m::Param::unnamed(m::Ty::Ptr)]}),attrs:vec![],blocks:None}));
        }
        code.append(resource_closures::drop_functions(layouts,&module.closures,&module.resources).unwrap());code.append(helpers.code);code.append(emitter.text);
        let machine=m::Module::from_code(module.target.triple.clone(),"component".into(),code).unwrap();
        machine.validate_complete(|_|Ok(())).unwrap();
    }
}
