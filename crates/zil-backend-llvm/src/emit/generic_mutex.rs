//! Stable Linux x86-64 pthread mutex head. The native object is forty bytes,
//! aligned to eight; the payload follows in the same malloc allocation.
use super::*;

fn layout(payload: &Type) -> CompileResult<m::Ty> {
    Ok(m::Ty::Struct(vec![
        m::Ty::array(5, m::Ty::Int(64)),
        aggregate_field_type(payload)?,
    ]))
}
fn supported_payload(payload: &Type, resources: &[ResourceDefinition]) -> bool {
    drop_plan::value_type(payload, resources)
}
fn event(name: Option<String>, op: m::Op) -> m::Event {
    m::Event::Instruction(m::Inst::new(name.map(m::name), op))
}
fn call(name: Option<String>, result: m::Ty, callee: &str, args: Vec<m::Arg>) -> m::Event {
    event(
        name,
        m::Op::Call {
            result,
            result_attrs: vec![],
            callee: m::global(callee.into()),
            args,
            vararg: None,
        },
    )
}
fn present(lir: &LirView<'_>) -> bool {
    lir.functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .any(|i| {
            matches!(
                i.kind,
                InstructionKind::MutexCreate { .. }
                    | InstructionKind::MutexLock { .. }
                    | InstructionKind::MutexValue { .. }
                    | InstructionKind::MutexUnlock { .. }
                    | InstructionKind::DropMutexGuard { .. }
            )
        })
}
pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    let mut code = m::Code::new();
    if !present(lir) {
        return Ok(code);
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported("generic pthread mutex requires Linux x86-64 ABI", None);
    }
    let existing_scoped = scoped_thread::present(lir);
    let existing_scoped_mutex = lir.functions.iter().any(|f| f.blocks.iter().flat_map(|b|&b.instructions).any(|i| match i.kind {
        InstructionKind::WithSyncCell { kind: zil_ir::SyncCellKind::MutexI32, .. } => true,
        InstructionKind::SyncI32 { address, .. } => matches!(f.values.get(address.0 as usize), Some(Type::Address { inner, .. }) if matches!(inner.as_ref(), Type::SyncCell(zil_ir::SyncCellKind::MutexI32))),
        _ => false,
    }));
    let existing_free = lir
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer || matches!(r.payload, Type::OwnedStore { .. }));
    let mut declarations = vec![("malloc", m::Ty::Ptr, vec![m::Ty::Int(64)])];
    if !existing_free {
        declarations.push(("free", m::Ty::Void, vec![m::Ty::Ptr]));
    }
    if !existing_scoped {
        declarations.push(("abort", m::Ty::Void, vec![]));
    }
    if !existing_scoped_mutex {
        declarations.extend([
            ("pthread_mutex_init", m::Ty::Int(32), vec![m::Ty::Ptr; 2]),
            ("pthread_mutex_destroy", m::Ty::Int(32), vec![m::Ty::Ptr]),
            ("pthread_mutex_lock", m::Ty::Int(32), vec![m::Ty::Ptr]),
            ("pthread_mutex_unlock", m::Ty::Int(32), vec![m::Ty::Ptr]),
        ]);
    }
    for (name, result, parameters) in declarations {
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
                    format!("{name} conflicts with generic pthread ABI"),
                    Some(existing.span),
                );
            }
        } else {
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
    }
    Ok(code)
}

impl FunctionEmitter<'_> {
    fn mutex_drop_payload(&mut self, payload: &Type, pointer: m::Val) -> CompileResult<()> {
        let root = llvm_type(payload)?;
        for (path, symbol) in packages::teardown_paths(payload, self.resources)? {
            let actual = if path.is_empty() {
                pointer.clone()
            } else {
                let field = self.temp();
                let mut indices = vec![m::Arg::new(m::Ty::Int(32), 0)];
                indices.extend(path.iter().map(|index| m::Arg::new(m::Ty::Int(32), *index)));
                self.event(event(
                    Some(field.clone()),
                    m::Op::Gep {
                        ty: root.clone(),
                        ptr: pointer.clone(),
                        indices,
                    },
                ));
                m::val(field)
            };
            self.event(call(
                None,
                m::Ty::Void,
                &symbol,
                vec![m::Arg::new(m::Ty::Ptr, actual)],
            ));
        }
        Ok(())
    }
    pub(super) fn mutex_create(
        &mut self,
        instruction: &Instruction,
        input: ValueId,
    ) -> CompileResult<()> {
        let ty = self.value_type(input)?.clone();
        if !supported_payload(&ty, self.resources) {
            return unsupported(
                "generic mutex native payload requires complete synchronous cleanup",
                Some(instruction.span),
            );
        }
        let result = result(instruction)?;
        let machine = layout(&ty)?;
        let union = llvm_type(self.value_type(result)?)?;
        let label = self.continuation;
        self.continuation += 1;
        let name = |part: &str| format!("mutex.{label}.{part}");
        self.event(event(
            Some(name("size")),
            m::Op::Cast {
                opcode: "ptrtoint".into(),
                arg: m::Arg::new(
                    m::Ty::Ptr,
                    m::Val::Gep {
                        ty: machine.clone(),
                        ptr: Box::new(m::Val::Null),
                        indices: vec![m::Arg::new(m::Ty::Int(64), 1)],
                    },
                ),
                to: m::Ty::Int(64),
            },
        ));
        self.event(call(
            Some(name("head")),
            m::Ty::Ptr,
            "malloc",
            vec![m::Arg::new(m::Ty::Int(64), m::local(name("size")))],
        ));
        self.event(event(
            Some(name("null")),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local(name("head")),
                right: m::Val::Null,
            },
        ));
        self.event(event(
            None,
            m::Op::CondBranch {
                condition: m::local(name("null")),
                yes: name("failed"),
                no: name("initialize"),
            },
        ));
        self.event(m::Event::Label(name("initialize")));
        self.event(call(
            Some(name("status")),
            m::Ty::Int(32),
            "pthread_mutex_init",
            vec![
                m::Arg::new(m::Ty::Ptr, m::local(name("head"))),
                m::Arg::new(m::Ty::Ptr, m::Val::Null),
            ],
        ));
        self.event(event(
            Some(name("init_failed")),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::local(name("status")),
                right: m::Val::Int(0),
            },
        ));
        self.event(event(
            None,
            m::Op::CondBranch {
                condition: m::local(name("init_failed")),
                yes: name("init_error"),
                no: name("ready"),
            },
        ));
        self.event(m::Event::Label(name("init_error")));
        self.event(call(
            None,
            m::Ty::Void,
            "free",
            vec![m::Arg::new(m::Ty::Ptr, m::local(name("head")))],
        ));
        self.event(event(None, m::Op::Branch(name("failed"))));
        self.event(m::Event::Label(name("ready")));
        self.event(event(
            Some(name("payload")),
            m::Op::Gep {
                ty: machine.clone(),
                ptr: m::local(name("head")),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 1),
                ],
            },
        ));
        self.event(event(
            None,
            m::Op::Store {
                arg: m::Arg::new(llvm_type(&ty)?, m::val(value(input))),
                ptr: m::local(name("payload")),
                align: alignment(&ty, self.closures)?,
            },
        ));
        self.event(event(
            Some(name("success")),
            m::Op::Insert {
                arg: m::Arg::new(union.clone(), m::Val::Zero),
                field: m::Arg::new(m::Ty::Ptr, m::local(name("head"))),
                index: 2,
            },
        ));
        self.event(event(None, m::Op::Branch(name("done"))));
        self.event(m::Event::Label(name("failed")));
        if !ty.is_copy_value() {
            self.event(event(
                Some(name("failed_payload")),
                m::Op::Alloca {
                    ty: llvm_type(&ty)?,
                    align: alignment(&ty, self.closures)?,
                },
            ));
            self.event(event(
                None,
                m::Op::Store {
                    arg: m::Arg::new(llvm_type(&ty)?, m::val(value(input))),
                    ptr: m::local(name("failed_payload")),
                    align: alignment(&ty, self.closures)?,
                },
            ));
            self.mutex_drop_payload(&ty, m::local(name("failed_payload")))?;
        }
        self.event(event(
            Some(name("failure")),
            m::Op::Insert {
                arg: m::Arg::new(union.clone(), m::Val::Zero),
                field: m::Arg::new(m::Ty::Int(1), true),
                index: 0,
            },
        ));
        self.event(event(None, m::Op::Branch(name("done"))));
        self.event(m::Event::Label(name("done")));
        self.event(event(
            Some(format!("v.{}", result.0)),
            m::Op::Phi {
                ty: union,
                incoming: vec![
                    (m::local(name("success")), name("ready")),
                    (m::local(name("failure")), name("failed")),
                ],
            },
        ));
        Ok(())
    }
    pub(super) fn mutex_lock(
        &mut self,
        instruction: &Instruction,
        head: ValueId,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let status = self.temp();
        let failed = self.temp();
        self.event(call(
            Some(status.clone()),
            m::Ty::Int(32),
            "pthread_mutex_lock",
            vec![m::Arg::new(m::Ty::Ptr, m::val(value(head)))],
        ));
        self.event(event(
            Some(failed.clone()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val(status),
                right: m::Val::Int(0),
            },
        ));
        self.trap_if(&failed);
        self.event(event(
            Some(format!("v.{}", result.0)),
            m::Op::Freeze(m::Arg::new(m::Ty::Ptr, m::val(value(head)))),
        ));
        Ok(())
    }
    pub(super) fn mutex_value(
        &mut self,
        instruction: &Instruction,
        guard: ValueId,
    ) -> CompileResult<()> {
        let Type::Address { inner, .. } = self.value_type(guard)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "guard projection requires address",
                Some(instruction.span),
            );
        };
        let Type::MutexGuard { payload, .. } = inner.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "guard projection requires guard",
                Some(instruction.span),
            );
        };
        let payload = payload.as_ref().clone();
        let head = self.temp();
        self.event(event(
            Some(head.clone()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::val(value(guard)),
                align: 8,
            },
        ));
        self.event(event(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Gep {
                ty: layout(&payload)?,
                ptr: m::val(head),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 1),
                ],
            },
        ));
        Ok(())
    }
    pub(super) fn mutex_unlock(
        &mut self,
        instruction: &Instruction,
        guard: ValueId,
    ) -> CompileResult<()> {
        let status = self.temp();
        let failed = self.temp();
        self.event(call(
            Some(status.clone()),
            m::Ty::Int(32),
            "pthread_mutex_unlock",
            vec![m::Arg::new(m::Ty::Ptr, m::val(value(guard)))],
        ));
        self.event(event(
            Some(failed.clone()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val(status),
                right: m::Val::Int(0),
            },
        ));
        self.trap_if(&failed);
        let _ = instruction;
        Ok(())
    }
    pub(super) fn drop_mutex_guard(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let live = self.temp();
        let head = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(event(
            Some(live.clone()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(event(
            None,
            m::Op::CondBranch {
                condition: m::val(live),
                yes: format!("mutex.guard.drop.{label}"),
                no: format!("mutex.guard.done.{label}"),
            },
        ));
        self.event(m::Event::Label(format!("mutex.guard.drop.{label}")));
        self.event(event(
            Some(head.clone()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local(format!("local.{}", local.0)),
                align: 8,
            },
        ));
        let status = self.temp();
        let failed = self.temp();
        self.event(call(
            Some(status.clone()),
            m::Ty::Int(32),
            "pthread_mutex_unlock",
            vec![m::Arg::new(m::Ty::Ptr, m::val(head))],
        ));
        self.event(event(
            Some(failed.clone()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val(status),
                right: m::Val::Int(0),
            },
        ));
        self.trap_if(&failed);
        self.event(event(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(event(
            None,
            m::Op::Branch(format!("mutex.guard.done.{label}")),
        ));
        self.event(m::Event::Label(format!("mutex.guard.done.{label}")));
        let _ = instruction;
        Ok(())
    }
    pub(super) fn drop_mutex_owner(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        payload: &Type,
    ) -> CompileResult<()> {
        if !supported_payload(payload, self.resources) {
            return unsupported(
                "generic mutex native payload teardown requires synchronous cleanup",
                Some(instruction.span),
            );
        }
        let live = self.temp();
        let head = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(event(
            Some(live.clone()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(event(
            None,
            m::Op::CondBranch {
                condition: m::val(live),
                yes: format!("mutex.owner.drop.{label}"),
                no: format!("mutex.owner.done.{label}"),
            },
        ));
        self.event(m::Event::Label(format!("mutex.owner.drop.{label}")));
        self.event(event(
            Some(head.clone()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local(format!("local.{}", local.0)),
                align: 8,
            },
        ));
        let status = self.temp();
        let failed = self.temp();
        self.event(call(
            Some(status.clone()),
            m::Ty::Int(32),
            "pthread_mutex_destroy",
            vec![m::Arg::new(m::Ty::Ptr, m::val(head.clone()))],
        ));
        self.event(event(
            Some(failed.clone()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::val(status),
                right: m::Val::Int(0),
            },
        ));
        self.trap_if(&failed);
        if !payload.is_copy_value() {
            let payload_ptr = self.temp();
            self.event(event(
                Some(payload_ptr.clone()),
                m::Op::Gep {
                    ty: layout(payload)?,
                    ptr: m::val(head.clone()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), 0),
                        m::Arg::new(m::Ty::Int(32), 1),
                    ],
                },
            ));
            self.mutex_drop_payload(payload, m::val(payload_ptr))?;
        }
        self.event(call(
            None,
            m::Ty::Void,
            "free",
            vec![m::Arg::new(m::Ty::Ptr, m::val(head))],
        ));
        self.event(event(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), false),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        ));
        self.event(event(
            None,
            m::Op::Branch(format!("mutex.owner.done.{label}")),
        ));
        self.event(m::Event::Label(format!("mutex.owner.done.{label}")));
        Ok(())
    }
}
