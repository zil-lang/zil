//! Stable owning registration storage. Busy protects F; only the strong backend
//! close contract ends all raw entries (including pre-atomic/rejected entries).
use super::*;

fn site(function: FunctionId, value: ValueId, role: &str) -> String {
    format!("zil.registration.{role}.{}.{}", function.0, value.0)
}
fn frame(environment: ClosureId) -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Ptr, m::Ty::Int(32), environment_type(environment)])
}
impl FunctionEmitter<'_> {
    fn reg_inst(&mut self, name: Option<&str>, op: m::Op) {
        self.event(m::Event::Instruction(m::Inst::new(name.map(str::to_owned), op)));
    }
    fn reg_label(&mut self, name: &str) { self.event(m::Event::Label(name.into())); }
    fn reg_begin(&mut self, name: String, ret: m::Ty, params: Vec<m::Param>) {
        self.event(m::Event::Begin(m::Function { name, linkage: m::Linkage::Private, signature:m::Signature::new(ret,params),attrs:vec![],blocks:None }));
        self.reg_label("entry");
    }
    fn reg_gep(&mut self, name: &str, ty: &m::Ty, base: m::Val, index:u32) {
        self.reg_inst(Some(name),m::Op::Gep { ty:ty.clone(),ptr:base,indices:vec![m::Arg::new(m::Ty::Int(32),0),m::Arg::new(m::Ty::Int(32),index)] });
    }
    fn reg_load(&mut self, name:&str, ty:m::Ty, ptr:&str, align:u32) {
        self.reg_inst(Some(name),m::Op::Load { ty,ptr:m::local(ptr.into()),align });
    }
    fn reg_store(&mut self, ty:m::Ty, value:m::Val, ptr:&str, align:u32) {
        self.reg_inst(None,m::Op::Store { arg:m::Arg::new(ty,value),ptr:m::local(ptr.into()),align });
    }
    fn reg_call(&mut self, name:Option<&str>, ret:m::Ty, callee:m::Val,args:Vec<m::Arg>) {
        self.reg_inst(name,m::Op::Call { result:ret,result_attrs:vec![],callee,args,vararg:None });
    }
    fn reg_cmp(&mut self,name:&str,ty:m::Ty,left:m::Val,right:m::Val) {
        self.reg_inst(Some(name),m::Op::Compare { predicate:"eq".into(),ty,left,right });
    }
    fn reg_cond(&mut self, condition:&str,yes:&str,no:&str) {
        self.reg_inst(None,m::Op::CondBranch { condition:m::local(condition.into()),yes:yes.into(),no:no.into() });
    }
    fn reg_return(&mut self, value:Option<m::Arg>) { self.reg_inst(None,m::Op::Return(value)); }
    fn reg_drop_environment(&mut self, id:ClosureId, ptr:&str) -> CompileResult<()> {
        if resource_closures::owns(self.closure_definition(id)?) {
            self.reg_call(None,m::Ty::Void,m::global(format!("zil.closure.drop.{}",id.0)),vec![m::Arg::new(m::Ty::Ptr,m::local(ptr.into()))]);
        }
        Ok(())
    }
    pub(super) fn registration_functions(&mut self,instruction:&Instruction,register:FunctionId,close:FunctionId,callback:ValueId)->CompileResult<()> {
        let result=result(instruction)?;
        let Type::Closure(environment)=self.value_type(callback)? else { return invalid("E-CODEGEN-REGISTRATION","registration requires concrete owned F",Some(instruction.span)); };
        let environment=*environment;
        let definition=self.closure_definition(environment)?.clone();
        let env=environment_type(environment);
        let align=alignment(&Type::Closure(environment),self.closures)?;
        let ty=frame(environment);
        let register=self.functions.get(&register).and_then(|f|f.foreign.as_ref()).map(|f|f.symbol.clone()).ok_or_else(||vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-REGISTRATION",Some(instruction.span),"register role is absent")])?;
        let close=self.functions.get(&close).and_then(|f|f.foreign.as_ref()).map(|f|f.symbol.clone()).ok_or_else(||vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-REGISTRATION",Some(instruction.span),"close role is absent")])?;
        let init=site(self.function.id,result,"new");
        let trampoline=site(self.function.id,result,"callback");
        let destroy=site(self.function.id,result,"destroy");
        self.reg_begin(init,m::Ty::Ptr,vec![m::Param::new(env.clone(),"environment")]);
        self.reg_inst(Some("temporary"),m::Op::Alloca { ty:env.clone(),align });
        self.reg_store(env.clone(),m::local("environment".into()),"temporary",align);
        self.reg_inst(Some("end"),m::Op::Gep { ty:ty.clone(),ptr:m::Val::Null,indices:vec![m::Arg::new(m::Ty::Int(32),1)] });
        self.reg_inst(Some("size"),m::Op::Cast { opcode:"ptrtoint".into(),arg:m::Arg::new(m::Ty::Ptr,m::local("end".into())),to:m::Ty::Int(64) });
        self.reg_call(Some("frame"),m::Ty::Ptr,m::global("calloc".into()),vec![m::Arg::new(m::Ty::Int(64),1),m::Arg::new(m::Ty::Int(64),m::local("size".into()))]);
        self.reg_cmp("empty",m::Ty::Ptr,m::local("frame".into()),m::Val::Null);
        self.reg_cond("empty","allocation_failure","install");
        self.reg_label("allocation_failure");
        self.reg_drop_environment(environment,"temporary")?;
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr,m::Val::Null)));
        self.reg_label("install");
        for (name,index) in [("destroy.ptr",0),("handle.ptr",1),("gate.ptr",2),("environment.ptr",3)] { self.reg_gep(name,&ty,m::local("frame".into()),index); }
        self.reg_store(m::Ty::Ptr,m::global(destroy.clone()),"destroy.ptr",8);
        self.reg_store(m::Ty::Ptr,m::Val::Null,"handle.ptr",8);
        self.reg_store(m::Ty::Int(32),m::val(0),"gate.ptr",4);
        self.reg_store(env.clone(),m::local("environment".into()),"environment.ptr",align);
        // All fields and the actual F are initialized before a synchronous callback.
        self.reg_call(Some("handle"),m::Ty::Ptr,m::global(register),vec![m::Arg::new(m::Ty::Ptr,m::global(trampoline.clone())),m::Arg::new(m::Ty::Ptr,m::local("frame".into()))]);
        self.reg_cmp("failed",m::Ty::Ptr,m::local("handle".into()),m::Val::Null);
        self.reg_cond("failed","installation_failure","published");
        self.reg_label("installation_failure");
        // A null result promises that all entered/raw accesses have ended too.
        self.reg_drop_environment(environment,"environment.ptr")?;
        self.reg_call(None,m::Ty::Void,m::global("free".into()),vec![m::Arg::new(m::Ty::Ptr,m::local("frame".into()))]);
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr,m::Val::Null)));
        self.reg_label("published");
        self.reg_store(m::Ty::Ptr,m::local("handle".into()),"handle.ptr",8);
        self.reg_return(Some(m::Arg::new(m::Ty::Ptr,m::local("frame".into()))));
        self.event(m::Event::End);

        self.reg_begin(trampoline,m::Ty::Int(64),vec![m::Param::new(m::Ty::Ptr,"frame"),m::Param::new(m::Ty::Int(32),"argument")]);
        self.reg_gep("gate.ptr",&ty,m::local("frame".into()),2);
        self.reg_inst(Some("old"),m::Op::AtomicRmw { operation:m::AtomicRmwOp::Or,arg:m::Arg::new(m::Ty::Int(32),1),ptr:m::local("gate.ptr".into()),align:4,ordering:m::AtomicOrdering::SeqCst });
        self.reg_cmp("admitted",m::Ty::Int(32),m::local("old".into()),m::val(0));
        self.reg_cond("admitted","invoke","reject");
        self.reg_label("reject");
        self.reg_cmp("owned_rejected_busy",m::Ty::Int(32),m::local("old".into()),m::val(2));
        self.reg_cond("owned_rejected_busy","release_rejected","rejected");
        self.reg_label("release_rejected");
        self.reg_inst(Some("rejected.old"),m::Op::AtomicRmw { operation:m::AtomicRmwOp::And,arg:m::Arg::new(m::Ty::Int(32),-2),ptr:m::local("gate.ptr".into()),align:4,ordering:m::AtomicOrdering::SeqCst });
        self.reg_inst(None,m::Op::Branch("rejected".into()));
        self.reg_label("rejected");
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64),1i64<<32)));
        self.reg_label("invoke");
        self.reg_gep("environment.ptr",&ty,m::local("frame".into()),3);
        let environment_arg=if resource_closures::by_pointer(&definition) { m::Arg::new(m::Ty::Ptr,m::local("environment.ptr".into())) } else { self.reg_load("environment",env.clone(),"environment.ptr",align);m::Arg::new(env,m::local("environment".into())) };
        self.reg_call(Some("answer"),m::Ty::Int(32),m::global(symbol(definition.function)),vec![environment_arg,m::Arg::new(m::Ty::Int(32),m::local("argument".into()))]);
        self.reg_inst(Some("released.old"),m::Op::AtomicRmw { operation:m::AtomicRmwOp::And,arg:m::Arg::new(m::Ty::Int(32),-2),ptr:m::local("gate.ptr".into()),align:4,ordering:m::AtomicOrdering::SeqCst });
        self.reg_inst(Some("answer.wide"),m::Op::Cast { opcode:"zext".into(),arg:m::Arg::new(m::Ty::Int(32),m::local("answer".into())),to:m::Ty::Int(64) });
        self.reg_return(Some(m::Arg::new(m::Ty::Int(64),m::local("answer.wide".into()))));
        self.event(m::Event::End);

        self.reg_begin(destroy,m::Ty::Void,vec![m::Param::new(m::Ty::Ptr,"frame")]);
        self.reg_gep("gate.ptr",&ty,m::local("frame".into()),2);
        self.reg_inst(Some("closing.old"),m::Op::AtomicRmw { operation:m::AtomicRmwOp::Or,arg:m::Arg::new(m::Ty::Int(32),2),ptr:m::local("gate.ptr".into()),align:4,ordering:m::AtomicOrdering::SeqCst });
        self.reg_gep("handle.ptr",&ty,m::local("frame".into()),1);
        self.reg_load("handle",m::Ty::Ptr,"handle.ptr",8);
        // Only this actual backend return establishes raw-entry quiescence.
        self.reg_call(None,m::Ty::Void,m::global(close),vec![m::Arg::new(m::Ty::Ptr,m::local("handle".into()))]);
        self.reg_inst(Some("closed.gate"),m::Op::AtomicLoad { ty:m::Ty::Int(32),ptr:m::local("gate.ptr".into()),align:4,ordering:m::AtomicOrdering::SeqCst });
        self.reg_cmp("drained",m::Ty::Int(32),m::local("closed.gate".into()),m::val(2));
        self.reg_cond("drained","destroy","broken_backend");
        self.reg_label("broken_backend");
        self.reg_call(None,m::Ty::Void,m::global("llvm.trap".into()),vec![]);
        self.reg_inst(None,m::Op::Unreachable);
        self.reg_label("destroy");
        self.reg_gep("environment.ptr",&ty,m::local("frame".into()),3);
        self.reg_drop_environment(environment,"environment.ptr")?;
        self.reg_call(None,m::Ty::Void,m::global("free".into()),vec![m::Arg::new(m::Ty::Ptr,m::local("frame".into()))]);
        self.reg_return(None);
        self.event(m::Event::End);
        Ok(())
    }
}
