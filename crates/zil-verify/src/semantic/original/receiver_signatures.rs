//! A lending result is scoped to the current Callable subject. It is neither
//! the creator's receiver nor a lifetime inferred from a closure's first κ.
use super::*;
impl CheckedOriginal{
    pub(in crate::semantic) fn receiver_signature_recipe(signature:&SemanticSignatureRecipe)->bool{
        matches!(&signature.result,SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Receiver,path,mode:StoredBorrowMode::SharedAddress}),..}if path.is_empty())
    }
    pub(super) fn callable_signature(&mut self,module:&TypedModule,context:&CheckedContext,signature:&SemanticSignatureRecipe,kind:CallableKind,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Prototype>{
        if !Self::receiver_signature_recipe(signature){return self.signature(module,context,signature,env,active,b);}
        if signature.call_kind!=CallKind::Direct||!matches!(kind,CallableKind::Fn|CallableKind::FnMut){return Err(bad("signature-local Receiver is available only to a nonconsuming Callable subject"));}
        let life=Life::ReceiverSignature(active.fresh(b)?);
        self.signature_receiver(module,context,signature,env,life,active,b)
    }
    pub(super) fn receiver_closure_signature(&mut self,module:&TypedModule,context:&CheckedContext,signature:&SemanticSignatureRecipe,owner:SemanticDefinitionId,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Prototype>{
        if signature.call_kind!=CallKind::Direct||context.definition(module,owner,b)?.kind!=SemanticDefinitionKind::Closure||env.current()?!=owner{return Err(bad("Receiver result has no exact original closure definition"));}
        self.signature_receiver(module,context,signature,env,Life::ReceiverHead(owner),active,b)
    }
    fn signature_receiver(&mut self,module:&TypedModule,context:&CheckedContext,signature:&SemanticSignatureRecipe,env:&Env,life:Life,active:&mut ActiveQueries,b:&mut Budget)->Check<Prototype>{
        let owner=env.current()?;let previous=self.current.replace(owner);let previous_premises=self.premise_environment.take();
        let result=self.signature_inner(module,context,signature,env,Some(life),active,b);
        self.premise_environment=previous_premises;self.current=previous;result
    }
    /// Only a real inline captured place is justified merely by borrowing the
    /// environment. Pointer dereference/index paths need their actual parent
    /// lifetime and retain the ordinary directed relation checks.
    pub(super) fn inline_capture_place(&self,owner:SemanticDefinitionId,expression:&SemanticOriginalExpr,b:&mut Budget)->Check<bool>{
        let mut expression=expression;
        loop{b.spend(1)?;match &expression.kind{
            CoreExprKind::Name(_)=>return Ok(true),
            CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>expression=inner,
            CoreExprKind::Member{object,..}=>{
                let Some(ty)=self.place_types.get(&(owner,object.id)).or_else(||self.expressions.get(&(owner,object.id)))else{return Ok(false);};
                if matches!(self.resolved(ty,b)?,Ty::Address{..}|Ty::Desc{..}){return Ok(false);}expression=object;
            },
            CoreExprKind::Index{object,..}=>{
                let Some(ty)=self.place_types.get(&(owner,object.id)).or_else(||self.expressions.get(&(owner,object.id)))else{return Ok(false);};
                if !matches!(self.resolved(ty,b)?,Ty::Array(..)){return Ok(false);}expression=object;
            },
            _=>return Ok(false),
        }}
    }
    pub(super) fn receiver_source_life(&self,owner:SemanticDefinitionId,life:&Life,seen:&mut BTreeSet<Life>,b:&mut Budget)->Check<bool>{
        let life=self.resolved_life(life,b)?;b.spend(seen.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
        if life==Life::Static{return Ok(true);}if !seen.insert(life.clone()){return Ok(false);}
        if self.receiver_physical_source(owner,&life,b)?{return Ok(true);}
        if let Life::Local(definition,node)=life{
            if definition!=owner{return Ok(false);}
            if self.receiver_borrows.contains(&(owner,node)){return Ok(true);}
            if let Some(parent)=self.borrow_parents.get(&(owner,node)){return self.receiver_source_life(owner,parent,seen,b);}
        }
        if let Life::CallRegion(id)=life{
            let Some((definition,_,_,sources))=self.call_regions.get(&id)else{return Ok(false);};
            if *definition!=owner||sources.is_empty(){return Ok(false);}
            for source in sources{b.spend(seen.len())?;if !self.receiver_source_life(owner,source,&mut seen.clone(),b)?{return Ok(false);}}return Ok(true);
        }
        if let Some(sources)=self.local_outer_sources(&life,b)?{
            if sources.is_empty(){return Ok(false);}
            for source in sources{b.spend(seen.len())?;if !self.receiver_source_life(owner,&source,&mut seen.clone(),b)?{return Ok(false);}}return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn receiver_physical_source(&self,owner:SemanticDefinitionId,life:&Life,b:&mut Budget)->Check<bool>{
        // A physical captured Address keeps its stronger original σ. Desc,
        // phantom arguments and equal numeric images provide no such source.
        if let Some(free)=self.free_values.get(&owner){for binding in free.values(){
            binding.identity.charge_clone(b)?;
            let capture=Binder::Capture{definition:owner,source:Box::new(binding.identity.clone())};
            if !self.used.get(&owner).is_some_and(|used|used.contains(&capture)){continue;}
            if let Ty::Address{life:source,..}=self.resolved(&binding.ty,b)?{if self.receiver_declared_life_equal(owner,life,&source,b)?{return Ok(true);}}
        }}Ok(false)
    }
    fn receiver_declared_life_equal(&self,owner:SemanticDefinitionId,left:&Life,right:&Life,b:&mut Budget)->Check<bool>{
        let left=self.resolved_life(left,b)?;let right=self.resolved_life(right,b)?;if left==right{return Ok(true);}
        let rows=self.life_equalities.get(&owner).map(Vec::as_slice).unwrap_or(&[]);let mut pending=vec![left];let mut seen=BTreeSet::new();
        while let Some(value)=pending.pop(){b.spend(rows.len().saturating_add(1))?;if !seen.insert(value.clone()){continue;}
            for(a,z)in rows{let a=self.resolved_life(a,b)?;let z=self.resolved_life(z,b)?;if a==value{if z==right{return Ok(true);}pending.push(z);}else if z==value{if a==right{return Ok(true);}pending.push(a);}}
        }Ok(false)
    }
    fn prove_receiver_result(&mut self,module:&TypedModule,owner:SemanticDefinitionId,b:&mut Budget)->Check<()>{
        let previous=self.current.replace(owner);
        let result=self.receiver_result_sources(module,owner,b);
        self.current=previous;
        for source in result?{b.spend(1)?;self.outer_flows.entry(owner).or_default().push((source,Life::ReceiverHead(owner)));}
        Ok(())
    }
    fn receiver_result_sources(&self,module:&TypedModule,owner:SemanticDefinitionId,b:&mut Budget)->Check<Vec<Life>>{
        if !self.checked.contains(&owner)||!self.prototypes.get(&owner).is_some_and(|p|p.call_kind==CallKind::Direct&&matches!(p.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut))){return Err(bad("Receiver lending needs a completely checked nonconsuming original closure"));}
        let index=self.source_index(module,&[owner],b)?;let function=index.definitions.get(&owner).ok_or_else(||bad("Receiver lending has no complete original body"))?;
        let mut returns=Vec::new();
        for statement in function.statements.values(){b.spend(1)?;let CoreStatementKind::Return(Some(value))=&statement.kind else{continue;};
            let ty=self.original_type(owner,&SemanticOriginalTypeSource::Expression(value.id),b)?;
            let Ty::Address{mutable:false,life,..}=self.resolved(&ty,b)?else{return Err(bad("Receiver lending return changes its root shared Address"));};
            if !self.receiver_source_life(owner,&life,&mut BTreeSet::new(),b)?{return Err(bad("Receiver lending return is not grounded in its exact original capture/view"));}b.spend(1)?;returns.push(life);
        }
        if returns.is_empty(){return Err(bad("Receiver lending has no proved original return"));}Ok(returns)
    }
    pub(super) fn adapt_receiver_result_goal(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,kind:CallableKind,wanted:&Prototype,actual:&mut Ty,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let Ty::Address{mutable:false,life:Life::ReceiverSignature(token),..}=&wanted.result else{return Ok(());};
        if !matches!(kind,CallableKind::Fn|CallableKind::FnMut){return Err(bad("consuming Callable cannot use a Receiver lending result"));}
        let provider=match self.resolved(receiver,b)?{Ty::Closure(id,..)=>Some(id),Ty::Opaque(owner,arguments)=>match self.opaque_hidden(module,context,owner,&arguments,active,b)?{Ty::Closure(id,..)=>Some(id),_=>None},_=>None};
        if let Some(owner)=provider{self.prove_receiver_result(module,owner,b)?;b.spend(1)?;self.receiver_results.insert(owner);}
        else if !matches!(actual,Ty::Address{mutable:false,life:Life::ReceiverSignature(_),..}){return Err(bad("Receiver lending source lacks its complete original Callable premise"));}
        let Ty::Address{mutable:false,life,..}=actual else{return Err(bad("Receiver lending provider changes its shared result mode"));};
        // Only this result's root upper is adapted; every invariant inner κ,
        // nominal argument and phantom occurrence is compared by the caller.
        *life=Life::ReceiverSignature(*token);Ok(())
    }
    pub(in crate::semantic) fn receiver_result_proven(&self,owner:SemanticDefinitionId,b:&mut Budget)->Check<bool>{b.spend(1)?;Ok(self.checked.contains(&owner)&&self.receiver_results.contains(&owner)&&self.prototypes.get(&owner).is_some_and(|p|p.call_kind==CallKind::Direct&&matches!(p.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut))))}
    pub(in crate::semantic) fn original_call_node(&self,module:&TypedModule,owner:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<NodeId>{
        let index=self.source_index(module,&[owner],b)?;let function=index.definitions.get(&owner).ok_or_else(||bad("Receiver call has no original function"))?;
        let Some(index::Node::Expression(node))=function.nodes.get(&site)else{return Err(bad("Receiver call site has no exact original expression"));};
        if !matches!(node.kind,CoreExprKind::Call{..})||!self.callable_nodes.contains_key(&(owner,node.id)){return Err(bad("Receiver call selects another original operation"));}Ok(node.id)
    }
    pub(super) fn receiver_call_life(&self,module:&TypedModule,owner:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<Life>{
        let index=self.source_index(module,&[owner],b)?;let function=index.definitions.get(&owner).ok_or_else(||bad("Receiver call has no original caller"))?;
        let(position,site)=self.original_call_site(owner,node,b)?;
        let site=function.sites.get(position).filter(|row|row.id==site).ok_or_else(||bad("Receiver call selects another original occurrence"))?;
        b.spend(function.definition.implicit_lifetimes.len())?;let mut selected=None;
        for row in &function.definition.implicit_lifetimes{b.spend(row.occurrence_path.len().saturating_add(site.occurrence_path.len()))?;if row.occurrence_path==site.occurrence_path{if selected.replace(row.ordinal).is_some(){return Err(bad("Receiver call has multiple original access windows"));}}}
        Ok(Life::Implicit(owner,selected.ok_or_else(||bad("Receiver call lacks its original full-occurrence access window"))?))
    }
    pub(super) fn bind_receiver_call(&mut self,module:&TypedModule,_context:&CheckedContext,mut prototype:Prototype,node:NodeId,env:&Env,b:&mut Budget)->Check<Prototype>{
        let Ty::Address{mutable:false,life,..}=&mut prototype.result else{return Ok(prototype);};
        if !matches!(life,Life::ReceiverSignature(_)|Life::ReceiverHead(_)){return Ok(prototype);}
        if prototype.call_kind!=CallKind::Direct||!matches!(prototype.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut)){return Err(bad("Receiver result is not a real nonconsuming call"));}
        let owner=env.current()?;let target=self.receiver_call_life(module,owner,node,b)?;
        b.spend(2)?;if let Some(previous)=self.receiver_call_roles.insert((owner,node),target.clone()){if previous!=target{return Err(bad("one original Receiver call changes its access-window identity"));}}self.outer_flows.entry(owner).or_default().push((Life::ReceiverRole(owner,node,StoredBorrowMode::SharedAddress),target.clone()));
        *life=target;Ok(prototype)
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn recipe(result:SemanticTypeRecipe)->SemanticSignatureRecipe{SemanticSignatureRecipe{parameters:vec![],result,result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0}}
    fn receiver(inner:SemanticTypeRecipe)->SemanticTypeRecipe{SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Receiver,path:vec![],mode:StoredBorrowMode::SharedAddress}),inner:Box::new(inner)}}
    #[test]
    fn receiver_signature_has_a_private_subject_and_no_code_or_inner_escape(){
        let limits=Limits::default();let mut b=Budget::new(&limits);let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut active=ActiveQueries::new();let mut checked=CheckedOriginal::default();
        let id=module.semantic.as_ref().unwrap().definitions.iter().find(|row|matches!(row.kind,SemanticDefinitionKind::Function)).unwrap().id;
        let env=Env{lexical:vec![Frame{definition:id,actuals:vec![],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let nested=SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Static,inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))};let signature=recipe(receiver(nested));
        let a=checked.callable_signature(&module,&context,&signature,CallableKind::Fn,&env,&mut active,&mut b).unwrap();
        let z=checked.callable_signature(&module,&context,&signature,CallableKind::FnMut,&env,&mut active,&mut b).unwrap();
        let(Ty::Address{life:a,inner,..},Ty::Address{life:z,..})=(&a.result,&z.result)else{panic!("shared root role")};
        assert!(matches!(a,Life::ReceiverSignature(_)));assert!(!checked.same_life(a,z,&mut b).unwrap());assert!(matches!(inner.as_ref(),Ty::Address{life:Life::Static,..}),"the complete inner lifetime stays invariant");
        assert!(checked.signature(&module,&context,&signature,&env,&mut active,&mut b).is_err(),"ordinary code has no receiver subject");
        for kind in [CallableKind::FnOnce,CallableKind::FunctionPointer]{assert!(checked.callable_signature(&module,&context,&signature,kind,&env,&mut active,&mut b).is_err());}
        let mut suspended=signature.clone();suspended.call_kind=CallKind::Suspend;assert!(checked.callable_signature(&module,&context,&suspended,CallableKind::Fn,&env,&mut active,&mut b).is_err());
        let inside=recipe(receiver(receiver(SemanticTypeRecipe::Atom(Type::I32))));assert!(checked.callable_signature(&module,&context,&inside,CallableKind::Fn,&env,&mut active,&mut b).is_err(),"a subject role does not enter a referent");
        let code=SemanticTypeRecipe::Code{kind:SemanticCodeKind::FunctionPointer,signature:Box::new(signature)};assert!(checked.ty(&module,&context,&code,&env,&mut active,&mut b).is_err());
    }
    #[test]
    fn receiver_source_requires_every_real_capture_anchor_without_lifetime_equality(){
        let mut checked=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};let other=SemanticDefinitionId{module:zil_base::ModuleId(0),index:1};
        let root=Life::Local(owner,NodeId(3));let ungrounded=Life::Local(owner,NodeId(4));checked.receiver_borrows.insert((owner,NodeId(3)));
        assert!(checked.receiver_source_life(owner,&root,&mut BTreeSet::new(),&mut b).unwrap());assert!(!checked.receiver_source_life(other,&root,&mut BTreeSet::new(),&mut b).unwrap());assert!(!checked.receiver_source_life(owner,&ungrounded,&mut BTreeSet::new(),&mut b).unwrap());
        checked.call_regions.insert(1,(owner,NodeId(5),vec![],vec![root.clone(),ungrounded]));assert!(!checked.receiver_source_life(owner,&Life::CallRegion(1),&mut BTreeSet::new(),&mut b).unwrap(),"one safe return source cannot erase an unsafe branch");
        assert!(!checked.same_life(&root,&Life::ReceiverHead(owner),&mut b).unwrap());
        let formal=Life::Formal(owner,0);let binding=Binder::Parameter{definition:other,ordinal:0};let capture=Binder::Capture{definition:owner,source:Box::new(binding.clone())};
        checked.used.insert(owner,BTreeSet::from([capture]));checked.free_values.insert(owner,BTreeMap::from([("ghost".into(),Binding{ty:Ty::Nominal(other,vec![Arg::Life(formal.clone())]),mutable:false,identity:binding.clone()})]));
        assert!(!checked.receiver_source_life(owner,&formal,&mut BTreeSet::new(),&mut b).unwrap(),"a phantom lifetime is no physical capture head");
        checked.free_values.get_mut(&owner).unwrap().get_mut("ghost").unwrap().ty=Ty::Address{mutable:false,life:formal.clone(),inner:Box::new(Ty::Atom(Type::I32))};
        assert!(checked.receiver_source_life(owner,&formal,&mut BTreeSet::new(),&mut b).unwrap());assert!(!checked.same_life(&formal,&Life::ReceiverHead(owner),&mut b).unwrap());
    }
}
