//! Local observation never materializes an owning value. Unpack is the sole
//! atomic transfer and runs only after all runtime tag guards have succeeded.
use super::*;
use zil_ir::VariantProjection;

impl FunctionEmitter<'_> {
    fn enum_tag_at(&mut self, ty: &Type, pointer: &str, output: String) -> CompileResult<()> {
        let Type::Enum { variants, .. } = ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "local variant observation requires Enum",
                None,
            );
        };
        let tag_pointer = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(tag_pointer.clone())),
            m::Op::Gep {
                ty: llvm_type(ty)?,
                ptr: m::val(pointer),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 0),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(output.clone())),
            m::Op::Load {
                ty: m::Ty::Int(32),
                ptr: m::val(tag_pointer),
                align: 4,
            },
        )));
        let invalid = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(invalid.clone())),
            m::Op::Compare {
                predicate: "uge".into(),
                ty: m::Ty::Int(32),
                left: m::val(output),
                right: m::val(variants.len() as u32),
            },
        )));
        self.trap_if(&invalid);
        Ok(())
    }
    fn guard_enum_variant(&mut self, ty: &Type, pointer: &str, variant: u32) -> CompileResult<()> {
        let tag = self.temp();
        self.enum_tag_at(ty, pointer, tag.clone())?;
        let mismatch = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(mismatch.clone())),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val(tag),
                right: m::val(variant),
            },
        )));
        self.trap_if(&mismatch);
        Ok(())
    }
    fn variant_field_pointer(
        &mut self,
        ty: &Type,
        pointer: &str,
        variant: u32,
        index: u32,
    ) -> CompileResult<(Type, String)> {
        let Type::Enum { variants, .. } = ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "variant path cannot traverse non-Enum",
                None,
            );
        };
        let Some((_, fields)) = variants.get(variant as usize) else {
            return invalid("E-CODEGEN-CFG", "variant path tag out of range", None);
        };
        let Some(field) = fields.get(index as usize) else {
            return invalid("E-CODEGEN-CFG", "variant field out of range", None);
        };
        let payload = self.temp();
        let address = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(payload.clone())),
            m::Op::Gep {
                ty: llvm_type(ty)?,
                ptr: m::val(pointer),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 1),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(address.clone())),
            m::Op::Gep {
                ty: llvm_type(&Type::Tuple(fields.clone()))?,
                ptr: m::val(payload),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), index),
                ],
            },
        )));
        Ok((field.clone(), address))
    }
    fn variant_local_pointer(
        &mut self,
        local: LocalId,
        path: &[VariantProjection],
    ) -> CompileResult<(Type, String)> {
        let mut ty = self.local_type(local)?.clone();
        let mut pointer = format!("%local.{}", local.0);
        for step in path {
            self.guard_enum_variant(&ty, &pointer, step.variant)?;
            (ty, pointer) = self.variant_field_pointer(&ty, &pointer, step.variant, step.index)?;
        }
        Ok((ty, pointer))
    }
    pub(super) fn variant_tag_local(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        path: &[VariantProjection],
    ) -> CompileResult<()> {
        let (ty, pointer) = self.variant_local_pointer(local, path)?;
        self.enum_tag_at(&ty, &pointer, value(result(instruction)?))
    }
    pub(super) fn read_variant_local(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        path: &[VariantProjection],
    ) -> CompileResult<()> {
        if path.is_empty() {
            return invalid(
                "E-CODEGEN-CFG",
                "Copy variant path must be nonempty",
                Some(instruction.span),
            );
        }
        let (ty, pointer) = self.variant_local_pointer(local, path)?;
        if !ty.is_copy_value() {
            return invalid(
                "E-CODEGEN-CFG",
                "variant observation cannot produce owned payload",
                Some(instruction.span),
            );
        }
        if ty != Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(value(result(instruction)?))),
                m::Op::Load {
                    ty: llvm_type(&ty)?,
                    ptr: m::val(pointer),
                    align: alignment(&ty, self.closures)?,
                },
            )));
        }
        Ok(())
    }
    pub(super) fn unpack_variant(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        variant: u32,
        targets: &[LocalId],
    ) -> CompileResult<()> {
        let ty = self.local_type(local)?.clone();
        let Type::Enum { variants, .. } = &ty else {
            return invalid("E-CODEGEN-CFG", "Unpack needs Enum", Some(instruction.span));
        };
        let Some((_, fields)) = variants.get(variant as usize) else {
            return invalid(
                "E-CODEGEN-CFG",
                "Unpack tag out of range",
                Some(instruction.span),
            );
        };
        if targets.len() != fields.len()
            || targets.iter().copied().collect::<BTreeSet<_>>().len() != targets.len()
            || targets.contains(&local)
        {
            return invalid(
                "E-CODEGEN-CFG",
                "Unpack requires all distinct exact destinations",
                Some(instruction.span),
            );
        }
        for (target, field) in targets.iter().zip(fields) {
            if self.local_type(*target)? != field {
                return invalid(
                    "E-CODEGEN-CFG",
                    "Unpack destination type differs",
                    Some(instruction.span),
                );
            }
        }
        self.guard_enum_variant(&ty, &format!("%local.{}", local.0), variant)?;
        // Complete all loads before publishing any destination. There are no
        // calls, failure edges, or suspension after the source debt is cleared.
        let mut values = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            if *field == Type::Unit {
                values.push(None);
                continue;
            }
            let (_, pointer) = self.variant_field_pointer(
                &ty,
                &format!("%local.{}", local.0),
                variant,
                index as u32,
            )?;
            let loaded = self.temp();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(loaded.clone())),
                m::Op::Load {
                    ty: llvm_type(field)?,
                    ptr: m::val(pointer),
                    align: alignment(field, self.closures)?,
                },
            )));
            values.push(Some(loaded));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        for (index, target) in targets.iter().enumerate().rev() {
            let field = &fields[index];
            if let Some(loaded) = &values[index] {
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(llvm_type(field)?, m::val(loaded.clone())),
                        ptr: m::local(format!("local.{}", target.0)),
                        align: alignment(field, self.closures)?,
                    },
                )));
            }
            if self.owns_storage(field) {
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), true),
                        ptr: m::local(format!("resource.live.{}", target.0)),
                        align: 1,
                    },
                )));
                self.package_flags(*target, false, true)?;
                self.owned_array_flags(*target, false, true)?;
            }
        }
        Ok(())
    }
    pub(super) fn drop_enum(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let ty = self.local_type(local)?.clone();
        if !matches!(ty, Type::Enum { .. }) || ty.is_copy_value() {
            return invalid(
                "E-CODEGEN-CFG",
                "DropEnum requires an owning Enum local",
                Some(instruction.span),
            );
        }
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(flag.clone())),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val(flag),
                yes: format!("sum.drop.{label}"),
                no: format!("sum.done.{label}"),
            },
        )));
        self.event(m::Event::Label(format!("sum.drop.{label}")));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(drop_plan::symbol(&ty)),
                args: vec![m::Arg::new(
                    m::Ty::Ptr,
                    m::local(format!("local.{}", local.0)),
                )],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("sum.done.{label}")),
        )));
        self.event(m::Event::Label(format!("sum.done.{label}")));
        Ok(())
    }
}
