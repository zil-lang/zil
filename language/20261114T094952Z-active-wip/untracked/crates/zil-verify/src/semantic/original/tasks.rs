//! Universal Task schemes retain whole symbolic input domains. Constructing a
//! frame references a worker; it never executes that worker or its Suspend edge.
use super::*;
use intrinsics::{Intrinsic,Use};

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn task_operation(&mut self,module:&TypedModule,context:&CheckedContext,kind:Intrinsic,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,use_:&mut Use,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        if matches!(kind,Intrinsic::TaskCreate|Intrinsic::TaskCreateOwned) {
            if arguments.is_empty(){return Err(bad("Task creation omits its original worker"));}
            let worker=self.expression(module,context,&arguments[0],env,bindings,None,active,b)?;
            let Ty::Function(worker)=worker else{return Err(bad("Task worker is not its exact original static definition"));};
            let prototype=self.operation_function(module,context,worker,active,b)?;
            self.operation_arity(&arguments[1..],prototype.parameters.len())?;
            if !matches!(prototype.call_kind,CallKind::Direct|CallKind::Suspend){return Err(bad("Task worker has another complete call convention"));}
            self.structural_obligation(module,context,&prototype.result,DynStructuralBound::Value,env,active,b)?;
            // Cancellation and an unobserved result must settle the complete
            // result. A concrete Copy sample cannot establish abstract cleanup.
            self.structural_obligation(module,context,&prototype.result,DynStructuralBound::SyncDrop,env,active,b)?;
            let mut retained=Retained::default();let mut inputs=Vec::new();
            b.spend(prototype.parameters.len())?;
            for (ordinal,((argument,parameter),mode)) in arguments[1..].iter().zip(&prototype.parameters).zip(&prototype.modes).enumerate(){
                let actual=self.argument(module,context,argument,parameter,*mode,env,bindings,active,b)?;
                self.unify(&actual,parameter,b)?;
                let actual=self.resolved(&actual,b)?;
                self.structural_obligation(module,context,&actual,DynStructuralBound::Value,env,active,b)?;
                self.structural_obligation(module,context,&actual,DynStructuralBound::SyncDrop,env,active,b)?;
                // All(T) is kept even if this particular sample happens to be
                // zero. The actual interpreter expands its full κ directory.
                if !self.task_zero_domain(module,context,&actual,env,active,b,0)?{
                    let classes=vec![ClassPart::All{ty:actual.clone(),path:vec![]}];
                    retained.domain.extend(classes.clone());
                    retained.ports.push(PortPart::FromType{ty:actual.clone(),classes:classes.clone()});
                    retained.retained.push(RetainedPart::All{path:vec![LifeTypeProjection::Parameter(ordinal as u32)],parents:classes.clone()});
                    retained.nominal.push(NominalPart::FromType{path:vec![LifeTypeProjection::Parameter(ordinal as u32)],ty:actual.clone(),classes});
                }
                inputs.push(actual);
            }
            if !self.task_zero_domain(module,context,&prototype.result,env,active,b,0)?{retained.retained.push(RetainedPart::All{path:vec![LifeTypeProjection::Result],parents:vec![ClassPart::All{ty:prototype.result.clone(),path:vec![]}]});}
            use_.worker=Some(worker);
            let task=Ty::Managed(Box::new(Ty::Carrier{kind:CarrierKind::Task,child:Box::new(prototype.result),retained:Box::new(retained)}));
            let error=if kind==Intrinsic::TaskCreateOwned{self.task_failure_type(module,context,Ty::Tuple(inputs),task.clone(),env,active,b)?}else{Ty::SealedError(SealedError::TaskCreate)};
            return Ok(Ty::ErrorUnion(Box::new(error),Box::new(task)));
        }
        self.operation_arity(arguments,1)?;
        let value=if kind==Intrinsic::TaskFinish {
            if !matches!(arguments[0].kind,CoreExprKind::Unary{op:UnaryOp::Give,..}){return Err(bad("Task destruction omits its explicit owning transfer"));}
            self.expression(module,context,&arguments[0],env,bindings,None,active,b)?
        }else{let(value,source)=self.operation_observe(module,context,&arguments[0],env,bindings,active,b)?;use_.buffer=Some(source);value};
        let value=self.resolved(&value,b)?;
        if kind==Intrinsic::TaskFinish&&!matches!(value,Ty::Managed(_)){return Err(bad("Task destruction consumes something other than its owning head"));}
        if matches!(kind,Intrinsic::TaskPoll|Intrinsic::TaskRequest|Intrinsic::TaskPollClose)&&!matches!(value,Ty::Address{mutable:true,ref inner,..} if matches!(inner.as_ref(),Ty::Managed(_))){return Err(bad("Task driving operation lacks an exclusive owning-head borrow"));}
        let(payload,_)=self.operation_payload(&value,env.current()?,arguments[0].id,b)?;
        let Ty::Carrier{kind:CarrierKind::Task,child,..}=payload else{return Err(bad("Task operation selects another complete frame/result type"));};
        Ok(match kind {
            Intrinsic::TaskValid=>Ty::Atom(Type::Bool),
            Intrinsic::TaskPoll=>Ty::ErrorUnion(Box::new(Ty::SealedError(SealedError::TaskPoll)),Box::new(self.poll_type(module,*child,b)?)),
            Intrinsic::TaskPollClose=>self.poll_type(module,Ty::Atom(Type::Unit),b)?,
            Intrinsic::TaskRequest|Intrinsic::TaskFinish=>Ty::Atom(Type::Unit),
            _=>return Err(bad("Task scheme received another intrinsic role")),
        })
    }

    fn task_failure_type(&mut self,module:&TypedModule,context:&CheckedContext,inputs:Ty,task:Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let copy=self.is_copy(module,context,&inputs,env,active,b)?;
        let wanted=if copy{zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD}else{zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE};
        let program=module.semantic.as_ref().ok_or_else(||bad("Task failure lacks its original declaration inventory"))?;
        b.spend(program.definitions.len())?;
        let mut rows=program.definitions.iter().filter(|row|matches!(&row.body,SemanticDefinitionBody::Nominal{declaration,..} if *declaration==wanted));
        let definition=rows.next().ok_or_else(||bad("Task failure has no exact canonical original declaration"))?;
        if rows.next().is_some()||definition.parameters.len()!=2||!definition.parameters.iter().all(|p|matches!(p,SemanticFormal::Type{requirements}if requirements.is_empty()))||!definition.requirements.is_empty()||!definition.environment.is_empty()||definition.parent.is_some(){return Err(bad("Task failure changes its complete original mixed declaration scheme"));}
        let SemanticDefinitionBody::Nominal{kind,body:SemanticNominalBody::Product{fields},..}=&definition.body else{return Err(bad("Task failure has another complete original product"));};
        if *kind!=if copy{LifeNominalKind::Record}else{LifeNominalKind::Package}||fields.len()!=2||fields[0].name!="reason"||fields[1].name!="inputs"||fields[1].ty!=SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0}){return Err(bad("Task failure changes its original input-return fields"));}
        let mut selected=self.environment(module,context,definition.id,active,b)?;
        selected.lexical[0].actuals=vec![Arg::Type(inputs.clone()),Arg::Type(task.clone())];
        let SemanticTypeRecipe::Nominal{definition:reason,arguments}=&fields[0].ty else{return Err(bad("Task failure reason lacks its canonical original declaration"));};
        if !arguments.is_empty()||!matches!(context.definition(module,*reason,b)?.body,SemanticDefinitionBody::Nominal{declaration,..}if declaration==zil_ir::builtin_life::TASK_CREATE_ERROR){return Err(bad("Task failure reason selects another complete error identity"));}
        Ok(Ty::Nominal(definition.id,vec![Arg::Type(inputs),Arg::Type(task)]))
    }

    /// An empty input Context is never evidence about an unknown result or a
    /// phantom argument. This proof traverses every original nominal actual.
    pub(super) fn task_zero_domain(&mut self,module:&TypedModule,context:&CheckedContext,value:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget,depth:usize)->Check<bool>{
        b.spend(1)?;if depth>=b.limits.max_call_depth.min(128){return Err(limit("original Task full-domain recursion exceeded"));}
        Ok(match self.resolved(value,b)?{
            Ty::Atom(_)|Ty::SealedError(_)|Ty::Function(_)=>true,
            Ty::Address{life:Life::Static,inner,..}|Ty::Desc{life:Life::Static,inner}=>self.task_zero_domain(module,context,&inner,env,active,b,depth+1)?,
            Ty::Address{..}|Ty::Desc{..}|Ty::Variable(..)|Ty::SelfType(_)|Ty::Projection{..}|Ty::Infer(_)=>false,
            Ty::Managed(child)|Ty::Store(child)|Ty::Slice(child)|Ty::Array(child,_)=>self.task_zero_domain(module,context,&child,env,active,b,depth+1)?,
            Ty::Tuple(fields)=>{for field in fields{if !self.task_zero_domain(module,context,&field,env,active,b,depth+1)?{return Ok(false);}}true},
            Ty::ErrorUnion(error,result)=>self.task_zero_domain(module,context,&error,env,active,b,depth+1)?&&self.task_zero_domain(module,context,&result,env,active,b,depth+1)?,
            Ty::Nominal(id,args)=>{
                for argument in &args{match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.task_zero_domain(module,context,ty,env,active,b,depth+1)?{return Ok(false);},Arg::Const(c)=>if !self.task_zero_domain(module,context,&c.carrier,env,active,b,depth+1)?{return Ok(false);}}}
                let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body,..}=&definition.body else{return Err(bad("Task domain nominal has no complete original body"));};
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=args;
                let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|f|&f.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>variants.iter().flat_map(|v|v.fields.iter()).collect::<Vec<_>>()};
                b.spend(fields.len())?;
                for field in fields{let field=self.ty(module,context,field,&selected,active,b)?;if !self.task_zero_domain(module,context,&field,env,active,b,depth+1)?{return Ok(false);}}true
            }
            Ty::Opaque(owner,args)=>{let hidden=self.opaque_hidden(module,context,owner,&args,active,b)?;self.task_zero_domain(module,context,&hidden,env,active,b,depth+1)?},
            Ty::Carrier{child,retained,..}=>retained.domain.is_empty()&&self.task_zero_domain(module,context,&child,env,active,b,depth+1)?,
            Ty::Code{parameters,result,..}=>{for p in parameters{if !self.task_zero_domain(module,context,&p,env,active,b,depth+1)?{return Ok(false);}}self.task_zero_domain(module,context,&result,env,active,b,depth+1)?},
            Ty::Closure(..)=>false,
        })
    }
}
