//! Stable external registration admission. This machine protocol does not grant
//! thread capability: the independent language checks still admit same-thread F.
//! Gate states: 0 OpenIdle, 1 OpenBusy, 2 ClosingIdle, 3 ClosingBusy, 4 Closed.
//! A callback and close compete on this one atomic gate. The separate driver
//! byte excludes request/poll/cleanup reentry, including from environment Drop.
//! Neither idle gate nor rejected callbacks prove raw-pointer quiescence: only
//! the provider's successful drain covers pre-trampoline holders and all access.
//!
//! The initialization release-store publishes F before register can expose the
//! raw pair. Each successful callback CAS acquires either this publication or
//! the previous callback's release RMW, so serialized FnMut calls see prior
//! writes. Closing an active callback preserves its busy bit; its release RMW
//! produces ClosingIdle, acquired by poll before F is destroyed. The driver
//! byte serializes owner/provider calls and keeps destructor reentry Pending.
//! Raw lifetime is independent of these memory-order edges: drain must settle
//! even rejected/pre-trampoline holders before cleanup or final free.
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
        m::Ty::Int(8),
        environment_type(environment),
    ])
}
impl FunctionEmitter<'_> {
    fn reg_atomic_load(&mut self, name: &str, bits: u32, ptr: &str) {
        self.reg_inst(
            Some(name),
            m::Op::AtomicLoad {
                ty: m::Ty::Int(bits),
                ptr: m::local(ptr.into()),
                align: bits / 8,
                ordering: m::AtomicOrdering::Acquire,
            },
        );
    }
    fn reg_atomic_store(&mut self, bits: u32, v: i64, ptr: &str) {
        self.reg_inst(
            None,
            m::Op::AtomicStore {
                arg: m::Arg::new(m::Ty::Int(bits), v),
                ptr: m::local(ptr.into()),
                align: bits / 8,
                ordering: m::AtomicOrdering::Release,
            },
        );
    }
    fn reg_claim(&mut self, name: &str, bits: u32, ptr: &str, from: m::Val, to: m::Val) {
        let result = m::Ty::Struct(vec![m::Ty::Int(bits), m::Ty::Int(1)]);
        self.reg_inst(
            Some(&format!("{name}.pair")),
            m::Op::AtomicCompareExchange {
                result: result.clone(),
                expected: m::Arg::new(m::Ty::Int(bits), from),
                desired: to,
                ptr: m::local(ptr.into()),
                align: bits / 8,
                success: m::AtomicOrdering::AcqRel,
                failure: m::AtomicOrdering::Acquire,
            },
        );
        self.reg_inst(
            Some(name),
            m::Op::Extract {
                arg: m::Arg::new(result, m::local(format!("{name}.pair"))),
                index: 1,
            },
        );
    }
    #[allow(dead_code)]
    pub(in crate::emit) fn threaded_registration_functions(
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
        // Publish the complete environment before exposing the raw pair. A
        // first callback CAS must acquire this release, even if the provider
        // invokes it during register rather than after returning the handle.
        self.reg_atomic_store(8, 0, "busy.ptr");
        self.reg_atomic_store(32, 0, "state.ptr");
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
        self.reg_claim("admitted", 32, "state.ptr", m::val(0), m::val(1));
        self.reg_cond("admitted", "invoke", "reject");
        self.reg_label("reject");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64), 1i64 << 32)));
        self.reg_label("invoke");
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
        // Release F's writes while preserving any close bit set during F.
        self.reg_inst(
            None,
            m::Op::AtomicRmw {
                operation: m::AtomicRmwOp::Subtract,
                arg: m::Arg::new(m::Ty::Int(32), 1),
                ptr: m::local("state.ptr".into()),
                align: 4,
                ordering: m::AtomicOrdering::Release,
            },
        );
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
        self.reg_gep("busy.ptr", &ty, m::local("frame".into()), 5);
        self.reg_claim("driver", 8, "busy.ptr", m::val(0), m::val(1));
        self.reg_cond("driver", "check", "done");
        self.reg_label("check");
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_inst(None, m::Op::Branch("retry".into()));
        self.reg_label("retry");
        self.reg_atomic_load("state", 32, "state.ptr");
        self.reg_inst(
            Some("open"),
            m::Op::Compare {
                predicate: "ult".into(),
                ty: m::Ty::Int(32),
                left: m::local("state".into()),
                right: m::val(2),
            },
        );
        self.reg_cond("open", "close_admission", "release");
        self.reg_label("close_admission");
        self.reg_inst(
            Some("closing"),
            m::Op::Binary {
                opcode: "add".into(),
                ty: m::Ty::Int(32),
                left: m::local("state".into()),
                right: m::val(2),
            },
        );
        self.reg_claim(
            "closed.admission",
            32,
            "state.ptr",
            m::local("state".into()),
            m::local("closing".into()),
        );
        self.reg_cond("closed.admission", "request", "retry");
        self.reg_label("request");
        self.reg_gep("handle.ptr", &ty, m::local("frame".into()), 3);
        self.reg_load("handle", m::Ty::Ptr, "handle.ptr", 8);
        self.reg_call(
            None,
            m::Ty::Void,
            m::global(request),
            vec![m::Arg::new(m::Ty::Ptr, m::local("handle".into()))],
        );
        self.reg_inst(None, m::Op::Branch("release".into()));
        self.reg_label("release");
        self.reg_atomic_store(8, 0, "busy.ptr");
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
        self.reg_gep("busy.ptr", &ty, m::local("frame".into()), 5);
        self.reg_claim("driver", 8, "busy.ptr", m::val(0), m::val(1));
        self.reg_cond("driver", "check_state", "pending");
        self.reg_label("check_state");
        self.reg_gep("state.ptr", &ty, m::local("frame".into()), 4);
        self.reg_atomic_load("state", 32, "state.ptr");
        self.reg_cmp(
            "closed",
            m::Ty::Int(32),
            m::local("state".into()),
            m::val(4),
        );
        self.reg_cond("closed", "release_ready", "check_idle");
        self.reg_label("check_idle");
        self.reg_cmp("idle", m::Ty::Int(32), m::local("state".into()), m::val(2));
        self.reg_cond("idle", "poll", "release_pending");
        self.reg_label("poll");
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
        self.reg_inst(None, m::Op::Branch("release_pending".into()));
        self.reg_label("release_pending");
        self.reg_atomic_store(8, 0, "busy.ptr");
        self.reg_inst(None, m::Op::Branch("pending".into()));
        self.reg_label("cleanup");
        self.reg_gep("env.ptr", &ty, m::local("frame".into()), 6);
        self.reg_drop_environment(id, "env.ptr")?;
        self.reg_atomic_store(32, 4, "state.ptr");
        self.reg_inst(None, m::Op::Branch("release_ready".into()));
        self.reg_label("release_ready");
        self.reg_atomic_store(8, 0, "busy.ptr");
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
        self.reg_atomic_load("state", 32, "state.ptr");
        self.reg_cmp(
            "closed",
            m::Ty::Int(32),
            m::local("state".into()),
            m::val(4),
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
