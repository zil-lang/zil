//! A compiler-created argument reborrow adapts this outer parameter role.
//! It does not equate two SSA heads, retag a pointee class, or change the
//! caller's original lifetime actual/result image.
use super::*;
impl CheckedContext {
    pub(super) fn foreign_projected_arguments(
        &self, module: &TypedModule, caller: &SemanticInstance, f: &TypedFunction,
        site: &SemanticSite, arguments: &[ValueId], b: &mut Budget,
    ) -> Check<()> {
        b.spend(arguments.len())?;
        let is_handle = |value: ValueId| f.values.get(value.0 as usize) == Some(&Type::ForeignHandle);
        if !arguments.iter().copied().any(is_handle) { return Ok(()); }
        let recipes = self.original.borrow().as_ref()
            .ok_or_else(|| bad("foreign argument lacks its original Call occurrence"))?
            .call_input_recipes(module, caller.definition, site.recipe, b)?;
        if arguments.len() != recipes.len() {
            return Err(bad("foreign call changes its exact original argument arity"));
        }
        for (actual, recipe) in arguments.iter().copied().zip(recipes) {
            let Some((node, _, Some(source @ original::intrinsics::BufferSource::Place { .. }))) = recipe else { continue; };
            if !is_handle(actual) { continue; }
            let mut operand = actual;
            loop {
                b.spend(1)?;
                let (block, at, producer) = self.site_index.producers.get(f, operand, b)?;
                let InstructionKind::Load(local) = producer.kind else { break; };
                let Some(source) = intrinsics::owner_sources::scalar_preparations::copy_value_source(module, f, block, at, local, b)? else { break; };
                operand = source;
            }
            self.call_input_source(module, caller, f, operand, node, Some(source), b)?;
        }
        Ok(())
    }

    pub(super) fn inferred_call_type(&self,module:&TypedModule,sources:&[(SemanticTypeRecipe,bool)],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let mut selected=None;
        for(source,shared_view)in sources{let value=self.evaluate_type(module,source,environment,external,active,b)?;let value=if *shared_view{self.call_shared_view(&value,b)?}else{value};if let Some(expected)=&selected{if !self.same_type(environment,expected,&value,b)?{return Err(bad("repeated inferred Type arguments have different whole intrinsic domains"));}}else{selected=Some(value);}}
        Ok(selected)
    }
    /// Only exported fresh borrow heads use the checked original operand
    /// image. An extra compiler reborrow is an input role, not the lifetime of
    /// an opaque/nominal result that retains the original caller occurrence.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_call_inputs(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,signature:&CheckedSignature,key:ScopeKey,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<(Vec<CheckedType>,Vec<bool>)>{
        if signature.parameters.len()!=arguments.len(){return Err(bad("actual Call inputs change the complete callee signature arity"));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("original Call inputs have no actual caller function"));};
        let f=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("original Call input actual caller is absent"))?;
        b.spend(arguments.len())?;
        let mut checked=Vec::with_capacity(arguments.len());b.spend(arguments.len())?;let mut exact_sources=vec![false;arguments.len()];
        let (original_regions, retains_task_inputs) = {
            let original = self.original.borrow();
            let original = original.as_ref().ok_or_else(||bad("Call inputs have no original region proof"))?;
            (original.call_region_parameter_recipes(module,caller.definition,site.recipe,environment,b)?,
             original.retains_task_inputs(caller.definition,site.recipe,b)?)
        };
        let mut adapted=Vec::new();
        let mut code_arguments=Vec::new();
        let mut descriptions=Vec::new();
        for(index,argument)in arguments.iter().enumerate(){
            let actual=value_at(values,*argument)?;
            actual.charge_clone(b)?;checked.push(actual.clone());
            // Ordinary ownership, manager/Desc category adaptation and whole
            // Type arguments keep their already reconstructed actual domain.
            // The later invariant callee P/R check is their authority. An
            // exported fresh head or a real exclusive-to-shared adapter also
            // proves its original source instead of borrowing another SSA's
            // same-shaped type. Neither path changes a child intrinsic class.
            if matches!(actual.ty,Type::FunctionPointer{..}) {
                b.spend(1)?;
                code_arguments.push(index);
            }
            if matches!(actual.ty,Type::Desc(_)) && matches!(signature.parameters[index].0.ty,Type::Desc(_)){
                b.spend(1)?;
                descriptions.push(index);
            }
            if !matches!(actual.ty,Type::Address{..}){continue;}
            b.spend(original_regions.len())?;if original_regions.iter().any(|(ordinal,_)|*ordinal as usize==index){b.spend(1)?;adapted.push(index);continue;}
            let exported=returned_input_head(signature,key,index,b)?;
            let local_window=self.original_local_call_window(caller,&signature.parameters[index].0,environment,b)?;
            let shared=matches!(actual.ty,Type::Address{mutable:false,..})&&matches!(signature.parameters[index].0.ty,Type::Address{mutable:false,..});
            if !retains_task_inputs&&!exported&&!local_window&&!(shared&&shared_reborrow_adapter(&self.site_index.producers,module,f,*argument,b)?){continue;}
            let (_, _, producer) = self.site_index.producers.get(f, *argument, b)?;
            crate::check_instruction(module,f,producer,b)?;
            if matches!(producer.kind,InstructionKind::ReborrowAddress{..}){b.spend(1)?;adapted.push(index);}
        }
        if adapted.is_empty()&&code_arguments.is_empty()&&descriptions.is_empty(){return Ok((checked,exact_sources));}
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("Call inputs have no universal original body proof"))?.call_input_recipes(module,caller.definition,site.recipe,b)?;
        if recipes.len()!=arguments.len(){return Err(bad("actual Call inputs change the exact original argument arity"));}
        for index in code_arguments {
            let Some((node,_,origin))=&recipes[index] else {
                return Err(bad("plain code argument has no exact original operand occurrence"));
            };
            self.call_input_source(module,caller,f,arguments[index],*node,*origin,b)
                .map_err(|error|error.with_note(format!("actual code argument {index} in original call {:?}, SSA {:?}",site.recipe,arguments[index])))?;
        }
        for index in descriptions{
            let Some((node,_,origin@Some(original::intrinsics::BufferSource::TemporaryOwner{..})))=&recipes[index] else{continue;};
            let argument=arguments[index];
            self.call_input_source(module,caller,f,argument,*node,*origin,b).map_err(|mut error|{
                error.relate(zil_base::DiagnosticRole::Input,None,environment.at.map(|point| zil_base::DiagnosticIrLocation{
                    function:point.function.0,block:Some(point.block.0),instruction:Some(point.instruction),
                }),format!("actual temporary description argument {index}, SSA {argument:?}, original call {:?}",site.recipe));
                error
            })?;
            let role=self.original.borrow().as_ref().ok_or_else(||bad("temporary description lacks its original call scope"))?
                .temporary_call_description(module,caller.definition,site.recipe,index,*node,environment,b)?;
            let class=self.life(module,&role,environment,b)?;
            let actual=&checked[index];
            b.spend(actual.life.slots.len().saturating_add(actual.life.retained.len()).saturating_add(actual.life.nominal_arguments.len()))?;
            let mut roots=actual.life.slots.iter().filter(|row|row.path.is_empty());
            let head=roots.next().filter(|row|row.mode==StoredBorrowMode::WritableDescription).ok_or_else(||bad("temporary description has no exact outer descriptor occurrence"))?;
            if roots.next().is_some()
                || actual.life.slots.iter().any(|row|!row.path.is_empty()&&row.binder==head.binder)
                || actual.life.retained.iter().any(|row|row.binder==head.binder)
                || actual.life.nominal_arguments.iter().any(|row|row.binder==head.binder){
                return Err(bad("temporary description outer role overlaps its underlying resource domain"));
            }
            let head=head.binder as usize;
            actual.charge_clone(b)?;
            let mut transported=actual.clone();
            *transported.classes.get_mut(head).ok_or_else(||bad("temporary description outer class is absent"))?=class.clone();
            *transported.origins.get_mut(head).ok_or_else(||bad("temporary description outer origin is absent"))?=IntrinsicOrigin::DerivedClass{source:class};
            checked[index]=transported;
            exact_sources[index]=true;
        }
        for index in adapted{
            let Some((node,recipe,origin))=&recipes[index]else{continue;};let argument=arguments[index];
            let source=self.evaluate_type(module,recipe,environment,external,active,b)?;
            let source=if matches!(source.ty,Type::Address{mutable:true,..})&&matches!(signature.parameters[index].0.ty,Type::Address{mutable:false,..}){
                if !shared_reborrow_adapter(&self.site_index.producers,module,f,argument,b)?{return Err(bad("Call shared view lacks its exact exclusive-to-shared ReborrowAddress adapter"));}
                self.call_shared_view(&source,b)?
            }else{source};
            self.call_input_source(module,caller,f,argument,*node,*origin,b)?;
            if !self.call_argument(module,caller,argument,values,&source,environment,true,b)?{
                return Err(bad("actual Call input changes its original whole operand or exact argument reborrow role").with_note(format!("original {:?}, call {:?}, operand {index} expression {:?}, actual SSA {:?}, point {:?}; source domain {}, actual domain {}",caller.definition,site.recipe,node,argument,environment.at,source.life.binder_count,checked[index].life.binder_count)));
            }
            checked[index]=source;exact_sources[index]=true;
        }
        Ok((checked,exact_sources))
    }
    /// A concrete callee lifetime can already be fixed by an invariant raw
    /// pointee to this caller's shared-local window. Its other root argument
    /// still needs the real original operand -> compiler reborrow proof.
    pub(super) fn original_local_call_window(&self,caller:&SemanticInstance,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<bool>{
        if !matches!(expected.ty,Type::Address{..}){return Ok(false);}
        b.spend(expected.life.slots.len())?;let mut slots=expected.life.slots.iter().filter(|row|row.path.is_empty());
        let Some(slot)=slots.next()else{return Ok(false);};if slots.next().is_some(){return Err(bad("Call local window has duplicate root occurrences"));}
        let Some(ClassKey::Implicit{scope,ordinal})=expected.classes.get(slot.binder as usize)else{return Ok(false);};
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        if *scope!=frame.key||frame.definition!=caller.definition{return Ok(false);}
        self.original.borrow().as_ref().ok_or_else(||bad("Call local window lacks its complete original body"))?.has_local_role(caller.definition,*ordinal,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_original_call_heads(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,signature:&mut CheckedSignature,arguments:&[&CheckedType],key:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let regions=self.original.borrow().as_ref().ok_or_else(||bad("Call private heads lack their complete original region proof"))?.call_region_parameter_recipes(module,caller.definition,site.recipe,environment,b)?;
        if regions.is_empty(){return self.instantiate_call_heads(signature,arguments,key,environment,b);}
        let mut boundary=trait_members::Boundary::default();
        for(ordinal,recipe)in regions{
            let target=self.life(module,&recipe,environment,b)?;
            let parameter=&signature.parameters.get(ordinal as usize).ok_or_else(||bad("Call region selects an absent full parameter ordinal"))?.0;
            if !matches!(parameter.ty,Type::Address{..}|Type::Desc(_)){return Err(bad("Call region changed its exact original root borrow category"));}
            let mut slots=parameter.life.slots.iter().filter(|slot|slot.path.is_empty());
            let slot=slots.next().ok_or_else(||bad("Call region parameter lacks its complete variable outer occurrence"))?;
            if slots.next().is_some(){return Err(bad("Call region parameter repeats its root occurrence"));}
            b.spend(parameter.life.slots.len().saturating_add(parameter.life.retained.len()).saturating_add(parameter.life.nominal_arguments.len()))?;
            if parameter.life.slots.iter().any(|row|!row.path.is_empty()&&row.binder==slot.binder)||parameter.life.retained.iter().any(|row|row.binder==slot.binder)||parameter.life.nominal_arguments.iter().any(|row|row.binder==slot.binder){return Err(bad("Call private outer role overlaps an existing child intrinsic domain"));}
            let source=parameter.classes.get(slot.binder as usize).ok_or_else(||bad("Call region root class is absent"))?;
            let private=signature.invocation.contains(source)||signature.bound.contains(source)||matches!(source,ClassKey::Implicit{scope,..}if *scope==key)||matches!(source,ClassKey::Outer{scope,operand:OperandKey::Parameter(index),path,mode}if *scope==key&&*index==ordinal&&path.is_empty()&&*mode==slot.mode);
            if !private&&!self.same(environment,source,&target,b)?{return Err(bad("Call region cannot replace an existing rigid root identity"));}
            boundary.class(self,environment,source,&target,b)?;
        }
        for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(self,environment,parameter,b)?;}
        signature.result=boundary.project_type(self,environment,&signature.result,b)?;
        self.instantiate_call_heads(signature,arguments,key,environment,b)
    }
    pub(super) fn instantiate_call_heads(&self,signature:&mut CheckedSignature,arguments:&[&CheckedType],key:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<()>{
        if arguments.len()!=signature.parameters.len(){return Err(bad("Call head substitution changes its actual arity"));}
        let mut invocation=Vec::<(ClassKey,Vec<ClassKey>)>::new();
        let mut static_heads=Vec::<ClassKey>::new();
        for(index,((expected,_),actual))in signature.parameters.iter().zip(arguments).enumerate(){
            for slot in &expected.life.slots{
                b.spend(1)?;if !slot.path.is_empty(){continue;}
                let source=expected.classes.get(slot.binder as usize).ok_or_else(||bad("Call parameter head class is absent"))?;
                let fresh=signature.invocation.contains(source)||signature.bound.contains(source)||matches!(source,ClassKey::Implicit{scope,..}if *scope==key)
                    ||matches!(source,ClassKey::Outer{scope,operand:OperandKey::Parameter(ordinal),path,mode}if *scope==key&&*ordinal==index as u32&&path.is_empty()&&*mode==slot.mode);
                if !fresh{continue;}
                let mut slots=actual.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==slot.mode);
                let Some(actual_slot)=slots.next()else{
                    let mut statics=actual.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&actual.ty));
                    if slot.mode!=StoredBorrowMode::SharedAddress||actual.ty!=expected.ty||statics.next().is_none()||statics.next().is_some(){return Err(unsupported("Call fresh input head needs its complete actual static/borrow role",None));}
                    if !static_heads.contains(source){static_heads.push(source.clone());}continue;
                };
                if slots.next().is_some(){return Err(bad("Call argument has duplicate head occurrences"));}
                let image=actual.classes.get(actual_slot.binder as usize).ok_or_else(||bad("Call actual head class is absent"))?;
                b.spend(invocation.len())?;
                if let Some((_,images))=invocation.iter_mut().find(|(key,_)|key==source){b.spend(images.len().saturating_add(1))?;if !images.contains(image){images.push(image.clone());}}
                else{invocation.push((source.clone(),vec![image.clone()]));}
            }
        }
        let mut boundary=trait_members::Boundary::default();let mut dependencies=Vec::new();
        for(source,images)in &invocation{
            let image=images.first().ok_or_else(||bad("Call outer role has no actual input image"))?;
            let mut equal=!static_heads.contains(source);for other in images{if !self.same(environment,image,other,b)?{equal=false;}}
            if equal{boundary.class(self,environment,source,image,b)?;}
            else{for image in images{b.spend(1)?;dependencies.push((image.clone(),source.clone()));}}
        }
        if !dependencies.is_empty(){let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;let mut premises=self.premises.borrow_mut();let rows=premises.entry(scope).or_default();let mut changed=false;for edge in dependencies{b.spend(rows.relations.outlives.len().saturating_add(1))?;if !rows.relations.outlives.contains(&edge){rows.relations.outlives.push(edge);changed=true;}}drop(premises);if changed{self.invalidate_closed_frames();}}

        for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(self,environment,parameter,b)?;}
        signature.result=boundary.project_type(self,environment,&signature.result,b)?;
        for source in static_heads{
            if invocation.iter().any(|(key,_)|key==&source){continue;}
            for(parameter,_)in &mut signature.parameters{static_call_head(parameter,&source,b)?;}
            static_call_head(&mut signature.result,&source,b)?;
            signature.invocation.retain(|key|key!=&source);signature.bound.retain(|key|key!=&source);
        }Ok(())
    }
    pub(super) fn call_shared_view(&self,source:&CheckedType,b:&mut Budget)->Check<CheckedType>{
        let Type::Address{mutable:true,inner}=&source.ty else{return Err(bad("Call shared source view is not an exclusive address"));};
        source.charge_clone(b)?;dynamic_metadata::ty(inner,b)?;let mut value=source.clone();value.replace_type(Type::Address{mutable:false,inner:inner.clone()});
        let mut outer=value.life.slots.iter().enumerate().filter(|(_,slot)|slot.path.is_empty());
        let Some((index,slot))=outer.next()else{return Err(bad("Call exclusive source lacks its actual outer occurrence"));};
        if outer.next().is_some()||slot.mode!=StoredBorrowMode::ExclusiveAddress{return Err(bad("Call exclusive source has another complete outer mode"));}
        let binder=slot.binder;
        b.spend(value.life.slots.len().saturating_add(value.life.retained.len()).saturating_add(value.life.nominal_arguments.len()))?;
        if value.life.slots.iter().any(|slot|!slot.path.is_empty()&&slot.binder==binder)||value.life.retained.iter().any(|row|row.binder==binder)||value.life.nominal_arguments.iter().any(|row|row.binder==binder){return Err(bad("Call shared outer view overlaps a whole child intrinsic class"));}
        value.life.slots[index].mode=StoredBorrowMode::SharedAddress;
        Ok(value)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_argument(&self,module:&TypedModule,caller:&SemanticInstance,argument:ValueId,values:&[Option<CheckedType>],expected:&CheckedType,environment:&EvalEnv,original_source_checked:bool,b:&mut Budget)->Check<bool>{
        let actual=value_at(values,argument)?;
        if self.compatible(environment,actual,expected,b)?{return Ok(true);}
        if actual.ty!=expected.ty||!matches!(actual.ty,Type::Address{..}){return Ok(false);}
        let mut rows=expected.life.slots.iter().filter(|row|row.path.is_empty());
        let Some(outer)=rows.next()else{return Ok(false);};if rows.next().is_some(){return Err(bad("Call parameter has duplicate outer borrow occurrences"));}
        // Independent child/phantom classes must retain their own origin.
        // A pre-existing collapsed representation cannot be repaired here.
        if expected.life.retained.iter().any(|row|row.binder==outer.binder)||expected.life.nominal_arguments.iter().any(|row|row.binder==outer.binder)||expected.life.slots.iter().any(|row|!row.path.is_empty()&&row.binder==outer.binder){return Err(unsupported("Call outer role shares an unresolved whole-child intrinsic class",None));}
        let Some(actual_outer)=actual.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{return Ok(false);};
        let source=expected.classes.get(outer.binder as usize).ok_or_else(||bad("Call original outer class is absent"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("Call borrow role has no actual caller"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("Call borrow caller is absent"))?;
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let mut current=argument;let mut seen=std::collections::BTreeSet::new();let mut related=false;
        loop{
            b.spend(1)?;if !seen.insert(current){return Err(bad("Call argument reborrow has an SSA producer cycle"));}
            let (_, _, producer) = self.site_index.producers.get(f, current, b)?;crate::check_instruction(module,f,producer,b)?;
            let InstructionKind::ReborrowAddress{address,mutable}=&producer.kind else{break;};
            current=*address;
            let parent=value_at(values,current)?;
            let shared_parent=outer.mode==StoredBorrowMode::SharedAddress&&!*mutable&&matches!(parent.ty,Type::Address{mutable:true,..});
            let view=if shared_parent{Some(self.call_shared_view(parent,b)?)}else{None};let parent=view.as_ref().unwrap_or(parent);
            let Some(parent_outer)=parent.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{break;};
            let key=parent.classes.get(parent_outer.binder as usize).ok_or_else(||bad("Call reborrow parent class is absent"))?;
            let related_source=self.same(environment,source,key,b)?||original_source_checked&&self.outlives(environment,key,source,b)?;
            if related_source{
                // The exact original expression head must be a real earlier
                // operand in this same caller, never another class image.
                let exact_occurrence=matches!(key,ClassKey::Outer{scope:owner,operand:OperandKey::Value(id),path,mode}if *owner==scope&&*id==current&&path.is_empty()&&(*mode==outer.mode||shared_parent&&*mode==StoredBorrowMode::ExclusiveAddress));
                let parameter_role=match key{
                    ClassKey::Formal{..}=>{
                        b.spend(environment.lexical.iter().fold(1usize,|n,frame|n.saturating_add(frame.actuals.len())))?;
                        environment.lexical.iter().any(|frame|frame.actuals.iter().any(|actual|matches!(actual,CheckedActual::Life(life)if life==key)))&&loaded_parameter(f,current,b)?
                    },
                    ClassKey::Implicit{scope:owner,..}if *owner==scope=>loaded_parameter(f,current,b)?,
                    _=>false,
                };
                if !exact_occurrence&&!parameter_role&&!original_source_checked{return Ok(false);}
                if !self.compatible(environment,parent,expected,b)?{return Ok(false);}
                related=true;break;
            }
        }
        if !related{return Ok(false);}
        expected.charge_clone(b)?;
        let mut role=expected.clone();
        role.classes[outer.binder as usize]=actual.classes.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head is absent"))?;
        role.origins[outer.binder as usize]=actual.origins.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head origin is absent"))?;
        self.same_type(environment,actual,&role,b)
    }
}

// Select an existing, complete signature occurrence. This is not a shape or
// same-sigma test: no class is substituted unless this exact fresh parameter
// head is also one of R's original external images.
fn returned_input_head(signature:&CheckedSignature,key:ScopeKey,index:usize,b:&mut Budget)->Check<bool>{
    let expected=&signature.parameters.get(index).ok_or_else(||bad("Call input head selects an absent parameter"))?.0;
    if !matches!(expected.ty,Type::Address{..}){return Ok(false);}
    b.spend(expected.life.slots.len())?;
    let mut outer=expected.life.slots.iter().filter(|row|row.path.is_empty());
    let Some(slot)=outer.next()else{return Ok(false);};
    if outer.next().is_some(){return Err(bad("Call input has several original outer head occurrences"));}
    let source=expected.classes.get(slot.binder as usize).ok_or_else(||bad("Call parameter head class is absent"))?;
    b.spend(signature.invocation.len().saturating_add(signature.bound.len()).saturating_add(signature.result.classes.len()))?;
    let fresh=signature.invocation.contains(source)||signature.bound.contains(source)
        ||matches!(source,ClassKey::Implicit{scope,..}if *scope==key)
        ||matches!(source,ClassKey::Outer{scope,operand:OperandKey::Parameter(ordinal),path,mode}if *scope==key&&*ordinal==index as u32&&path.is_empty()&&*mode==slot.mode);
    Ok(fresh&&signature.result.classes.contains(source))
}

fn loaded_parameter(f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<bool>{
    let mut selected=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for operation in &block.instructions{if operation.result==Some(value)&&selected.replace(operation).is_some(){return Err(bad("Call parameter head has several actual SSA definitions"));}}}
    b.spend(f.parameters.len())?;
    Ok(matches!(selected.map(|row|&row.kind),Some(InstructionKind::Load(local))if f.parameters.contains(local)))
}

// Specialize only a fresh outer invocation occurrence. An intrinsic child
// remains a variable class even when it happens to have the same outside image.
fn static_call_head(value:&mut CheckedType,source:&ClassKey,b:&mut Budget)->Check<()>{
    b.spend(value.classes.len().saturating_add(value.life.slots.len()).saturating_add(value.life.retained.len()).saturating_add(value.life.nominal_arguments.len()))?;
    let indices=value.classes.iter().enumerate().filter(|(_,key)|*key==source).map(|(index,_)|index as u32).collect::<Vec<_>>();
    for index in indices.into_iter().rev(){
        let mut outer=value.life.slots.iter().filter(|row|row.binder==index);
        let row=outer.next().ok_or_else(||bad("static invocation cannot erase a whole intrinsic child class"))?;
        if !row.path.is_empty()||row.mode!=StoredBorrowMode::SharedAddress||outer.next().is_some()||value.life.retained.iter().any(|row|row.binder==index)||value.life.nominal_arguments.iter().any(|row|row.binder==index)||!matches!(value.ty,Type::Address{mutable:false,..}){return Err(bad("static invocation shares a retained or non-shared original role"));}
        value.life.slots.retain(|row|row.binder!=index);
        for row in &mut value.life.slots{if row.binder>index{row.binder-=1;}}
        for row in &mut value.life.retained{if row.binder>index{row.binder-=1;}}
        for row in &mut value.life.nominal_arguments{if row.binder>index{row.binder-=1;}}
        value.classes.remove(index as usize);value.origins.remove(index as usize);value.life.binder_count=value.classes.len() as u32;
        let row=StaticOccurrence::Direct{path:vec![],leaf:value.raw_type_copy(b)?,mode:StoredBorrowMode::SharedAddress};if !value.statics.contains(&row){value.statics.push(row);}
    }Ok(())
}

#[cfg(test)]
mod input_image_tests{
    use super::*;
    fn address(key:ClassKey)->CheckedType{
        CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Address{mutable:false,inner:Box::new(Type::I32)}),life:StorageLifeShape{binder_count:1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}],retained:vec![],nominal_arguments:vec![]},origins:vec![IntrinsicOrigin::DerivedClass{source:key.clone()}],classes:vec![key],statics:vec![]}
    }
    // Original operand proofs supply these images; this test exercises only
    // the scoped invocation substitution, never a Source or borrow authority.
    #[test]
    fn invocation_result_preserves_original_head_images_and_opaque_occurrences(){
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/phantom-closure-complete-creator.wire"),&Limits::default()).unwrap();
        let mut b=Budget::new(&Limits::default());
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let owner=SemanticDefinitionId{module:module.module_id,index:0};
        let call=ScopeKey::Witness{definition:owner,frame:crate::semantic::api::FrameToken(0)};
        let caller=ScopeKey::Instance(SemanticInstanceId{module:module.module_id,index:0});
        let env=EvalEnv{lexical:vec![LexicalFrame{definition:owner,key:caller,actuals:vec![],environment:vec![],self_type:None}],assumptions:vec![caller],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let fresh=[ClassKey::Implicit{scope:call,ordinal:0},ClassKey::Implicit{scope:call,ordinal:1}];
        let origins=(0..2).map(|class|IntrinsicOrigin::Existing{scope:caller,operand:OperandKey::Receiver,path:vec![],class}).collect::<Vec<_>>();
        let template=CheckedSignature{parameters:fresh.iter().cloned().map(|key|(address(key),PassingMode::Borrow)).collect(),result:CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Closure(ClosureId(0))),life:StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:fresh.to_vec(),origins:origins.clone(),statics:vec![]},result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]};
        assert!(returned_input_head(&template,call,0,&mut b).unwrap());
        assert!(returned_input_head(&template,call,1,&mut b).unwrap());
        // Unit/scalar calls, manager descriptions and already fixed caller
        // classes do not export a fresh compiler-address role. Their actual
        // whole domains stay under the existing callee P/R proof.
        let mut ordinary=template.clone();
        ordinary.result=CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Unit),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
        assert!(!returned_input_head(&ordinary,call,0,&mut b).unwrap());
        ordinary=template.clone();ordinary.parameters[0].0.type_cell=crate::semantic::CheckedTypeData::new(Type::Desc(Box::new(Type::I32)));
        ordinary.parameters[0].0.life.slots[0].mode=StoredBorrowMode::WritableDescription;
        assert!(!returned_input_head(&ordinary,call,0,&mut b).unwrap());
        ordinary=template.clone();ordinary.parameters[0].0=address(ClassKey::Implicit{scope:caller,ordinal:0});
        ordinary.result.classes[0]=ordinary.parameters[0].0.classes[0].clone();
        assert!(!returned_input_head(&ordinary,call,0,&mut b).unwrap());
        ordinary=template.clone();ordinary.result.classes=vec![fresh[1].clone();2];
        assert!(!returned_input_head(&ordinary,call,0,&mut b).unwrap(),"another same-shaped parameter cannot donate a returning head");
        for equal_image in [false,true]{
            let a=ClassKey::Implicit{scope:caller,ordinal:0};
            let z=ClassKey::Implicit{scope:caller,ordinal:if equal_image{0}else{1}};
            let arguments=[address(a.clone()),address(z.clone())];
            let mut signature=template.clone();
            context.instantiate_call_heads(&mut signature,&arguments.iter().collect::<Vec<_>>(),call,&env,&mut b).unwrap();
            assert_eq!(signature.result.classes,vec![a,z]);
            assert_eq!(signature.result.life.binder_count,2);
            assert_eq!(signature.result.origins,origins);
            assert_ne!(signature.result.origins[0],signature.result.origins[1]);
            assert!(!context.same(&env,&fresh[0],&signature.result.classes[0],&mut b).unwrap(),"a call image must not become a global equality");
        }
        let mut repeated=template;
        repeated.parameters[1].0=address(fresh[0].clone());
        let arguments=[address(ClassKey::Implicit{scope:caller,ordinal:0}),address(ClassKey::Implicit{scope:caller,ordinal:1})];
        context.instantiate_call_heads(&mut repeated,&arguments.iter().collect::<Vec<_>>(),call,&env,&mut b).unwrap();
        assert_eq!(repeated.result.classes,fresh);
        for argument in &arguments{assert!(context.outlives(&env,&argument.classes[0],&fresh[0],&mut b).unwrap());assert!(!context.same(&env,&argument.classes[0],&fresh[0],&mut b).unwrap());}
        assert!(!context.outlives(&env,&arguments[0].classes[0],&arguments[1].classes[0],&mut b).unwrap());
        assert!(!context.outlives(&env,&fresh[0],&arguments[0].classes[0],&mut b).unwrap());
        assert_eq!(repeated.result.life.binder_count,2,"directed input adaptation must preserve the complete result domain");
    }
}

fn shared_reborrow_adapter(producers:&sites::Producers,module:&TypedModule,f:&TypedFunction,argument:ValueId,b:&mut Budget)->Check<bool>{
    let mut current=argument;let mut seen=std::collections::BTreeSet::new();
    loop{
        b.spend(1)?;if !seen.insert(current){return Err(bad("Call shared reborrow adapter has a producer cycle"));}
        let (_, _, producer) = producers.get(f, current, b)?;crate::check_instruction(module,f,producer,b)?;
        let InstructionKind::ReborrowAddress{address,mutable:false}=producer.kind else{return Ok(false);};
        match f.values.get(address.0 as usize){Some(Type::Address{mutable:true,..})=>return Ok(true),Some(Type::Address{mutable:false,..})=>current=address,_=>return Ok(false)}
    }
}

#[cfg(test)]
mod shared_view_tests{
    use super::*;
    #[test]
    fn exclusive_call_view_keeps_every_inner_origin_even_when_outside_images_are_equal(){
        let limits=Limits::default();let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/phantom-closure-complete-creator.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let instance=module.semantic.as_ref().unwrap().instances[0].id;let scope=ScopeKey::Instance(instance);
        let image=ClassKey::Implicit{scope,ordinal:0};let outer=ClassKey::Outer{scope,operand:OperandKey::Value(ValueId(1)),path:vec![],mode:StoredBorrowMode::ExclusiveAddress};
        let origin=|class|IntrinsicOrigin::Existing{scope,operand:OperandKey::Value(ValueId(0)),path:vec![LifeTypeProjection::Pointee],class};
        // This tests complete domain transport, not admission of this synthetic
        // scalar pointee or an actual SSA/owner permission.
        let source=CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Address{mutable:true,inner:Box::new(Type::I32)}),life:StorageLifeShape{binder_count:3,slots:vec![StorageLifeSlot{path:vec![],binder:2,mode:StoredBorrowMode::ExclusiveAddress}],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class:0,binder:0},StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class:1,binder:1}],nominal_arguments:vec![]},classes:vec![image.clone(),image,outer.clone()],origins:vec![origin(0),origin(1),IntrinsicOrigin::DerivedClass{source:outer}],statics:vec![]};
        let view=context.call_shared_view(&source,&mut b).unwrap();
        assert!(matches!(view.ty,Type::Address{mutable:false,..}));assert_eq!(view.life.binder_count,3);assert_eq!(view.life.retained,source.life.retained);assert_eq!(view.classes,source.classes);assert_eq!(view.origins,source.origins);assert_ne!(view.origins[0],view.origins[1]);
        let env=EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None};
        assert!(!context.same_type(&env,&source,&view,&mut b).unwrap(),"a private view is not a global type equality");
        let mut changed=view.clone();changed.origins.swap(0,1);assert!(!context.same_type(&env,&view,&changed,&mut b).unwrap());
        assert!(context.call_shared_view(&view,&mut b).is_err());
        assert!(context.reborrow_value(&view,scope,ValueId(2),true,&mut b).unwrap_err().message.contains("upgrades a shared address"));
    }
}

#[cfg(test)]
mod region_image_tests {
    use super::*;
    fn address(class:ClassKey)->CheckedType{CheckedType{type_cell:CheckedTypeData::new(Type::Address{mutable:false,inner:Box::new(Type::I32)}),life:StorageLifeShape{binder_count:1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}],retained:vec![],nominal_arguments:vec![]},origins:vec![IntrinsicOrigin::DerivedClass{source:class.clone()}],classes:vec![class],statics:vec![]}}
    #[test]
    fn original_call_regions_are_not_replaced_by_their_input_images(){
        let limits=Limits::default();let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/phantom-closure-complete-creator.wire"),&limits).unwrap();let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let instance=&module.semantic.as_ref().unwrap().instances[0];let caller=ScopeKey::Instance(instance.id);let call=ScopeKey::Witness{definition:instance.definition,frame:FrameToken(900)};
        let env=EvalEnv{lexical:vec![LexicalFrame{definition:instance.definition,key:caller,actuals:vec![],environment:vec![],self_type:None}],assumptions:vec![caller],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let sigma=ClassKey::Implicit{scope:caller,ordinal:0};let fresh=[ClassKey::Implicit{scope:call,ordinal:0},ClassKey::Implicit{scope:call,ordinal:1}];let regions=[ClassKey::Implicit{scope:caller,ordinal:7},ClassKey::Implicit{scope:caller,ordinal:8}];
        let origins=(0..2).map(|class|IntrinsicOrigin::Existing{scope:caller,operand:OperandKey::Receiver,path:vec![],class}).collect::<Vec<_>>();
        // A scoped transport control, not an SSA or closure admission proof.
        let mut signature=CheckedSignature{parameters:fresh.iter().cloned().map(|class|(address(class),PassingMode::Borrow)).collect(),result:CheckedType{type_cell:CheckedTypeData::new(Type::Closure(ClosureId(0))),life:StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:fresh.to_vec(),origins:origins.clone(),statics:vec![]},result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]};
        let mut boundary=trait_members::Boundary::default();for(source,target)in fresh.iter().zip(&regions){boundary.class(&context,&env,source,target,&mut b).unwrap();context.premises.borrow_mut().entry(caller).or_default().relations.outlives.push((sigma.clone(),target.clone()));}
        for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(&context,&env,parameter,&mut b).unwrap();}signature.result=boundary.project_type(&context,&env,&signature.result,&mut b).unwrap();
        let inputs=[address(sigma.clone()),address(sigma.clone())];context.instantiate_call_heads(&mut signature,&inputs.iter().collect::<Vec<_>>(),call,&env,&mut b).unwrap();
        assert_eq!(signature.result.classes,regions);assert_eq!(signature.result.life.binder_count,2);assert_eq!(signature.result.origins,origins);assert_ne!(signature.result.origins[0],signature.result.origins[1]);
        for(parameter,_)in &signature.parameters{assert!(context.compatible(&env,&inputs[0],parameter,&mut b).unwrap());assert!(!context.same(&env,&sigma,&parameter.classes[0],&mut b).unwrap());}
    }
}

impl CheckedContext {
    /// Match the implicit physical validity precondition of each real Address
    /// parameter. These are directed outside-life obligations, not equalities
    /// of the pointee's complete nominal or retained intrinsic classes.
    pub(super) fn check_call_input_wf(&self,module:&TypedModule,signature:&CheckedSignature,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,b:&mut Budget)->Check<()> {
        if signature.parameters.len()!=arguments.len(){return Err(bad("call validity omits a complete input ordinal"));}
        for((expected,_),argument)in signature.parameters.iter().zip(arguments){
            if !matches!(expected.ty,Type::Address{..}){continue;}
            let actual=value_at(values,*argument)?;
            let mut heads=expected.life.slots.iter().filter(|row|row.path.is_empty());
            let Some(head)=heads.next()else{continue;};if heads.next().is_some(){return Err(bad("call validity has ambiguous outer parameter heads"));}
            let target=expected.classes.get(head.binder as usize).ok_or_else(||bad("call validity outer parameter class is absent"))?;
            let child=self.child_value(module,actual,&[LifeTypeProjection::Pointee],b)?;
            b.spend(child.life.slots.len())?;
            for slot in &child.life.slots{
                if !matches!(slot.mode,StoredBorrowMode::SharedAddress|StoredBorrowMode::ExclusiveAddress){continue;}
                let source=child.classes.get(slot.binder as usize).ok_or_else(||bad("call validity physical address source class is absent"))?;
                if !self.outlives(environment,source,target,b)?{return Err(bad("call input has no proved physical-address validity bound for its borrow window"));}
            }
        }Ok(())
    }
}

impl CheckedContext {
    /// An actual Address formation (or a valid Address input) promises a live
    /// physical pointee for its window. Preserve only the corresponding
    /// physical Address bounds. This creates no target, permission or κ alias;
    /// the real operation/entry and slot-loan kernels remain obligatory.
    pub(super) fn record_address_validity(&self,module:&TypedModule,value:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        if !matches!(value.ty,Type::Address{..}){return Ok(());}
        let mut heads=value.life.slots.iter().filter(|row|row.path.is_empty());
        let Some(head)=heads.next()else{return Ok(());};if heads.next().is_some(){return Err(bad("actual Address validity has ambiguous outer heads"));}
        let target=value.classes.get(head.binder as usize).ok_or_else(||bad("actual Address validity outer class is absent"))?;
        let child=self.child_value(module,value,&[LifeTypeProjection::Pointee],b)?;
        let mut edges=Vec::new();
        b.spend(child.life.slots.len())?;
        for slot in &child.life.slots{if matches!(slot.mode,StoredBorrowMode::SharedAddress|StoredBorrowMode::ExclusiveAddress){
            let source=child.classes.get(slot.binder as usize).ok_or_else(||bad("actual Address physical source class is absent"))?;
            edges.push((source.clone(),target.clone()));
        }}
        if edges.is_empty(){return Ok(());}
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let mut premises=self.premises.borrow_mut();let rows=premises.entry(frame.key).or_default();let mut changed=false;
        for edge in edges{b.spend(rows.relations.outlives.len().saturating_add(1))?;if !rows.relations.outlives.contains(&edge){rows.relations.outlives.push(edge);changed=true;}}
        drop(premises);if changed{self.invalidate_closed_frames();}Ok(())
    }
}

impl CheckedContext {
    /// A stored shared view may use the exact already-proved parent of its
    /// real ReborrowAddress. The SSA loan keeps its own root; no global edge or
    /// equality lengthens that root or changes a retained child κ.
    pub(super) fn stored_borrow_view(&self,module:&TypedModule,caller:&SemanticInstance,argument:ValueId,values:&[Option<CheckedType>],expected:&CheckedType,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>> {
        if !matches!(expected.ty,Type::Address{mutable:false,..}){return Ok(None);}
        if !self.call_argument(module,caller,argument,values,expected,environment,true,b)?{return Ok(None);}
        let mut heads=expected.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==StoredBorrowMode::SharedAddress);
        let Some(head)=heads.next()else{return Ok(None);};if heads.next().is_some(){return Err(bad("stored shared view has ambiguous original outer roles"));}
        let outer=expected.classes.get(head.binder as usize).ok_or_else(||bad("stored shared view original outer role is absent"))?.clone();
        let child=self.child_value(module,value_at(values,argument)?,&[LifeTypeProjection::Pointee],b)?;
        let view=dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(outer)},&[child],active,b)?;
        if !self.same_type(environment,&view,expected,b)?{return Err(bad("stored shared view changes a complete original child domain"));}
        Ok(Some(view))
    }
}

#[cfg(test)]
mod local_window_argument_tests{
    use super::*;
    #[test]
    fn checked_original_source_adapts_a_real_reborrow_to_its_local_window_only(){
        let limits=Limits::default();let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/phantom-closure-complete-creator.wire"),&limits).unwrap();let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let caller=module.semantic.as_ref().unwrap().instances[0].clone();let scope=ScopeKey::Instance(caller.id);
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{panic!("fixture must have a concrete actual entry")};
        let f=&mut module.functions[function.0 as usize];f.locals.truncate(1);f.locals[0].ty=Type::Address{mutable:false,inner:Box::new(Type::I32)};f.parameters=vec![LocalId(0)];f.values=vec![f.locals[0].ty.clone();2];let span=f.span;
        f.blocks[0].instructions=vec![Instruction{result:Some(ValueId(0)),kind:InstructionKind::Load(LocalId(0)),span},Instruction{result:Some(ValueId(1)),kind:InstructionKind::ReborrowAddress{address:ValueId(0),mutable:false},span}];
        let sigma=ClassKey::Formal{scope,ordinal:0};let window=ClassKey::Implicit{scope,ordinal:7};let other=ClassKey::Implicit{scope,ordinal:8};let child=ClassKey::Outer{scope,operand:OperandKey::Value(ValueId(1)),path:vec![],mode:StoredBorrowMode::SharedAddress};
        let address=|key:ClassKey|CheckedType{type_cell:CheckedTypeData::new(Type::Address{mutable:false,inner:Box::new(Type::I32)}),life:StorageLifeShape{binder_count:1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}],retained:vec![],nominal_arguments:vec![]},origins:vec![IntrinsicOrigin::DerivedClass{source:key.clone()}],classes:vec![key],statics:vec![]};
        let env=EvalEnv{lexical:vec![LexicalFrame{definition:caller.definition,key:scope,actuals:vec![CheckedActual::Life(sigma.clone())],environment:vec![],self_type:None}],assumptions:vec![scope],images:vec![],bounds:vec![],invocations:vec![],at:None};
        // A completed original initializer/assignment supplies this direction;
        // this is a scoped transport control, not a source or SSA admission.
        context.premises.borrow_mut().entry(scope).or_default().relations.outlives.push((sigma.clone(),window.clone()));
        let values=vec![Some(address(sigma.clone())),Some(address(child))];
        assert!(!context.call_argument(&module,&caller,ValueId(1),&values,&address(window.clone()),&env,false,&mut b).unwrap());
        assert!(context.call_argument(&module,&caller,ValueId(1),&values,&address(window.clone()),&env,true,&mut b).unwrap());
        assert!(!context.call_argument(&module,&caller,ValueId(1),&values,&address(other),&env,true,&mut b).unwrap());
        assert!(!context.same(&env,&sigma,&window,&mut b).unwrap());assert!(!context.outlives(&env,&window,&sigma,&mut b).unwrap());
    }
}
