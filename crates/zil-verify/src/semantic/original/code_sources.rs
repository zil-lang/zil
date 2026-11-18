//! A static code value retains its exact original target. Equal call geometry
//! never selects another body, foreign ABI, generic instance or closure entry.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn code_source(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<Option<SemanticDefinitionId>>{b.spend(1)?;Ok(self.code_sources.get(&(definition,node)).copied())}
    pub(in crate::semantic) fn thin_literal(&self, child: SemanticDefinitionId, b: &mut Budget) -> Check<Option<SemanticSiteId>> {
        if self.thin_literals.is_empty() { return Ok(None); }
        b.spend(self.thin_literals.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        Ok(self.thin_literals.get(&child).copied())
    }

    /// Seed the original child's inference slots before checking its complete body.
    /// Only a pointer type already established by the creator supplies these inputs.
    pub(super) fn contextual_code_inputs(
        &mut self, child: &SemanticDefinition, expected: Option<&Ty>, b: &mut Budget,
    ) -> Check<Option<Ty>> {
        let Some(expected) = expected else { return Ok(None) };
        let code = self.resolved(expected, b)?;
        let Ty::Code { kind: SemanticCodeKind::FunctionPointer, parameters, result, modes,
            result_mode, call_kind, bound_lifetimes, invocation_lifetimes } = &code else { return Ok(None) };
        let SemanticDefinitionBody::FunctionWithBody { signature, receiver_parameter: None,
            foreign: None, body: SemanticOriginalFunctionBody::Defined(_), .. } = &child.body
            else { return Err(bad("contextual fn literal lacks its receiver-free original body")) };
        b.spend(parameters.len().saturating_add(1))?;
        if child.kind != SemanticDefinitionKind::Closure || !child.environment.is_empty()
            || !child.parameters.is_empty() || *call_kind != CallKind::Direct
            || signature.call_kind != *call_kind || signature.result_mode != *result_mode
            || *bound_lifetimes != 0 || *invocation_lifetimes != 0
            || signature.bound_lifetimes != 0 || signature.invocation_lifetimes != 0
            || signature.parameters.len() != parameters.len()
            || !signature.parameters.iter().map(|p| p.mode).eq(modes.iter().copied())
        { return Err(bad("contextual fn literal changes its environment or complete original signature")); }
        for (ordinal, ty) in parameters.iter().enumerate() {
            let key = (child.id, ordinal as u32);
            if let Some(previous) = self.parameters.get(&key).cloned() { self.unify(&previous, ty, b)?; }
            else { self.parameters.insert(key, ty.clone()); }
        }
        if let Some(previous) = self.results.get(&child.id).cloned() { self.unify(&previous, result, b)?; }
        else { self.results.insert(child.id, result.as_ref().clone()); }
        Ok(Some(code))
    }

    pub(super) fn finish_contextual_code(&mut self, parent: SemanticDefinitionId, node: NodeId,
        child: SemanticDefinitionId, site: SemanticSiteId, expected: &Ty, b: &mut Budget) -> Check<()> {
        let used = self.used.get(&child);
        b.spend(used.map_or(0, BTreeSet::len).saturating_add(1))?;
        if used.is_some_and(|rows| rows.iter().any(|binder| matches!(binder, Binder::Capture { .. }))) {
            return Err(bad("contextual fn literal reads a captured creator binding"));
        }
        let signature = self.prototypes.get(&child).ok_or_else(||bad("contextual fn literal has no checked body signature"))?;
        let actual = Ty::Code { kind: SemanticCodeKind::FunctionPointer, parameters: signature.parameters.clone(),
            result: Box::new(signature.result.clone()), modes: signature.modes.clone(), result_mode: signature.result_mode,
            call_kind: signature.call_kind, bound_lifetimes: signature.bound_lifetimes,
            invocation_lifetimes: signature.invocation_lifetimes };
        self.unify(&actual, expected, b)?;
        self.code_sources.insert((parent, node), child);
        self.thin_literals.insert(child,site);
        Ok(())
    }

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
