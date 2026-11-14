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
            if methods.len()!=1||!self.checked.contains(&methods[0].id){return Err(bad("resource Drop lacks its unique universally checked original entry"));}
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
            PassingMode::Give=>if matches!(expression.kind,CoreExprKind::Unary{op:UnaryOp::Give,..}){Ok(())}else{Err(bad("original give result omits explicit ownership transfer"))},
            _=>Err(bad("original resource Return lacks its owning result mode")),
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
