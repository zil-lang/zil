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
        let mut parameter_names=BTreeSet::new();
        for(ordinal,(parameter,ty))in parameters.iter().zip(&prototype.parameters).enumerate(){
            b.spend(1)?;
            if !parameter_names.insert(&parameter.name){return Err(bad("original parameter shadows another parameter or creator binding"));}
            let provisional=self.parameters.get(&(id,ordinal as u32)).cloned().ok_or_else(||bad("original parameter inference slot absent"))?;
            self.unify(&provisional,ty,b)?;
            if let Some(annotation)=&parameter.ty{let expected=self.ty(module,context,annotation,&prototype.environment,active,b)?;self.unify(ty,&expected,b)?;}
            bindings.insert(parameter.name.clone(),Binding{ty:ty.clone(),mutable:false});
        }
        let inferred_result=self.results.get(&id).cloned().ok_or_else(||bad("original result inference slot absent"))?;
        self.unify(&inferred_result,&prototype.result,b)?;
        self.prototypes.insert(id,prototype.clone());
        let mut equalities=Vec::new();
        for frame in &prototype.environment.lexical {
            let original=context.definition(module,frame.definition,b)?;
            let mut lexical=prototype.environment.clone();let depth=lexical.lexical.iter().position(|row|row.definition==frame.definition).unwrap();lexical.lexical=lexical.lexical[depth..].to_vec();
            for requirement in &original.requirements {self.collect_equalities(module,context,requirement,&lexical,&mut equalities,active,b)?;}
            for formal in &original.parameters {if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.collect_equalities(module,context,requirement,&lexical,&mut equalities,active,b)?;}}}
        }
        self.equalities.insert(id,equalities);
        match body {
            SemanticOriginalFunctionBody::Defined(body)=>{
                let returns=self.block(module,context,body,&prototype.environment,&mut bindings,&prototype.result,0,active,b)?;
                if !returns {self.unify(&prototype.result,&Ty::Atom(Type::Unit),b)?;}
            },
            SemanticOriginalFunctionBody::External=>{
                let program=module.semantic.as_ref().ok_or_else(||bad("original program absent"))?;
                let targets=program.instances.iter().filter(|row|row.definition==id).collect::<Vec<_>>();b.spend(program.instances.len())?;
                if targets.is_empty(){return Err(unsupported("original external declaration has no complete independently checked ABI",None));}
                for target in targets {let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=target.target else{return Err(bad("original external targets nonfunction"));};if !module.functions.get(function.0 as usize).is_some_and(|row|row.foreign.is_some()){return Err(bad("original external body hides a real implementation"));}}
            },
            SemanticOriginalFunctionBody::Declaration=>return Err(unsupported("abstract original member requires its exact trait declaration/member proof",None)),
        }
        Ok(())
    }
    fn collect_equalities(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,env:&Env,output:&mut Vec<(Ty,Ty)>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::TypeEqual{left,right}=>output.push((self.ty(module,context,left,env,active,b)?,self.ty(module,context,right,env,active,b)?)),
            SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{let mut env=env.clone();if *lifetimes!=0{let token=active.fresh(b)?;b.spend(*lifetimes as usize)?;env.bounds.insert(0,(0..*lifetimes).map(|index|Life::Bound(token,index)).collect());}for requirement in requirements{self.collect_equalities(module,context,requirement,&env,output,active,b)?;}},
            _=>{},
        }Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn block(&mut self,module:&TypedModule,context:&CheckedContext,block:&SemanticOriginalBlock,env:&Env,bindings:&mut BTreeMap<String,Binding>,result:&Ty,loops:usize,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let mut returns=false;
            for statement in &block.statements {
                b.spend(1)?;
                let completed=match &statement.kind {
                    CoreStatementKind::Binding{name,mutable,ty,initializer}=>{
                        let expected=match ty{Some(recipe)=>Some(self.ty(module,context,recipe,env,active,b)?),None=>None};
                        let actual=match initializer{Some(value)=>self.expression(module,context,value,env,bindings,expected.as_ref(),active,b)?,None=>match expected{Some(ref ty)=>ty.clone(),None=>return Err(bad("original uninitialized binding lacks its full type"))}};
                        if let Some(expected)=expected{self.unify(&actual,&expected,b)?;}
                        self.bindings.insert((env.current()?,statement.id),actual.clone());bindings.insert(name.clone(),Binding{ty:actual,mutable:*mutable});false
                    },
                    CoreStatementKind::TupleBinding{pattern,mutable,ty,initializer}=>{let expected=ty.as_ref().map(|recipe|self.ty(module,context,recipe,env,active,b)).transpose()?;let actual=self.expression(module,context,initializer,env,bindings,expected.as_ref(),active,b)?;self.pattern(module,context,pattern,&actual,*mutable,env,bindings,active,b)?;self.bindings.insert((env.current()?,statement.id),actual);false},
                    CoreStatementKind::Expression(expression)=>{self.expression(module,context,expression,env,bindings,None,active,b)?;false},
                    CoreStatementKind::Return(expression)=>{let actual=match expression{Some(expression)=>self.expression(module,context,expression,env,bindings,Some(result),active,b)?,None=>Ty::Atom(Type::Unit)};self.unify(&actual,result,b)?;true},
                    CoreStatementKind::Assign{target,value}=>{self.writable_original(target,bindings)?;let expected=self.expression(module,context,target,env,bindings,None,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.unify(&actual,&expected,b)?;false},
                    CoreStatementKind::CompoundAssign{target,op,value}=>{self.writable_original(target,bindings)?;let expected=self.expression(module,context,target,env,bindings,None,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.numeric_binary_original(*op,&expected,&actual,b)?;false},
                    CoreStatementKind::If{condition,then_body,else_body}=>{self.condition_original(module,context,condition,env,bindings,active,b)?;let then_returns=self.block(module,context,then_body,env,&mut bindings.clone(),result,loops,active,b)?;let else_returns=match else_body{Some(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,None=>false};then_returns&&else_returns},
                    CoreStatementKind::IfLet{pattern,value,then_body,else_body}=>{let actual=self.expression(module,context,value,env,bindings,None,active,b)?;let mut branch=bindings.clone();self.pattern(module,context,pattern,&actual,false,env,&mut branch,active,b)?;let yes=self.block(module,context,then_body,env,&mut branch,result,loops,active,b)?;let no=match else_body{Some(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,None=>false};yes&&no},
                    CoreStatementKind::While{condition,body}=>{self.condition_original(module,context,condition,env,bindings,active,b)?;self.block(module,context,body,env,&mut bindings.clone(),result,loops+1,active,b)?;false},
                    CoreStatementKind::WhileLet{pattern,value,body}=>{let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let mut inner=bindings.clone();self.pattern(module,context,pattern,&ty,false,env,&mut inner,active,b)?;self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;false},
                    CoreStatementKind::ForRange{name,start,end,body}=>{let ty=self.expression(module,context,start,env,bindings,None,active,b)?;let end=self.expression(module,context,end,env,bindings,Some(&ty),active,b)?;self.unify(&ty,&end,b)?;if !matches!(self.resolved(&ty,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original range endpoints are not integers"));}let mut inner=bindings.clone();inner.insert(name.clone(),Binding{ty,mutable:false});self.block(module,context,body,env,&mut inner,result,loops+1,active,b)?;false},
                    CoreStatementKind::Match{value,arms}=>{let ty=self.expression(module,context,value,env,bindings,None,active,b)?;let mut all=!arms.is_empty();for arm in arms{let mut inner=bindings.clone();self.pattern(module,context,&arm.pattern,&ty,false,env,&mut inner,active,b)?;all&=self.block(module,context,&arm.body,env,&mut inner,result,loops,active,b)?;}all},
                    CoreStatementKind::Block(body)|CoreStatementKind::Unsafe(body)=>self.block(module,context,body,env,&mut bindings.clone(),result,loops,active,b)?,
                    CoreStatementKind::Defer(body)=>{self.block(module,context,body,env,&mut bindings.clone(),&Ty::Atom(Type::Unit),0,active,b)?;false},
                    CoreStatementKind::Loop(body)=>{self.block(module,context,body,env,&mut bindings.clone(),result,loops+1,active,b)?;false},
                    CoreStatementKind::Break|CoreStatementKind::Continue if loops!=0=>false,
                    _=>return Err(unsupported("original statement has no complete symbolic operation rule",Some(statement.span))),
                };
                returns|=completed;
            }Ok(returns)
        })
    }
    pub(super) fn writable_original(&self,target:&SemanticOriginalExpr,bindings:&BTreeMap<String,Binding>)->Check<()>{match &target.kind{CoreExprKind::Name(name)if bindings.get(name).is_some_and(|row|row.mutable)=>Ok(()),CoreExprKind::Unary{op:UnaryOp::Dereference,..}|CoreExprKind::Member{..}|CoreExprKind::Index{..}=>Ok(()),_=>Err(bad("original assignment is not a mutable binding or checked place"))}}
    #[allow(clippy::too_many_arguments)]
    pub(super) fn condition_original(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {let ty=self.expression(module,context,expression,env,bindings,Some(&Ty::Atom(Type::Bool)),active,b)?;self.unify(&ty,&Ty::Atom(Type::Bool),b)}
    pub(super) fn numeric_binary_original(&mut self,op:BinaryOp,left:&Ty,right:&Ty,b:&mut Budget)->Check<Ty>{self.unify(left,right,b)?;let ty=self.resolved(left,b)?;let Ty::Atom(ref atom)=ty else{return Err(unsupported("original arithmetic needs a proved scalar operator contract",None));};let valid=match op{BinaryOp::And|BinaryOp::Or=>*atom==Type::Bool,BinaryOp::Equal|BinaryOp::NotEqual=>atom.is_integer()||atom.is_float()||*atom==Type::Bool,BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual=>atom.is_integer()||atom.is_float(),BinaryOp::Add|BinaryOp::Subtract|BinaryOp::Multiply|BinaryOp::Divide|BinaryOp::Remainder=>atom.is_integer()||atom.is_float(),_=>atom.is_integer()};if !valid{return Err(bad("original binary operator is invalid for its complete scalar carrier"));}Ok(if matches!(op,BinaryOp::And|BinaryOp::Or|BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual){Ty::Atom(Type::Bool)}else{ty})}
}
