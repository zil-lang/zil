//! Forming a static C callback reference does not execute the selected body.
//! Its complete original definition and ABI remain independent obligations.
use super::*;
impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn c_callback(&mut self,module:&TypedModule,context:&CheckedContext,site:SemanticSiteId,written:&[Arg],arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        if !written.is_empty()||arguments.len()!=1{return Err(bad("c_callback requires exactly one nongeneric original static function"));}
        let source=self.expression(module,context,&arguments[0],env,bindings,None,active,b)?;
        let Ty::Function(target)=self.resolved(&source,b)?else{return Err(bad("c_callback original argument is not a static function definition"));};
        let definition=context.definition(module,target,b)?;
        if definition.kind!=SemanticDefinitionKind::Function||definition.parent.is_some(){return Err(unsupported("c_callback needs its exact root static function definition",None));}
        let SemanticDefinitionBody::FunctionWithBody{signature,body:SemanticOriginalFunctionBody::Defined(_),receiver_parameter:None,foreign:None,..}=&definition.body else{return Err(bad("c_callback requires a defined ordinary original function without a receiver or foreign ABI"));};
        let mut parent=Some(target);let mut seen=BTreeSet::new();
        while let Some(id)=parent{b.spend(seen.len().saturating_add(1))?;if !seen.insert(id){return Err(bad("c_callback original parent chain has a cycle"));}if seen.len()>b.limits.max_call_depth.min(128){return Err(limit("c_callback original parent depth exceeded"));}let row=context.definition(module,id,b)?;if !row.parameters.is_empty()||!row.environment.is_empty(){return Err(bad("c_callback cannot erase generic or sealed environment arguments"));}parent=row.parent;}
        if definition.kind==SemanticDefinitionKind::Closure||signature.call_kind!=CallKind::Direct||signature.bound_lifetimes!=0||signature.invocation_lifetimes!=0||signature.result_mode!=PassingMode::Infer||signature.parameters.iter().any(|parameter|parameter.mode!=PassingMode::Infer){return Err(bad("c_callback requires the exact Direct monomorphic value signature"));}
        // A forward definition with an inferred Result must be checked in its
        // own original domain. A reference to an active explicit prototype is
        // not an execution edge or a reason to assume its body complete.
        if !self.checked.contains(&target)&&!self.active.contains(&target){self.definition(module,context,target,active,b)?;}
        let environment=self.environment(module,context,target,active,b)?;
        let prototype=self.signature(module,context,signature,&environment,active,b)?;
        for parameter in &prototype.parameters{if !original_c_scalar(&self.resolved(parameter,b)?){return Err(bad("c_callback original parameter is outside the numeric C ABI"));}}
        let result=self.resolved(&prototype.result,b)?;
        if result!=Ty::Atom(Type::Unit)&&!original_c_scalar(&result){return Err(bad("c_callback original result is outside the numeric C ABI"));}
        self.execute_effect(env,CallKind::Direct,b)?;
        self.intrinsic_calls.insert((env.current()?,site),intrinsics::Use{kind:intrinsics::Intrinsic::CCallback,buffer:None,worker:Some(target),providers:None});
        Ok(Ty::Code{kind:SemanticCodeKind::ForeignCallback,parameters:prototype.parameters,result:Box::new(prototype.result),modes:prototype.modes,result_mode:prototype.result_mode,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0})
    }
}
fn original_c_scalar(ty:&Ty)->bool{matches!(ty,Ty::Atom(ty)if crate::native_values::callback_parameter(ty))}
