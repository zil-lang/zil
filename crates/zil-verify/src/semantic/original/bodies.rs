//! Check every original operation, including uninstantiated and unreachable arms.
//! This pass establishes type relationships only. Runtime loans, effects and
//! cleanup are subsequently proved against the actual Typed instruction graph.
use super::*;
impl CheckedOriginal {
    pub(super) fn definition(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if self.checked.contains(&id){return Ok(());}
        if !self.active.insert(id){return Err(bad("original closure/body checking cycle has no established prototype"));}
        let previous=self.current.replace(id);
        let result=self.definition_inner(module,context,id,active,b);
        self.current=previous;self.active.remove(&id);
        if result.is_ok(){self.checked.insert(id);}result
    }
    fn definition_inner(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let definition=context.definition(module,id,b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,parameters,receiver_parameter,body,..}=&definition.body else{return Err(unsupported("Some semantic function lacks its complete original body",None));};
        if parameters.len()!=signature.parameters.len()||receiver_parameter.is_some_and(|ordinal|ordinal as usize>=parameters.len()){return Err(bad("original value parameter binder inventory differs from its signature"));}
        for ordinal in 0..parameters.len(){if !self.parameters.contains_key(&(id,ordinal as u32)){let value=self.fresh(b)?;self.parameters.insert((id,ordinal as u32),value);}}
        if !self.results.contains_key(&id){let value=self.fresh(b)?;self.results.insert(id,value);}
        let env=self.environment(module,context,id,active,b)?;
        let prototype=self.signature(module,context,signature,&env,active,b)?;
        let mut bindings=self.free_values.get(&id).cloned().unwrap_or_default();
        for binding in bindings.values_mut(){binding.identity=Binder::Capture{definition:id,source:Box::new(binding.identity.clone())};}
        let mut parameter_names=BTreeSet::new();
        for(ordinal,(parameter,ty))in parameters.iter().zip(&prototype.parameters).enumerate(){
            b.spend(1)?;
            if !parameter_names.insert(&parameter.name){return Err(bad("original parameter shadows another parameter or creator binding"));}
            if parameter.mode!=signature.parameters[ordinal].mode{return Err(bad("original value binder changes its declared passing mode"));}
            let provisional=self.parameters.get(&(id,ordinal as u32)).cloned().ok_or_else(||bad("original parameter inference slot absent"))?;
            self.unify(&provisional,ty,b)?;
            if let Some(annotation)=&parameter.ty{let expected=self.ty(module,context,annotation,&prototype.environment,active,b)?;self.unify(ty,&expected,b)?;}
            bindings.insert(parameter.name.clone(),Binding{ty:ty.clone(),mutable:false,identity:Binder::Parameter{definition:id,ordinal:ordinal as u32}});
        }
        let inferred_result=self.results.get(&id).cloned().ok_or_else(||bad("original result inference slot absent"))?;
        self.unify(&inferred_result,&prototype.result,b)?;
        self.prototypes.insert(id,prototype.clone());
        for binding in bindings.values(){self.declare(&prototype.environment,binding,b)?;}
        let mut equalities=Vec::new();
        for frame in &prototype.environment.lexical {
            let original=context.definition(module,frame.definition,b)?;
            let mut lexical=prototype.environment.clone();let depth=lexical.lexical.iter().position(|row|row.definition==frame.definition).unwrap();lexical.lexical=lexical.lexical[depth..].to_vec();
            for requirement in &original.requirements {self.collect_equalities(module,context,requirement,&lexical,&mut equalities,active,b)?;}
            for formal in &original.parameters {if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.collect_equalities(module,context,requirement,&lexical,&mut equalities,active,b)?;}}}
        }
        self.equalities.insert(id,equalities);
        self.check_names(module,context,definition,&prototype.environment,active,b)?;
        for(mode,ty)in prototype.modes.iter().zip(&prototype.parameters){if *mode==PassingMode::Copy{self.require_copy(module,context,ty,&prototype.environment,active,b)?;}}
        if prototype.result_mode==PassingMode::Copy{self.require_copy(module,context,&prototype.result,&prototype.environment,active,b)?;}
        match body {
            SemanticOriginalFunctionBody::Defined(body)=>{
                let returns=self.block(module,context,body,&prototype.environment,&mut bindings,&prototype.result,0,active,b)?;
                if !returns {self.unify(&prototype.result,&Ty::Atom(Type::Unit),b)?;self.queue_cleanup(&prototype.environment,None,b)?;}
            },
            SemanticOriginalFunctionBody::External=>{
                let program=module.semantic.as_ref().ok_or_else(||bad("original program absent"))?;
                let targets=program.instances.iter().filter(|row|row.definition==id).collect::<Vec<_>>();b.spend(program.instances.len())?;
                if targets.is_empty(){return Err(unsupported("original external declaration has no complete independently checked ABI",None));}
                for target in targets {let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=target.target else{return Err(bad("original external targets nonfunction"));};if !module.functions.get(function.0 as usize).is_some_and(|row|row.foreign.is_some()){return Err(bad("original external body hides a real implementation"));}}
            },
            SemanticOriginalFunctionBody::Declaration=>return Err(unsupported("abstract original member requires its exact trait declaration/member proof",None)),
        }
        if definition.kind==SemanticDefinitionKind::Closure {
            let consumes=self.uses.get(&id).is_some_and(|uses|uses.moved.iter().any(|place|matches!(place.binder,Binder::Capture{..})));
            let writes=self.written.get(&id).is_some_and(|rows|rows.iter().any(|binder|matches!(binder,Binder::Capture{..})));
            if let Some(prototype)=self.prototypes.get_mut(&id){prototype.callable_kind=Some(if consumes{CallableKind::FnOnce}else if writes{CallableKind::FnMut}else{CallableKind::Fn});}
        }
        Ok(())
    }
    fn collect_equalities(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,env:&Env,output:&mut Vec<(Ty,Ty)>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::LifeEqual{left,right}=>{let left=self.life(module,context,left,env,active,b)?;let right=self.life(module,context,right,env,active,b)?;let owner=self.current.ok_or_else(||bad("original lifetime premise has no declaration context"))?;self.life_equalities.entry(owner).or_default().push((left,right));},
            SemanticRequirementRecipe::TypeEqual{left,right}=>output.push((self.ty(module,context,left,env,active,b)?,self.ty(module,context,right,env,active,b)?)),
            SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{let mut env=env.clone();if *lifetimes!=0{let token=active.fresh(b)?;b.spend(*lifetimes as usize)?;env.bounds.insert(0,(0..*lifetimes).map(|index|Life::Bound(token,index)).collect());}for requirement in requirements{self.collect_equalities(module,context,requirement,&env,output,active,b)?;}},
            _=>{},
        }Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn block(&mut self,module:&TypedModule,context:&CheckedContext,block:&SemanticOriginalBlock,env:&Env,bindings:&mut BTreeMap<String,Binding>,result:&Ty,loops:usize,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let live_before=self.live_keys(env,b)?;
            let mut returns=false;
            for statement in &block.statements {
                b.spend(1)?;self.remember_names(env,SemanticOriginalNode::Statement(statement.id),bindings,b)?;
                let completed=match &statement.kind {
                    CoreStatementKind::Binding{name,mutable,ty,initializer}=>{
                        let expected=match ty{Some(recipe)=>Some(self.ty(module,context,recipe,env,active,b)?),None=>None};
                        let actual=match initializer{Some(value)=>self.expression(module,context,value,env,bindings,expected.as_ref(),active,b)?,None=>match expected{Some(ref ty)=>ty.clone(),None=>return Err(bad("original uninitialized binding lacks its full type"))}};
                        if let Some(expected)=expected{self.unify(&actual,&expected,b)?;}
                        self.bindings.insert((env.current()?,statement.id),actual.clone());let binding=Binding{ty:actual,mutable:*mutable,identity:Binder::Local{definition:env.current()?,node:statement.id,name:name.clone()}};self.declare(env,&binding,b)?;bindings.insert(name.clone(),binding);false
                    },
                    CoreStatementKind::TupleBinding{pattern,mutable,ty,initializer}=>{let expected=ty.as_ref().map(|recipe|self.ty(module,context,recipe,env,active,b)).transpose()?;let actual=self.expression(module,context,initializer,env,bindings,expected.as_ref(),active,b)?;self.pattern_root(module,context,statement.id,&[],pattern,&actual,*mutable,env,bindings,active,b)?;self.bindings.insert((env.current()?,statement.id),actual);false},
                    CoreStatementKind::Expression(expression)=>{let value=self.expression(module,context,expression,env,bindings,None,active,b)?;self.overwrite_cleanup(env,expression.id,&value,None,b)?;false},
                    CoreStatementKind::Return(expression)=>{if self.in_defer.get(&env.current()?).copied().unwrap_or(0)!=0{return Err(bad("original deferred cleanup cannot return from its caller"));}let actual=match expression{Some(expression)=>self.expression(module,context,expression,env,bindings,Some(result),active,b)?,None=>Ty::Atom(Type::Unit)};self.unify(&actual,result,b)?;self.queue_cleanup(env,None,b)?;true},
                    CoreStatementKind::Assign{target,value}=>{self.mutable_place(module,context,target,env,bindings,active,b)?;let(expected,place)=self.place(module,context,target,env,bindings,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.unify(&actual,&expected,b)?;self.overwrite_cleanup(env,target.id,&expected,place.as_ref(),b)?;if let Some(place)=place{self.uses.entry(env.current()?).or_default().initialize(&place,b)?;}false},
                    CoreStatementKind::CompoundAssign{target,op,value}=>{self.mutable_place(module,context,target,env,bindings,active,b)?;let expected=self.expression(module,context,target,env,bindings,None,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.numeric_binary_original(*op,&expected,&actual,b)?;false},
                    CoreStatementKind::If{condition,then_body,else_body}=>{
                        self.condition_original(module,context,condition,env,bindings,active,b)?;let before=self.use_snapshot(env,b)?;
                        let yes_returns=self.block(module,context,then_body,env,&mut bindings.clone(),result,loops,active,b)?;let yes=self.use_snapshot(env,b)?;
                        self.set_uses(env,before.clone())?;let no_returns=match else_body{Some(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,None=>false};let no=self.use_snapshot(env,b)?;
                        self.branch_uses(env,&before,&yes,yes_returns,&no,no_returns,b)?;yes_returns&&no_returns
                    },
                    CoreStatementKind::IfLet{pattern,value,then_body,else_body}=>{
                        let actual=self.expression(module,context,value,env,bindings,None,active,b)?;let before=self.use_snapshot(env,b)?;let live_before=self.live_keys(env,b)?;let mut branch=bindings.clone();self.pattern_root(module,context,statement.id,&[],pattern,&actual,false,env,&mut branch,active,b)?;
                        let yes_returns=self.block(module,context,then_body,env,&mut branch,result,loops,active,b)?;if yes_returns{self.forget_scope(env,&live_before,b)?;}else{self.exit_scope(env,&live_before,b)?;}let yes=self.use_snapshot(env,b)?;
                        self.set_uses(env,before.clone())?;let no_returns=match else_body{Some(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,None=>false};let no=self.use_snapshot(env,b)?;
                        self.branch_uses(env,&before,&yes,yes_returns,&no,no_returns,b)?;yes_returns&&no_returns
                    },
                    CoreStatementKind::While{condition,body}=>{
                        let entry=self.use_snapshot(env,b)?;self.condition_original(module,context,condition,env,bindings,active,b)?;let after_condition=self.use_snapshot(env,b)?;
                        let returns=self.block(module,context,body,env,&mut bindings.clone(),result,loops+1,active,b)?;let backedge=self.use_snapshot(env,b)?;
                        if !returns{self.loop_uses(module,context,env,&entry,&backedge,bindings,active,b)?;}self.set_uses(env,after_condition)?;false
                    },
                    CoreStatementKind::WhileLet{pattern,value,body}=>{
                        let entry=self.use_snapshot(env,b)?;let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let after_condition=self.use_snapshot(env,b)?;
                        let live_before=self.live_keys(env,b)?;let mut inner=bindings.clone();self.pattern_root(module,context,statement.id,&[],pattern,&ty,false,env,&mut inner,active,b)?;
                        let returns=self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;if returns{self.forget_scope(env,&live_before,b)?;}else{self.exit_scope(env,&live_before,b)?;}let backedge=self.use_snapshot(env,b)?;
                        if !returns{self.loop_uses(module,context,env,&entry,&backedge,bindings,active,b)?;}self.set_uses(env,after_condition)?;false
                    },
                    CoreStatementKind::ForRange{name,start,end,body}=>{let ty=self.expression(module,context,start,env,bindings,None,active,b)?;let end=self.expression(module,context,end,env,bindings,Some(&ty),active,b)?;self.unify(&ty,&end,b)?;if !matches!(self.resolved(&ty,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original range endpoints are not integers"));}let mut inner=bindings.clone();let binding=Binding{ty,mutable:false,identity:Binder::Local{definition:env.current()?,node:statement.id,name:name.clone()}};self.declare(env,&binding,b)?;inner.insert(name.clone(),binding);let before=self.use_snapshot(env,b)?;let returns=self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;let after=self.use_snapshot(env,b)?;if !returns{self.loop_uses(module,context,env,&before,&after,bindings,active,b)?;}self.set_uses(env,before)?;false},
                    CoreStatementKind::Match{value,arms}=>{
                        let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let before=self.use_snapshot(env,b)?;let mut continuing=None;let mut all=!arms.is_empty();
                        for(arm_index,arm)in arms.iter().enumerate(){self.set_uses(env,before.clone())?;let arm_live=self.live_keys(env,b)?;let mut inner=bindings.clone();self.pattern_root(module,context,statement.id,&[arm_index as u32],&arm.pattern,&ty,false,env,&mut inner,active,b)?;
                            let returns=self.block(module,context,&arm.body,env,&mut inner,result,loops,active,b)?;if returns{self.forget_scope(env,&arm_live,b)?;}else{self.exit_scope(env,&arm_live,b)?;}all&=returns;
                            if !returns{let branch=self.use_snapshot(env,b)?;if let Some(previous)=&mut continuing{let previous:&mut Uses=previous;previous.join(&branch,b)?;}else{continuing=Some(branch);}}
                        }
                        self.set_uses(env,continuing.unwrap_or(before))?;all
                    },
                    CoreStatementKind::Block(body)|CoreStatementKind::Unsafe(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,
                    CoreStatementKind::Defer(body)=>{
                        let before=self.use_snapshot(env,b)?;let id=env.current()?;*self.in_defer.entry(id).or_default()+=1;
                        let checked=self.block(module,context,body,env,&mut bindings.clone(),&Ty::Atom(Type::Unit),0,active,b);*self.in_defer.entry(id).or_default()-=1;checked?;
                        let after=self.use_snapshot(env,b)?;self.set_uses(env,before.clone())?;
                        if after.moved.difference(&before.moved).any(|place|bindings.values().any(|binding|binding.identity==place.binder)){return Err(unsupported("original deferred value transfer requires its complete scope-exit use schedule",None));}false
                    },
                    CoreStatementKind::Loop(body)=>{let before=self.use_snapshot(env,b)?;let returns=self.block(module,context,body,env,&mut bindings.clone(),result,loops+1,active,b)?;let after=self.use_snapshot(env,b)?;if !returns{self.loop_uses(module,context,env,&before,&after,bindings,active,b)?;}false},
                    CoreStatementKind::Break|CoreStatementKind::Continue if loops!=0=>false,
                    _=>return Err(unsupported("original statement has no complete symbolic operation rule",Some(statement.span))),
                };
                returns|=completed;
            }if returns{self.forget_scope(env,&live_before,b)?;}else{self.exit_scope(env,&live_before,b)?;}Ok(returns)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn condition_original(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {let ty=self.expression(module,context,expression,env,bindings,Some(&Ty::Atom(Type::Bool)),active,b)?;self.unify(&ty,&Ty::Atom(Type::Bool),b)}
    pub(super) fn numeric_binary_original(&mut self,op:BinaryOp,left:&Ty,right:&Ty,b:&mut Budget)->Check<Ty>{self.unify(left,right,b)?;let ty=self.resolved(left,b)?;let Ty::Atom(ref atom)=ty else{return Err(unsupported("original arithmetic needs a proved scalar operator contract",None));};let valid=match op{BinaryOp::And|BinaryOp::Or=>*atom==Type::Bool,BinaryOp::Equal|BinaryOp::NotEqual=>atom.is_integer()||atom.is_float()||*atom==Type::Bool,BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual=>atom.is_integer()||atom.is_float(),BinaryOp::Add|BinaryOp::Subtract|BinaryOp::Multiply|BinaryOp::Divide|BinaryOp::Remainder=>atom.is_integer()||atom.is_float(),_=>atom.is_integer()};if !valid{return Err(bad("original binary operator is invalid for its complete scalar carrier"));}Ok(if matches!(op,BinaryOp::And|BinaryOp::Or|BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual){Ty::Atom(Type::Bool)}else{ty})}
}
