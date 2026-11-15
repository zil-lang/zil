//! A static code value retains its exact original target. Equal call geometry
//! never selects another body, foreign ABI, generic instance or closure entry.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn code_source(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<Option<SemanticDefinitionId>>{b.spend(1)?;Ok(self.code_sources.get(&(definition,node)).copied())}
    #[allow(clippy::too_many_arguments)]
    pub(super) fn code_value(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,actual:&Ty,expected:Option<&Ty>,value_role:bool,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let actual=self.resolved(actual,b)?;
        let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;
        let requested=value_role||matches!(contextual,Some(Ty::Code{..}|Ty::Infer(_)));
        let Ty::Function(target)=actual else{
            if let CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=&expression.kind{if let Some(target)=self.code_source(env.current()?,inner.id,b)?{b.spend(1)?;self.code_sources.insert((env.current()?,expression.id),target);}}
            return Ok(actual);
        };
        if !requested{return Ok(Ty::Function(target));}
        let definition=context.definition(module,target,b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,body:SemanticOriginalFunctionBody::Defined(_),foreign:None,receiver_parameter:None,..}=&definition.body else{return Err(unsupported("ordinary static code requires its real defined nonforeign receiver-free target",Some(expression.span)));};
        if definition.kind!=SemanticDefinitionKind::Function||signature.call_kind!=CallKind::Direct{return Err(unsupported("ordinary static code cannot erase a closure or Suspend role",Some(expression.span)));}
        let mut parent=Some(target);let mut seen=BTreeSet::new();
        while let Some(id)=parent{b.spend(seen.len().saturating_add(1))?;if !seen.insert(id){return Err(bad("static code target has a cyclic original ancestry"));}let row=context.definition(module,id,b)?;if !row.parameters.is_empty()||!row.environment.is_empty(){return Err(unsupported("ordinary static code needs its complete monomorphic original target",Some(expression.span)));}parent=row.parent;}
        if !self.checked.contains(&target)&&!self.active.contains(&target){self.definition(module,context,target,active,b)?;}
        let selected=self.environment(module,context,target,active,b)?;
        let prototype=self.signature(module,context,signature,&selected,active,b)?;
        if prototype.bound_lifetimes!=0||prototype.invocation_lifetimes!=0{return Err(unsupported("ordinary thin code retains an unimplemented original quantified signature",Some(expression.span)));}
        let mut scoped=false;
        let mut inspect=|life:&Life,b:&mut Budget|{b.spend(1)?;scoped=true;Ok(life.clone())};
        for ty in prototype.parameters.iter().chain(std::iter::once(&prototype.result)){self.map_invocation_type(ty,&mut inspect,0,b)?;}
        if scoped{return Err(unsupported("ordinary thin code needs its complete borrowed signature-role coercion",Some(expression.span)));}
        for requirement in &definition.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
        let code=Ty::Code{kind:SemanticCodeKind::FunctionPointer,parameters:prototype.parameters,result:Box::new(prototype.result),modes:prototype.modes,result_mode:prototype.result_mode,call_kind:prototype.call_kind,bound_lifetimes:0,invocation_lifetimes:0};
        if let Some(expected)=expected{self.unify(&code,expected,b)?;}
        b.spend(2)?;self.code_sources.insert((env.current()?,expression.id),target);self.expressions.insert((env.current()?,expression.id),code.clone());
        Ok(code)
    }
}
