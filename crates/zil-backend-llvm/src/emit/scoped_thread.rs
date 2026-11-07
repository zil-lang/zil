//! Linux x86-64 pthread adapter. The checked payload is i32 or shared &i32.
//! The parent's private helper activation owns stable context until join.
use super::*;
use zil_ir::{AtomicOrdering, SyncCellKind, SyncI32Op};

fn context_type(payload: &m::Ty) -> m::Ty {
    m::Ty::Struct(vec![payload.clone(), m::Ty::Int(32)])
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
fn field(name: &str, base: &str, index: u32, payload: &m::Ty) -> m::Event {
    inst(
        Some(name.into()),
        m::Op::Gep {
            ty: context_type(payload),
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
        .any(|i| {
            matches!(
                i.kind,
                InstructionKind::WithScopedThread { .. }
                    | InstructionKind::WithSyncCell { .. }
                    | InstructionKind::SyncI32 { .. }
                    | InstructionKind::SyncI32CompareExchange { .. }
            )
        })
}
pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    let mut code = m::Code::new();
    if !present(lir) {
        return Ok(code);
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return Err(vec![Diagnostic::unsupported(
            Phase::Codegen,
            "scoped pthread ABI requires x86_64-unknown-linux-gnu",
            None,
        )]);
    }
    // On this target pthread_t is unsigned long (i64), with pointer-aligned
    // storage; start_routine has the actual C void* (void*) calling convention.
    let mut threads = false;
    let mut mutexes = false;
    for function in lir.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            match instruction.kind {
                InstructionKind::WithScopedThread { .. } => threads = true,
                InstructionKind::WithSyncCell {
                    kind: SyncCellKind::MutexI32,
                    ..
                } => mutexes = true,
                InstructionKind::SyncI32 { address, .. } => {
                    mutexes |= matches!(function.values.get(address.0 as usize),
                        Some(Type::Address { inner, .. }) if matches!(inner.as_ref(), Type::SyncCell(SyncCellKind::MutexI32)));
                }
                _ => {}
            }
        }
    }
    let mut declarations = Vec::new();
    if threads {
        declarations.push(("pthread_create", m::Ty::Int(32), vec![m::Ty::Ptr; 4]));
        declarations.push((
            "pthread_join",
            m::Ty::Int(32),
            vec![m::Ty::Int(64), m::Ty::Ptr],
        ));
    }
    if threads || mutexes {
        declarations.push(("abort", m::Ty::Void, vec![]));
    }
    if mutexes {
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
        let payload = llvm_type(self.value_type(argument)?)?;
        self.event(call(
            Some(&format!("v.{}", id.0)),
            result_type(),
            helper(self.function.id, id),
            vec![
                m::Arg::new(payload, m::val(value(argument))),
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
            return Err(vec![Diagnostic::unsupported(
                Phase::Codegen,
                "scoped pthread ABI requires x86_64-unknown-linux-gnu",
                Some(instruction.span),
            )]);
        }
        let id = result(instruction)?;
        let InstructionKind::WithScopedThread { argument, .. } = instruction.kind else {
            unreachable!()
        };
        let payload = llvm_type(self.value_type(argument)?)?;
        let payload_align = if payload == m::Ty::Ptr { 8 } else { 4 };
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
                    m::Param::new(payload.clone(), "thread.argument"),
                    m::Param::new(environment_type(body_id), "thread.body.value"),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        for (name, ty, align) in [
            ("thread.context", context_type(&payload), payload_align),
            ("thread.handle.slot", m::Ty::Int(64), 8),
            ("thread.body", environment_type(body_id), body_align),
        ] {
            self.event(inst(Some(name.into()), m::Op::Alloca { ty, align }));
        }
        self.event(field("thread.argument.ptr", "thread.context", 0, &payload));
        self.event(field("thread.result.ptr", "thread.context", 1, &payload));
        self.event(store(
            payload.clone(),
            m::local("thread.argument".into()),
            "thread.argument.ptr",
            payload_align,
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
        self.event(field("argument.ptr", "context", 0, &payload));
        self.event(load(
            "argument",
            payload.clone(),
            "argument.ptr",
            payload_align,
        ));
        self.event(call(
            Some("worker.result"),
            m::Ty::Int(32),
            symbol(worker),
            vec![m::Arg::new(payload.clone(), m::local("argument".into()))],
        ));
        self.event(field("result.ptr", "context", 1, &payload));
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

fn sync_result_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(1), m::Ty::Int(32)])
}
fn sync_helper(function: FunctionId, id: ValueId) -> String {
    format!("zil.sync.scope.{}.{}", function.0, id.0)
}
fn sync_layout(kind: SyncCellKind) -> m::Ty {
    match kind {
        // glibc x86-64 pthread_mutex_t: 40 bytes, alignment 8. This layout is
        // target gated before emission; no cross-platform size assumption.
        SyncCellKind::MutexI32 => {
            m::Ty::Struct(vec![m::Ty::array(5, m::Ty::Int(64)), m::Ty::Int(32)])
        }
        SyncCellKind::AtomicI32 => m::Ty::Int(32),
        SyncCellKind::AtomicI64 | SyncCellKind::AtomicU64 | SyncCellKind::AtomicUsize => {
            m::Ty::Int(64)
        }
    }
}
fn machine_order(ordering: AtomicOrdering) -> m::AtomicOrdering {
    match ordering {
        AtomicOrdering::Relaxed => m::AtomicOrdering::Relaxed,
        AtomicOrdering::Acquire => m::AtomicOrdering::Acquire,
        AtomicOrdering::Release => m::AtomicOrdering::Release,
        AtomicOrdering::AcqRel => m::AtomicOrdering::AcqRel,
        AtomicOrdering::SeqCst => m::AtomicOrdering::SeqCst,
    }
}

impl FunctionEmitter<'_> {
    pub(super) fn call_sync_cell_helper(
        &mut self,
        instruction: &Instruction,
        initial: ValueId,
        body: ValueId,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "cell scope body must be a closure",
                Some(instruction.span),
            );
        };
        self.event(call(
            Some(&format!("v.{}", id.0)),
            sync_result_type(),
            sync_helper(self.function.id, id),
            vec![
                m::Arg::new(m::Ty::Int(32), m::val(value(initial))),
                m::Arg::new(environment_type(*body_id), m::val(value(body))),
            ],
        ));
        Ok(())
    }

    fn sync_status(&mut self, status: &str, prefix: &str) {
        self.event(inst(
            Some(format!("{prefix}.failed")),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::local(status.into()),
                right: m::Val::Int(0),
            },
        ));
        self.event(inst(
            None,
            m::Op::CondBranch {
                condition: m::local(format!("{prefix}.failed")),
                yes: format!("{prefix}.abort"),
                no: format!("{prefix}.ok"),
            },
        ));
        self.event(m::Event::Label(format!("{prefix}.abort")));
        self.event(call(None, m::Ty::Void, "abort".into(), vec![]));
        self.event(inst(None, m::Op::Unreachable));
        self.event(m::Event::Label(format!("{prefix}.ok")));
    }

    pub(super) fn sync_compare_exchange(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
        expected: ValueId,
        desired: ValueId,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> CompileResult<()> {
        if !matches!(self.value_type(address)?,Type::Address{mutable:false,inner} if inner.as_ref()==&Type::SyncCell(SyncCellKind::AtomicI32))
        {
            return invalid(
                "E-CODEGEN-SYNC",
                "compare exchange requires the actual AtomicI32 address",
                Some(instruction.span),
            );
        }
        self.event(inst(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::AtomicCompareExchange {
                result: m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(1)]),
                expected: m::Arg::new(m::Ty::Int(32), m::val(value(expected))),
                desired: m::val(value(desired)),
                ptr: m::val(value(address)),
                align: 4,
                success: machine_order(success),
                failure: machine_order(failure),
            },
        ));
        Ok(())
    }
    pub(super) fn sync_i32(
        &mut self,
        instruction: &Instruction,
        op: SyncI32Op,
        address: ValueId,
        operand: Option<ValueId>,
        ordering: AtomicOrdering,
    ) -> CompileResult<()> {
        let id = result(instruction)?;
        let Type::Address {
            mutable: false,
            inner,
        } = self.value_type(address)?
        else {
            return invalid(
                "E-CODEGEN-CFG",
                "cell operation requires shared sealed address",
                Some(instruction.span),
            );
        };
        let Type::SyncCell(kind) = inner.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "cell operation cannot use an ordinary address",
                Some(instruction.span),
            );
        };
        let kind = *kind;
        let pointer = m::val(value(address));
        let operand = operand.map(|id| m::val(value(id)));
        if kind == SyncCellKind::AtomicI32 {
            let operation = match op {
                SyncI32Op::Load => m::Op::AtomicLoad {
                    ty: m::Ty::Int(32),
                    ptr: pointer,
                    align: 4,
                    ordering: machine_order(ordering),
                },
                SyncI32Op::Store => m::Op::AtomicStore {
                    arg: m::Arg::new(
                        m::Ty::Int(32),
                        operand.ok_or_else(|| {
                            vec![Diagnostic::error(
                                Phase::Codegen,
                                "E-CODEGEN-CFG",
                                Some(instruction.span),
                                "atomic store operand is absent",
                            )]
                        })?,
                    ),
                    ptr: pointer,
                    align: 4,
                    ordering: machine_order(ordering),
                },
                SyncI32Op::FetchAdd => m::Op::AtomicRmw {
                    operation: m::AtomicRmwOp::Add,
                    arg: m::Arg::new(
                        m::Ty::Int(32),
                        operand.ok_or_else(|| {
                            vec![Diagnostic::error(
                                Phase::Codegen,
                                "E-CODEGEN-CFG",
                                Some(instruction.span),
                                "atomic add operand is absent",
                            )]
                        })?,
                    ),
                    ptr: pointer,
                    align: 4,
                    ordering: machine_order(ordering),
                },
            };
            self.event(inst(
                (op != SyncI32Op::Store).then(|| format!("v.{}", id.0)),
                operation,
            ));
            return Ok(());
        }
        let prefix = format!("sync.op.{}", id.0);
        self.event(call(
            Some(&format!("{prefix}.locked")),
            m::Ty::Int(32),
            "pthread_mutex_lock".into(),
            vec![m::Arg::new(m::Ty::Ptr, pointer.clone())],
        ));
        self.sync_status(&format!("{prefix}.locked"), &format!("{prefix}.lock"));
        let slot = format!("{prefix}.payload");
        self.event(inst(
            Some(slot.clone()),
            m::Op::Gep {
                ty: sync_layout(kind),
                ptr: pointer.clone(),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 1),
                ],
            },
        ));
        match op {
            SyncI32Op::Load => self.event(load(&format!("v.{}", id.0), m::Ty::Int(32), &slot, 4)),
            SyncI32Op::Store => {
                let operand = operand.ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        Some(instruction.span),
                        "mutex store operand is absent",
                    )]
                })?;
                self.event(store(m::Ty::Int(32), operand, &slot, 4));
            }
            SyncI32Op::FetchAdd => {
                let operand = operand.ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        Some(instruction.span),
                        "mutex add operand is absent",
                    )]
                })?;
                self.event(load(&format!("v.{}", id.0), m::Ty::Int(32), &slot, 4));
                self.event(inst(
                    Some(format!("{prefix}.sum")),
                    m::Op::Binary {
                        opcode: "add".into(),
                        ty: m::Ty::Int(32),
                        left: m::local(format!("v.{}", id.0)),
                        right: operand,
                    },
                ));
                self.event(store(
                    m::Ty::Int(32),
                    m::local(format!("{prefix}.sum")),
                    &slot,
                    4,
                ));
            }
        }
        self.event(call(
            Some(&format!("{prefix}.unlocked")),
            m::Ty::Int(32),
            "pthread_mutex_unlock".into(),
            vec![m::Arg::new(m::Ty::Ptr, pointer)],
        ));
        self.sync_status(&format!("{prefix}.unlocked"), &format!("{prefix}.unlock"));
        Ok(())
    }

    pub(super) fn sync_cell_functions(
        &mut self,
        instruction: &Instruction,
        kind: SyncCellKind,
        body: ValueId,
    ) -> CompileResult<()> {
        if self.target.triple != "x86_64-unknown-linux-gnu" {
            return Err(vec![Diagnostic::unsupported(
                Phase::Codegen,
                "cell scope requires Linux x64 native synchronization ABI",
                Some(instruction.span),
            )]);
        }
        let id = result(instruction)?;
        let Type::Closure(body_id) = self.value_type(body)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "cell scope body requires concrete closure",
                Some(instruction.span),
            );
        };
        let body_id = *body_id;
        let definition = self.closure_definition(body_id)?.clone();
        let body_align = alignment(&Type::Closure(body_id), self.closures)?;
        self.event(m::Event::Begin(m::Function {
            name: sync_helper(self.function.id, id),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(
                sync_result_type(),
                vec![
                    m::Param::new(m::Ty::Int(32), "sync.initial"),
                    m::Param::new(environment_type(body_id), "sync.body.value"),
                ],
            ),
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        self.event(inst(
            Some("sync.cell".into()),
            m::Op::Alloca {
                ty: sync_layout(kind),
                align: 8,
            },
        ));
        // Reuse the exact owned-closure drop helper on both success and init
        // failure. FnOnce calls already clear responsibility bits they consume.
        self.event(inst(
            Some("thread.body".into()),
            m::Op::Alloca {
                ty: environment_type(body_id),
                align: body_align,
            },
        ));
        self.event(store(
            environment_type(body_id),
            m::local("sync.body.value".into()),
            "thread.body",
            body_align,
        ));
        if kind == SyncCellKind::MutexI32 {
            self.event(call(
                Some("sync.initialized"),
                m::Ty::Int(32),
                "pthread_mutex_init".into(),
                vec![
                    m::Arg::new(m::Ty::Ptr, m::local("sync.cell".into())),
                    m::Arg::new(m::Ty::Ptr, m::Val::Null),
                ],
            ));
            self.event(inst(
                Some("sync.failed".into()),
                m::Op::Compare {
                    predicate: "ne".into(),
                    ty: m::Ty::Int(32),
                    left: m::local("sync.initialized".into()),
                    right: m::Val::Int(0),
                },
            ));
            self.event(inst(
                None,
                m::Op::CondBranch {
                    condition: m::local("sync.failed".into()),
                    yes: "sync.failure".into(),
                    no: "sync.success".into(),
                },
            ));
            self.event(m::Event::Label("sync.failure".into()));
            self.drop_thread_body(&definition);
            self.event(inst(
                None,
                m::Op::Return(Some(m::Arg::new(sync_result_type(), m::Val::Zero))),
            ));
            self.event(m::Event::Label("sync.success".into()));
            self.event(inst(
                Some("sync.payload".into()),
                m::Op::Gep {
                    ty: sync_layout(kind),
                    ptr: m::local("sync.cell".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), 0),
                        m::Arg::new(m::Ty::Int(32), 1),
                    ],
                },
            ));
        }
        self.event(store(
            m::Ty::Int(32),
            m::local("sync.initial".into()),
            if kind == SyncCellKind::MutexI32 {
                "sync.payload"
            } else {
                "sync.cell"
            },
            4,
        ));
        let body_arg = if resource_closures::by_pointer(&definition) {
            m::Arg::new(m::Ty::Ptr, m::local("thread.body".into()))
        } else {
            self.event(load(
                "sync.body.loaded",
                environment_type(body_id),
                "thread.body",
                body_align,
            ));
            m::Arg::new(
                environment_type(body_id),
                m::local("sync.body.loaded".into()),
            )
        };
        self.event(call(
            Some("sync.body.result"),
            m::Ty::Int(32),
            symbol(definition.function),
            vec![
                body_arg,
                m::Arg::new(m::Ty::Ptr, m::local("sync.cell".into())),
            ],
        ));
        self.drop_thread_body(&definition);
        if kind == SyncCellKind::MutexI32 {
            self.event(call(
                Some("sync.destroyed"),
                m::Ty::Int(32),
                "pthread_mutex_destroy".into(),
                vec![m::Arg::new(m::Ty::Ptr, m::local("sync.cell".into()))],
            ));
            self.sync_status("sync.destroyed", "sync.destroy");
        }
        self.event(inst(
            Some("sync.result.tag".into()),
            m::Op::Insert {
                arg: m::Arg::new(sync_result_type(), m::Val::Zero),
                field: m::Arg::new(m::Ty::Int(1), true),
                index: 0,
            },
        ));
        self.event(inst(
            Some("sync.result".into()),
            m::Op::Insert {
                arg: m::Arg::new(sync_result_type(), m::local("sync.result.tag".into())),
                field: m::Arg::new(m::Ty::Int(32), m::local("sync.body.result".into())),
                index: 1,
            },
        ));
        self.event(inst(
            None,
            m::Op::Return(Some(m::Arg::new(
                sync_result_type(),
                m::local("sync.result".into()),
            ))),
        ));
        self.event(m::Event::End);
        Ok(())
    }
}
