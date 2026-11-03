//! Linux x86-64 pthread adapter. Only the i32 payload/result cross threads.
//! The parent's private helper activation owns stable context until join.
use super::*;

fn context_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(32)])
}
fn result_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32), m::Ty::Int(32)])
}
fn helper(function: FunctionId, id: ValueId) -> String {
    format!("zil.thread.scope.{}.{}", function.0, id.0)
}
fn start(function: FunctionId, id: ValueId) -> String {
    format!("zil.thread.start.{}.{}", function.0, id.0)
}
fn inst(name: Option<String>, op: m::Op) -> m::Event {
    m::Event::Instruction(m::Inst::new(name, op))
}
fn call(name: Option<&str>, ty: m::Ty, callee: String, args: Vec<m::Arg>) -> m::Event {
    inst(
        name.map(str::to_owned),
        m::Op::Call {
            result: ty,
            result_attrs: vec![],
            callee: m::global(callee),
            args,
            vararg: None,
        },
    )
}
fn field(name: &str, base: &str, index: u32) -> m::Event {
    inst(
        Some(name.into()),
        m::Op::Gep {
            ty: context_type(),
            ptr: m::local(base.into()),
            indices: vec![
                m::Arg::new(m::Ty::Int(32), 0),
                m::Arg::new(m::Ty::Int(32), index),
            ],
        },
    )
}
fn load(name: &str, ty: m::Ty, slot: &str, align: u32) -> m::Event {
    inst(
        Some(name.into()),
        m::Op::Load {
            ty,
            ptr: m::local(slot.into()),
            align,
        },
    )
}
fn store(ty: m::Ty, value: m::Val, slot: &str, align: u32) -> m::Event {
    inst(
        None,
        m::Op::Store {
            arg: m::Arg::new(ty, value),
            ptr: m::local(slot.into()),
            align,
        },
    )
}
pub(super) fn present(lir: &LirView<'_>) -> bool {
    lir.functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .any(|i| matches!(i.kind, InstructionKind::WithScopedThread { .. }))
}
pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    let mut code = m::Code::new();
    if !present(lir) {
        return Ok(code);
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return invalid(
            "E-UNSUPPORTED",
            "scoped pthread ABI requires x86_64-unknown-linux-gnu",
            None,
        );
    }
    // On this target pthread_t is unsigned long (i64), with pointer-aligned
    // storage; start_routine has the actual C void* (void*) calling convention.
    for (name, result, parameters) in [
        ("pthread_create", m::Ty::Int(32), vec![m::Ty::Ptr; 4]),
        (
            "pthread_join",
            m::Ty::Int(32),
            vec![m::Ty::Int(64), m::Ty::Ptr],
        ),
        ("abort", m::Ty::Void, vec![]),
    ] {
        if let Some(existing) = lir.functions.iter().find(|f| {
            f.foreign
                .as_ref()
                .is_some_and(|foreign| foreign.symbol == name)
        }) {
            let actual = existing
                .parameters
                .iter()
                .map(|id| llvm_type(&existing.locals[id.0 as usize].ty))
                .collect::<CompileResult<Vec<_>>>()?;
            if actual != parameters || llvm_type(&existing.return_type)? != result {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} conflicts with the scoped pthread ABI"),
                    Some(existing.span),
                );
            }
            // The ordinary foreign declaration is emitted once by its owner.
            continue;
        }
        code.push(m::Event::Declare(m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(
                result,
                parameters.into_iter().map(m::Param::unnamed).collect(),
            ),
            attrs: vec![],
            blocks: None,
        }));
    }
    Ok(code)
}

impl FunctionEmitter<'_> {
    pub(super) fn call_scoped_thread_helper(
        &mut self,
        instruction: &Instruction,
        argument: ValueId,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped thread body requires a concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        self.event(call(
            Some(&format!("v.{}", id.0)),
            result_type(),
            helper(self.function.id, id),
            vec![
                m::Arg::new(m::Ty::Int(32), m::val(value(argument))),
                m::Arg::new(environment_type(body_id), m::val(value(body))),
            ],
        ));
        Ok(())
    }

    fn drop_thread_body(&mut self, body: &ClosureDefinition) {
        if resource_closures::owns(body) {
            self.event(call(
                None,
                m::Ty::Void,
                format!("zil.closure.drop.{}", body.id.0),
                vec![m::Arg::new(m::Ty::Ptr, m::local("thread.body".into()))],
            ));
        }
    }

    pub(super) fn scoped_thread_functions(
        &mut self,
        instruction: &Instruction,
        worker: FunctionId,
        body: ValueId,
    ) -> CompileResult<()> {
        if self.target.triple != "x86_64-unknown-linux-gnu" {
            return invalid(
                "E-UNSUPPORTED",
                "scoped pthread ABI requires x86_64-unknown-linux-gnu",
                Some(instruction.span),
            );
        }
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped thread body requires a concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        let definition = self.closure_definition(body_id)?.clone();
        if definition.result != Type::I32 || !definition.parameters.is_empty() {
            return invalid(
                "E-CODEGEN-CFG",
                "scoped thread body must return i32 without arguments",
                Some(instruction.span),
            );
        }
        let body_align = alignment(&Type::Closure(body_id), self.closures)?;
        self.event(m::Event::Begin(m::Function {
            name: helper(self.function.id, id),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(
                result_type(),
                vec![
                    m::Param::new(m::Ty::Int(32), "thread.argument"),
                    m::Param::new(environment_type(body_id), "thread.body.value"),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        for (name, ty, align) in [
            ("thread.context", context_type(), 4),
            ("thread.handle.slot", m::Ty::Int(64), 8),
            ("thread.body", environment_type(body_id), body_align),
        ] {
            self.event(inst(Some(name.into()), m::Op::Alloca { ty, align }));
        }
        self.event(field("thread.argument.ptr", "thread.context", 0));
        self.event(field("thread.result.ptr", "thread.context", 1));
        self.event(store(
            m::Ty::Int(32),
            m::local("thread.argument".into()),
            "thread.argument.ptr",
            4,
        ));
        self.event(store(
            m::Ty::Int(32),
            m::Val::Int(0),
            "thread.result.ptr",
            4,
        ));
        self.event(store(
            environment_type(body_id),
            m::local("thread.body.value".into()),
            "thread.body",
            body_align,
        ));
        self.event(call(
            Some("thread.created"),
            m::Ty::Int(32),
            "pthread_create".into(),
            vec![
                m::Arg::new(m::Ty::Ptr, m::local("thread.handle.slot".into())),
                m::Arg::new(m::Ty::Ptr, m::Val::Null),
                m::Arg::new(m::Ty::Ptr, m::global(start(self.function.id, id))),
                m::Arg::new(m::Ty::Ptr, m::local("thread.context".into())),
            ],
        ));
        self.event(inst(
            Some("thread.failed".into()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::local("thread.created".into()),
                right: m::Val::Int(0),
            },
        ));
        self.event(inst(
            None,
            m::Op::CondBranch {
                condition: m::local("thread.failed".into()),
                yes: "thread.failure".into(),
                no: "thread.success".into(),
            },
        ));

        self.event(m::Event::Label("thread.failure".into()));
        self.drop_thread_body(&definition);
        self.event(inst(None, m::Op::Branch("thread.done".into())));

        self.event(m::Event::Label("thread.success".into()));
        let body_arg = if resource_closures::by_pointer(&definition) {
            m::Arg::new(m::Ty::Ptr, m::local("thread.body".into()))
        } else {
            self.event(load(
                "thread.body.loaded",
                environment_type(body_id),
                "thread.body",
                body_align,
            ));
            m::Arg::new(
                environment_type(body_id),
                m::local("thread.body.loaded".into()),
            )
        };
        self.event(call(
            Some("thread.body.result"),
            m::Ty::Int(32),
            symbol(definition.function),
            vec![body_arg],
        ));
        self.drop_thread_body(&definition);
        self.event(load(
            "thread.handle",
            m::Ty::Int(64),
            "thread.handle.slot",
            8,
        ));
        self.event(call(
            Some("thread.joined"),
            m::Ty::Int(32),
            "pthread_join".into(),
            vec![
                m::Arg::new(m::Ty::Int(64), m::local("thread.handle".into())),
                m::Arg::new(m::Ty::Ptr, m::Val::Null),
            ],
        ));
        self.event(inst(
            Some("thread.join.failed".into()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::local("thread.joined".into()),
                right: m::Val::Int(0),
            },
        ));
        self.event(inst(
            None,
            m::Op::CondBranch {
                condition: m::local("thread.join.failed".into()),
                yes: "thread.abort".into(),
                no: "thread.join.success".into(),
            },
        ));
        self.event(m::Event::Label("thread.abort".into()));
        self.event(call(None, m::Ty::Void, "abort".into(), vec![]));
        self.event(inst(None, m::Op::Unreachable));

        self.event(m::Event::Label("thread.join.success".into()));
        // No load of the child's independently written slot precedes join.
        self.event(load(
            "thread.worker.result",
            m::Ty::Int(32),
            "thread.result.ptr",
            4,
        ));
        for (name, base, field, index) in [
            (
                "thread.result.created",
                m::Val::Zero,
                m::Arg::new(m::Ty::Int(1), true),
                0,
            ),
            (
                "thread.result.worker",
                m::local("thread.result.created".into()),
                m::Arg::new(m::Ty::Int(32), m::local("thread.worker.result".into())),
                1,
            ),
            (
                "thread.result.complete",
                m::local("thread.result.worker".into()),
                m::Arg::new(m::Ty::Int(32), m::local("thread.body.result".into())),
                2,
            ),
        ] {
            self.event(inst(
                Some(name.into()),
                m::Op::Insert {
                    arg: m::Arg::new(result_type(), base),
                    field,
                    index,
                },
            ));
        }
        self.event(inst(None, m::Op::Branch("thread.done".into())));
        self.event(m::Event::Label("thread.done".into()));
        self.event(inst(
            Some("thread.result".into()),
            m::Op::Phi {
                ty: result_type(),
                incoming: vec![
                    (m::Val::Zero, "thread.failure".into()),
                    (
                        m::local("thread.result.complete".into()),
                        "thread.join.success".into(),
                    ),
                ],
            },
        ));
        self.event(inst(
            None,
            m::Op::Return(Some(m::Arg::new(
                result_type(),
                m::local("thread.result".into()),
            ))),
        ));
        self.event(m::Event::End);

        self.event(m::Event::Begin(m::Function {
            name: start(self.function.id, id),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(m::Ty::Ptr, vec![m::Param::new(m::Ty::Ptr, "context")]),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        self.event(field("argument.ptr", "context", 0));
        self.event(load("argument", m::Ty::Int(32), "argument.ptr", 4));
        self.event(call(
            Some("worker.result"),
            m::Ty::Int(32),
            symbol(worker),
            vec![m::Arg::new(m::Ty::Int(32), m::local("argument".into()))],
        ));
        self.event(field("result.ptr", "context", 1));
        self.event(store(
            m::Ty::Int(32),
            m::local("worker.result".into()),
            "result.ptr",
            4,
        ));
        self.event(inst(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Ptr, m::Val::Null))),
        ));
        self.event(m::Event::End);
        Ok(())
    }
}
