//! A Typed entry may implement several original instantiation worlds. The
//! calling occurrence selects a complete source environment, never the first
//! entry with a matching machine ABI. Each retained world is replayed by the
//! independent function pass as well.
use super::*;

pub(super) struct CallWorld {
    environment: EvalEnv,
    boundary: trait_members::Boundary,
}

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_worlds(&self,module:&TypedModule,function:FunctionId,definition:SemanticDefinitionId,expected:&EvalEnv,caller:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<CallWorld>> {
        let program=module.semantic.as_ref().ok_or_else(||bad("actual Call lacks its original instance directory"))?;
        b.spend(program.instances.len())?;
        let mut matched=Vec::new();
        for instance in program.instances.iter().filter(|row|row.definition==definition&&matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function)) {
            // An actual lexical ancestor is an identity, not a same-typed
            // substitute. Witness frames name independently selected impl
            // scopes and are instead compared by their full actuals below.
            let mut parent=instance.parent;
            let mut compatible=true;
            for frame in expected.lexical.iter().skip(1) {
                b.spend(1)?;
                let Some(id)=parent else{compatible=false;break;};
                let row=self.instance(module,id,b)?;
                if row.definition!=frame.definition||matches!(frame.key,ScopeKey::Instance(expected)if expected!=id){compatible=false;break;}
                parent=row.parent;
            }
            if !compatible||parent.is_some(){continue;}
            let actual=self.environment_for_instance(module,instance.id,active,b)?;
            if actual.lexical.len()!=expected.lexical.len(){return Err(bad("actual Call world changes its full lexical ancestry"));}
            // Different concrete specializations can share one emitted body.
            // Filter by complete substituted shapes and constant values; class
            // identities are checked by a single joint boundary, not erased.
            let mut shape=true;
            for(a,z)in actual.lexical.iter().zip(&expected.lexical){
                b.spend(a.actuals.len().saturating_add(a.environment.len()).saturating_add(1))?;
                if a.definition!=z.definition||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){shape=false;break;}
                if !a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)).all(|(a,z)|same_actual_shape(a,z)){shape=false;break;}
                if !match(&a.self_type,&z.self_type){(Some(a),Some(z))=>same_value_shape(a,z),(None,None)=>true,_=>false}{shape=false;break;}
            }
            if !shape{continue;}
            let mut boundary=trait_members::Boundary::default();
            for(a,z)in actual.lexical.iter().zip(&expected.lexical){
                boundary.scope(a.key,z.key,b)?;
                for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){boundary.actual(self,caller,a,z,b)?;}
                if let(Some(a),Some(z))=(&a.self_type,&z.self_type){boundary.value(self,caller,a,z,b)?;}
            }
            b.spend(1)?;
            matched.push(CallWorld{environment:actual,boundary});
        }
        if matched.is_empty(){return Err(bad("actual Call has no complete source instance in its exact lexical world"));}
        Ok(matched)
    }

    // Construct the complete callee domain while original Life formals are
    // still rigid. Only then apply this occurrence's outside images. Rebuilding
    // P<A,B> after A and B both map to a would lose its original N2 boundary.
    pub(super) fn world_signature(&self,module:&TypedModule,recipe:&SemanticSignatureRecipe,worlds:Vec<CallWorld>,caller:&EvalEnv,external:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedSignature>{
        let mut selected:Option<CheckedSignature>=None;
        for mut world in worlds{
            let definition=self.definition(module,world.environment.lexical.first().ok_or_else(||bad("Call world has no current original definition"))?.definition,b)?;
            self.evaluate_requirements(module,&definition.requirements,&world.environment,RequirementMode::DeclarationPremise,external,active,b)?;
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{self.evaluate_requirements(module,requirements,&world.environment,RequirementMode::DeclarationPremise,external,active,b)?;}}
            let mut signature=self.evaluate_signature(module,recipe,&world.environment,external,active,b)?;
            for(parameter,_)in &mut signature.parameters{*parameter=world.boundary.project_type(self,caller,parameter,b)?;}
            signature.result=world.boundary.project_type(self,caller,&signature.result,b)?;
            if let Some(expected)=&selected{
                if signature.bound.len()!=expected.bound.len()||signature.invocation.len()!=expected.invocation.len()||signature.call_kind!=expected.call_kind||signature.result_mode!=expected.result_mode||signature.parameters.len()!=expected.parameters.len(){return Err(bad("compatible Call worlds disagree on their complete original signature"));}
                let mut alpha=trait_members::Boundary::default();
                for(a,z)in signature.bound.iter().zip(&expected.bound).chain(signature.invocation.iter().zip(&expected.invocation)){alpha.class(self,caller,a,z,b)?;}
                for(parameter,_)in &mut signature.parameters{*parameter=alpha.project_type(self,caller,parameter,b)?;}
                signature.result=alpha.project_type(self,caller,&signature.result,b)?;
                for((actual,mode),(expected,expected_mode))in signature.parameters.iter().zip(&expected.parameters){if mode!=expected_mode||!self.same_type(caller,actual,expected,b)?{return Err(unsupported("compatible Call worlds retain distinct original parameter domains",None));}}
                if !self.same_type(caller,&signature.result,&expected.result,b)?{return Err(unsupported("compatible Call worlds retain distinct original result domains",None));}
            }else{selected=Some(signature);}
        }
        selected.ok_or_else(||bad("Call signature has no independently selected original world"))
    }
}
fn same_value_shape(a:&CheckedType,z:&CheckedType)->bool {
    a.ty==z.ty&&a.life==z.life&&a.classes.len()==z.classes.len()&&a.origins.len()==z.origins.len()&&a.statics==z.statics
}
fn same_actual_shape(a:&CheckedActual,z:&CheckedActual)->bool {
    match(a,z){(CheckedActual::Type(a),CheckedActual::Type(z))=>same_value_shape(a,z),(CheckedActual::Life(_),CheckedActual::Life(_))=>true,(CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})=>av==zv&&same_value_shape(a,z),_=>false}
}
