//! A Typed entry may implement several original instantiation worlds. The
//! calling occurrence selects a complete source environment, never the first
//! entry with a matching machine ABI. Each retained world is replayed by the
//! independent function pass as well.
use super::*;
mod rigid;
pub(in crate::semantic) use rigid::RigidWorlds;

pub(super) struct CallWorld {
    pub(super) instance:SemanticInstanceId,
    environment: EvalEnv,
    boundary: trait_members::Boundary,
}

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_worlds(&self,module:&TypedModule,function:FunctionId,definition:SemanticDefinitionId,expected:&EvalEnv,caller:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<CallWorld>> {
        let program=module.semantic.as_ref().ok_or_else(||bad("actual Call lacks its original instance directory"))?;
        let candidates = self.world_candidates(module, program, function, definition, b)?;
        let mut matched=Vec::new();
        for index in candidates {
            let instance = &program.instances[index];
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
            matched.push(CallWorld{instance:instance.id,environment:actual,boundary});
        }
        if matched.is_empty(){return Err(bad("actual Call has no complete source instance in its exact lexical world"));}
        Ok(matched)
    }

    // Construct the complete callee domain while original Life formals are
    // still rigid. Only then apply this occurrence's outside images. Rebuilding
    // P<A,B> after A and B both map to a would lose its original N2 boundary.
    pub(super) fn world_signature(&self,module:&TypedModule,recipe:&SemanticSignatureRecipe,worlds:Vec<CallWorld>,caller:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedSignature>{
        let multiple = worlds.len() > 1;
        let mut selected:Option<CheckedSignature>=None;
        let mut projected=None;
        for world in worlds{
            let definition=self.definition(module,world.environment.lexical.first().ok_or_else(||bad("Call world has no current original definition"))?.definition,b)?;
            if !multiple || !self.world_declarations_present(module,definition,recipe,&world,active,b)? {
                self.evaluate_requirements(module,&definition.requirements,&world.environment,RequirementMode::DeclarationPremise,external,active,b)?;
                for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{self.evaluate_requirements(module,requirements,&world.environment,RequirementMode::DeclarationPremise,external,active,b)?;}}
            }
            let (mut signature, proof)=if multiple {
                match self.rigid_entry_signature(module,definition,recipe,&world,external,active,projected.as_ref(),b)? {
                    rigid::EntryResult::Checked(sig, proof) => (sig, proof),
                    rigid::EntryResult::SameProjection => continue,
                }
            } else {
                (self.entry_signature(module,definition,recipe,&world.environment,external,active,b)?, None)
            };
            world.boundary.project_entry(self,caller,&mut signature,b)?;
            if let Some(expected)=&selected{
                if signature.bound.len()!=expected.bound.len()||signature.invocation.len()!=expected.invocation.len()||signature.call_kind!=expected.call_kind||signature.result_mode!=expected.result_mode||signature.parameters.len()!=expected.parameters.len(){return Err(bad("compatible Call worlds disagree on their complete original signature"));}
                let mut alpha=trait_members::Boundary::default();
                for(a,z)in signature.bound.iter().zip(&expected.bound).chain(signature.invocation.iter().zip(&expected.invocation)){alpha.class(self,caller,a,z,b)?;}
                alpha.project_entry(self,caller,&mut signature,b)?;
                for((actual,mode),(expected,expected_mode))in signature.parameters.iter().zip(&expected.parameters){if mode!=expected_mode||!self.same_type(caller,actual,expected,b)?{return Err(unsupported("compatible Call worlds retain distinct original parameter domains",None));}}
                if !self.same_type(caller,&signature.result,&expected.result,b)?{return Err(unsupported("compatible Call worlds retain distinct original result domains",None));}
            }else{selected=Some(signature); projected=proof;}
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

impl CheckedContext {
    /// Called only after every retained actual world/body is complete. The
    /// source correspondence never becomes a permission before this seal.
    pub(in crate::semantic) fn complete_nominal_result(&self,module:&TypedModule,function:&TypedFunction,source:&dynamic_witness::semantic_external::OriginalMemberReceipt,worlds:&[(SemanticInstanceId,EvalEnv)],selected:&[SemanticInstanceId],b:&mut Budget)->Check<()> {
        let Some((expected_sources,_))=self.nominal_result_sources(&source.environment,&source.actual_signature,b)?else{return Ok(());};
        let Some(returns)=self.nominal_result_constructors(module,function,b)?else{return Ok(());};
        for id in selected{
            b.spend(worlds.len())?;
            let (_,environment)=worlds.iter().find(|(world,_)|world==id).ok_or_else(||bad("nominal result completion loses its exact checked world environment"))?;
            let world=self.instance(module,*id,b)?;
            if world.target!=SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function.id))||world.definition!=source.body{return Err(bad("nominal result completion selects another actual body world"));}
            if environment.lexical.len()!=source.environment.lexical.len(){return Err(bad("nominal result completion changes its original lexical ancestry"));}
            let mut boundary=trait_members::Boundary::default();
            for(a,z)in environment.lexical.iter().zip(&source.environment.lexical){
                if a.definition!=z.definition||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Err(bad("nominal result completion changes its complete source frame"));}
                boundary.scope(a.key,z.key,b)?;
                for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){boundary.actual(self,&source.environment,a,z,b)?;}
                match(&a.self_type,&z.self_type){(Some(a),Some(z))=>boundary.value(self,&source.environment,a,z,b)?,(None,None)=>{},_=>return Err(bad("nominal result completion changes complete Self"))}
            }
            let mut values=returns.iter().map(|key|key.result).collect::<std::collections::BTreeSet<_>>();
            for block in &function.blocks{b.spend(1)?;if let Terminator::Return(Some(value))=block.terminator{values.insert(value);}}
            for value in values{
                let occurrences=self.occurrences.borrow();
                let actual=occurrences.get(&(ScopeKey::Instance(*id),OperandKey::Value(value))).ok_or_else(||bad("nominal result constructor has no completely checked actual SSA receipt"))?;
                let actual=boundary.project_type(self,&source.environment,actual,b)?;
                if !self.same_type(&source.environment,&actual,&source.actual_signature.result,b)?{return Err(bad("nominal result constructor changes the required complete intrinsic result"));}
                let actual_signature=CheckedSignature{result:actual,..source.actual_signature.clone()};
                let Some((actual_sources,_))=self.nominal_result_sources(&source.environment,&actual_signature,b)?else{return Err(bad("nominal result constructor loses its original outside source directory"));};
                if !self.same_type(&source.environment,&actual_sources,&expected_sources,b)?{return Err(bad("nominal result constructor changes an independently required outside source"));}
            }
        }Ok(())
    }
}
