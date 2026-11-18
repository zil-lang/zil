//! Environment-free code is formed from an exact original static definition.
//! A later indirect call uses the complete original code scheme, never a
//! first same-signature body or a target inferred from machine pointer types.
use super::*;
impl CheckedContext{
    pub(super) fn check_literal_entry(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,b:&mut Budget)->Check<()>{
        let source=self.original.borrow();
        let source=source.as_ref().ok_or_else(||bad("literal entry lacks its checked original body"))?;
        let site=source.thin_literal(instance.definition,b)?;
        let Some(recipe)=site else{
            if f.environment.is_none()||instance.environment.is_none(){return Err(bad("ordinary closure omits its checked creator environment"));}
            return Ok(());
        };
        if instance.environment.is_some()||f.environment.is_some(){return Err(bad("contextual fn literal acquired a physical or semantic closure environment"));}
        let parent=instance.parent.ok_or_else(||bad("contextual fn literal entry lost its creator"))?;
        let sites=self.site_index.recipe(module,parent,recipe,sites::Action::TypeUse,b)?;
        let [site]=sites.as_slice() else{return Err(bad("thin literal entry lacks its unique creator TypeUse"));};
        let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("thin literal creation lacks its actual address"));};
        let creator=self.instance(module,parent,b)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(owner))=creator.target else{return Err(bad("thin literal creator has no typed function"));};
        let previous=module.functions.get(owner.0 as usize).and_then(|f|f.blocks.get(block.0 as usize))
            .and_then(|block|instruction.checked_sub(1).and_then(|n|block.instructions.get(n as usize)));
        if !matches!(previous,Some(Instruction{kind:InstructionKind::FunctionRef{function},..}) if *function==f.id){
            return Err(bad("thin literal instance belongs to a different actual FunctionRef target"));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn function_pointer(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::FunctionRef{function}=instruction.kind else{return Err(bad("ordinary code formation selects another actual operation"));};
        crate::check_instruction(module,f,instruction,b)?;
        let result=instruction.result.ok_or_else(||bad("ordinary code reference omits its actual SSA result"))?;
        let program=module.semantic.as_ref().ok_or_else(||bad("ordinary code reference has no original source programme"))?;
        let caller_definition=self.definition(module,caller.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,sites:recipes,..}=&caller_definition.body else{return Err(bad("ordinary code reference creator has no complete original body"));};
        let point=environment.at.ok_or_else(||bad("ordinary code reference has no actual CFG point"))?;
        let next=point.instruction.checked_add(1).ok_or_else(||bad("code reference end exceeds its block"))?;
        let sites=self.actual_sites(module,caller.id,point.block,next,b)?;
        let mut source=None;
        let mut claims=Vec::new();
        let mut thin_site=false;
        for site in sites {
            let SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),expected}=&site.action else{continue;};
            if *value!=result || !matches!(site.at,SemanticSiteAddress::Typed{edge:SemanticSiteEdge::Before,..}) {continue;}
            b.spend(site_nodes.len())?;
            let link=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("code reference use has no original node link"))?;
            let SemanticOriginalNode::Expression(node)=link.node else{continue;};
            let original=self.original.borrow();
            let original=original.as_ref().ok_or_else(||bad("code reference has no universal original proof"))?;
            let Some(target)=original.code_source(caller.definition,node,b)? else{continue;};
            if self.site_index.recipe(module,caller.id,site.recipe,sites::Action::TypeUse,b)?.len()!=1{
                return Err(bad("one code formation recipe claims multiple actual occurrences"));
            }
            b.spend(recipes.len())?;
            let recipe=recipes.iter().find(|recipe|recipe.id==site.recipe).ok_or_else(||bad("code formation has no original recipe"))?;
            if let SemanticSiteRecipeKind::TypeUse{expected:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(expected_node)}}=&recipe.kind{
                if *expected_node!=node{return Err(bad("named code TypeUse selects a different original expression"));}
            }
            if let SemanticSiteRecipeKind::Closure{definition,environment}= &recipe.kind {
                if thin_site || *definition!=target || !environment.is_empty(){return Err(bad("thin literal selects another original child or repeats its creation"));}
                thin_site=true;
            }
            if source.replace(target).is_some_and(|old|old!=target){return Err(bad("one actual code reference substitutes several original targets"));}
            claims.push((site,expected));
        }
        let source=source.ok_or_else(||bad("actual code reference omits its exact original formation occurrence"))?;
        let definition=self.definition(module,source,b)?;
        let expected_owner=if thin_site{SemanticDefinitionKind::Closure}else{SemanticDefinitionKind::Function};
        let parent=thin_site.then_some(caller.definition);
        if definition.kind!=expected_owner||definition.parent!=parent||!definition.parameters.is_empty()||!definition.environment.is_empty(){return Err(bad("ordinary code reference changes its original owner or its environment"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,foreign:None,receiver_parameter:None,body:SemanticOriginalFunctionBody::Defined(_),..}=&definition.body else{return Err(bad("ordinary code reference changes its original body or ABI role"));};
        b.spend(program.instances.len())?;
        if program.instances.iter().any(|row|row.target==SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) && (row.definition!=source || row.environment.is_some())){
            return Err(bad("function address selects an entry with conflicting original target or environment"));
        }
        let key=ScopeKey::Witness{definition:source,frame:active.fresh(b)?};
        let mut lexical=vec![LexicalFrame{definition:source,key,actuals:vec![],environment:vec![],self_type:None}];
        if thin_site{b.spend(environment.lexical.len())?;lexical.extend_from_slice(&environment.lexical);}
        let selected=EvalEnv{lexical,assumptions:environment.assumptions.clone(),images:environment.images.clone(),bounds:vec![],invocations:vec![],at:environment.at};
        let worlds=self.call_worlds(module,function,source,&selected,environment,active,b)?;
        self.evaluate_requirements(module,&definition.requirements,&selected,RequirementMode::ActualObligation,external,active,b)?;
        let signature=self.world_signature(module,signature,worlds,environment,external,active,b)?;
        let value=self.ordinary_code(module,SemanticCodeKind::FunctionPointer,&signature,b)?;
        if let Type::BorrowedFunctionPointer{life,..}=&value.ty {
            let entry=module.functions.get(function.0 as usize).ok_or_else(||bad("borrowed code has no actual entry"))?;
            if entry.storage_life.as_ref()!=Some(life){
                return Err(bad("borrowed FunctionRef target changes its complete invocation relation"));
            }
        }
        for(site,claim)in claims{
            let required=self.site_expected(module,caller,site,environment,external,active,b)?;
            if !self.same_type(environment,&value,&required,b)?{return Err(bad("code formation changes its complete original context type"));}
            self.claim_type_use(module,claim,&required,environment,b)?;
        }
        if f.values.get(result.0 as usize)!=Some(&value.ty){return Err(bad("actual code reference changes the complete original code type"));}Ok(value)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn indirect_call(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,callee:&CheckedType,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let recipe=self.original.borrow().as_ref().ok_or_else(||bad("indirect code call has no universal original body proof"))?.indirect_signature_recipe(module,caller.definition,site.recipe,environment,b)?;
        let mut signature=self.evaluate_signature(module,&recipe,environment,external,active,b)?;
        let expected=self.ordinary_code(module,SemanticCodeKind::FunctionPointer,&signature,b)?;
        if !self.same_type(environment,callee,&expected,b)?{return Err(bad("indirect code call changes its complete original code scheme"));}
        let key=ScopeKey::Witness{definition:caller.definition,frame:active.fresh(b)?};
        let(source_inputs,source_checks)=self.original_call_inputs(module,caller,site,&signature,key,arguments,values,environment,external,active,b)?;
        let inputs=source_inputs.iter().collect::<Vec<_>>();
        self.instantiate_original_call_heads(module,caller,site,&mut signature,&inputs,key,environment,b)?;
        for(index,((expected,_),argument))in signature.parameters.iter().zip(arguments).enumerate(){if !self.call_argument(module,caller,*argument,values,expected,environment,source_checks[index],b)?{return Err(bad("indirect code argument changes its complete original type/class/source role"));}}
        self.check_call_input_wf(module,&signature,arguments,values,environment,b)?;
        Ok((signature.result.ty!=Type::Unit).then_some(signature.result))
    }
}
