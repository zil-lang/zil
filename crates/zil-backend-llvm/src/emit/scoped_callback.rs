//! One fixed Direct stack context for a verified scoped foreign callback.
//! The external no-future promise is checked at the FFI declaration boundary;
//! this lowering never manufactures a heap root or accepts a raw LIR authority.
use super::*;

fn context_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(1), m::Ty::Int(1)])
}

fn context_name(id: ValueId) -> String {
    format!("scoped.ctx.{}", id.0)
}

fn body_name(id: ValueId) -> String {
    format!("scoped.body.{}", id.0)
}

fn trampoline_name(function: FunctionId, id: ValueId) -> String {
    format!("zil.scoped.callback.{}.{}", function.0, id.0)
}

fn helper_name(function: FunctionId, id: ValueId) -> String {
    format!("zil.scoped.with.{}.{}", function.0, id.0)
}

fn result_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)])
}

fn field(name: String, base: m::Val, index: u32) -> m::Event {
    m::Event::Instruction(m::Inst::new(
        Some(name),
        m::Op::Gep {
            ty: context_type(),
            ptr: base,
            indices: vec![
                m::Arg::new(m::Ty::Int(32), 0),
                m::Arg::new(m::Ty::Int(32), index),
            ],
        },
    ))
}

impl FunctionEmitter<'_> {
    pub(super) fn call_scoped_callback_helper(
        &mut self,
        instruction: &Instruction,
        _register: FunctionId,
        _close: FunctionId,
        callback: ValueId,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped body requires a concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", id.0)),
            m::Op::Call {
                result: result_type(),
                result_attrs: vec![],
                callee: m::global(helper_name(self.function.id, id)),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::val(value(callback))),
                    m::Arg::new(environment_type(body_id), m::val(value(body))),
                ],
                vararg: None,
            },
        )));
        Ok(())
    }

    pub(super) fn scoped_callback_helper(
        &mut self,
        instruction: &Instruction,
        register: FunctionId,
        close: FunctionId,
        callback: ValueId,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped body requires a concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        self.event(m::Event::Begin(m::Function {
            name: helper_name(self.function.id, id),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(
                result_type(),
                vec![
                    m::Param::new(m::Ty::Ptr, format!("v.{}", callback.0)),
                    m::Param::new(environment_type(body_id), format!("v.{}", body.0)),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        self.scoped_callback_slots(instruction, body)?;
        self.with_foreign_callback(instruction, register, close, callback, body)?;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(
                result_type(),
                m::local(format!("v.{}", id.0)),
            ))),
        )));
        self.event(m::Event::End);
        Ok(())
    }

    pub(super) fn scoped_callback_slots(
        &mut self,
        instruction: &Instruction,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped callback body requires a concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(context_name(id)),
            m::Op::Alloca {
                ty: context_type(),
                align: 8,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(body_name(id)),
            m::Op::Alloca {
                ty: environment_type(body_id),
                align: alignment(&Type::Closure(body_id), self.closures)?,
            },
        )));
        Ok(())
    }

    fn drop_scoped_body(&mut self, body_id: ClosureId, slot: &str) -> CompileResult<()> {
        if resource_closures::owns(self.closure_definition(body_id)?) {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(format!("zil.closure.drop.{}", body_id.0)),
                    args: vec![m::Arg::new(m::Ty::Ptr, m::local(slot.into()))],
                    vararg: None,
                },
            )));
        }
        Ok(())
    }

    pub(super) fn with_foreign_callback(
        &mut self,
        instruction: &Instruction,
        register: FunctionId,
        close: FunctionId,
        callback: ValueId,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Address {
            inner,
            mutable: true,
        } = self.value_type(callback)?
        else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped callback requires a mutable closure address",
                Some(instruction.span),
            );
        };
        let Type::Closure(_) = inner.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped callback has no concrete environment",
                Some(instruction.span),
            );
        };
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped callback body has no concrete environment",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        let body_def = self.closure_definition(body_id)?.clone();
        if body_def.result != Type::I32 || !body_def.parameters.is_empty() {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped body requires FnOnce() -> i32",
                Some(instruction.span),
            );
        }
        let register_name = self
            .functions
            .get(&register)
            .and_then(|f| f.foreign.as_ref())
            .map(|f| f.symbol.clone())
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    Some(instruction.span),
                    "scoped register declaration is absent",
                )]
            })?;
        let close_name = self
            .functions
            .get(&close)
            .and_then(|f| f.foreign.as_ref())
            .map(|f| f.symbol.clone())
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    Some(instruction.span),
                    "scoped close declaration is absent",
                )]
            })?;
        let ctx = context_name(id);
        let slot = body_name(id);
        let tag = format!("scoped.{}", id.0);
        let field_f = format!("{tag}.f");
        let field_closing = format!("{tag}.closing");
        let field_active = format!("{tag}.active");
        self.event(field(field_f.clone(), m::local(ctx.clone()), 0));
        self.event(field(field_closing.clone(), m::local(ctx.clone()), 1));
        self.event(field(field_active.clone(), m::local(ctx.clone()), 2));
        for (ptr, arg, align) in [
            (field_f, m::Arg::new(m::Ty::Ptr, m::val(value(callback))), 8),
            (field_closing.clone(), m::Arg::new(m::Ty::Int(1), false), 1),
            (field_active, m::Arg::new(m::Ty::Int(1), false), 1),
        ] {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg,
                    ptr: m::local(ptr),
                    align,
                },
            )));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(environment_type(body_id), m::val(value(body))),
                ptr: m::local(slot.clone()),
                align: alignment(&Type::Closure(body_id), self.closures)?,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.handle")),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global(register_name),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::global(trampoline_name(self.function.id, id))),
                    m::Arg::new(m::Ty::Ptr, m::local(ctx)),
                ],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.failed")),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local(format!("{tag}.handle")),
                right: m::Val::Null,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local(format!("{tag}.failed")),
                yes: format!("{tag}.failure"),
                no: format!("{tag}.success"),
            },
        )));

        self.event(m::Event::Label(format!("{tag}.failure")));
        self.drop_scoped_body(body_id, &slot)?;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.failure.value")),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)]),
                    m::Val::Zero,
                ),
                field: m::Arg::new(m::Ty::Int(1), false),
                index: 0,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.failure.result")),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)]),
                    m::local(format!("{tag}.failure.value")),
                ),
                field: m::Arg::new(m::Ty::Int(32), 0),
                index: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("{tag}.done")),
        )));

        self.event(m::Event::Label(format!("{tag}.success")));
        let body_arg = if resource_closures::by_pointer(&body_def) {
            m::Arg::new(m::Ty::Ptr, m::local(slot.clone()))
        } else {
            let loaded = format!("{tag}.body.value");
            self.event(m::Event::Instruction(m::Inst::new(
                Some(loaded.clone()),
                m::Op::Load {
                    ty: environment_type(body_id),
                    ptr: m::local(slot.clone()),
                    align: alignment(&Type::Closure(body_id), self.closures)?,
                },
            )));
            m::Arg::new(environment_type(body_id), m::local(loaded))
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.body.result")),
            m::Op::Call {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                callee: m::global(symbol(body_def.function)),
                args: vec![body_arg],
                vararg: None,
            },
        )));
        self.drop_scoped_body(body_id, &slot)?;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), true),
                ptr: m::local(field_closing),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(close_name),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local(format!("{tag}.handle")))],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.success.value")),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)]),
                    m::Val::Zero,
                ),
                field: m::Arg::new(m::Ty::Int(1), true),
                index: 0,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("{tag}.success.result")),
            m::Op::Insert {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)]),
                    m::local(format!("{tag}.success.value")),
                ),
                field: m::Arg::new(m::Ty::Int(32), m::local(format!("{tag}.body.result"))),
                index: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("{tag}.done")),
        )));
        self.event(m::Event::Label(format!("{tag}.done")));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", id.0)),
            m::Op::Phi {
                ty: m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)]),
                incoming: vec![
                    (
                        m::local(format!("{tag}.failure.result")),
                        format!("{tag}.failure"),
                    ),
                    (
                        m::local(format!("{tag}.success.result")),
                        format!("{tag}.success"),
                    ),
                ],
            },
        )));
        Ok(())
    }

    pub(super) fn scoped_callback_trampoline(
        &mut self,
        instruction: &Instruction,
        callback: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Address {
            inner,
            mutable: true,
        } = self.value_type(callback)?
        else {
            return invalid(
                "E-CODEGEN-CFG",
                "trampoline requires a mutable closure address",
                Some(instruction.span),
            );
        };
        let Type::Closure(callback_id) = inner.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "trampoline requires concrete closure identity",
                Some(instruction.span),
            );
        };
        let callback_id = *callback_id;
        let definition = self.closure_definition(callback_id)?.clone();
        if definition.result != Type::I32 || definition.parameters != [Type::I32] {
            return invalid(
                "E-CODEGEN-CFG",
                "trampoline requires FnMut(i32) -> i32",
                Some(instruction.span),
            );
        }
        self.event(m::Event::Begin(m::Function {
            name: trampoline_name(self.function.id, id),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(
                m::Ty::Int(64),
                vec![
                    m::Param::new(m::Ty::Ptr, "context"),
                    m::Param::new(m::Ty::Int(32), "argument"),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        self.event(field("closing.ptr".into(), m::local("context".into()), 1));
        self.event(field("active.ptr".into(), m::local("context".into()), 2));
        self.event(m::Event::Instruction(m::Inst::new(
            Some("closing".into()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local("closing.ptr".into()),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some("active".into()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local("active.ptr".into()),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some("denied".into()),
            m::Op::Binary {
                opcode: "or".into(),
                ty: m::Ty::Int(1),
                left: m::local("closing".into()),
                right: m::local("active".into()),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("denied".into()),
                yes: "reject".into(),
                no: "call".into(),
            },
        )));
        self.event(m::Event::Label("reject".into()));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(64), 1i64 << 32))),
        )));
        self.event(m::Event::Label("call".into()));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), true),
                ptr: m::local("active.ptr".into()),
                align: 1,
            },
        )));
        self.event(field("f.ptr".into(), m::local("context".into()), 0));
        self.event(m::Event::Instruction(m::Inst::new(
            Some("f".into()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("f.ptr".into()),
                align: 8,
            },
        )));
        let env_arg = if resource_closures::by_pointer(&definition) {
            m::Arg::new(m::Ty::Ptr, m::local("f".into()))
        } else {
            self.event(m::Event::Instruction(m::Inst::new(
                Some("f.value".into()),
                m::Op::Load {
                    ty: environment_type(callback_id),
                    ptr: m::local("f".into()),
                    align: alignment(&Type::Closure(callback_id), self.closures)?,
                },
            )));
            m::Arg::new(environment_type(callback_id), m::local("f.value".into()))
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some("answer".into()),
            m::Op::Call {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                callee: m::global(symbol(definition.function)),
                args: vec![
                    env_arg,
                    m::Arg::new(m::Ty::Int(32), m::local("argument".into())),
                ],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local("active.ptr".into()),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some("answer.wide".into()),
            m::Op::Cast {
                opcode: "zext".into(),
                arg: m::Arg::new(m::Ty::Int(32), m::local("answer".into())),
                to: m::Ty::Int(64),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(
                m::Ty::Int(64),
                m::local("answer.wide".into()),
            ))),
        )));
        self.event(m::Event::End);
        Ok(())
    }
}
