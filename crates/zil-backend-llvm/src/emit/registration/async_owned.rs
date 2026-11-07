//! Same-thread stable external registration. No raw pointer is freed by request
//! or Pending drain; the provider's successful drain covers ALL raw entries.
use super::*;
fn site(f: FunctionId, v: ValueId, role: &str) -> String {
    format!("zil.async_registration.{role}.{}.{}", f.0, v.0)
}
fn frame(environment: ClosureId) -> m::Ty {
    m::Ty::Struct(vec![
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Int(32),
        m::Ty::Int(1),
        environment_type(environment),
    ])
}
impl FunctionEmitter<'_> {
    pub(in crate::emit) fn async_registration_instruction(
        &mut self,
        i: &Instruction,
    ) -> CompileResult<()> {
        match i.kind {
            InstructionKind::AsyncRegistrationStart { callback, .. } => {
                let out = result(i)?;
                let Type::Closure(id) = self.value_type(callback)? else {
                    return unsupported(
                        "pollable registration needs concrete closure",
                        Some(i.span),
                    );
                };
                self.reg_call(
                    Some(&format!("v.{}", out.0)),
                    m::Ty::Ptr,
                    m::global(site(self.function.id, out, "new")),
                    vec![m::Arg::new(environment_type(*id), m::val(value(callback)))],
                );
            }
            InstructionKind::AsyncRegistrationValid { registration } => {
                let name = format!("async.valid.head.{}", result(i)?.0);
                self.reg_inst(
                    Some(&name),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::val(value(registration)),
                        align: 8,
                    },
                );
                self.reg_inst(
                    Some(&format!("v.{}", result(i)?.0)),
                    m::Op::Compare {
                        predicate: "ne".into(),
                        ty: m::Ty::Ptr,
                        left: m::local(name),
                        right: m::Val::Null,
                    },
                );
            }
            InstructionKind::AsyncRegistrationRequestClose { registration } => {
                self.reg_call(
                    None,
                    m::Ty::Void,
                    m::global("zil.async_registration.request_head".into()),
                    vec![m::Arg::new(m::Ty::Ptr, m::val(value(registration)))],
                );
            }
            InstructionKind::AsyncRegistrationPollClose { registration } => {
                let out = result(i)?;
                let ty = llvm_type(self.value_type(out)?)?;
                self.reg_call(
                    Some(&format!("async.closed.{}", out.0)),
                    m::Ty::Int(1),
                    m::global("zil.async_registration.poll_head".into()),
                    vec![m::Arg::new(m::Ty::Ptr, m::val(value(registration)))],
                );
                self.reg_inst(
                    Some(&format!("async.tag.{}", out.0)),
                    m::Op::Cast {
                        opcode: "zext".into(),
                        arg: m::Arg::new(
                            m::Ty::Int(1),
                            m::local(format!("async.closed.{}", out.0)),
                        ),
                        to: m::Ty::Int(32),
                    },
                );
                self.reg_inst(
                    Some(&format!("v.{}", out.0)),
                    m::Op::Insert {
                        arg: m::Arg::new(ty, m::Val::Zero),
                        field: m::Arg::new(
                            m::Ty::Int(32),
                            m::local(format!("async.tag.{}", out.0)),
                        ),
                        index: 0,
                    },
                );
            }
            InstructionKind::AsyncRegistrationFinishDestroy { registration }
                    | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => self.reg_call(
                None,
                m::Ty::Void,
                m::global("zil.async_registration.finish".into()),
                vec![m::Arg::new(m::Ty::Ptr, m::val(value(registration)))],
            ),
            _ => unreachable!(),
        }
        Ok(())
    }
    pub(in crate::emit) fn async_registration_functions(
        &mut self,
        i: &Instruction,
        register: FunctionId,
        request: FunctionId,
        poll: FunctionId,
        callback: ValueId,
    ) -> CompileResult<()> {
        let out = result(i)?;
        let Type::Closure(id) = self.value_type(callback)? else {
            return unsupported("pollable registration needs concrete closure", Some(i.span));
        };
        let id = *id;
        let c = self.closure_definition(id)?;
        let by_pointer = resource_closures::by_pointer(c);
        let entry = c.function;
        let env = environment_type(id);
        let align = alignment(&Type::Closure(id), self.closures)?;
        let ty = frame(id);
        let provider = |f: FunctionId| {
            self.functions
                .get(&f)
                .and_then(|f| f.foreign.as_ref())
                .map(|f| f.symbol.clone())
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-REGISTRATION",
                        Some(i.span),
                        "async provider absent",
                    )]
                })
        };
        let register = provider(register)?;
        let request = provider(request)?;
        let poll = provider(poll)?;
        let new = site(self.function.id, out, "new");
        let cb = site(self.function.id, out, "callback");
        let req = site(self.function.id, out, "request");
        let drain = site(self.function.id, out, "poll");
        let finish = site(self.function.id, out, "finish");
        self.reg_begin(
            new,
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
        self.reg_cond("empty", "failure", "install");
        self.reg_label("failure");
        self.reg_drop_environment(id, "temporary")?;
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr, m::Val::Null)));
        self.reg_label("install");
        for (name, n) in [
            ("request.ptr", 0),
            ("poll.ptr", 1),
            ("finish.ptr", 2),
            ("handle.ptr", 3),
            ("state.ptr", 4),
            ("busy.ptr", 5),
            ("env.ptr", 6),
        ] {
            self.reg_gep(name, &ty, m::local("frame".into()), n);
        }
        for (p, v) in [
            ("request.ptr", req.clone()),
            ("poll.ptr", drain.clone()),
            ("finish.ptr", finish.clone()),
        ] {
            self.reg_store(m::Ty::Ptr, m::global(v), p, 8);
        }
        self.reg_store(
            env.clone(),
            m::local("environment".into()),
            "env.ptr",
            align,
        );
        self.reg_call(
            Some("handle"),
            m::Ty::Ptr,
            m::global(register),
            vec![
                m::Arg::new(m::Ty::Ptr, m::global(cb.clone())),
                m::Arg::new(m::Ty::Ptr, m::local("frame".into())),
            ],
        );
        self.reg_cmp(
            "failed",
            m::Ty::Ptr,
            m::local("handle".into()),
            m::Val::Null,
        );
        self.reg_cond("failed", "install_failure", "published");
        self.reg_label("install_failure");
        self.reg_drop_environment(id, "env.ptr")?;
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
            cb,
            m::Ty::Int(64),
            vec![
                m::Param::new(m::Ty::Ptr, "frame"),
                m::Param::new(m::Ty::Int(32), "argument"),
            ],
        );
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_load("state", m::Ty::Int(32), "state.ptr", 4);
        self.reg_cmp("open", m::Ty::Int(32), m::local("state".into()), m::val(0));
        self.reg_cond("open", "check_busy", "reject");
        self.reg_label("check_busy");
        self.reg_gep("busy.ptr", &ty, m::local("frame".into()), 5);
        self.reg_load("busy", m::Ty::Int(1), "busy.ptr", 1);
        self.reg_cond("busy", "reject", "invoke");
        self.reg_label("reject");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64), 1i64 << 32)));
        self.reg_label("invoke");
        self.reg_store(m::Ty::Int(1), m::val(true), "busy.ptr", 1);
        self.reg_gep("env.ptr", &ty, m::local("frame".into()), 6);
        let arg = if by_pointer {
            m::Arg::new(m::Ty::Ptr, m::local("env.ptr".into()))
        } else {
            self.reg_load("env", env.clone(), "env.ptr", align);
            m::Arg::new(env, m::local("env".into()))
        };
        self.reg_call(
            Some("answer"),
            m::Ty::Int(32),
            m::global(symbol(entry)),
            vec![
                arg,
                m::Arg::new(m::Ty::Int(32), m::local("argument".into())),
            ],
        );
        self.reg_store(m::Ty::Int(1), m::val(false), "busy.ptr", 1);
        self.reg_inst(
            Some("wide"),
            m::Op::Cast {
                opcode: "zext".into(),
                arg: m::Arg::new(m::Ty::Int(32), m::local("answer".into())),
                to: m::Ty::Int(64),
            },
        );
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64), m::local("wide".into()))));
        self.event(m::Event::End);
        self.reg_begin(
            req.clone(),
            m::Ty::Void,
            vec![m::Param::new(m::Ty::Ptr, "frame")],
        );
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_load("state", m::Ty::Int(32), "state.ptr", 4);
        self.reg_cmp("open", m::Ty::Int(32), m::local("state".into()), m::val(0));
        self.reg_cond("open", "request", "done");
        self.reg_label("request");
        self.reg_store(m::Ty::Int(32), m::val(1), "state.ptr", 4);
        // Preserve an already active callback's busy bit through reentrant close.
        self.reg_gep("busy.ptr", &ty, m::local("frame".into()), 5);
        self.reg_load("was.busy", m::Ty::Int(1), "busy.ptr", 1);
        self.reg_store(m::Ty::Int(1), m::val(true), "busy.ptr", 1);
        self.reg_gep("handle.ptr", &ty, m::local("frame".into()), 3);
        self.reg_load("handle", m::Ty::Ptr, "handle.ptr", 8);
        self.reg_call(
            None,
            m::Ty::Void,
            m::global(request),
            vec![m::Arg::new(m::Ty::Ptr, m::local("handle".into()))],
        );
        self.reg_store(m::Ty::Int(1), m::local("was.busy".into()), "busy.ptr", 1);
        self.reg_inst(None, m::Op::Branch("done".into()));
        self.reg_label("done");
        self.reg_return(None);
        self.event(m::Event::End);
        self.reg_begin(
            drain,
            m::Ty::Int(1),
            vec![m::Param::new(m::Ty::Ptr, "frame")],
        );
        self.reg_call(
            None,
            m::Ty::Void,
            m::global(req),
            vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
        );
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_load("state", m::Ty::Int(32), "state.ptr", 4);
        self.reg_cmp(
            "closed",
            m::Ty::Int(32),
            m::local("state".into()),
            m::val(2),
        );
        self.reg_cond("closed", "ready", "check_busy");
        self.reg_label("check_busy");
        self.reg_gep("busy.ptr", &ty, m::local("frame".into()), 5);
        self.reg_load("busy", m::Ty::Int(1), "busy.ptr", 1);
        self.reg_cond("busy", "pending", "poll");
        self.reg_label("poll");
        self.reg_store(m::Ty::Int(1), m::val(true), "busy.ptr", 1);
        self.reg_gep("handle.ptr", &ty, m::local("frame".into()), 3);
        self.reg_load("handle", m::Ty::Ptr, "handle.ptr", 8);
        self.reg_call(
            Some("status"),
            m::Ty::Int(32),
            m::global(poll),
            vec![m::Arg::new(m::Ty::Ptr, m::local("handle".into()))],
        );
        self.reg_cmp(
            "nofuture",
            m::Ty::Int(32),
            m::local("status".into()),
            m::val(1),
        );
        self.reg_cond("nofuture", "cleanup", "check_pending");
        self.reg_label("check_pending");
        self.reg_cmp(
            "is.pending",
            m::Ty::Int(32),
            m::local("status".into()),
            m::val(0),
        );
        self.reg_cond("is.pending", "release", "broken");
        self.reg_label("release");
        self.reg_store(m::Ty::Int(1), m::val(false), "busy.ptr", 1);
        self.reg_inst(None, m::Op::Branch("pending".into()));
        self.reg_label("cleanup");
        self.reg_gep("env.ptr", &ty, m::local("frame".into()), 6);
        self.reg_drop_environment(id, "env.ptr")?;
        self.reg_store(m::Ty::Int(32), m::val(2), "state.ptr", 4);
        self.reg_store(m::Ty::Int(1), m::val(false), "busy.ptr", 1);
        self.reg_inst(None, m::Op::Branch("ready".into()));
        self.reg_label("broken");
        self.reg_call(None, m::Ty::Void, m::global("llvm.trap".into()), vec![]);
        self.reg_inst(None, m::Op::Unreachable);
        self.reg_label("pending");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(1), false)));
        self.reg_label("ready");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(1), true)));
        self.event(m::Event::End);
        self.reg_begin(
            finish,
            m::Ty::Void,
            vec![m::Param::new(m::Ty::Ptr, "frame")],
        );
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_load("state", m::Ty::Int(32), "state.ptr", 4);
        self.reg_cmp(
            "closed",
            m::Ty::Int(32),
            m::local("state".into()),
            m::val(2),
        );
        self.reg_cond("closed", "free", "broken");
        self.reg_label("broken");
        self.reg_call(None, m::Ty::Void, m::global("llvm.trap".into()), vec![]);
        self.reg_inst(None, m::Op::Unreachable);
        self.reg_label("free");
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

pub(super) fn runtime(meter: &tasks::Meter) -> CompileResult<m::Code> {
    meter.spend(8192)?;
    let mut out = m::Code::new();
    let inst =
        |name: Option<&str>, op| m::Event::Instruction(m::Inst::new(name.map(str::to_owned), op));
    let head = m::Ty::Struct(vec![m::Ty::Ptr; 4]);
    for (op, index, ret, nullable) in [
        ("request", 0, m::Ty::Void, m::Ty::Void),
        ("poll", 1, m::Ty::Int(1), m::Ty::Int(1)),
        ("finish", 2, m::Ty::Void, m::Ty::Void),
    ] {
        out.push(m::Event::Begin(m::Function {
            name: format!("zil.async_registration.{op}"),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(ret.clone(), vec![m::Param::new(m::Ty::Ptr, "frame")]),
            attrs: vec![],
            blocks: None,
        }));
        out.push(m::Event::Label("entry".into()));
        out.push(inst(
            Some("null"),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local("frame".into()),
                right: m::Val::Null,
            },
        ));
        out.push(inst(
            None,
            m::Op::CondBranch {
                condition: m::local("null".into()),
                yes: "empty".into(),
                no: "invoke".into(),
            },
        ));
        out.push(m::Event::Label("empty".into()));
        out.push(inst(
            None,
            m::Op::Return(if nullable == m::Ty::Void {
                None
            } else {
                Some(m::Arg::new(m::Ty::Int(1), true))
            }),
        ));
        out.push(m::Event::Label("invoke".into()));
        out.push(inst(
            Some("slot"),
            m::Op::Gep {
                ty: head.clone(),
                ptr: m::local("frame".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), index),
                ],
            },
        ));
        out.push(inst(
            Some("function"),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("slot".into()),
                align: 8,
            },
        ));
        out.push(inst(
            if ret == m::Ty::Void {
                None
            } else {
                Some("answer")
            },
            m::Op::Call {
                result: ret.clone(),
                result_attrs: vec![],
                callee: m::local("function".into()),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
                vararg: None,
            },
        ));
        out.push(inst(
            None,
            m::Op::Return(if ret == m::Ty::Void {
                None
            } else {
                Some(m::Arg::new(ret, m::local("answer".into())))
            }),
        ));
        out.push(m::Event::End);
    }
    for (op, target, ret) in [
        ("request_head", "request", m::Ty::Void),
        ("poll_head", "poll", m::Ty::Int(1)),
        ("drop", "poll", m::Ty::Int(1)),
    ] {
        out.push(m::Event::Begin(m::Function {
            name: format!("zil.async_registration.{op}"),
            linkage: m::Linkage::Private,
            signature: m::Signature::new(ret.clone(), vec![m::Param::new(m::Ty::Ptr, "head")]),
            attrs: vec![],
            blocks: None,
        }));
        out.push(m::Event::Label("entry".into()));
        out.push(inst(
            Some("frame"),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("head".into()),
                align: 8,
            },
        ));
        out.push(inst(
            if ret == m::Ty::Void {
                None
            } else {
                Some("answer")
            },
            m::Op::Call {
                result: ret.clone(),
                result_attrs: vec![],
                callee: m::global(format!("zil.async_registration.{target}")),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
                vararg: None,
            },
        ));
        if op == "drop" {
            out.push(inst(
                None,
                m::Op::CondBranch {
                    condition: m::local("answer".into()),
                    yes: "finish".into(),
                    no: "done".into(),
                },
            ));
            out.push(m::Event::Label("finish".into()));
            out.push(inst(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global("zil.async_registration.finish".into()),
                    args: vec![m::Arg::new(m::Ty::Ptr, m::local("frame".into()))],
                    vararg: None,
                },
            ));
            out.push(inst(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Ptr, m::Val::Null),
                    ptr: m::local("head".into()),
                    align: 8,
                },
            ));
            out.push(inst(None, m::Op::Branch("done".into())));
            out.push(m::Event::Label("done".into()));
        }
        out.push(inst(
            None,
            m::Op::Return(if ret == m::Ty::Void {
                None
            } else {
                Some(m::Arg::new(ret, m::local("answer".into())))
            }),
        ));
        out.push(m::Event::End);
    }
    Ok(out)
}
