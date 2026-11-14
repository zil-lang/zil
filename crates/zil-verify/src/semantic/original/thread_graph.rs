//! The original remote-execution profile is proved from the whole symbolic
//! entry graph, including unselected branches and temporary cleanup types.
//! Transfer and a same-named provider do not establish this role.
use super::*;
impl CheckedOriginal {
    pub(super) fn thread_callable(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        if self.builtin_premise(module,context,ty,SemanticBuiltinTrait::ThreadCallable,env,active,b)?{return Ok(true);}
        let root=match self.resolved(ty,b)?{Ty::Closure(id,..)=>id,_=>return Ok(false)};
        if !self.closed_value(module,context,ty,env,active,b)?{return Ok(false);}
        let mut queue=vec![root];let mut seen=BTreeSet::new();let previous=self.current;
        let result=(||{
            while let Some(id)=queue.pop(){b.spend(1)?;if !seen.insert(id){continue;}
                if !self.checked.contains(&id){return Ok(false);}
                let definition=context.definition(module,id,b)?;
                if !matches!(definition.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(_),..}){return Ok(false);}
                let prototype=self.prototypes.get(&id).cloned().ok_or_else(||bad("thread entry lacks its universally checked original prototype"))?;
                if prototype.call_kind!=CallKind::Direct{return Ok(false);}
                self.current=Some(id);
                for parameter in prototype.parameters.iter().chain(std::iter::once(&prototype.result)){if !self.thread_type(module,context,parameter,&prototype.environment,active,b)?{return Ok(false);}}
                let expression_types=self.expressions.iter().filter(|((owner,_),_)|*owner==id).map(|(_,ty)|ty.clone()).collect::<Vec<_>>();b.spend(self.expressions.len())?;
                for ty in expression_types{if matches!(ty,Ty::Function(_)){continue;}if !self.thread_type(module,context,&ty,&prototype.environment,active,b)?{return Ok(false);}}
                for ((owner,_),use_) in &self.intrinsic_calls{b.spend(1)?;if *owner==id&&!matches!(use_.kind,intrinsics::Intrinsic::BytesLen|intrinsics::Intrinsic::BytesCapacity|intrinsics::Intrinsic::Convert(_)){return Ok(false);}}
                for ((owner,_),target) in &self.call_targets{b.spend(1)?;if *owner==id{queue.push(*target);}}
                let callees=self.callable_nodes.iter().filter(|((owner,_),_)|*owner==id).map(|(_,node)|*node).collect::<Vec<_>>();b.spend(self.callable_nodes.len())?;
                for node in callees{
                    let Some(receiver)=self.expressions.get(&(id,node)).cloned() else{return Ok(false);};
                    match self.resolved(&receiver,b)?{Ty::Function(_)=>{},Ty::Closure(target,..)=>queue.push(target),receiver=>if !self.builtin_premise(module,context,&receiver,SemanticBuiltinTrait::ThreadCallable,&prototype.environment,active,b)?{return Ok(false);}}
                }
            }Ok(true)
        })();self.current=previous;result
    }
    fn thread_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|Ok(match self.resolved(ty,b)?{
            Ty::Atom(ty)=>ty.is_b0_scalar(),Ty::SealedError(_)=>true,
            Ty::Managed(inner)|Ty::Desc{inner,..} if *inner==Ty::Atom(Type::ByteBuffer)=>true,
            Ty::Tuple(fields)=>{for field in fields{if !self.thread_type(module,context,&field,env,active,b)?{return Ok(false);}}true},
            Ty::Array(inner,_)=>self.thread_type(module,context,&inner,env,active,b)?,
            Ty::ErrorUnion(error,result)=>self.thread_type(module,context,&error,env,active,b)?&&self.thread_type(module,context,&result,env,active,b)?,
            Ty::Nominal(id,arguments)=>{
                let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("thread entry nominal is absent"));};
                if *kind==LifeNominalKind::Resource||self.custom_drop(module,id,b)?{return Ok(false);}
                for argument in &arguments{match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.thread_type(module,context,ty,env,active,b)?{return Ok(false);},Arg::Const(_)=>{}}}
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|row|&row.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>variants.iter().flat_map(|row|&row.fields).collect()};
                for recipe in fields{let ty=self.ty(module,context,recipe,&selected,active,b)?;if !self.thread_type(module,context,&ty,env,active,b)?{return Ok(false);}}true
            },
            _=>false,
        }))
    }
}
