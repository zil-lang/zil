//! Owned creation, incoming ownership and descriptions are separate original
//! lineages. A future Drop or concrete Managed type cannot create ownership.
use super::*;
impl CheckedOriginal {
    pub(super) fn resource_sync_drop(&mut self,module:&TypedModule,context:&CheckedContext,payload:&Ty,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let payload=self.resolved(payload,b)?;
        match &payload{
            Ty::Atom(Type::ByteBuffer)=>return Ok(true),
            Ty::Store(element)=>return self.sync_drop(module,context,element,caller,active,b),
            Ty::Carrier{kind:CarrierKind::Task|CarrierKind::AsyncRegistration,..}=>return Ok(false),
            Ty::Carrier{kind:CarrierKind::Mutex,child,..}=>return self.sync_drop(module,context,child,caller,active,b),
            Ty::Carrier{kind:CarrierKind::Registration,child,..}=>return self.sync_drop(module,context,child,caller,active,b),
            _=>{},
        }
        let Ty::Nominal(id,arguments)=&payload else{return Ok(false);};
        let nominal=context.definition(module,*id,b)?;
        if !matches!(nominal.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}){return Ok(false);}
        let program=module.semantic.as_ref().ok_or_else(||bad("resource cleanup lacks original declarations"))?;
        let mut selected=None;
        for scope in &program.definitions{
            b.spend(1)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{self_type,trait_kind:SemanticBuiltinTrait::Drop})=&scope.body else{continue;};
            // Only a fully matched original Self chooses a provider. More
            // general impl matching is an obligation, never the first name.
            let SemanticTypeRecipe::Nominal{definition,arguments:recipes}=self_type else{return Err(unsupported("Drop Self requires its complete original structural pattern",None));};
            if definition!=id{continue;}
            if !scope.parameters.is_empty(){return Err(unsupported("generic Drop needs its whole original Self/where instantiation",None));}
            let environment=self.environment(module,context,scope.id,active,b)?;
            let mut actuals=Vec::new();for recipe in recipes{actuals.push(self.actual(module,context,recipe,&environment,active,b)?);}
            if actuals!=*arguments{continue;}
            if selected.replace(scope.id).is_some(){return Err(bad("resource cleanup has ambiguous original Drop implementations"));}
        }
        if let Some(scope)=selected{
            let methods=program.definitions.iter().filter(|row|row.parent==Some(scope)&&row.kind==SemanticDefinitionKind::Method).collect::<Vec<_>>();b.spend(program.definitions.len())?;
            if methods.len()!=1{return Err(bad("resource Drop lacks its unique original entry"));}
            if !self.checked.contains(&methods[0].id){self.definition(module,context,methods[0].id,active,b)?;}
            let prototype=self.prototypes.get(&methods[0].id).ok_or_else(||bad("original Drop has no complete prototype"))?;
            if prototype.parameters.len()!=1||prototype.result!=Ty::Atom(Type::Unit)||prototype.call_kind!=CallKind::Direct{return Err(bad("resource Drop has another original effect or signature"));}
            if !matches!(&prototype.parameters[0],Ty::Desc{inner,..} if **inner==payload){return Err(bad("resource Drop receives something other than its exact Self description"));}
            return Ok(true);
        }
        let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&nominal.body else{return Err(bad("resource payload is not a complete original product"));};
        let mut selected=self.environment(module,context,*id,active,b)?;selected.lexical[0].actuals=arguments.clone();
        for field in fields{let ty=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.sync_drop(module,context,&ty,caller,active,b)?{return Ok(false);}}Ok(true)
    }
    pub(super) fn resource_payload(&self,module:&TypedModule,context:&CheckedContext,ty:&Ty,b:&mut Budget)->Check<bool>{Ok(match self.resolved(ty,b)?{
        Ty::Atom(Type::ByteBuffer)|Ty::Store(_)|Ty::Carrier{kind:CarrierKind::Mutex|CarrierKind::Task|CarrierKind::Registration|CarrierKind::AsyncRegistration,..}=>true,
        Ty::Nominal(id,_)=>matches!(context.definition(module,id,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}),
        _=>false,
    })}
    pub(super) fn mode_type(&self,module:&TypedModule,context:&CheckedContext,ty:Ty,mode:PassingMode,parameter:Option<u32>,env:&Env,b:&mut Budget)->Check<Ty>{
        if !self.resource_payload(module,context,&ty,b)?{return Ok(ty);}
        Ok(match mode{
            PassingMode::Create|PassingMode::Give=>Ty::Managed(Box::new(ty)),
            PassingMode::Infer|PassingMode::Describe=>{let ordinal=parameter.ok_or_else(||unsupported("bare resource result needs its explicit owning or borrowed result contract",None))?;Ty::Desc{life:Life::Head(env.current()?,ordinal),inner:Box::new(ty)}},
            _=>return Err(bad("resource payload has a nonresource passing mode")),
        })
    }
    pub(super) fn return_resource(&mut self,expression:Option<&SemanticOriginalExpr>,actual:&Ty,env:&Env,b:&mut Budget)->Check<()> {
        let prototype=self.prototypes.get(&env.current()?).ok_or_else(||bad("resource Return has no original signature"))?;
        let ty=self.resolved(actual,b)?;
        let resource=matches!(ty,Ty::Managed(_));
        if !resource{return Ok(());}
        let expression=expression.ok_or_else(||bad("resource Return lacks an operand"))?;
        match prototype.result_mode{
            PassingMode::Create=>{
                if !self.created.contains(&(env.current()?,expression.id)){return Err(bad("original create result transfers an incoming or unproved resource lineage"));}
                let mut source=expression;loop{b.spend(1)?;match &source.kind{CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}|CoreExprKind::Unary{op:UnaryOp::Give,operand:inner}=>source=inner,CoreExprKind::Name(_)=>{if let Some(binding)=self.value_bindings.get(&(env.current()?,source.id)){self.creation_returns.push(binding.clone());}break;},_=>break,}}
                Ok(())
            },
            // An explicit whole Managed<T> result is already an owning
            // value contract. Bare resource Infer results were rejected by
            // mode_type; neither spelling creates incoming ownership here.
            PassingMode::Give|PassingMode::Infer|PassingMode::Move=>{
                if !matches!(self.resolved(&prototype.result,b)?,Ty::Managed(_)){return Err(bad("original resource Return changes its complete owning result type"));}
                if self.transfer_operand(env.current()?,expression,b)?{Ok(())}else{Err(bad("original give result omits explicit ownership transfer"))}
            },
            _=>Err(bad("original resource Return lacks its owning result mode")),
        }
    }
    /// A checked owning rvalue already carries the duty produced by its
    /// operation. A Place, even a recently created local, still requires give.
    pub(super) fn transfer_operand(&self,owner:SemanticDefinitionId,expression:&SemanticOriginalExpr,b:&mut Budget)->Check<bool>{
        b.spend(1)?;
        match &expression.kind{
            CoreExprKind::Unary{op:UnaryOp::Give,..}=>Ok(true),
            CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}|CoreExprKind::Try(inner)=>self.transfer_operand(owner,inner,b),
            CoreExprKind::Sequence{value,..}=>self.transfer_operand(owner,value,b),
            CoreExprKind::If{then_value,else_value,..}=>Ok(self.transfer_operand(owner,then_value,b)?&&self.transfer_operand(owner,else_value,b)?),
            CoreExprKind::Call{..}=>{
                let mut ty=self.original_type(owner,&SemanticOriginalTypeSource::Expression(expression.id),b)?;
                while let Ty::ErrorUnion(_,result)=ty{ty=self.resolved(&result,b)?;}
                if !matches!(ty,Ty::Managed(_)){return Ok(false);}
                let(_,site)=self.original_call_site(owner,expression.id,b)?;
                Ok(self.call_targets.contains_key(&(owner,site))||self.trait_calls.contains_key(&(owner,site))||self.callable_nodes.contains_key(&(owner,expression.id))||self.intrinsic_calls.contains_key(&(owner,site)))
            },
            CoreExprKind::Record{..}|CoreExprKind::RecordConstructor{..}=>Ok(self.created.contains(&(owner,expression.id))&&matches!(self.original_type(owner,&SemanticOriginalTypeSource::Expression(expression.id),b)?,Ty::Managed(_))),
            _=>Ok(false),
        }
    }
    pub(super) fn creation(&self,definition:SemanticDefinitionId,expression:&SemanticOriginalExpr,bindings:&BTreeMap<String,Binding>,b:&mut Budget)->Check<bool>{
        b.spend(1)?;
        if !matches!(expression.kind,CoreExprKind::Name(_))&&self.created.contains(&(definition,expression.id)){return Ok(true);}
        Ok(match &expression.kind{
            CoreExprKind::Name(name)=>bindings.get(name).is_some_and(|row|self.created_bindings.contains(&row.identity)&&!self.creation_tainted.contains(&row.identity)),
            CoreExprKind::Group(inner)|CoreExprKind::Try(inner)|CoreExprKind::Ascribed{value:inner,..}|CoreExprKind::Unary{op:UnaryOp::Give,operand:inner}=>self.creation(definition,inner,bindings,b)?,
            _=>false,
        })
    }
    pub(super) fn constructed_resource(&mut self,module:&TypedModule,context:&CheckedContext,ty:Ty,expression:&SemanticOriginalExpr,env:&Env,b:&mut Budget)->Check<Ty>{
        if self.resource_payload(module,context,&ty,b)?{b.spend(1)?;self.created.insert((env.current()?,expression.id));Ok(Ty::Managed(Box::new(ty)))}else{Ok(ty)}
    }
}

#[cfg(test)]
mod return_mode_tests{
    use super::*;
    #[test]
    fn explicit_managed_result_keeps_transfer_and_creation_obligations(){
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let env=Env{lexical:vec![Frame{definition:owner,actuals:vec![],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let result=Ty::Managed(Box::new(Ty::Atom(Type::ByteBuffer)));
        let mut checked=CheckedOriginal::default();
        checked.prototypes.insert(owner,Prototype{parameters:vec![],result:result.clone(),environment:env.clone(),modes:vec![],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:None});
        let mut b=Budget::new(&Limits::default());
        let name=SemanticOriginalExpr{id:NodeId(0),span:Default::default(),kind:CoreExprKind::Name("incoming".into())};
        let give=SemanticOriginalExpr{id:NodeId(1),span:Default::default(),kind:CoreExprKind::Unary{op:UnaryOp::Give,operand:Box::new(name.clone())}};
        for mode in [PassingMode::Infer,PassingMode::Move,PassingMode::Give]{
            checked.prototypes.get_mut(&owner).unwrap().result_mode=mode;
            assert!(checked.return_resource(Some(&give),&result,&env,&mut b).is_ok());
            assert!(checked.return_resource(Some(&name),&result,&env,&mut b).is_err(),"a known owning result never silently moves an incoming Place");
        }
        for mode in [PassingMode::Copy,PassingMode::Borrow,PassingMode::Describe,PassingMode::Create]{
            checked.prototypes.get_mut(&owner).unwrap().result_mode=mode;
            assert!(checked.return_resource(Some(&give),&result,&env,&mut b).is_err(),"an incoming transfer cannot grant another result role or fresh creation");
        }
        checked.prototypes.get_mut(&owner).unwrap().result_mode=PassingMode::Infer;
        checked.prototypes.get_mut(&owner).unwrap().result=Ty::Atom(Type::ByteBuffer);
        assert!(checked.return_resource(Some(&give),&result,&env,&mut b).is_err(),"a bare resource Infer result does not acquire an owning result contract");
    }
}
