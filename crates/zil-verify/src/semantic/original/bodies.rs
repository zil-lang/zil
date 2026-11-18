//! Check every original operation, including uninstantiated and unreachable arms.
//! This pass establishes type relationships only. Runtime loans, effects and
//! cleanup are subsequently proved against the actual Typed instruction graph.
use super::*;
impl CheckedOriginal {
    pub(super) fn definition(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if self.checked.contains(&id){return Ok(());}
        if !self.active.insert(id){return Err(bad("original closure/body checking cycle has no established prototype"));}
        let previous=self.current.replace(id);
        let previous_premises=self.premise_environment.take();
        let result=self.definition_inner(module,context,id,active,b).map_err(|mut error|{error.message.push_str(&format!(" [original definition {id:?}]"));error});
        self.premise_environment=previous_premises;
        self.current=previous;self.active.remove(&id);
        if result.is_ok(){self.checked.insert(id);}result
    }
    fn definition_inner(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let definition=context.definition(module,id,b)?;
        self.function_head(definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,parameters,receiver_parameter,protocol_contracts,body,..}=&definition.body else{return Err(unsupported("Some semantic function lacks its complete original body",None));};
        // The complete original protocol relation is checked after all
        // symbolic bodies are available; a Typed relation never supplies it.
        let _ = protocol_contracts;
        if parameters.len()!=signature.parameters.len()||receiver_parameter.is_some_and(|ordinal|ordinal as usize>=parameters.len()){return Err(bad("original value parameter binder inventory differs from its signature"));}
        for ordinal in 0..parameters.len(){if !self.parameters.contains_key(&(id,ordinal as u32)){let value=self.fresh(b)?;self.parameters.insert((id,ordinal as u32),value);}}
        if !self.results.contains_key(&id){let value=self.fresh(b)?;self.results.insert(id,value);}
        let env=self.environment(module,context,id,active,b)?;
        let prototype=if definition.kind==SemanticDefinitionKind::Closure&&Self::receiver_signature_recipe(signature){self.receiver_closure_signature(module,context,signature,id,&env,active,b)?}else{self.signature(module,context,signature,&env,active,b)?};
        // Subsequent annotations, constructor schemes and associated types
        // all prove their obligations in this exact independently formed Γ.
        // The outer definition wrapper restores its caller even on failure.
        self.premise_environment=Some(self.copy_environment(&prototype.environment,b)?);
        let mut bindings=self.free_values.get(&id).cloned().unwrap_or_default();
        for binding in bindings.values_mut(){binding.identity=Binder::Capture{definition:id,source:Box::new(binding.identity.clone())};}
        let mut parameter_names=BTreeSet::new();
        for(ordinal,(parameter,ty))in parameters.iter().zip(&prototype.parameters).enumerate(){
            b.spend(1)?;
            if !parameter_names.insert(&parameter.name){return Err(bad("original parameter shadows another parameter or creator binding"));}
            if parameter.mode!=signature.parameters[ordinal].mode{return Err(bad("original value binder changes its declared passing mode"));}
            let provisional=self.parameters.get(&(id,ordinal as u32)).cloned().ok_or_else(||bad("original parameter inference slot absent"))?;
            self.unify(&provisional,ty,b)?;
            if let Some(annotation)=&parameter.ty{let expected=self.ty(module,context,annotation,&prototype.environment,active,b)?;let expected=self.mode_type(module,context,expected,parameter.mode,Some(ordinal as u32),&prototype.environment,b)?;self.unify(ty,&expected,b)?;}
            bindings.insert(parameter.name.clone(),Binding{ty:ty.clone(),mutable:false,identity:Binder::Parameter{definition:id,ordinal:ordinal as u32}});
        }
        let inferred_result=self.results.get(&id).cloned().ok_or_else(||bad("original result inference slot absent"))?;
        if !matches!(&prototype.result,Ty::Opaque(owner,_)if *owner==id){self.unify(&inferred_result,&prototype.result,b)?;}
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
        for parameter in &prototype.parameters{self.assume_input_address_wf(module,context,parameter,id,active,b)?;}
        self.check_names(module,context,definition,&prototype.environment,active,b)?;
        for(mode,ty)in prototype.modes.iter().zip(&prototype.parameters){if *mode==PassingMode::Copy{self.require_copy(module,context,ty,&prototype.environment,active,b)?;}}
        if prototype.result_mode==PassingMode::Copy{self.require_copy(module,context,&prototype.result,&prototype.environment,active,b)?;}
        match body {
            SemanticOriginalFunctionBody::Defined(body)=>{
                let body_result=if matches!(&prototype.result,Ty::Opaque(owner,_)if *owner==id){inferred_result.clone()}else{prototype.result.clone()};
                let returns=self.block(module,context,body,&prototype.environment,&mut bindings,&body_result,0,active,b)?;
                if !returns {self.unify(&body_result,&Ty::Atom(Type::Unit),b)?;self.queue_cleanup(&prototype.environment,None,b)?;}
            },
            SemanticOriginalFunctionBody::External=>{
                let program=module.semantic.as_ref().ok_or_else(||bad("original program absent"))?;
                let targets=program.instances.iter().filter(|row|row.definition==id).collect::<Vec<_>>();b.spend(program.instances.len())?;
                if targets.is_empty(){return Err(unsupported("original external declaration has no complete independently checked ABI",None));}
                for target in targets {if let Some(source)=context.frozen_functions.get(&target.id){self.frozen_external(module,context,&prototype,source,active,b)?;continue;}let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=target.target else{return Err(bad("original external targets nonfunction"));};self.typed_external(module,context,&prototype,function,active,b)?;}
            },
            SemanticOriginalFunctionBody::Declaration=>self.abstract_method(module,context,definition,&prototype,active,b)?,
        }
        self.check_opaque_result(module,context,id,&prototype.environment,active,b)?;
        if definition.kind==SemanticDefinitionKind::Closure {
            let consumes=self.uses.get(&id).is_some_and(|uses|uses.moved.iter().any(|place|matches!(place.binder,Binder::Capture{..})));
            let writes=self.written.get(&id).is_some_and(|rows|rows.iter().any(|binder|matches!(binder,Binder::Capture{..})));
            if let Some(prototype)=self.prototypes.get_mut(&id){prototype.callable_kind=Some(if consumes{CallableKind::FnOnce}else if writes{CallableKind::FnMut}else{CallableKind::Fn});}
            if Self::receiver_signature_recipe(signature){if self.prototypes.get(&id).is_some_and(|p|p.callable_kind==Some(CallableKind::FnOnce)){return Err(bad("a consuming closure cannot export its environment Receiver window"));}b.spend(1)?;self.receiver_results.insert(id);}
        }
        Ok(())
    }
    pub(super) fn collect_equalities(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,env:&Env,output:&mut Vec<(Ty,Ty)>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::LifeEqual{left,right}=>{let left=self.life(module,context,left,env,active,b)?;let right=self.life(module,context,right,env,active,b)?;let owner=self.current.ok_or_else(||bad("original lifetime premise has no declaration context"))?;self.life_equalities.entry(owner).or_default().push((left,right));},
            SemanticRequirementRecipe::Outlives{longer,shorter}=>{let longer=self.life(module,context,longer,env,active,b)?;let shorter=self.life(module,context,shorter,env,active,b)?;let owner=self.current.ok_or_else(||bad("original Outlives premise has no active declaration"))?;self.life_outlives.entry(owner).or_default().push((longer,shorter));},
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
                        let family=expected.as_ref().map(|ty|self.raw_binding_family(module,context,ty,b)).transpose()?.unwrap_or(false);
                        if family {
                            let initializer=initializer.as_ref().ok_or_else(||bad("raw resource binding requires its actual category-determining initializer"))?;
                            let mut root=initializer;
                            while let CoreExprKind::Group(inner)=&root.kind {b.spend(1)?;root=inner;}
                            self.binding_families.insert((env.current()?,root.id),expected.as_ref().unwrap().clone());
                        }
                        let actual=match initializer{Some(value)=>{
                            let wanted=if family{None}else{expected.as_ref()};
                            if let Some(alias)=self.description_binding(module,context,value,wanted,env,bindings,active,b)? {alias}
                            else{self.expression(module,context,value,env,bindings,wanted,active,b)?}
                        },None=>match expected{Some(ref ty)=>ty.clone(),None=>return Err(bad("original uninitialized binding lacks its full type"))}};
                        let actual=if let Some(initializer)=initializer{self.code_value(module,context,initializer,&actual,expected.as_ref(),true,env,active,b)?}else{actual};
                        if let Some(expected)=expected{if family{self.bind_family_result(&actual,&expected,b)?;}else{self.relate_value(&actual,&expected,b)?;}}
                        let actual=if *mutable&&ty.is_none(){self.introduce_shared_local(module,context,statement.id,&actual,env,b)?}else{actual};
                        self.bindings.insert((env.current()?,statement.id),actual.clone());let binding=Binding{ty:actual,mutable:*mutable,identity:Binder::Local{definition:env.current()?,node:statement.id,name:name.clone()}};if let Some(initializer)=initializer{if self.creation(env.current()?,initializer,bindings,b)?{b.spend(1)?;self.created_bindings.insert(binding.identity.clone());}}self.declare(env,&binding,b)?;bindings.insert(name.clone(),binding);false
                    },
                    CoreStatementKind::TupleBinding{pattern,mutable,ty,initializer}=>{let expected=ty.as_ref().map(|recipe|self.ty(module,context,recipe,env,active,b)).transpose()?;let actual=self.expression(module,context,initializer,env,bindings,expected.as_ref(),active,b)?;self.pattern_root(module,context,statement.id,&[],pattern,&actual,*mutable,env,bindings,active,b)?;self.bindings.insert((env.current()?,statement.id),actual);false},
                    CoreStatementKind::Expression(expression)=>{let value=self.expression(module,context,expression,env,bindings,None,active,b)?;self.overwrite_cleanup(env,expression.id,&value,None,b)?;false},
                    CoreStatementKind::Return(expression)=>{if self.in_defer.get(&env.current()?).copied().unwrap_or(0)!=0{return Err(bad("original deferred cleanup cannot return from its caller"));}let actual=match expression{Some(expression)=>self.expression(module,context,expression,env,bindings,Some(result),active,b)?,None=>Ty::Atom(Type::Unit)};self.relate_value(&actual,result,b)?;self.return_resource(expression.as_ref(),&actual,env,b)?;self.queue_cleanup(env,None,b)?;true},
                    CoreStatementKind::Assign{target,value}=>{let(expected,place)=self.place(module,context,target,env,bindings,active,b)?;self.assignment_place(module,context,target,&expected,env,bindings,active,b)?;if !self.shared_local_assignment(module,context,target,value,&expected,place.as_ref(),env,bindings,active,b)?{let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.relate_value(&actual,&expected,b)?;}self.overwrite_cleanup(env,target.id,&expected,place.as_ref(),b)?;if let Some(place)=place{if !self.creation(env.current()?,value,bindings,b)?{b.spend(1)?;self.creation_tainted.insert(place.binder.clone());}self.uses.entry(env.current()?).or_default().initialize(&place,b)?;}false},
                    CoreStatementKind::CompoundAssign{target,op,value}=>{let expected=self.expression(module,context,target,env,bindings,None,active,b)?;self.assignment_place(module,context,target,&expected,env,bindings,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.numeric_binary_original(*op,&expected,&actual,b)?;false},
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
                        if !returns{self.loop_uses(module,context,env,&entry,&backedge,bindings,active,b)?;}self.set_uses(env,after_condition)?;Self::constant_true(condition,b)?&&!Self::loop_break(body,b)?
                    },
                    CoreStatementKind::WhileLet{pattern,value,body}=>{
                        let entry=self.use_snapshot(env,b)?;let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let after_condition=self.use_snapshot(env,b)?;
                        let live_before=self.live_keys(env,b)?;let mut inner=bindings.clone();self.pattern_root(module,context,statement.id,&[],pattern,&ty,false,env,&mut inner,active,b)?;
                        let returns=self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;if returns{self.forget_scope(env,&live_before,b)?;}else{self.exit_scope(env,&live_before,b)?;}let backedge=self.use_snapshot(env,b)?;
                        if !returns{self.loop_uses(module,context,env,&entry,&backedge,bindings,active,b)?;}self.set_uses(env,after_condition)?;false
                    },
                    CoreStatementKind::ForRange{name,start,end,body}=>{let ty=self.range_endpoints(module,context,start,end,env,bindings,active,b)?;let scope_before=self.live_keys(env,b)?;let mut inner=bindings.clone();let binding=Binding{ty,mutable:false,identity:Binder::Local{definition:env.current()?,node:statement.id,name:name.clone()}};self.declare(env,&binding,b)?;inner.insert(name.clone(),binding);let before=self.use_snapshot(env,b)?;let returns=self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;if returns{self.forget_scope(env,&scope_before,b)?;}else{self.exit_scope(env,&scope_before,b)?;}let after=self.use_snapshot(env,b)?;if !returns{self.loop_uses(module,context,env,&before,&after,bindings,active,b)?;}self.set_uses(env,before)?;false},
                    CoreStatementKind::ForEach{pattern,iterator,body}=>{
                        let mut range=iterator;while let CoreExprKind::Group(inner)=&range.kind{b.spend(1)?;range=inner;}
                        let CoreExprKind::Range{start,end}=&range.kind else{return Err(unsupported("original iteration needs its complete checked iterator protocol",Some(iterator.span)));};
                        if !matches!(pattern.kind,CorePatternKind::Name(_)|CorePatternKind::Wildcard){return Err(bad("integer range requires one irrefutable original value pattern"));}
                        self.remember_names(env,SemanticOriginalNode::Expression(iterator.id),bindings,b)?;
                        let ty=self.range_endpoints(module,context,start,end,env,bindings,active,b)?;
                        let before=self.use_snapshot(env,b)?;let scope_before=self.live_keys(env,b)?;let mut inner=bindings.clone();
                        self.pattern_root(module,context,statement.id,&[],pattern,&ty,false,env,&mut inner,active,b)?;
                        let returns=self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;
                        if returns{self.forget_scope(env,&scope_before,b)?;}else{self.exit_scope(env,&scope_before,b)?;}
                        let after=self.use_snapshot(env,b)?;if !returns{self.loop_uses(module,context,env,&before,&after,bindings,active,b)?;}self.set_uses(env,before)?;false
                    },
                    CoreStatementKind::Match{value,arms}=>{
                        let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let before=self.use_snapshot(env,b)?;let mut continuing=None;let mut all=!arms.is_empty();
                        for(arm_index,arm)in arms.iter().enumerate(){self.set_uses(env,before.clone())?;let arm_live=self.live_keys(env,b)?;let mut inner=bindings.clone();self.pattern_root(module,context,statement.id,&[arm_index as u32],&arm.pattern,&ty,false,env,&mut inner,active,b)?;
                            let returns=self.block(module,context,&arm.body,env,&mut inner,result,loops,active,b)?;if returns{self.forget_scope(env,&arm_live,b)?;}else{self.exit_scope(env,&arm_live,b)?;}all&=returns;
                            if !returns{let branch=self.use_snapshot(env,b)?;if let Some(previous)=&mut continuing{let previous:&mut Uses=previous;previous.join(&branch,b)?;}else{continuing=Some(branch);}}
                        }
                        self.set_uses(env,continuing.unwrap_or(before))?;all
                    },
                    CoreStatementKind::Block(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,
                    CoreStatementKind::Unsafe(body)=>{let id=env.current()?;*self.unsafe_depth.entry(id).or_default()+=1;let checked=self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b);*self.unsafe_depth.entry(id).or_default()-=1;checked?},
                    CoreStatementKind::Defer(body)=>{
                        let before=self.use_snapshot(env,b)?;let id=env.current()?;*self.in_defer.entry(id).or_default()+=1;
                        let checked=self.block(module,context,body,env,&mut bindings.clone(),&Ty::Atom(Type::Unit),0,active,b);*self.in_defer.entry(id).or_default()-=1;checked?;
                        let after=self.use_snapshot(env,b)?;self.set_uses(env,before.clone())?;
                        if after.moved.difference(&before.moved).any(|place|bindings.values().any(|binding|binding.identity==place.binder)){return Err(unsupported("original deferred value transfer requires its complete scope-exit use schedule",None));}false
                    },
                    CoreStatementKind::Loop(body)=>{let before=self.use_snapshot(env,b)?;let returns=self.block(module,context,body,env,&mut bindings.clone(),result,loops+1,active,b)?;let after=self.use_snapshot(env,b)?;if !returns{self.loop_uses(module,context,env,&before,&after,bindings,active,b)?;}!Self::loop_break(body,b)?},
                    CoreStatementKind::Break|CoreStatementKind::Continue if loops!=0=>false,
                    _=>return Err(unsupported("original statement has no complete symbolic operation rule",Some(statement.span))),
                };
                returns|=completed;
            }if returns{self.forget_scope(env,&live_before,b)?;}else{self.exit_scope(env,&live_before,b)?;}Ok(returns)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn condition_original(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {let ty=self.expression(module,context,expression,env,bindings,Some(&Ty::Atom(Type::Bool)),active,b)?;self.unify(&ty,&Ty::Atom(Type::Bool),b)}
    pub(super) fn numeric_binary_original(&mut self,op:BinaryOp,left:&Ty,right:&Ty,b:&mut Budget)->Check<Ty>{self.unify(left,right,b)?;let ty=self.resolved(left,b)?;let Ty::Atom(ref atom)=ty else{return Err(unsupported("original arithmetic needs a proved scalar operator contract",None));};let valid=match op{BinaryOp::And|BinaryOp::Or=>*atom==Type::Bool,BinaryOp::Equal|BinaryOp::NotEqual=>atom.is_integer()||atom.is_float()||matches!(atom,Type::Bool|Type::ForeignHandle),BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual=>atom.is_integer()||atom.is_float(),BinaryOp::Add|BinaryOp::Subtract|BinaryOp::Multiply|BinaryOp::Divide|BinaryOp::Remainder=>atom.is_integer()||atom.is_float(),_=>atom.is_integer()};if !valid{return Err(bad("original binary operator is invalid for its complete scalar carrier"));}Ok(if matches!(op,BinaryOp::And|BinaryOp::Or|BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual){Ty::Atom(Type::Bool)}else{ty})}
}

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    fn range_endpoints(&mut self,module:&TypedModule,context:&CheckedContext,start:&SemanticOriginalExpr,end:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let(left,right)=if Self::contextual_numeric_literal(start,b)?{
            let right=self.expression(module,context,end,env,bindings,None,active,b)?;let left=self.expression(module,context,start,env,bindings,Some(&right),active,b)?;(left,right)
        }else{let left=self.expression(module,context,start,env,bindings,None,active,b)?;let right=self.expression(module,context,end,env,bindings,Some(&left),active,b)?;(left,right)};
        self.unify(&left,&right,b)?;let ty=self.resolved(&left,b)?;if !matches!(&ty,Ty::Atom(ty)if ty.is_integer()){return Err(bad("original range endpoints are not one complete integer carrier"));}Ok(ty)
    }
    fn constant_true(mut expression:&SemanticOriginalExpr,b:&mut Budget)->Check<bool>{
        loop {
            b.spend(1)?;
            match &expression.kind {
                CoreExprKind::Group(inner)=>expression=inner,
                CoreExprKind::Bool(value)=>return Ok(*value),
                _=>return Ok(false),
            }
        }
    }
    fn loop_break(body:&SemanticOriginalBlock,b:&mut Budget)->Check<bool>{
        let mut pending=vec![body];
        while let Some(body)=pending.pop(){
            b.spend(body.statements.len().saturating_add(1))?;
            for statement in &body.statements {
                match &statement.kind {
                    CoreStatementKind::Break=>return Ok(true),
                    CoreStatementKind::If{then_body,else_body,..}|CoreStatementKind::IfLet{then_body,else_body,..}=>{
                        pending.push(then_body);
                        if let Some(body)=else_body{pending.push(body);}
                    },
                    CoreStatementKind::Match{arms,..}=>{b.spend(arms.len())?;for arm in arms{pending.push(&arm.body);}},
                    CoreStatementKind::Block(body)|CoreStatementKind::Unsafe(body)=>pending.push(body),
                    // Nested loops bind their own break; deferred bodies have
                    // an independent control context checked by block().
                    _=>{},
                }
            }
        }
        Ok(false)
    }
    pub(super) fn contextual_numeric_literal(expression:&SemanticOriginalExpr,b:&mut Budget)->Check<bool>{
        let mut pending=vec![expression];
        while let Some(expression)=pending.pop(){
            b.spend(1)?;
            match &expression.kind{
                CoreExprKind::Integer(text)|CoreExprKind::Float(text)=>{
                    b.spend(text.len().saturating_add(12))?;
                    if ["isize","usize","i64","i32","i16","i8","u64","u32","u16","u8","f64","f32"].iter().any(|suffix|text.ends_with(*suffix)){return Ok(false);}
                },
                CoreExprKind::Group(inner)|CoreExprKind::CompileTime(inner)|CoreExprKind::Unary{op:UnaryOp::Negate|UnaryOp::BitNot,operand:inner}=>pending.push(inner),
                CoreExprKind::Sequence{bindings,value}if bindings.is_empty()=>pending.push(value),
                CoreExprKind::Binary{op:BinaryOp::Add|BinaryOp::Subtract|BinaryOp::Multiply|BinaryOp::Divide|BinaryOp::Remainder|BinaryOp::AddWrapping|BinaryOp::SubtractWrapping|BinaryOp::MultiplyWrapping|BinaryOp::BitAnd|BinaryOp::BitOr|BinaryOp::BitXor|BinaryOp::ShiftLeft|BinaryOp::ShiftRight,left,right}=>{pending.push(left);pending.push(right);},
                _=>return Ok(false),
            }
        }
        Ok(true)
    }
}
