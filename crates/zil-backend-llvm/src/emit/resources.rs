//! Finite inline headers and plain access descriptions. No heap box, reference
//! count, address-derived identity, or optimizer alias promise is introduced.
use super::*;

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    if dynamic_store::is_resource(ty, resources) || byte_buffer::is_resource(ty, resources) {
        return true;
    }
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            matches!(payload.as_ref(), Type::Record { name, .. }
                if payload.is_copy_value() && resources.iter().any(|r| &r.name == name && &r.payload == payload.as_ref()))
        }
        _ => false,
    }
}

impl FunctionEmitter<'_> {
    fn desc_field(&self, desc: ValueId, index: u32) -> CompileResult<(m::Ty, Type)> {
        let Type::Desc(payload) = self.value_type(desc)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource access requires a description",
                Some(self.function.span),
            );
        };
        let Type::Record { fields, .. } = payload.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource payload is not a finite record",
                Some(self.function.span),
            );
        };
        let Some((_, field)) = fields.get(index as usize) else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource field is outside its payload",
                Some(self.function.span),
            );
        };
        Ok((llvm_type(payload)?, field.clone()))
    }

    pub(super) fn read_desc(
        &mut self,
        instruction: &Instruction,
        desc: ValueId,
        index: u32,
    ) -> CompileResult<()> {
        let (payload, field) = self.desc_field(desc, index)?;
        if field == Type::Unit {
            return Ok(());
        }
        let pointer = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((pointer).clone())),
            m::Op::Gep {
                ty: (payload).clone(),
                ptr: m::val((value(desc)).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result(instruction)?)).clone())),
            m::Op::Load {
                ty: (llvm_type(&field)?).clone(),
                ptr: m::val((pointer).clone()),
                align: (alignment(&field, self.closures)?) as u32,
            },
        )));
        Ok(())
    }

    pub(super) fn write_desc(
        &mut self,
        _instruction: &Instruction,
        desc: ValueId,
        index: u32,
        input: ValueId,
    ) -> CompileResult<()> {
        let (payload, field) = self.desc_field(desc, index)?;
        if field == Type::Unit {
            return Ok(());
        }
        let pointer = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((pointer).clone())),
            m::Op::Gep {
                ty: (payload).clone(),
                ptr: m::val((value(desc)).clone()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new((llvm_type(&field)?).clone(), m::val((value(input)).clone())),
                ptr: m::val((pointer).clone()),
                align: (alignment(&field, self.closures)?) as u32,
            },
        )));
        Ok(())
    }

    pub(super) fn drop_managed(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let Type::Managed(payload) = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "cleanup requires an actual managed local",
                Some(instruction.span),
            );
        };
        let Some(definition) = self
            .resources
            .iter()
            .find(|r| &r.payload == payload.as_ref())
        else {
            return invalid(
                "E-CODEGEN-CFG",
                "cleanup has no verified resource definition",
                Some(instruction.span),
            );
        };
        let teardown = definition.teardown;
        let byte_buffer = definition.payload == Type::ByteBuffer;
        let store_drop = if matches!(definition.payload, Type::OwnedStore { .. }) {
            Some(dynamic_store::helper(
                &definition.payload,
                self.resources,
                "drop",
            )?)
        } else {
            None
        };
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((flag).clone())),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val((flag).clone()),
                yes: format!("resource.drop.{}", label),
                no: format!("resource.done.{}", label),
            },
        )));
        self.event(m::Event::Label(format!("resource.drop.{}", label)));
        // The activation owns the obligation before invoking user code. There
        // is no write into a payload after its teardown could have released it.
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        if let Some(store_drop) = store_drop {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(store_drop.to_string()),
                    args: vec![m::Arg::new(
                        m::Ty::Ptr,
                        m::local(format!("local.{}", local.0)),
                    )],
                    vararg: None,
                },
            )));
        } else if byte_buffer {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global("zil.bytes.drop".into()),
                    args: vec![m::Arg::new(
                        m::Ty::Ptr,
                        m::local(format!("local.{}", local.0)),
                    )],
                    vararg: None,
                },
            )));
        } else if let Some(teardown) = teardown {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(symbol(teardown).to_string()),
                    args: vec![m::Arg::new(
                        m::Ty::Ptr,
                        m::local(format!("local.{}", local.0)),
                    )],
                    vararg: None,
                },
            )));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("resource.done.{}", label)),
        )));
        self.event(m::Event::Label(format!("resource.done.{}", label)));
        Ok(())
    }
}
