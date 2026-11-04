//! Stable owning registration storage. Busy protects F; only the strong backend
//! close contract ends all raw entries (including pre-atomic/rejected entries).
use super::*;

fn site(function: FunctionId, value: ValueId, role: &str) -> String {
    format!("zil.registration.{role}.{}.{}", function.0, value.0)
}
fn frame(environment: ClosureId) -> m::Ty {
    m::Ty::Struct(vec![
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Int(32),
        environment_type(environment),
    ])
}
impl FunctionEmitter<'_> {
    fn reg_inst(&mut self, name: Option<&str>, op: m::Op) {
        self.event(m::Event::Instruction(m::Inst::new(
            name.map(str::to_owned),
            op,
        )));
    }
    fn reg_label(&mut self, name: &str) {
        self.event(m::Event::Label(name.into()));
    }
    fn reg_begin(&mut self, name: String, ret: m::Ty, params: Vec<m::Param>) {
        self.event(m::Event::Begin(m::Function {
            name,
            linkage: m::Linkage::Private,
            signature: m::Signature::new(ret, params),
            attrs: vec![],
            blocks: None,
        }));
        self.reg_label("entry");
    }
    fn reg_gep(&mut self, name: &str, ty: &m::Ty, base: m::Val, index: u32) {
        self.reg_inst(
            Some(name),
            m::Op::Gep {
                ty: ty.clone(),
                ptr: base,
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), index),
                ],
            },
        );
    }
    fn reg_load(&mut self, name: &str, ty: m::Ty, ptr: &str, align: u32) {
        self.reg_inst(
            Some(name),
            m::Op::Load {
                ty,
                ptr: m::local(ptr.into()),
                align,
            },
        );
    }
    fn reg_store(&mut self, ty: m::Ty, value: m::Val, ptr: &str, align: u32) {
        self.reg_inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(ty, value),
                ptr: m::local(ptr.into()),
                align,
            },
        );
    }
    fn reg_call(&mut self, name: Option<&str>, ret: m::Ty, callee: m::Val, args: Vec<m::Arg>) {
        self.reg_inst(
            name,
            m::Op::Call {
                result: ret,
                result_attrs: vec![],
                callee,
                args,
                vararg: None,
            },
        );
    }
    fn reg_cmp(&mut self, name: &str, ty: m::Ty, left: m::Val, right: m::Val) {
        self.reg_inst(
            Some(name),
            m::Op::Compare {
                predicate: "eq".into(),
                ty,
                left,
                right,
            },
        );
    }
    fn reg_cond(&mut self, condition: &str, yes: &str, no: &str) {
        self.reg_inst(
            None,
            m::Op::CondBranch {
                condition: m::local(condition.into()),
                yes: yes.into(),
                no: no.into(),
            },
        );
    }
    fn reg_return(&mut self, value: Option<m::Arg>) {
        self.reg_inst(None, m::Op::Return(value));
    }
    fn reg_drop_environment(&mut self, id: ClosureId, ptr: &str) -> CompileResult<()> {
        if resource_closures::owns(self.closure_definition(id)?) {
            self.reg_call(
                None,
                m::Ty::Void,
                m::global(format!("zil.closure.drop.{}", id.0)),
                vec![m::Arg::new(m::Ty::Ptr, m::local(ptr.into()))],
            );
        }
        Ok(())
    }
    pub(super) fn registration_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        match instruction.kind {
            InstructionKind::RegistrationStart { callback, .. } => {
                let result = result(instruction)?;
                let Type::Closure(id) = self.value_type(callback)? else {
                    return invalid(
                        "E-CODEGEN-REGISTRATION",
                        "registration requires concrete F",
                        Some(instruction.span),
                    );
                };
                let ty = environment_type(*id);
                self.reg_call(
                    Some(&format!("v.{}", result.0)),
                    m::Ty::Ptr,
                    m::global(site(self.function.id, result, "new")),
                    vec![m::Arg::new(ty, m::val(value(callback)))],
                );
            }
            InstructionKind::RegistrationValid { registration } => {
                let head = format!("registration.head.{}", result(instruction)?.0);
                self.reg_inst(
                    Some(&head),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::val(value(registration)),
                        align: 8,
                    },
                );
                self.reg_inst(
                    Some(&format!("v.{}", result(instruction)?.0)),
                    m::Op::Compare {
                        predicate: "ne".into(),
                        ty: m::Ty::Ptr,
                        left: m::local(head),
                        right: m::Val::Null,
                    },
                );
            }
            InstructionKind::RegistrationClose { registration } => {
                self.reg_call(
                    None,
                    m::Ty::Void,
                    m::global("zil.registration.close".into()),
                    vec![m::Arg::new(m::Ty::Ptr, m::val(value(registration)))],
                );
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub(super) fn registration_functions(
        &mut self,
        instruction: &Instruction,
        register: FunctionId,
        close: FunctionId,
        callback: ValueId,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let Type::Closure(environment) = self.value_type(callback)? else {
            return invalid(
                "E-CODEGEN-REGISTRATION",
                "registration requires concrete owned F",
                Some(instruction.span),
            );
        };
        let environment = *environment;
        let definition = self.closure_definition(environment)?;
        let by_pointer = resource_closures::by_pointer(definition);
        let entry = definition.function;
        let env = environment_type(environment);
        let align = alignment(&Type::Closure(environment), self.closures)?;
        let ty = frame(environment);
        let register = self
            .functions
            .get(&register)
            .and_then(|f| f.foreign.as_ref())
            .map(|f| f.symbol.clone())
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-REGISTRATION",
                    Some(instruction.span),
                    "register role is absent",
                )]
            })?;
        let close = self
            .functions
            .get(&close)
            .and_then(|f| f.foreign.as_ref())
            .map(|f| f.symbol.clone())
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-REGISTRATION",
                    Some(instruction.span),
                    "close role is absent",
                )]
            })?;
        let init = site(self.function.id, result, "new");
        let trampoline = site(self.function.id, result, "callback");
        let destroy = site(self.function.id, result, "destroy");
        self.reg_begin(
            init,
            m::Ty::Ptr,
            vec![m::Param::new(env.clone(), "environment")],
        );
        self.reg_inst(
            Some("temporary"),
            m::Op::Alloca {
                ty: env.clone(),
                align,
            },
        );
        self.reg_store(
            env.clone(),
            m::local("environment".into()),
            "temporary",
            align,
        );
        self.reg_inst(
            Some("end"),
            m::Op::Gep {
                ty: ty.clone(),
                ptr: m::Val::Null,
                indices: vec![m::Arg::new(m::Ty::Int(32), 1)],
            },
        );
        self.reg_inst(
            Some("size"),
            m::Op::Cast {
                opcode: "ptrtoint".into(),
                arg: m::Arg::new(m::Ty::Ptr, m::local("end".into())),
                to: m::Ty::Int(64),
            },
        );
        self.reg_call(
            Some("frame"),
            m::Ty::Ptr,
            m::global("calloc".into()),
            vec![
                m::Arg::new(m::Ty::Int(64), 1),
                m::Arg::new(m::Ty::Int(64), m::local("size".into())),
            ],
        );
        self.reg_cmp("empty", m::Ty::Ptr, m::local("frame".into()), m::Val::Null);
        self.reg_cond("empty", "allocation_failure", "install");
        self.reg_label("allocation_failure");
        self.reg_drop_environment(environment, "temporary")?;
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr, m::Val::Null)));
        self.reg_label("install");
        for (name, index) in [
            ("destroy.ptr", 0),
            ("handle.ptr", 1),
            ("gate.ptr", 2),
            ("environment.ptr", 3),
        ] {
            self.reg_gep(name, &ty, m::local("frame".into()), index);
        }
        self.reg_store(m::Ty::Ptr, m::global(destroy.clone()), "destroy.ptr", 8);
        self.reg_store(m::Ty::Ptr, m::Val::Null, "handle.ptr", 8);
        self.reg_store(m::Ty::Int(32), m::val(0), "gate.ptr", 4);
        self.reg_store(
            env.clone(),
            m::local("environment".into()),
            "environment.ptr",
            align,
        );
        // All fields and the actual F are initialized before a synchronous callback.
        self.reg_call(
            Some("handle"),
            m::Ty::Ptr,
            m::global(register),
            vec![
                m::Arg::new(m::Ty::Ptr, m::global(trampoline.clone())),
                m::Arg::new(m::Ty::Ptr, m::local("frame".into())),
            ],
        );
        self.reg_cmp(
            "failed",
            m::Ty::Ptr,
            m::local("handle".into()),
            m::Val::Null,
        );
        self.reg_cond("failed", "installation_failure", "published");
        self.reg_label("installation_failure");
        // A null result promises that all entered/raw accesses have ended too.
        self.reg_drop_environment(environment, "environment.ptr")?;
        self.reg_call(
            None,
            m::Ty::Void,
            m::global("free".into()),
            vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
        );
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr, m::Val::Null)));
        self.reg_label("published");
        self.reg_store(m::Ty::Ptr, m::local("handle".into()), "handle.ptr", 8);
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr, m::local("frame".into()))));
        self.event(m::Event::End);

        self.reg_begin(
            trampoline,
            m::Ty::Int(64),
            vec![
                m::Param::new(m::Ty::Ptr, "frame"),
                m::Param::new(m::Ty::Int(32), "argument"),
            ],
        );
        self.reg_gep("gate.ptr", &ty, m::local("frame".into()), 2);
        self.reg_inst(
            Some("old"),
            m::Op::AtomicRmw {
                operation: m::AtomicRmwOp::Or,
                arg: m::Arg::new(m::Ty::Int(32), 1),
                ptr: m::local("gate.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::SeqCst,
            },
        );
        self.reg_cmp(
            "admitted",
            m::Ty::Int(32),
            m::local("old".into()),
            m::val(0),
        );
        self.reg_cond("admitted", "invoke", "reject");
        self.reg_label("reject");
        self.reg_cmp(
            "owned_rejected_busy",
            m::Ty::Int(32),
            m::local("old".into()),
            m::val(2),
        );
        self.reg_cond("owned_rejected_busy", "release_rejected", "rejected");
        self.reg_label("release_rejected");
        self.reg_inst(
            Some("rejected.old"),
            m::Op::AtomicRmw {
                operation: m::AtomicRmwOp::And,
                arg: m::Arg::new(m::Ty::Int(32), -2),
                ptr: m::local("gate.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::SeqCst,
            },
        );
        self.reg_inst(None, m::Op::Branch("rejected".into()));
        self.reg_label("rejected");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64), 1i64 << 32)));
        self.reg_label("invoke");
        self.reg_gep("environment.ptr", &ty, m::local("frame".into()), 3);
        let environment_arg = if by_pointer {
            m::Arg::new(m::Ty::Ptr, m::local("environment.ptr".into()))
        } else {
            self.reg_load("environment", env.clone(), "environment.ptr", align);
            m::Arg::new(env, m::local("environment".into()))
        };
        self.reg_call(
            Some("answer"),
            m::Ty::Int(32),
            m::global(symbol(entry)),
            vec![
                environment_arg,
                m::Arg::new(m::Ty::Int(32), m::local("argument".into())),
            ],
        );
        self.reg_inst(
            Some("released.old"),
            m::Op::AtomicRmw {
                operation: m::AtomicRmwOp::And,
                arg: m::Arg::new(m::Ty::Int(32), -2),
                ptr: m::local("gate.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::SeqCst,
            },
        );
        self.reg_inst(
            Some("answer.wide"),
            m::Op::Cast {
                opcode: "zext".into(),
                arg: m::Arg::new(m::Ty::Int(32), m::local("answer".into())),
                to: m::Ty::Int(64),
            },
        );
        self.reg_return(Some(m::Arg::new(
            m::Ty::Int(64),
            m::local("answer.wide".into()),
        )));
        self.event(m::Event::End);

        self.reg_begin(
            destroy,
            m::Ty::Void,
            vec![m::Param::new(m::Ty::Ptr, "frame")],
        );
        self.reg_gep("gate.ptr", &ty, m::local("frame".into()), 2);
        self.reg_inst(
            Some("closing.old"),
            m::Op::AtomicRmw {
                operation: m::AtomicRmwOp::Or,
                arg: m::Arg::new(m::Ty::Int(32), 2),
                ptr: m::local("gate.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::SeqCst,
            },
        );
        self.reg_gep("handle.ptr", &ty, m::local("frame".into()), 1);
        self.reg_load("handle", m::Ty::Ptr, "handle.ptr", 8);
        // Only this actual backend return establishes raw-entry quiescence.
        self.reg_call(
            None,
            m::Ty::Void,
            m::global(close),
            vec![m::Arg::new(m::Ty::Ptr, m::local("handle".into()))],
        );
        self.reg_inst(
            Some("closed.gate"),
            m::Op::AtomicLoad {
                ty: m::Ty::Int(32),
                ptr: m::local("gate.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::SeqCst,
            },
        );
        self.reg_cmp(
            "drained",
            m::Ty::Int(32),
            m::local("closed.gate".into()),
            m::val(2),
        );
        self.reg_cond("drained", "destroy", "broken_backend");
        self.reg_label("broken_backend");
        self.reg_call(None, m::Ty::Void, m::global("llvm.trap".into()), vec![]);
        self.reg_inst(None, m::Op::Unreachable);
        self.reg_label("destroy");
        self.reg_gep("environment.ptr", &ty, m::local("frame".into()), 3);
        self.reg_drop_environment(environment, "environment.ptr")?;
        self.reg_call(
            None,
            m::Ty::Void,
            m::global("free".into()),
            vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
        );
        self.reg_return(None);
        self.event(m::Event::End);
        Ok(())
    }
}

pub(super) fn runtime(
    lir: &LirView<'_>,
    existing: &m::Code,
    limits: &zil_base::Limits,
    meter: &tasks::Meter,
) -> CompileResult<m::Code> {
    meter.spend(
        existing
            .0
            .len()
            .saturating_add(lir.functions.len())
            .saturating_add(1024),
    )?;
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported("retained registration currently requires Linux x64", None);
    }
    for f in lir.functions {
        if f.foreign
            .as_ref()
            .is_some_and(|a| a.symbol.starts_with("zil.registration."))
        {
            return invalid(
                "E-CODEGEN-FOREIGN",
                "foreign symbol conflicts with sealed registration runtime",
                Some(f.span),
            );
        }
    }
    let mut out = m::Code::new();
    for (name, ret, params) in [
        ("calloc", m::Ty::Ptr, vec![m::Ty::Int(64), m::Ty::Int(64)]),
        ("free", m::Ty::Void, vec![m::Ty::Ptr]),
    ] {
        if let Some(f) = lir
            .functions
            .iter()
            .find(|f| f.foreign.as_ref().is_some_and(|a| a.symbol == name))
        {
            meter.spend(f.parameters.len().saturating_add(1))?;
            let before = meter.remaining();
            let mut left = before;
            for local in &f.parameters {
                copy_aggregates::reserve_enum_metadata(
                    &f.locals[local.0 as usize].ty,
                    &mut left,
                    0,
                    limits,
                )?;
            }
            copy_aggregates::reserve_enum_metadata(&f.return_type, &mut left, 0, limits)?;
            meter.spend(before - left)?;
            let signature = f
                .parameters
                .iter()
                .map(|id| llvm_type(&f.locals[id.0 as usize].ty))
                .collect::<CompileResult<Vec<_>>>()?;
            if signature != params || llvm_type(&f.return_type)? != ret {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "allocator signature conflicts with registration runtime",
                    Some(f.span),
                );
            }
        } else if !existing
            .0
            .iter()
            .any(|event| matches!(event,m::Event::Declare(f) if f.name==name))
        {
            out.push(m::Event::Declare(m::Function {
                name: name.into(),
                linkage: m::Linkage::External,
                signature: m::Signature::new(
                    ret,
                    params.into_iter().map(m::Param::unnamed).collect(),
                ),
                attrs: vec![],
                blocks: None,
            }));
        }
    }
    let instruction =
        |name: Option<&str>, op| m::Event::Instruction(m::Inst::new(name.map(str::to_owned), op));
    out.push(m::Event::Begin(m::Function {
        name: "zil.registration.close".into(),
        linkage: m::Linkage::Private,
        signature: m::Signature::new(m::Ty::Void, vec![m::Param::new(m::Ty::Ptr, "frame")]),
        attrs: vec![],
        blocks: None,
    }));
    out.push(m::Event::Label("entry".into()));
    out.push(instruction(
        Some("empty"),
        m::Op::Compare {
            predicate: "eq".into(),
            ty: m::Ty::Ptr,
            left: m::local("frame".into()),
            right: m::Val::Null,
        },
    ));
    out.push(instruction(
        None,
        m::Op::CondBranch {
            condition: m::local("empty".into()),
            yes: "done".into(),
            no: "close".into(),
        },
    ));
    out.push(m::Event::Label("close".into()));
    out.push(instruction(
        Some("destroy"),
        m::Op::Load {
            ty: m::Ty::Ptr,
            ptr: m::local("frame".into()),
            align: 8,
        },
    ));
    out.push(instruction(
        None,
        m::Op::Call {
            result: m::Ty::Void,
            result_attrs: vec![],
            callee: m::local("destroy".into()),
            args: vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
            vararg: None,
        },
    ));
    out.push(instruction(None, m::Op::Branch("done".into())));
    out.push(m::Event::Label("done".into()));
    out.push(instruction(None, m::Op::Return(None)));
    out.push(m::Event::End);
    out.push(m::Event::Begin(m::Function {
        name: "zil.registration.drop".into(),
        linkage: m::Linkage::Private,
        signature: m::Signature::new(m::Ty::Void, vec![m::Param::new(m::Ty::Ptr, "head")]),
        attrs: vec![],
        blocks: None,
    }));
    out.push(m::Event::Label("entry".into()));
    out.push(instruction(
        Some("frame"),
        m::Op::Load {
            ty: m::Ty::Ptr,
            ptr: m::local("head".into()),
            align: 8,
        },
    ));
    out.push(instruction(
        None,
        m::Op::Call {
            result: m::Ty::Void,
            result_attrs: vec![],
            callee: m::global("zil.registration.close".into()),
            args: vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
            vararg: None,
        },
    ));
    out.push(instruction(None, m::Op::Return(None)));
    out.push(m::Event::End);
    Ok(out)
}

pub(super) fn reserve_function(
    f: &TypedFunction,
    closures: &BTreeMap<ClosureId, &ClosureDefinition>,
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    meter: &tasks::Meter,
) -> CompileResult<()> {
    fn ty(
        t: &Type,
        closures: &BTreeMap<ClosureId, &ClosureDefinition>,
        active: &mut BTreeSet<ClosureId>,
        depth: usize,
        meter: &tasks::Meter,
    ) -> CompileResult<()> {
        meter.spend(64)?;
        if depth > 128 {
            return Err(vec![Diagnostic::limit(
                Phase::Codegen,
                "registration inline layout depth exceeds native bound",
                None,
            )]);
        }
        match t {
            Type::Dyn { .. } => {
                return unsupported(
                    "dynamic registration environments require explicit retained interface lowering",
                    None,
                );
            }
            Type::Closure(id) => {
                meter.spend(active.len().saturating_add(1))?;
                if !active.insert(*id) {
                    return invalid(
                        "E-CODEGEN-REGISTRATION",
                        "recursive inline registration environment",
                        None,
                    );
                }
                let c = closures.get(id).ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-REGISTRATION",
                        None,
                        "registration environment absent",
                    )]
                })?;
                meter.spend(c.captures.len().saturating_add(c.lending_captures.len()))?;
                for t in &c.captures {
                    ty(t, closures, active, depth + 1, meter)?;
                }
                active.remove(id);
            }
            Type::Managed(p) | Type::Desc(p)
                if matches!(
                    p.as_ref(),
                    Type::Registration { .. }
                        | Type::Task { .. }
                        | Type::TaskI32
                        | Type::ByteBuffer
                        | Type::OwnedStore { .. }
                ) => {}
            Type::BorrowedStorage { storage: t, .. }
            | Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::Registration { environment: t }
            | Type::Opaque { hidden: t, .. }
            | Type::OwnedStore { element: t }
            | Type::Array { element: t, .. }
            | Type::Slice { element: t } => ty(t, closures, active, depth + 1, meter)?,
            Type::Tuple(ts) => {
                for t in ts {
                    ty(t, closures, active, depth + 1, meter)?
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                meter.spend(name.len())?;
                for (name, t) in fields {
                    meter.spend(name.len())?;
                    ty(t, closures, active, depth + 1, meter)?
                }
            }
            Type::Enum { name, variants } => {
                meter.spend(name.len())?;
                for (name, ts) in variants {
                    meter.spend(name.len())?;
                    for t in ts {
                        ty(t, closures, active, depth + 1, meter)?
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                ty(error, closures, active, depth + 1, meter)?;
                ty(result, closures, active, depth + 1, meter)?
            }
            Type::BorrowedFunctionPointer { .. }
            | Type::FunctionPointer { .. }
            | Type::ForeignCallback { .. } => {}
            Type::Named(name) | Type::Unsupported(name) => meter.spend(name.len())?,
            Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::ForeignHandle
            | Type::CWideString
            | Type::ByteBuffer
            | Type::Task { .. }
            | Type::TaskI32
            | Type::SyncCell(_) => {}
        }
        Ok(())
    }
    for b in &f.blocks {
        meter.spend(1)?;
        for i in &b.instructions {
            meter.spend(1)?;
            match i.kind {
                InstructionKind::RegistrationStart {
                    register,
                    close,
                    callback,
                    ..
                } => {
                    for id in [register, close] {
                        meter.spend(functions.len().saturating_add(32))?;
                        let foreign = functions
                            .get(&id)
                            .and_then(|f| f.foreign.as_ref())
                            .ok_or_else(|| {
                                vec![Diagnostic::error(
                                    Phase::Codegen,
                                    "E-CODEGEN-REGISTRATION",
                                    None,
                                    "registration role target absent",
                                )]
                            })?;
                        meter.spend(foreign.symbol.len().saturating_mul(4))?;
                    }
                    // Fixed helper/trampoline/close node and text construction plus the
                    // actual expanded alignment/capture classification walks.
                    meter.spend(8192)?;
                    ty(
                        &f.values[callback.0 as usize],
                        closures,
                        &mut BTreeSet::new(),
                        0,
                        meter,
                    )?;
                }
                InstructionKind::RegistrationValid { .. }
                | InstructionKind::RegistrationClose { .. } => meter.spend(128)?,
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration343_actual_helper_reservation_has_exact_cumulative_limit() {
        // Internal planning fixture only; source/runtime/Verify evidence lives
        // in the driver tests. This does not grant authority to the raw model.
        let f = TypedFunction {
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "budget".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: zil_ir::PassingMode::Infer,
            locals: vec![],
            values: vec![Type::Closure(ClosureId(0)), Type::Unit],
            blocks: vec![zil_ir::BasicBlock {
                id: zil_ir::BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::RegistrationStart {
                        register: FunctionId(1),
                        close: FunctionId(2),
                        callback: ValueId(0),
                        region: 0,
                    },
                    span: Span::default(),
                }],
                terminator: Terminator::Return(None),
            }],
            entry: zil_ir::BlockId(0),
            span: Span::default(),
        };
        let capture = Type::Record {
            name: "wide".into(),
            fields: (0..2048)
                .map(|i| (format!("field_{i}"), Type::I32))
                .collect(),
        };
        let definition = ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(3),
            parameters: vec![Type::I32],
            result: Type::I32,
            captures: vec![capture],
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        };
        let closures = BTreeMap::from([(definition.id, &definition)]);
        let mut register = f.clone();
        register.id = FunctionId(1);
        register.foreign = Some(zil_ir::ForeignDeclaration {
            abi: "C".into(),
            symbol: "register".into(),
            noescape_parameters: vec![],
            static_callbacks: true,
            scoped_callback: Some(zil_ir::ScopedCallbackRole::Register),
        });
        let mut close = register.clone();
        close.id = FunctionId(2);
        close.foreign.as_mut().unwrap().symbol = "close".into();
        let functions = BTreeMap::from([(register.id, &register), (close.id, &close)]);
        let meter = tasks::Meter::new(usize::MAX);
        reserve_function(&f, &closures, &functions, &meter).unwrap();
        let n = usize::MAX - meter.remaining();
        let meter = tasks::Meter::new(n);
        reserve_function(&f, &closures, &functions, &meter).unwrap();
        assert_eq!(meter.remaining(), 0);
        let meter = tasks::Meter::new(n - 1);
        assert_eq!(
            reserve_function(&f, &closures, &functions, &meter).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        let meter = tasks::Meter::new(n);
        reserve_function(&f, &closures, &functions, &meter).unwrap();
        assert_eq!(meter.remaining(), 0);
        let cost = |closures: &BTreeMap<ClosureId, &ClosureDefinition>,
                    functions: &BTreeMap<FunctionId, &TypedFunction>| {
            let meter = tasks::Meter::new(usize::MAX);
            reserve_function(&f, closures, functions, &meter).unwrap();
            usize::MAX - meter.remaining()
        };
        let mut long_register = register.clone();
        long_register.foreign.as_mut().unwrap().symbol = "R".repeat(65536);
        let long_functions =
            BTreeMap::from([(long_register.id, &long_register), (close.id, &close)]);
        assert!(cost(&closures, &long_functions) >= n + 4 * (65536 - 8));
        let mut child = definition.clone();
        child.id = ClosureId(1);
        child.captures = vec![Type::I32];
        let mut single = definition.clone();
        single.captures = vec![Type::Closure(child.id)];
        let mut repeated = single.clone();
        repeated.captures = vec![Type::Closure(child.id); 8];
        let one = BTreeMap::from([(single.id, &single), (child.id, &child)]);
        let eight = BTreeMap::from([(repeated.id, &repeated), (child.id, &child)]);
        assert!(cost(&eight, &functions) >= cost(&one, &functions) + 7 * 128);
        let mut cyclic = definition.clone();
        cyclic.captures = vec![Type::Closure(cyclic.id)];
        let cycle = BTreeMap::from([(cyclic.id, &cyclic)]);
        assert_eq!(
            reserve_function(&f, &cycle, &functions, &tasks::Meter::new(n)).unwrap_err()[0]
                .category,
            zil_base::ResultCategory::SafeReject
        );
        let mut heap = definition.clone();
        heap.captures = vec![Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(Type::Package {
                name: "heap".into(),
                fields: vec![("callback".into(), Type::Closure(heap.id))],
            }),
        }))];
        let heap_map = BTreeMap::from([(heap.id, &heap)]);
        assert!(cost(&heap_map, &functions) < cost(&one, &functions));
        eprintln!("registration343 actual helper planning N={n}");
    }
}
