//! Actual ghost SSA/local flow. Type metadata never supplies a data target.
//! Every admitted instruction is replayed against its real operands and CFG.
use super::*;
use std::collections::VecDeque;
mod intrinsics;
mod capture_values;
mod ghost_captures;
mod closed_values;
mod operations;
mod products;
mod aggregate_values;
mod child_values;
mod sum_values;
mod call_borrows;
mod trait_members;
mod worlds;
mod task_operations;
mod dyn_values;
mod static_values;
mod callbacks;
mod scoped_threads;
mod closure_calls;
mod code_calls;
mod type_uses;
mod callable_sources;
mod local_roles;
mod replacements;
mod persistent_slots;
#[derive(Clone,PartialEq,Eq)]
struct State {locals:Vec<Option<CheckedType>>}
pub(crate) fn check_program(module:&TypedModule,b:&mut Budget)->Check<()> {
    check_program_mode(module,None,b)
}
pub(super) fn check_program_mode(module:&TypedModule,frozen:Option<&FrozenContext<'_>>,b:&mut Budget)->Check<()> {
    b.diagnostic_stage("semantic catalogue");
    let context=CheckedContext::catalogue_mode(module,frozen,b)?;
    let program=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?;
    crate::foreign::check_declarations(module,b)?;
    let mut active=ActiveQueries::new();
    b.diagnostic_stage("original universal bodies");
    let mut original=original::CheckedOriginal::check(module,&context,&mut active,b)?;
    b.diagnostic_stage("original frozen schemes");
    original.frozen_schemes(module,&context,&mut active,b)?;
    original.verify_direct_consuming_goals(module,&context,&mut active,b)?;
    *context.original.borrow_mut()=Some(original);
    let mut external=dynamic_witness::semantic_external::Proofs::default();
    b.diagnostic_stage("task context metadata");
    context.prepare_task_contexts(module,&mut external,&mut active,b)?;
    b.diagnostic_stage("frozen specialization boundaries");
    if let Some(frozen)=frozen {frozen.specializations(module,&context,&mut active,b)?;}
    let mut checked_instances=std::collections::BTreeSet::new();
    let mut checked_worlds=Vec::new();
    let mut checked_results=Vec::new();
    let mut ordered=Vec::new();
    for instance in &program.instances {
        let mut depth=0usize;let mut parent=instance.parent;
        while let Some(id)=parent {b.spend(1)?;depth+=1;parent=context.instance(module,id,b)?.parent;}
        ordered.push((depth,instance));
    }
    b.spend(ordered.len().saturating_mul(ordered.len().checked_ilog2().map_or(1,|n|n as usize+1)))?;
    ordered.sort_by_key(|(depth,instance)|(*depth,instance.id));
    let total=if b.diagnostic_progress.is_some(){ordered.iter().filter(|(_,instance)|matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_)))).count()}else{0};
    let mut ordinal=0;
    for (_,instance) in ordered {
        if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target {
            ordinal+=1;b.diagnostic_stage("actual world environment");
            if let Some(progress)=&mut b.diagnostic_progress{progress.world=Some((ordinal,total,instance.id,function));}
            let environment=context.environment_for_instance(module,instance.id,&mut active,b)?;
            let definition=context.definition(module,instance.definition,b)?;
            context.evaluate_requirements(module,&definition.requirements,&environment,RequirementMode::DeclarationPremise,&mut external,&mut active,b)?;
            for formal in &definition.parameters {
                if let SemanticFormal::Type{requirements}=formal {context.evaluate_requirements(module,requirements,&environment,RequirementMode::DeclarationPremise,&mut external,&mut active,b)?;}
            }
            let SemanticDefinitionBody::FunctionWithBody{signature,..}=&definition.body else {return Err(bad("actual function has no original signature"));};
            b.diagnostic_stage("actual world signature");
            let signature=context.evaluate_signature(module,signature,&environment,&mut external,&mut active,b)?;
            b.diagnostic_stage("actual world body");
            context.function(module,instance,function,&signature,&environment,&mut external,&mut active,b)?;
            context.mark_task_worker_checked(module,instance,&signature,b)?;
            checked_instances.insert(instance.id);
            b.spend(environment.lexical.len().saturating_add(1))?;
            checked_worlds.push((instance.id,environment));
            b.spend(1)?;checked_results.push((instance.id,signature.result));
        }
    }
    b.diagnostic_stage("actual closure boundaries");
    for boundary in context.closure_boundaries.borrow().iter() {
        b.spend(1)?;
        if !checked_instances.contains(&boundary.child)||!context.seals.borrow().contains_key(&boundary.child){return Err(bad("actual Closure boundary has no completely checked child entry and creator seal"));}
    }
    for cursor in context.pending_closures.borrow().iter() {
        b.spend(cursor.arguments.len().saturating_add(cursor.sealed.len()).saturating_add(cursor.environment.lexical.len()).saturating_add(cursor.expected.classes.len()))?;
        let instance=context.instance(module,cursor.instance,b)?;
        let definition=context.definition(module,instance.definition,b)?;
        if !checked_instances.contains(&instance.id)||!context.seals.borrow().contains_key(&instance.id) {return Err(unsupported("pending Closure type cursor has no complete actual creator seal and checked entry",None));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target else {return Err(bad("pending Closure cursor selects a nonexecutable scope"));};
        let entry=module.functions.get(function.0 as usize).ok_or_else(||bad("pending Closure entry is absent"))?;
        let closure=entry.environment.ok_or_else(||bad("pending Closure entry lacks its environment identity"))?;
        let relation=entry.storage_life.as_ref().ok_or_else(||bad("pending Closure entry complete header is Unknown"))?;
        if cursor.expected.ty!=Type::Closure(closure)||cursor.expected.life.binder_count!=relation.fixed_binder_count||cursor.expected.classes.len()!=relation.fixed_binder_count as usize||cursor.arguments.len()!=definition.parameters.len()||cursor.sealed.len()!=definition.environment.len() {return Err(bad("pending Closure cursor contradicts its fully checked actual entry/domain"));}
        b.spend(checked_worlds.len())?;
        let actual=checked_worlds.iter().find(|(id,_)|*id==instance.id).ok_or_else(||bad("pending Closure cursor has no exact completed original world"))?;
        context.finish_closure_cursor(module,cursor,&actual.1,b)?;
        // The caller-side cursor may instantiate the generic creator's rigid
        // lifetime into another caller domain. Its original recipe and every
        // actual Call substitution are checked separately; no global union
        // equates those outside images with the rigid creator's identities.
    }
    b.diagnostic_stage("actual external methods");
    context.prepare_static_definitions(module,&checked_worlds,&mut external,&mut active,b)?;
    context.finish_external_methods(module,&mut external,&mut active,b)?;
    context.defer_callable_shapes(module,&mut external,&checked_instances,b)?;
    context.complete_static_members(module,&checked_instances,&checked_worlds,b)?;
    // External shape requests are obligations, not proof tokens. No Some
    // program is published while its actual entry/static memory is pending.
    if !external.callable_shapes.is_empty()||!external.pending_statics.is_empty() {
        return Err(unsupported("semantic external signature still has undischarged actual entry/static obligations",None));
    }
    // Scope-only tables cannot hide actual class-bearing functions. A complete
    // semantic program accounts for every nonforeign Typed function.
    for function in &module.functions {
        b.spend(program.instances.len())?;
        if function.foreign.is_none()&&!program.instances.iter().any(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==function.id)) {return Err(bad("semantic programme omits an actual function instance"));}
    }
    b.diagnostic_stage("actual constructor publication");
    context.publish_constructor_classes(module,&checked_worlds,b)?;
    context.publish_local_role_transfers(module,&checked_worlds,b)?;
    b.spend(1)?;
    b.semantic_checked=Some(std::rc::Rc::new(dynamic_witness::semantic_patterns::Session::new(module,context,active,checked_worlds,checked_results)));
    Ok(())
}
impl CheckedContext {
    pub(crate) fn closed_input_original(&self,module:&TypedModule,definition:SemanticDefinitionId,ordinal:u32,path:&[ContractProjection],active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        self.original.borrow_mut().as_mut().ok_or_else(||bad("ClosedEnvironment input has no completed original programme"))?.closed_input_original(module,self,definition,ordinal,path,active,b)
    }
    pub(crate) fn finish_original_callable_retirements(&self,module:&TypedModule,worlds:&[(SemanticInstanceId,EvalEnv)],b:&mut Budget)->Check<()>{
        self.original.borrow().as_ref().ok_or_else(||bad("callable retirement final query lacks the completed original programme"))?.finish_callable_retirements(module,self,worlds,b)
    }
    pub(super) fn record(&self,scope:ScopeKey,key:OperandKey,value:&CheckedType,b:&mut Budget)->Check<()> {
        value.charge_clone(b)?;
        b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
        let mut aliases=self.origin_aliases.borrow_mut();
        for (class,origin) in value.origins.iter().enumerate() {
            let occurrence=IntrinsicOrigin::Existing{scope,operand:key.clone(),path:vec![],class:class as u32};
            if &occurrence!=origin {aliases.insert(occurrence,origin.clone());}
        }
        for row in &value.life.retained {
            b.spend(row.path.len().saturating_add(1))?;
            let origin=value.origins.get(row.binder as usize).ok_or_else(||bad("actual child origin is absent"))?;
            let occurrence=IntrinsicOrigin::Existing{scope,operand:key.clone(),path:row.path.clone(),class:row.class};
            if &occurrence!=origin {aliases.insert(occurrence,origin.clone());}
        }
        drop(aliases);
        self.occurrences.borrow_mut().insert((scope,key),value.clone());Ok(())
    }
    pub(super) fn compatible(&self,environment:&EvalEnv,actual:&CheckedType,expected:&CheckedType,b:&mut Budget)->Check<bool> {
        if self.same_type(environment,actual,expected,b)? {return Ok(true);}
        if actual.ty!=expected.ty||!matches!(actual.ty,Type::Address{..}|Type::Desc(_)) {return Ok(false);}
        b.spend(actual.life.retained.len().saturating_add(expected.life.retained.len()).saturating_add(actual.life.nominal_arguments.len()).saturating_add(expected.life.nominal_arguments.len()))?;
        if actual.life.retained.len()!=expected.life.retained.len()||actual.life.nominal_arguments.len()!=expected.life.nominal_arguments.len() {return Ok(false);}
        for (a,c) in actual.life.retained.iter().zip(&expected.life.retained) {
            if a.path!=c.path||a.class!=c.class {return Ok(false);}
            if !self.same_origin(environment,actual.origins.get(a.binder as usize).ok_or_else(||bad("actual child origin is absent"))?,expected.origins.get(c.binder as usize).ok_or_else(||bad("expected child origin is absent"))?,b)? {return Ok(false);}
            let a=actual.classes.get(a.binder as usize).ok_or_else(||bad("actual child class is absent"))?;
            let c=expected.classes.get(c.binder as usize).ok_or_else(||bad("expected child class is absent"))?;
            if !self.same(environment,a,c,b)? {return Ok(false);}
        }
        for (a,c) in actual.life.nominal_arguments.iter().zip(&expected.life.nominal_arguments) {
            if a.path!=c.path||a.declaration!=c.declaration||a.argument!=c.argument {return Ok(false);}
            if !self.same_origin(environment,actual.origins.get(a.binder as usize).ok_or_else(||bad("actual nominal origin is absent"))?,expected.origins.get(c.binder as usize).ok_or_else(||bad("expected nominal origin is absent"))?,b)? {return Ok(false);}
            let a=actual.classes.get(a.binder as usize).ok_or_else(||bad("actual nominal class is absent"))?;
            let c=expected.classes.get(c.binder as usize).ok_or_else(||bad("expected nominal class is absent"))?;
            if !self.same(environment,a,c,b)? {return Ok(false);}
        }
        b.spend(actual.life.slots.len().saturating_add(expected.life.slots.len()))?;
        let actual_children=actual.life.slots.iter().filter(|slot|!slot.path.is_empty()).collect::<Vec<_>>();
        let expected_children=expected.life.slots.iter().filter(|slot|!slot.path.is_empty()).collect::<Vec<_>>();
        if actual_children.len()!=expected_children.len(){return Ok(false);}
        for(a,c)in actual_children.iter().zip(&expected_children){
            if a.path!=c.path||a.mode!=c.mode{return Ok(false);}
            if !self.same(environment,actual.classes.get(a.binder as usize).ok_or_else(||bad("actual nested borrow class is absent"))?,expected.classes.get(c.binder as usize).ok_or_else(||bad("expected nested borrow class is absent"))?,b)?||!self.same_origin(environment,actual.origins.get(a.binder as usize).ok_or_else(||bad("actual nested borrow origin is absent"))?,expected.origins.get(c.binder as usize).ok_or_else(||bad("expected nested borrow origin is absent"))?,b)?{return Ok(false);}
        }
        let a=actual.life.slots.iter().find(|row|row.path.is_empty());
        let c=expected.life.slots.iter().find(|row|row.path.is_empty());
        match (a,c) {
            (Some(a),Some(c)) if a.mode==c.mode=>self.outlives(environment,actual.classes.get(a.binder as usize).ok_or_else(||bad("actual outer class is absent"))?,expected.classes.get(c.binder as usize).ok_or_else(||bad("expected outer class is absent"))?,b),
            (None,Some(c))=>Ok(actual.statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode} if path.is_empty()&&leaf==&actual.ty&&*mode==c.mode))),
            _=>Ok(false),
        }
    }
    fn function(&self,module:&TypedModule,instance:&SemanticInstance,function:FunctionId,signature:&CheckedSignature,environment:&EvalEnv,external:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("semantic actual function is absent"))?;
        let original_definition=self.definition(module,instance.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{protocol_contracts,foreign,resume_bound,..}=&original_definition.body else{return Err(bad("actual protocol bridge lacks its original body"));};
        if f.foreign!=*foreign||f.resume_bound!=*resume_bound{return Err(bad("actual external/resume header differs from its complete original declaration"));}
        if f.protocol_contracts!=*protocol_contracts{return Err(bad("actual phase contract differs from the complete original protocol declaration"));}
        if f.parameters.len()!=signature.parameters.len()||f.return_type!=signature.result.ty||f.result_mode!=signature.result_mode||f.call_kind!=signature.call_kind {return Err(bad("actual function ABI differs from the original complete signature"));}
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        b.spend(f.locals.len().saturating_add(f.values.len()).saturating_add(f.blocks.len()))?;
        let mut initial=State{locals:vec![None;f.locals.len()]};
        for (index,(parameter,(expected,_))) in f.parameters.iter().zip(&signature.parameters).enumerate() {
            let local=f.locals.get(parameter.0 as usize).ok_or_else(||bad("actual parameter local is absent"))?;
            if local.ty!=expected.ty {return Err(bad("actual parameter whole type differs from its original recipe"));}
            initial.locals[parameter.0 as usize]=Some(expected.clone());
            self.record(scope,OperandKey::Parameter(index as u32),expected,b)?;
            self.record_address_validity(module,expected,environment,b)?;
        }
        self.prepare_local_roles(module,instance,environment,b)?;
        self.header(module,instance,f,signature,environment,b)?;
        if f.foreign.is_some() {
            let definition=self.definition(module,instance.definition,b)?;
            if !matches!(definition.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::External,..}){return Err(bad("actual foreign declaration replaces an original user body"));}
            return Ok(());
        }
        let mut states=vec![None;f.blocks.len()];
        let entry=f.entry.0 as usize;
        if entry>=states.len() {return Err(bad("actual CFG entry is absent"));}
        states[entry]=Some(initial);
        let mut queue=VecDeque::from([entry]);
        let mut values=vec![None;f.values.len()];
        let mut constraints=vec![None;f.locals.len()];
        while let Some(block_id)=queue.pop_front() {
            b.spend(1)?;
            let mut state=states[block_id].clone().ok_or_else(||bad("reachable semantic block has no incoming state"))?;
            let block=&f.blocks[block_id];
            for (ordinal,instruction) in block.instructions.iter().enumerate() {
                let mut environment=environment.clone();
                environment.at=Some(SitePoint{function,block:block.id,instruction:ordinal as u32,after:false});
                let sites=self.actual_sites(module,instance.id,block.id,ordinal as u32,b)?;
                for site in &sites {
                    if let SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(local),expected}=&site.action {
                        let value=self.site_expected(module,instance,site,&environment,external,active,b)?;
                        self.claim(module,expected,&value,&environment,b)?;
                        let value=if f.locals[local.0 as usize].ty==value.ty{value}else{self.persistent_constraint(module,instance,f,*local,&value,b)?};
                        if let Some(old)=&constraints[local.0 as usize] {if !self.same_type(&environment,old,&value,b)? {return Err(bad("local declarations impose different semantic constraints"));}}
                        constraints[local.0 as usize]=Some(value);
                    }
                }
                // An existing operand's Before observation belongs to the
                // input boundary. Calls may consume its independently bound
                // source occurrence; their own results remain unavailable.
                let mut input_uses=std::collections::BTreeSet::new();
                for site in &sites{
                    if matches!(site.at,SemanticSiteAddress::Typed{edge:SemanticSiteEdge::Before,..})&&matches!(&site.action,SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}if Some(*value)!=instruction.result&&values.get(value.0 as usize).and_then(Option::as_ref).is_some()){
                        b.spend(1)?;self.check_type_use(module,instance,f,site,&state,&values,signature,&environment,external,active,b)?;input_uses.insert(site.recipe);
                    }
                }
                let produced=match &instruction.kind {
                    InstructionKind::StaticUtf8{..}=>Some(self.static_utf8(module,instance,f,instruction,b)?),
                    InstructionKind::Constant(constant)=>{
                        let value=atomic(&constant.ty(),b)?;
                        for site in sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Construct{..})){
                            let expected=self.site_expected(module,instance,site,&environment,external,active,b)?;
                            if !self.same_type(&environment,&value,&expected,b)?{return Err(bad("actual constant constructor differs from its original payload role"));}
                            if let SemanticSiteAction::Construct{value:claim,..}=&site.action{self.claim(module,claim,&expected,&environment,b)?;}
                        }
                        Some(value)
                    },
                    InstructionKind::Load(local)|InstructionKind::TakeLocal{local}=>{
                        let value=state.locals.get(local.0 as usize).and_then(Option::as_ref).cloned().ok_or_else(||bad("semantic local use is not initialized on every incoming edge"))?;
                        self.record(scope,OperandKey::Local{local:*local,at:environment.at.unwrap()},&value,b)?;
                        if matches!(instruction.kind,InstructionKind::TakeLocal{..}) {state.locals[local.0 as usize]=None;}
                        Some(value)
                    }
                    InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=>{
                        let child=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual borrowed local has no initialized semantic occupant"))?;
                        let result=instruction.result.ok_or_else(||bad("actual local borrow has no SSA result"))?;
                        let mode=if matches!(instruction.kind,InstructionKind::BorrowLocalMut{..}){StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                        let outer=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
                        Some(dynamic_witness::semantic_external::construct(self,&environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(outer)},std::slice::from_ref(child),active,b)?)
                    }
                    InstructionKind::ReborrowAddress{address,mutable}=>{
                        let result=instruction.result.ok_or_else(||bad("actual reborrow has no SSA result"))?;
                        Some(self.reborrow_value(value_at(&values,*address)?,scope,result,*mutable,b)?)
                    }
                    InstructionKind::DescribeLocal{local}=>{
                        let owner=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual description has no initialized owner"))?;

                        let Type::Managed(_payload)=&owner.ty else{return Err(bad("actual description has no initialized complete owning carrier"));};
                        crate::resources::check_type_for(module,&owner.ty,Some(instruction.span),b)?;
                        let child=self.child_value(module,owner,&[LifeTypeProjection::Pointee],b)?;
                        let result=instruction.result.ok_or_else(||bad("actual description has no result"))?;
                        let mode=StoredBorrowMode::WritableDescription;
                        let outer=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
                        Some(dynamic_witness::semantic_external::construct(self,&environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(outer)},&[child],active,b)?)
                    }
                    InstructionKind::DropManaged{local}=>{
                        if let Some(value)=state.locals.get(local.0 as usize).and_then(Option::as_ref){

                            crate::resources::check_type_for(module,&value.ty,Some(instruction.span),b)?;
                        }
                        state.locals[local.0 as usize]=None;None
                    }
                    InstructionKind::ConvertInteger{value,target}|InstructionKind::ConvertNumber{value,target}=>{
                        if sites.iter().any(|site|matches!(site.action,SemanticSiteAction::BodyCall{..})){self.actual_intrinsic(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?}
                        else{let source=&value_at(&values,*value)?.ty;if (!source.is_integer()&&!source.is_float())||(!target.is_integer()&&!target.is_float()){return Err(bad("numeric operation has a nonnumeric endpoint"));}Some(atomic(target,b)?)}
                    }
                    InstructionKind::MakeDynOwned{..}|InstructionKind::TaskYield|InstructionKind::ByteBufferNew|InstructionKind::ByteBufferReserve{..}|InstructionKind::ByteBufferPush{..}|InstructionKind::ByteBufferTruncate{..}|InstructionKind::ByteBufferLen{..}|InstructionKind::ByteBufferCapacity{..}|InstructionKind::ByteBufferBorrow{..}|InstructionKind::OwnedStoreNew{..}|InstructionKind::OwnedStoreReserve{..}|InstructionKind::OwnedStorePush{..}|InstructionKind::OwnedStorePop{..}|InstructionKind::OwnedStoreBorrow{..}|InstructionKind::OwnedStoreBorrowMut{..}|InstructionKind::OwnedStoreLen{..}|InstructionKind::OwnedStoreCapacity{..}|InstructionKind::TaskCreate{..}|InstructionKind::TaskCreateOwned{..}|InstructionKind::TaskValid{..}|InstructionKind::TaskPoll{..}|InstructionKind::TaskRequestCancel{..}|InstructionKind::TaskPollClose{..}|InstructionKind::TaskFinishDestroy{..}|InstructionKind::MutexCreate{..}|InstructionKind::MutexLock{..}|InstructionKind::MutexValue{..}|InstructionKind::MutexUnlock{..}|InstructionKind::AsyncRegistrationStart{..}|InstructionKind::AsyncRegistrationValid{..}|InstructionKind::AsyncRegistrationRequestClose{..}|InstructionKind::AsyncRegistrationPollClose{..}|InstructionKind::AsyncRegistrationFinishDestroy{..}|InstructionKind::AsyncRegistrationFinishDestroyChecked{..}|InstructionKind::WithScopedThread{..}=>self.actual_intrinsic(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?,
                    InstructionKind::ReadCapture{index}|InstructionKind::TakeCapture{index}=>{
                        let value=self.occurrences.borrow().get(&(scope,OperandKey::Capture(*index))).cloned().ok_or_else(||bad("actual capture read has no sealed creator operand"))?;
                        Some(value)
                    }
                    InstructionKind::BindPackageCapture{..}|InstructionKind::EndPackageCapture{..}|InstructionKind::DropPackageField{..}|InstructionKind::StorePackageField{..}|InstructionKind::ReadPackageField{..}|InstructionKind::TakePackageField{..}|InstructionKind::DescribePackageField{..}|InstructionKind::BorrowPackageField{..}|InstructionKind::DescribeCapture{..}|InstructionKind::BorrowCapture{..}|InstructionKind::WriteCapture{..}|InstructionKind::DropPackage{..}|InstructionKind::ExtractField{..}|InstructionKind::AddressField{..}|InstructionKind::DescribeAddress{..}|InstructionKind::ReadDesc{..}|InstructionKind::WriteDesc{..}=>self.owner_field_instruction(module,instance,f,instruction,&mut state,&values,&environment,active,b)?,
                    InstructionKind::SliceLength{..} if sites.iter().any(|site|matches!(site.action,SemanticSiteAction::BodyCall{..}))=>self.actual_intrinsic(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?,
                    InstructionKind::LoadStoredBorrow{..}|InstructionKind::StoreStoredBorrow{..}|InstructionKind::MakeVariant{..}|InstructionKind::VariantTag{..}|InstructionKind::ReadVariant{..}|InstructionKind::ExtractIndex{..}|InstructionKind::UpdateIndex{..}|InstructionKind::AddressIndex{..}|InstructionKind::AddressSlice{..}|InstructionKind::SliceLength{..}=>self.product_instruction(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?,
                    InstructionKind::MakeErrorUnion{..}|InstructionKind::ReadErrorUnion{..}|InstructionKind::TakeErrorUnion{..}|InstructionKind::ErrorTag{..}|InstructionKind::VariantTagLocal{..}|InstructionKind::ReadVariantLocal{..}|InstructionKind::PlaceTagLocal{..}|InstructionKind::TakeVariantField{..}|InstructionKind::TakeTupleField{..}|InstructionKind::BorrowPlaceLocal{..}|InstructionKind::UnpackTuple{..}|InstructionKind::UnpackVariant{..}|InstructionKind::DropErrorUnion{..}|InstructionKind::DropEnum{..}|InstructionKind::DropTuple{..}=>self.sum_instruction(module,instance,f,instruction,&sites,&mut state,&values,signature,&environment,external,active,b)?,
                    InstructionKind::ReservePlaceLocal{..}|InstructionKind::ReservePlaceAddress{..}|InstructionKind::CommitPlaceReplace{..}|InstructionKind::AbortPlaceReservation{..}=>self.replacement_instruction(module,instance,f,instruction,&mut state,&values,&environment,external,active,b)?,
                    InstructionKind::TakeOwnedArrayElement{..}|InstructionKind::StoreOwnedArrayElement{..}|InstructionKind::DropOwnedArrayElement{..}|InstructionKind::BorrowOwnedArrayElement{..}|InstructionKind::DropOwnedArray{..}=>self.persistent_instruction(module,instance,f,instruction,&mut state,&values,&environment,active,b)?,
                    InstructionKind::ReadAddress{address}=>{
                        let address=value_at(&values,*address)?;
                        let Type::Address{..}=&address.ty else{return Err(bad("actual dereference operand is not an address"));};
                        Some(self.child_value(module,address,&[LifeTypeProjection::Pointee],b)?)
                    }
                    InstructionKind::WriteAddress{address,value}=>{
                        let address=value_at(&values,*address)?;let value=value_at(&values,*value)?;
                        if !matches!(&address.ty,Type::Address{mutable:true,..}){return Err(bad("actual address write lacks an exclusive head"));}
                        let child=self.child_value(module,address,&[LifeTypeProjection::Pointee],b)?;
                        if !self.compatible(&environment,value,&child,b)?{return Err(bad("actual address write changes its complete child type/domain"));}None
                    }
                    InstructionKind::Store{local,value}=>{
                        let actual=*value;
                        let value=value_at(&values,*value)?;
                        let mut normalized=None;let mut local_role=false;
                        if let Some(expected)=&constraints[local.0 as usize] {
                            if self.compatible(&environment,value,expected,b)?{
                                normalized=self.local_role_value(module,value,expected,&environment,active,b)?;
                                local_role=normalized.is_some();
                            }else{
                                normalized=self.stored_borrow_view(module,instance,actual,&values,expected,&environment,active,b)?;
                                if normalized.is_none(){return Err(bad("actual Store violates the declared rigid semantic class constraint"));}
                            }
                        }
                        if f.locals[local.0 as usize].ty!=value.ty {return Err(bad("actual Store changes its local complete type"));}
                        if local_role{if let Some(value)=&normalized{self.record_local_role_write(instance,*local,actual,value,&environment,b)?;}}
                        state.locals[local.0 as usize]=Some(match normalized{Some(value)=>value,None=>value.clone()});None
                    }
                    InstructionKind::StorageLive(local)|InstructionKind::StorageDead(local)=>{state.locals[local.0 as usize]=None;None},
                    InstructionKind::MakeAggregate{fields}|InstructionKind::MakePackage{fields}|InstructionKind::MakeManaged{fields}|InstructionKind::MakeOwnedTuple{fields}|InstructionKind::MakeOwnedArray{elements:fields}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Construct{..}));
                        let site=declarations.next();
                        if declarations.next().is_some() {return Err(bad("actual aggregate construction has duplicate semantic claims"));}
                        let value=if let Some(site)=site{self.site_expected(module,instance,site,&environment,external,active,b)?}else if matches!(instruction.kind,InstructionKind::MakeOwnedArray{..}){self.persistent_constructor(module,instance,f,instruction,&values,b)?}else{self.internal_copy_aggregate(module,f,instruction,fields,&values,b)?};
                        let mut storage=&value.ty;
                        while let Type::BorrowedStorage{storage:inner,..}|Type::Managed(inner)=storage {storage=inner;}
                        let expected_fields=match storage {
                            Type::Tuple(fields)=>fields.iter().collect::<Vec<_>>(),
                            Type::Record{fields,..}|Type::Package{fields,..}=>fields.iter().map(|(_,ty)|ty).collect(),
                            Type::Array{element,length}=>{b.spend(*length as usize)?;vec![element.as_ref();*length as usize]},
                            _=>return Err(bad("actual construction does not match the original complete product storage")),
                        };
                        if fields.len()!=expected_fields.len() {return Err(bad("actual construction omits or duplicates original fields"));}
                        for (index,(field,expected)) in fields.iter().zip(expected_fields).enumerate() {
                            let actual=value_at(&values,*field)?;
                            if actual.ty!=*expected {return Err(bad("actual construction field differs from its original complete type"));}
                            let mut path=Vec::new();let mut root=&value.ty;
                            loop{match root{Type::BorrowedStorage{storage,..}=>root=storage,Type::Managed(inner)=>{path.push(LifeTypeProjection::Pointee);root=inner;},_=>break}}
                            path.push(if matches!(root,Type::Array{..}){LifeTypeProjection::Element}else{LifeTypeProjection::Place(PlaceProjection::Field(index as u32))});
                            let child=self.child_value(module,&value,&path,b)?;
                            if !self.compatible(&environment,actual,&child,b)?{return Err(bad("actual construction changes a complete original child domain/source"));}
                        }
                        if let Some(result)=instruction.result {self.record(scope,OperandKey::Value(result),&value,b)?;}
                        if let Some(site)=site{if let SemanticSiteAction::Construct{value:claim,..}=&site.action {self.claim(module,claim,&value,&environment,b)?;}}
                        Some(value)
                    }
                    InstructionKind::Call{function:callee,arguments}|InstructionKind::SuspendCall{function:callee,arguments}|InstructionKind::ForeignCall{function:callee,arguments,..}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Call{..}|SemanticSiteAction::BodyCall{..}));
                        let site=declarations.next().ok_or_else(||bad("actual Call has no original semantic specialization site"))?;
                        if declarations.next().is_some() {return Err(bad("actual Call has duplicate semantic specialization claims"));}
                        let result=self.actual_call(module,instance,site,*callee,arguments,&values,&environment,external,active,b)?;
                        if matches!(instruction.kind,InstructionKind::ForeignCall{..}){let expected=crate::foreign::check_call(module,f,*callee,arguments,Some(instruction.span),b)?;if expected.as_ref()!=result.as_ref().map(|value|&value.ty){return Err(bad("actual foreign result differs from its complete original C signature"));}}
                        else if module.functions.get(callee.0 as usize).is_some_and(|row|row.foreign.is_some()){return Err(bad("ordinary Call substitutes for an original foreign execution"));}
                        result
                    },
                    InstructionKind::CallDynSlot{..}=>self.actual_dyn_call(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?,
                    InstructionKind::BorrowDynOwned{..}|InstructionKind::MakeDynBorrow{..}=>Some(self.actual_dyn_borrow(module,f,instruction,&state,&values,scope,&environment,active,b)?),
                    InstructionKind::DropMutexGuard{..}=>{self.drop_mutex_guard(module,f,instruction,&mut state,b)?;None},
                    InstructionKind::DropClosure{local}=>{
                        crate::closures::check_resource_instruction(module,f,instruction,b)?;
                        if let Some(value)=state.locals.get(local.0 as usize).and_then(Option::as_ref){
                            if f.locals.get(local.0 as usize).is_none_or(|row|row.ty!=value.ty)||!matches!(value.ty,Type::Closure(_)){return Err(bad("closure cleanup changes its complete initialized original local"));}
                            dynamic_witness::intrinsic_domain(module,&value.ty,b)?;
                        }
                        state.locals[local.0 as usize]=None;None
                    },
                    InstructionKind::DropDynOwned{local}=>{
                        crate::dynamic_dispatch::instruction(module,f,instruction,b)?;
                        let actual=f.locals.get(local.0 as usize).ok_or_else(||bad("erased cleanup has no exact actual local"))?;
                        if !matches!(actual.ty,Type::OwnedDyn{..}){return Err(bad("erased cleanup has another complete owner kind"));}
                        if let Some(owner)=state.locals.get(local.0 as usize).and_then(Option::as_ref){if owner.ty!=actual.ty{return Err(bad("erased cleanup changes its complete initialized original owner"));}}
                        dynamic_witness::structural_actual(module,&actual.ty,DynStructuralBound::SyncDrop,b)?;
                        // Conditional cleanup of an already transferred slot
                        // is empty. Actual ownership/debt/loan kernels still
                        // prove every live owner is finalized exactly once.
                        state.locals[local.0 as usize]=None;None
                    },
                    InstructionKind::FunctionRef{..}=>Some(self.function_pointer(module,instance,f,instruction,&environment,external,active,b)?),
                    InstructionKind::ForeignFunctionRef{..}=>self.actual_intrinsic(module,instance,f,instruction,&sites,&values,&environment,external,active,b)?,
                    InstructionKind::MakeClosure{closure,captures}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::Closure{..}));
                        let site=declarations.next().ok_or_else(||bad("actual Closure creation has no original semantic site"))?;
                        if declarations.next().is_some() {return Err(bad("actual Closure creation has duplicate semantic claims"));}
                        Some(self.seal_closure(module,instance,site,*closure,captures,&values,&environment,external,active,b)?)
                    }
                    InstructionKind::CallIndirect{..}|InstructionKind::CallClosure{..}|InstructionKind::CallClosureOnce{..}|InstructionKind::CallClosureBorrowed{..}|InstructionKind::CallClosureShared{..}|InstructionKind::CallClosureMut{..}=>{
                        let mut declarations=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::CallableCall{..}|SemanticSiteAction::BodyCall{..}));
                        let site=declarations.next().ok_or_else(||bad("actual CallableCall has no original callee recipe"))?;
                        if declarations.next().is_some(){return Err(bad("actual CallableCall has duplicate original claims"));}
                        self.scalar_callable(module,instance,site,&instruction.kind,&state,&values,&environment,external,active,b)?
                    }
                    InstructionKind::ScopeEnter{..}|InstructionKind::ScopeExit{..}|InstructionKind::UnsafeEnter{..}|InstructionKind::UnsafeExit{..}=>None,
                    InstructionKind::Unary{operand,..}=>{let value=value_at(&values,*operand)?;if !value.ty.is_b0_scalar() {return Err(bad("unary operation changes a semantic carrier"));}Some(atomic(f.values.get(instruction.result.ok_or_else(||bad("unary result is absent"))?.0 as usize).ok_or_else(||bad("unary result type is absent"))?,b)?)},
                    InstructionKind::Binary{left,right,..}=>{if !value_at(&values,*left)?.ty.is_b0_scalar()||!value_at(&values,*right)?.ty.is_b0_scalar() {return Err(bad("binary operation changes a semantic carrier"));}Some(atomic(f.values.get(instruction.result.ok_or_else(||bad("binary result is absent"))?.0 as usize).ok_or_else(||bad("binary result type is absent"))?,b)?)},
                    _=>return Err(unsupported("actual instruction lacks complete independent semantic transfer",Some(instruction.span))),
                };
                if let Some(id)=instruction.result {
                    let value=produced.ok_or_else(||bad("actual result has no independently reconstructed semantic value"))?;
                    if f.values.get(id.0 as usize)!=Some(&value.ty) {return Err(bad("actual SSA complete type differs from its original semantic value"));}
                    if let Some(old)=values.get(id.0 as usize).and_then(Option::as_ref) {if !self.same_type(&environment,old,&value,b)? {return Err(bad("actual SSA definition changes rigid classes across CFG visits"));}}
                    values[id.0 as usize]=Some(value.clone());self.record(scope,OperandKey::Value(id),&value,b)?;
                    self.record_address_validity(module,&value,&environment,b)?;
                }
                for site in &sites {
                    if matches!(site.action,SemanticSiteAction::TypeUse{..})&&!input_uses.contains(&site.recipe) {
                        let mut site_environment=environment.clone();
                        if let Some(at)=&mut site_environment.at{at.after=matches!(instruction.kind,InstructionKind::MakeClosure{..})&&matches!(site.at,SemanticSiteAddress::Typed{edge:SemanticSiteEdge::After,..});}
                        self.check_type_use(module,instance,f,site,&state,&values,signature,&site_environment,external,active,b)?;
                    }
                }
            }
            let successors=match &block.terminator {
                Terminator::Return(Some(value))=>{if !self.compatible(environment,value_at(&values,*value)?,&signature.result,b)? {return Err(bad("actual Return violates the original rigid semantic result"));}vec![]},
                Terminator::Return(None)=>{if signature.result.ty!=Type::Unit {return Err(bad("semantic non-Unit function has a Unit Return"));}vec![]},
                Terminator::Jump(target)=>vec![*target],Terminator::Branch{condition,then_block,else_block}=>{if value_at(&values,*condition)?.ty!=Type::Bool {return Err(bad("actual semantic CFG condition is not Bool"));}vec![*then_block,*else_block]},
                Terminator::Trap=>vec![],Terminator::Unreachable=>return Err(bad("reachable semantic Unreachable terminator")),
            };
            for target in successors {
                let target=target.0 as usize;
                if target>=states.len() {return Err(bad("actual semantic successor is absent"));}
                let changed=if let Some(old)=&mut states[target] {
                    let mut changed=false;
                    for (old,new) in old.locals.iter_mut().zip(&state.locals) {
                        b.spend(1)?;
                        match (&*old,new) {
                            (Some(left),Some(right))=>if !self.same_type(environment,left,right,b)? {return Err(bad("CFG join merges incompatible original rigid classes"));},
                            (Some(_),None)=>{*old=None;changed=true;},_=>{},
                        }
                    }
                    changed
                }else{states[target]=Some(state.clone());true};
                if changed {queue.push_back(target);}
            }
        }
        Ok(())
    }
    fn seal_closure(&self,module:&TypedModule,creator:&SemanticInstance,site:&SemanticSite,closure:ClosureId,captures:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType> {
        let creator_definition=self.definition(module,creator.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&creator_definition.body else{return Err(bad("actual Closure creator has no original body"));};
        let recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("actual Closure has no original creation occurrence"))?;
        let SemanticSiteRecipeKind::Closure{definition,environment:recipes}=&recipe.kind else{return Err(bad("actual Closure has a different original emission role"));};
        let child_definition=self.definition(module,*definition,b)?;
        if child_definition.parent!=Some(creator.definition){return Err(bad("Closure source definition has another original parent"));}
        let SemanticSiteAction::Closure{closure:SemanticClosureTarget::Typed(actual),environment:claims,..}=&site.action else{return Err(bad("actual Closure retains a Core creation claim"));};
        if *actual!=closure||recipes.len()!=child_definition.environment.len()||claims.len()!=recipes.len() {return Err(bad("Closure environment omits an original free variable or chooses another definition"));}
        let mut definitions=module.closures.iter().filter(|row|row.id==closure);
        let actual_definition=definitions.next().ok_or_else(||bad("actual Closure definition is absent"))?;
        if definitions.next().is_some()||actual_definition.captures.len()!=captures.len(){return Err(bad("actual Closure has an ambiguous definition or incomplete physical captures"));}
        for (capture,expected) in captures.iter().zip(&actual_definition.captures) {if value_at(values,*capture)?.ty!=*expected {return Err(bad("actual capture whole type differs from its real Closure definition"));}}
        let program=module.semantic.as_ref().ok_or_else(||bad("semantic programme is absent"))?;
        let mut children=program.instances.iter().filter(|row|row.definition==*definition&&row.parent==Some(creator.id)&&matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) if function==actual_definition.function)&&row.environment.as_ref().is_some_and(|env|matches!(env.closure,SemanticClosureTarget::Typed(id) if id==closure)));
        let child=children.next().ok_or_else(||bad("actual Closure has no exact child/creator semantic instance"))?;
        if children.next().is_some(){return Err(bad("actual Closure child instance is ambiguous"));}
        let actual_environment=child.environment.as_ref().ok_or_else(||bad("actual Closure has no sealed environment claim"))?;
        let mut checked=Vec::new();
        for (index,((recipe,formal),claim)) in recipes.iter().zip(&child_definition.environment).zip(claims).enumerate() {
            let expected=self.evaluate_actual(module,&formal.source,environment,external,active,b)?;
            let proposed=self.evaluate_actual(module,recipe,environment,external,active,b)?;
            if !self.actual_same(environment,&expected,&proposed,b)?{return Err(bad("creation environment recipe differs from the original parent-scoped free variable"));}
            self.actual_claim(module,claim,&expected,environment,b)?;
            self.actual_claim(module,actual_environment.actuals.get(index).ok_or_else(||bad("child environment actual is absent"))?,&expected,environment,b)?;
            match (&formal.kind,&expected) {
                (SemanticFormal::Lifetime,CheckedActual::Life(_))=>{},
                (SemanticFormal::Type{requirements},CheckedActual::Type(_))=>self.evaluate_requirements(module,requirements,environment,RequirementMode::ActualObligation,external,active,b)?,
                (SemanticFormal::Const{carrier},CheckedActual::Const{carrier:actual,..})=>{let expected=self.evaluate_type(module,carrier,environment,external,active,b)?;if !self.same_type(environment,actual,&expected,b)?{return Err(bad("sealed Const environment has a different original parent-scoped carrier"));}},
                _=>return Err(bad("sealed environment has a different original formal kind")),
            }
            checked.push(expected);
        }
        if let Some(previous)=self.seals.borrow().get(&child.id) {for (left,right) in previous.iter().zip(&checked){if !self.actual_same(environment,left,right,b)?{return Err(bad("different actual creation sites do not seal the same child environment"));}}}
        b.spend(checked.len())?;
        // Logical source selectors keep occurrence identities even when their
        // constructor images are equal. No phantom occurrence creates data.
        let mut anchors=Vec::<(SemanticClassExpr,ClassKey)>::new();
        for(ordinal,value)in checked.iter().enumerate(){match value{
            CheckedActual::Life(key)=>anchors.push((SemanticClassExpr::Environment{scope:SemanticScopeRef::Ancestor(0),ordinal:ordinal as u32},key.clone())),
            CheckedActual::Type(value)if !value.classes.is_empty()=>{ghost_captures::complete_environment(module,value,b)?;for(class,key)in value.classes.iter().enumerate(){b.spend(1)?;anchors.push((SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal as u32),path:vec![],class:class as u32},key.clone()));}},
            _=>{},
        }}
        for value in &checked{b.spend(1)?;match value{CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>{value.charge_clone(b)?;b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;},CheckedActual::Life(_)=>{}}}
        let sealed=checked.clone();
        self.seals.borrow_mut().insert(child.id,checked);
        let child_environment=self.environment_for_instance(module,child.id,active,b)?;
        let child_scope=ScopeKey::Instance(child.id);
        let mut capture_images=Vec::new();let mut logical_captures=BTreeMap::new();let mut borrowed_captures=BTreeMap::new();
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&creator_definition.body else{unreachable!()};
        let node=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("Closure creator has no exact original node"))?;
        let SemanticOriginalNode::Expression(creation_node)=node.node else{return Err(bad("Closure creator original node is not an expression"));};
        for (index,capture) in captures.iter().enumerate() {
            let mut value=value_at(values,*capture)?.clone();
            if let Some(head)=self.capture_implicit(module,child,index as u32,b)? {
                let ClassKey::Implicit{ordinal,..}=&head else{return Err(bad("capture head does not select an original implicit occurrence"));};
                self.capture_local_source(module,creator,child,*ordinal,*capture,b)?;
                if !matches!(&value.ty,Type::Address{..}){return Err(bad("implicit captured head has another complete borrow carrier"));}
                let pointee=self.child_value(module,&value,&[LifeTypeProjection::Pointee],b)?;
                let ordinal=self.environment_capture(module,creator,child,creation_node,*capture,&pointee,&sealed,environment,b)?;
                borrowed_captures.insert(index as u32,ordinal);
                let slot=value.life.slots.iter().find(|row|row.path.is_empty()).ok_or_else(||bad("actual implicit capture has no variable head occurrence"))?;
                let binder=slot.binder as usize;
                if binder>=value.classes.len()||binder>=value.origins.len(){return Err(bad("implicit capture head class is absent"));}
                // A constructor-bound existential class names this real head.
                // The input SSA/loan is unchanged; no global equality equates
                // the creator activation with the child's persistent domain.
                let ClassKey::Implicit{ordinal,..}=&head else{unreachable!()};
                anchors.push((SemanticClassExpr::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal:*ordinal},value.classes[binder].clone()));
                capture_images.push((head.clone(),value.classes[binder].clone()));
                value.classes[binder]=head.clone();value.origins[binder]=IntrinsicOrigin::DerivedClass{source:head};
            }else{
                let ordinal=self.environment_capture(module,creator,child,creation_node,*capture,&value,&sealed,environment,b)?;
                logical_captures.insert(index as u32,ordinal);
            }
            self.record(child_scope,OperandKey::Capture(index as u32),&value,b)?;
        }
        let mut constructor_environment=child_environment.clone();
        constructor_environment.assumptions=environment.assumptions.clone();
        constructor_environment.images.extend(capture_images.iter().cloned());
        self.evaluate_requirements(module,&child_definition.requirements,&constructor_environment,RequirementMode::ActualObligation,external,active,b)?;
        for formal in &child_definition.parameters {if let SemanticFormal::Type{requirements}=formal {self.evaluate_requirements(module,requirements,&constructor_environment,RequirementMode::ActualObligation,external,active,b)?;}}
        let relation=actual_definition.storage_life.as_ref().ok_or_else(||bad("Closure complete Fixed/Invocation relation is Unknown"))?;
        let entry=module.functions.get(actual_definition.function.0 as usize).ok_or_else(||bad("Closure actual entry is absent"))?;
        if entry.environment!=Some(closure)||entry.storage_life.as_ref()!=Some(relation) {return Err(bad("Closure and actual entry identity/complete header differ"));}
        signature_life::charge(relation,b)?;
        let mut classes=Vec::new();let mut origins=Vec::new();let mut statics=Vec::new();
        let mut anchored=Vec::new();
        for class in 0..relation.fixed_binder_count {
            let mut rows=child.header_embedding.iter().filter(|row|row.header==StorageBinder::Fixed(class));
            let row=rows.next().ok_or_else(||bad("Closure has an unbound actual free Fixed class"))?;
            if rows.next().is_some(){return Err(bad("Closure Fixed occurrence is ambiguously embedded"));}
            self.class(module,&row.actual,&child_environment,b)?;
            b.spend(anchors.len().saturating_add(anchored.len()))?;
            let mut sources=anchors.iter().filter(|(source,_)|source==&row.actual);
            let (_,image)=sources.next().ok_or_else(||unsupported("Closure free Fixed class has no independently checked original capture/ghost source",None))?;
            if sources.next().is_some()||anchored.contains(&row.actual){return Err(bad("Closure free class has conflicting or repeated original source anchors"));}
            anchored.push(row.actual.clone());
            // This is a local constructor-boundary image, never global Γ.
            let key=image.clone();
            let SemanticSiteAction::Closure{result:SemanticOperand::Value(result),..}=&site.action else {return Err(bad("actual Closure result is not its defining SSA value"));};
            let creator_scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
            let origin=IntrinsicOrigin::Existing{scope:creator_scope,operand:OperandKey::Value(*result),path:vec![],class};
            let canonical=IntrinsicOrigin::Existing{scope:ScopeKey::Instance(child.id),operand:OperandKey::Receiver,path:vec![],class};
            b.spend(1)?;self.origin_aliases.borrow_mut().insert(origin.clone(),canonical.clone());
            origins.push(canonical.clone());classes.push(key);
        }
        // Every original occurrence remains present. Several occurrences can
        // share one newly constructed role only when their original terms are
        // equal including intrinsic identity, never just their outside image.
        for(source,_)in &anchors{
            let mut matches=0usize;
            for representative in &anchored{
                if self.same_capture_anchor(source,representative,&sealed,environment,b)?{matches+=1;}
            }
            if matches!=1{return Err(bad("Closure intrinsic domain omits or duplicates an original capture/ghost source term"));}
        }
        for (index,_) in captures.iter().enumerate() {
            if let Some(ordinal)=logical_captures.get(&(index as u32)){
                let CheckedActual::Type(value)=&sealed[*ordinal]else{unreachable!()};
                self.environment_capture_header(module,relation,child,index as u32,*ordinal,value,&sealed,environment,b)?;
                continue;
            }
            if value_at(values,captures[index])?.classes.is_empty(){continue;}
            let head=self.capture_implicit(module,child,index as u32,b)?.ok_or_else(||bad("actual capture loses its original implicit head"))?;
            let fixed=relation.bindings.iter().filter(|row|row.position.base==StorageBase::Capture(index as u32)&&row.position.path.is_empty()).collect::<Vec<_>>();
            if fixed.len()!=1{return Err(bad("Closure actual captured head lacks its unique complete header occurrence"));}
            let StorageBinder::Fixed(class)=fixed[0].binder else{return Err(bad("persistent captured head is not in the Closure Fixed domain"));};
            let row=child.header_embedding.iter().find(|row|row.header==StorageBinder::Fixed(class)).ok_or_else(||bad("actual captured head loses its original Fixed embedding"))?;
            if self.class(module,&row.actual,&child_environment,b)?!=head{return Err(bad("Closure captured head maps to another original class occurrence"));}
            if let Some(ordinal)=borrowed_captures.get(&(index as u32)){
                let CheckedActual::Type(pointee)=&sealed[*ordinal]else{unreachable!()};
                self.borrowed_capture_header(relation,child,index as u32,*ordinal,pointee,&sealed,environment,b)?;
            }
        }
        for position in &relation.static_bindings {
            let leaf=match position.base {
                StorageBase::Capture(index)=>actual_definition.captures.get(index as usize),
                StorageBase::Parameter(index)=>actual_definition.parameters.get(index as usize),
                StorageBase::Result=>Some(&actual_definition.result),StorageBase::Receiver=>None,
            }.ok_or_else(||bad("opaque Static occurrence selects an absent actual leaf"))?;
            let leaf=super::closures::static_leaf(module,leaf,&position.path,b)?;
            let mode=match leaf {Type::Address{mutable:false,..}=>StoredBorrowMode::SharedAddress,Type::Desc(_)=>StoredBorrowMode::ReadOnlyDescription,_=>return Err(bad("opaque Static occurrence is not a complete shared borrow leaf"))};
            dynamic_metadata::ty(leaf,b)?;b.spend(position.path.len().saturating_add(1))?;
            statics.push(StaticOccurrence::Logical{closure,position:position.clone(),leaf:leaf.clone(),mode});
        }
        let mut life=empty_life();life.binder_count=classes.len() as u32;
        let value=CheckedType{type_cell: crate::semantic::CheckedTypeData::new(Type::Closure(closure)),life,classes,origins,statics};
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&creator_definition.body else{unreachable!()};
        let node=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("Closure creator has no exact original node"))?;
        let SemanticOriginalNode::Expression(node)=node.node else{return Err(bad("Closure creator original node is not an expression"));};
        let at=environment.at.ok_or_else(||bad("Closure creator has no actual CFG point"))?;
        let creator_scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let mut boundaries=self.closure_boundaries.borrow_mut();
        b.spend(boundaries.len().saturating_add(value.classes.len()))?;
        if let Some(old)=boundaries.iter().find(|row|row.creator==creator_scope&&row.node==node&&row.at==at) {
            if old.child!=child.id||old.value!=value{return Err(bad("actual Closure creation changes its complete intrinsic domain or outside image"));}
        }else{boundaries.push(ClosureBoundary{creator:creator_scope,node,at,child:child.id,value:value.clone()});}
        Ok(value)
    }
    fn actual_same(&self,environment:&EvalEnv,left:&CheckedActual,right:&CheckedActual,b:&mut Budget)->Check<bool> {
        match (left,right) {
            (CheckedActual::Life(left),CheckedActual::Life(right))=>self.same(environment,left,right,b),
            (CheckedActual::Type(left),CheckedActual::Type(right))=>self.same_type(environment,left,right,b),
            (CheckedActual::Const{carrier:left,value:a},CheckedActual::Const{carrier:right,value:c})=>Ok(a==c&&self.same_type(environment,left,right,b)?),_=>Ok(false),
        }
    }
    fn actual_claim(&self,module:&TypedModule,claim:&SemanticActual,expected:&CheckedActual,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let valid=match (claim,expected) {
            (SemanticActual::Life(actual),CheckedActual::Life(expected))=>self.same(environment,&self.class(module,actual,environment,b)?,expected,b)?,
            (SemanticActual::Type(actual),CheckedActual::Type(expected))=>{self.claim(module,actual,expected,environment,b)?;true},
            (SemanticActual::Const{ty,value},CheckedActual::Const{carrier,value:expected})=>{self.claim(module,ty,carrier,environment,b)?;value==expected},_=>false,
        };
        if !valid{return Err(bad("actual sealed environment differs from the independently interpreted creator source"));}Ok(())
    }
    fn header(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,signature:&CheckedSignature,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let Some(header)=f.storage_life.as_ref() else {
            if instance.header_embedding.is_empty()&&self.zero_header_function(module,f,b)?&&signature.result.classes.is_empty()&&self.zero_header_type(module,&signature.result.ty,b)?{
                let mut zero=true;for(value,_)in &signature.parameters{if !value.classes.is_empty()||!self.zero_header_type(module,&value.ty,b)?{zero=false;break;}}
                if zero{
                    for value in signature.parameters.iter().map(|(value,_)|value).chain(std::iter::once(&signature.result)){
                        if !task_domains::defer_zero_header_statics(module,value,b)?{return Err(bad("zero-variable semantic header has an unproved direct Static occurrence"));}
                    }
                    return Ok(());
                }
            }
            return Err(bad("actual semantic header is Unknown"));
        };
        signature_life::charge(header,b)?;
        let mut images=Vec::new();
        for row in &instance.header_embedding {images.push((row.header,self.class(module,&row.actual,environment,b)?));}
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let base=|base:StorageBase|->Check<CheckedType>{
            match base {
                StorageBase::Parameter(index)=>signature.parameters.get(index as usize).map(|(value,_)|value.clone()).ok_or_else(||bad("header selects an absent original parameter")),
                StorageBase::Result=>Ok(signature.result.clone()),
                StorageBase::Capture(index)=>self.occurrences.borrow().get(&(scope,OperandKey::Capture(index))).cloned().ok_or_else(||bad("header Capture has no actual sealed creator value")),
                StorageBase::Receiver=>self.occurrences.borrow().get(&(scope,OperandKey::Receiver)).cloned().ok_or_else(||unsupported("actual receiver header requires its complete independent borrowed head scope",None)),
            }
        };
        for row in &header.bindings {
            let value=base(row.position.base)?;
            let binder=if row.position.path.iter().any(|part|!matches!(part,ContractProjection::Place(_))){
                // Opaque Task/closure/Context ports are logical selectors, not
                // physical Place paths. The existing complete declaration
                // walker maps their original child ordinals injectively.
                task_domains::raw_input_port(module,&value.ty,&value.life,&row.position.path,b)?.2
            }else{
                let path=row.position.path.iter().map(|part|match part{ContractProjection::Place(step)=>Ok(*step),_=>unreachable!()}).collect::<Check<Vec<_>>>()?;
                let mut slots=value.life.slots.iter().filter(|slot|slot.path==path);
                let slot=slots.next().ok_or_else(||bad("actual header invents a variable borrow occurrence absent from its original signature"))?;
                if slots.next().is_some(){return Err(bad("original borrow occurrence is ambiguously mapped"));}slot.binder
            };
            let expected=value.classes.get(binder as usize).ok_or_else(||bad("original borrow occurrence has no semantic class"))?;
            let actual=&images.iter().find(|(binder,_)|*binder==row.binder).ok_or_else(||bad("actual header variable has no complete lexical embedding"))?.1;
            if !self.same(environment,actual,expected,b)? {return Err(unsupported("actual header borrow role requires a proved original per-occurrence lifetime adaptation",None));}
        }
        for row in &header.classes {
            let value=base(row.position.base)?;
            let class=if row.position.path.is_empty(){row.position.class}else{
                let mut rows=value.life.retained.iter().filter(|retained|retained.path==row.position.path&&retained.class==row.position.class);
                let row=rows.next().ok_or_else(||bad("actual header class has no exact original child occurrence"))?;
                if rows.next().is_some(){return Err(bad("original child class has duplicate maps"));}row.binder
            };
            let expected=value.classes.get(class as usize).ok_or_else(||bad("actual header class escapes the complete original domain"))?;
            let actual=&images.iter().find(|(binder,_)|*binder==row.binder).ok_or_else(||bad("header class has no exact lexical embedding"))?.1;
            if !self.same(environment,actual,expected,b)? {return Err(unsupported("actual header class needs its original occurrence-preserving rigid/partition map",None));}
        }
        for row in &header.static_bindings {
            let value=base(row.base)?;
            let path=row.path.iter().map(|part|match part {ContractProjection::Place(step)=>Ok(LifeTypeProjection::Place(*step)),_=>Err(unsupported("header Static logical occurrence needs the complete checked source directory",None))}).collect::<Check<Vec<_>>>()?;
            if !value.statics.iter().any(|occurrence|matches!(occurrence,StaticOccurrence::Direct{path:actual,..} if *actual==path)||matches!(occurrence,StaticOccurrence::Logical{closure:_,position,..} if position==row)) {return Err(bad("actual header erases an original variable class into a Static occurrence"));}
        }
        // The ordinary signature kernel subsequently checks exact full typed
        // coverage; here every original root class must also remain explicit.
        for (base_kind,value) in signature.parameters.iter().enumerate().map(|(i,(value,_))|(StorageBase::Parameter(i as u32),value)).chain(std::iter::once((StorageBase::Result,&signature.result))) {
            if matches!(value.ty,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}) {
                for class in 0..value.classes.len() as u32 {
                    b.spend(header.classes.len())?;
                    if !header.classes.iter().any(|row|row.position.base==base_kind&&row.position.path.is_empty()&&row.position.class==class) {return Err(bad("actual header omits an original nominal/opaque class occurrence"));}
                }
            }
        }
        Ok(())
    }
    fn scalar_callable(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,opcode:&InstructionKind,state:&State,values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>> {
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("callable caller has no original body"));};
        let recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("CallableCall has no original occurrence"))?;
        let expected=match &recipe.kind{
            SemanticSiteRecipeKind::CallableCall{callee}=>self.evaluate_type(module,callee,environment,external,active,b)?,
            SemanticSiteRecipeKind::OriginalCall=>{
                let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("actual callable original body absent"));};
                let node=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("original callable site has no exact source node"))?;
                let SemanticOriginalNode::Expression(node)=node.node else{return Err(bad("original callable site is not an expression"));};
                let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("original callable body has no symbolic proof"))?;
                let source=original.callable_source(module,caller.definition,node,b)?;
                self.evaluate_type(module,&SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(source)},environment,external,active,b)?
            },
            _=>return Err(bad("CallableCall has a different original role")),
        };
        if matches!(&site.action,SemanticSiteAction::BodyCall{actuals}if !actuals.is_empty()){return Err(bad("nongeneric original callable invocation adds actual arguments"));}
        let (actual,arguments)=match opcode {
            InstructionKind::CallIndirect{callee,arguments}|InstructionKind::CallClosure{callee,arguments}|InstructionKind::CallClosureOnce{callee,arguments}|InstructionKind::CallClosureBorrowed{callee,arguments,..}=>(value_at(values,*callee)?,arguments),
            InstructionKind::CallClosureShared{callee,arguments}|InstructionKind::CallClosureMut{callee,arguments}=>(state.locals.get(callee.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual callable local is not initialized"))?,arguments),_=>return Err(bad("actual opcode is not a closure invocation")),
        };
        let invocation_borrow;
        let original_value=if matches!(opcode,InstructionKind::CallClosureBorrowed{..})
            &&matches!(expected.ty,Type::Closure(_))&&matches!(actual.ty,Type::Address{..}){
            let InstructionKind::CallClosureBorrowed{callee,..}=opcode else{unreachable!()};
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("borrowed callable has no actual original caller entry"));};
            let f=module.functions.get(function.0 as usize).filter(|f|f.id==function).ok_or_else(||bad("borrowed callable actual caller entry is absent"))?;
            let node=self.original_callable_source(module,caller,site,b)?;
            self.original_projection_source(module,caller,f,*callee,caller.definition,node,true,b)?;
            invocation_borrow=self.child_value(module,actual,&[LifeTypeProjection::Pointee],b)?;&invocation_borrow
        }else{actual};
        if !self.same_type(environment,original_value,&expected,b)?{return Err(bad("actual callee occupant differs from its original full callable type").with_note(format!("original {:?}, site {:?}; actual {:?} domain {}, expected {:?} domain {}",caller.definition,site.recipe,original_value.ty,original_value.life.binder_count,expected.ty,expected.life.binder_count)));}
        if matches!(opcode,InstructionKind::CallIndirect{..}){return self.indirect_call(module,caller,site,actual,arguments,values,environment,external,active,b);}
        let id=match &actual.ty {Type::Closure(id)=>*id,Type::Address{inner,..}=>match inner.as_ref(){Type::Closure(id)=>*id,_=>return Err(bad("borrowed callee does not address a concrete Closure"))},_=>return Err(unsupported("actual callable requires its full code/trait dispatch proof",None))};
        b.spend(module.closures.len())?;
        let mut definitions=module.closures.iter().filter(|row|row.id==id);
        let definition=definitions.next().ok_or_else(||bad("actual callable definition is absent"))?;
        if definitions.next().is_some()||definition.parameters.len()!=arguments.len(){return Err(bad("actual callable identity or arity differs"));}
        let receiver=if matches!(actual.ty,Type::Address{..}){self.child_value(module,actual,&[LifeTypeProjection::Pointee],b)?}else{actual.clone()};
        self.closure_call(module,caller,site,id,&receiver,arguments,values,environment,external,active,b)

    }
    fn actual_call(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,callee:FunctionId,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>> {
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("semantic caller has no original body"));};
        let site_recipe=sites.iter().find(|row|row.id==site.recipe).ok_or_else(||bad("actual Call has no original recipe"))?;
        if !matches!(site_recipe.kind,SemanticSiteRecipeKind::Call{..}|SemanticSiteRecipeKind::OriginalCall){return Err(bad("actual Call has another original operation role"));}
        let member=self.original.borrow().as_ref().ok_or_else(||bad("original member Call has no independent symbolic proof"))?.trait_call_recipes(module,caller.definition,site.recipe,environment,b)?;
        if let Some(member)=member{return self.actual_trait_member(module,caller,site,callee,arguments,values,environment,member,external,active,b);}
        let (resolved_target,actuals)=self.original.borrow().as_ref().ok_or_else(||bad("original body Call has no independent symbolic proof"))?.call_recipes(module,caller.definition,site.recipe,environment,b)?;
        let target=&resolved_target;
        let target_definition=self.definition(module,*target,b)?;
        let claims=match &site.action{
            SemanticSiteAction::Call{target:SemanticFunctionTarget::Typed(actual_target),actuals} if *actual_target==callee=>actuals,
            SemanticSiteAction::BodyCall{actuals}=>actuals,
            _=>return Err(bad("actual Call retains another target or Core specialization")),
        };
        if actuals.len()!=target_definition.parameters.len()||claims.len()!=actuals.len(){return Err(bad("actual Call changes the independently resolved target/full actual order"));}
        let mut checked=Vec::new();
        for (ordinal,(recipe,claim)) in actuals.iter().zip(claims).enumerate() {
            let sources=self.original.borrow().as_ref().ok_or_else(||bad("Call inference has no checked original body"))?.call_type_sources(caller.definition,site.recipe,ordinal as u32,b)?;
            let expected=if sources.is_empty(){self.evaluate_actual(module,recipe,environment,external,active,b)?}else{
                if !matches!(recipe,SemanticActualRecipe::Type(_)){return Err(bad("inferred whole Type source changed its complete actual kind"));}
                CheckedActual::Type(self.inferred_call_type(module,&sources,environment,external,active,b)?.ok_or_else(||bad("inferred whole Type has no original source occurrence"))?)
            };
            let equal=match (&expected,claim) {
                (CheckedActual::Type(expected),SemanticActual::Type(claim))=>{self.claim(module,claim,expected,environment,b)?;true},
                (CheckedActual::Const{carrier,value},SemanticActual::Const{ty,value:actual})=>{self.claim(module,ty,carrier,environment,b)?;value==actual},
                (CheckedActual::Life(expected),SemanticActual::Life(actual))=>self.same(environment,expected,&self.class(module,actual,environment,b)?,b)?,
                _=>false,
            };
            if !equal {return Err(bad("actual Call actual differs from its original symbolic inference result"));}
            checked.push(expected);
        }
        let token=active.fresh(b)?;
        let key=ScopeKey::Witness{definition:*target,frame:token};
        let mut lexical=Vec::new();
        if let Some(parent)=target_definition.parent {
            let position=environment.lexical.iter().position(|frame|frame.definition==parent).ok_or_else(||unsupported("Call target lexical parent is not available in the actual caller context",None))?;
            lexical.extend_from_slice(&environment.lexical[position..]);
        }
        lexical.insert(0,LexicalFrame{definition:*target,key,actuals:checked,environment:vec![],self_type:None});
        if !target_definition.environment.is_empty() {return Err(unsupported("ordinary Call cannot manufacture a closure's sealed ghost environment",None));}
        let call_environment=EvalEnv{lexical,assumptions:environment.assumptions.clone(),images:environment.images.clone(),bounds:environment.bounds.clone(),invocations:vec![],at:environment.at};
        let worlds=self.call_worlds(module,callee,*target,&call_environment,environment,active,b)?;
        self.evaluate_requirements(module,&target_definition.requirements,&call_environment,RequirementMode::ActualObligation,external,active,b)?;
        for formal in &target_definition.parameters {if let SemanticFormal::Type{requirements}=formal {self.evaluate_requirements(module,requirements,&call_environment,RequirementMode::ActualObligation,external,active,b)?;}}
        let SemanticDefinitionBody::FunctionWithBody{signature,..}=&target_definition.body else {return Err(bad("actual Call target is not an original function"));};
        let mut signature=self.world_signature(module,signature,worlds,environment,external,active,b)?;
        if signature.parameters.len()!=arguments.len() {return Err(bad("actual Call operand count differs from its original signature"));}
        b.spend(arguments.len())?;
        let(source_inputs,source_checks)=self.original_call_inputs(module,caller,site,&signature,key,arguments,values,environment,external,active,b)?;
        let inputs=source_inputs.iter().collect::<Vec<_>>();
        self.instantiate_original_call_heads(module,caller,site,&mut signature,&inputs,key,environment,b)?;
        let actual_function=module.functions.get(callee.0 as usize).ok_or_else(||bad("actual Call function is absent"))?;
        for (index,((expected,_),argument)) in signature.parameters.iter().zip(arguments).enumerate() {
            if !self.call_argument(module,caller,*argument,values,expected,environment,source_checks[index],b)? {
                let actual=value_at(values,*argument)?;
                return Err(bad("actual Call argument violates the original invariant class specialization")
                    .with_note(format!("caller {:?}, original {:?}, call {:?}, callee {:?}, parameter {index}, actual SSA {:?}, original source checked {}; actual/expected N {}/{}, retained {}/{}, nominal {}/{}",caller.target,caller.definition,site.recipe,callee,argument,source_checks[index],actual.life.binder_count,expected.life.binder_count,actual.life.retained.len(),expected.life.retained.len(),actual.life.nominal_arguments.len(),expected.life.nominal_arguments.len()))
                    .with_note(format!("actual class/origin prefix {:?} / {:?}; expected {:?} / {:?}",&actual.classes[..actual.classes.len().min(8)],&actual.origins[..actual.origins.len().min(8)],&expected.classes[..expected.classes.len().min(8)],&expected.origins[..expected.origins.len().min(8)])));
            }
        }
        self.check_call_input_wf(module,&signature,arguments,values,environment,b)?;
        if actual_function.return_type!=signature.result.ty {return Err(bad("actual Call specialization result whole type differs"));}
        for(index,((expected,_),argument))in signature.parameters.iter().zip(arguments).enumerate(){
            if self.original_local_call_window(caller,expected,environment,b)?{self.record_local_role_call(module,caller,u32::try_from(index).map_err(|_|limit("Call local view parameter exceeds its domain"))?,*argument,expected,environment,b)?;}
        }
        Ok(if signature.result.ty==Type::Unit {None}else{Some(signature.result)})
    }
    #[allow(clippy::too_many_arguments)]
    fn check_type_use(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,site:&SemanticSite,state:&State,values:&[Option<CheckedType>],signature:&CheckedSignature,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let SemanticSiteAction::TypeUse{subject,expected}=&site.action else{return Err(bad("TypeUse consumer selects another semantic action"));};
        let required=self.site_expected(module,instance,site,environment,external,active,b)?;
        let value=self.type_use_operand(module,instance,f,subject,&required,state,values,signature,environment,b)?;
        self.bind_implicit_use(module,instance,site,subject,&value,&required,environment,b)?;
        self.claim_type_use(module,expected,&required,environment,b)?;
        if !self.compatible(environment,&value,&required,b)?{
            b.spend(value.classes.len().saturating_add(required.classes.len()).saturating_add(value.life.retained.len()).saturating_add(required.life.retained.len()))?;
            return Err(bad(&format!("actual TypeUse violates its original rigid class requirement [instance {:?}, site {:?}, subject {:?}, actual domain {}, required domain {}]",instance.id,site.recipe,subject,value.life.binder_count,required.life.binder_count))
                .with_note(format!("actual complete shape {:?}; required complete shape {:?}",value.life,required.life))
                .with_note(format!("actual classes {:?}, origins {:?}; required classes {:?}, origins {:?}",value.classes,value.origins,required.classes,required.origins)));
        }Ok(())
    }
    fn bind_implicit_use(&self,module:&TypedModule,instance:&SemanticInstance,site:&SemanticSite,subject:&SemanticOperand,actual:&CheckedType,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let definition=self.definition(module,instance.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("implicit occurrence has no original function body"));};
        let recipe=sites.iter().find(|recipe|recipe.id==site.recipe).ok_or_else(||bad("implicit occurrence has no original site"))?;
        let SemanticSiteRecipeKind::TypeUse{expected:source_recipe}=&recipe.kind else{return Ok(());};
        let expanded;let mut source=None;
        let source_recipe=if let SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:original_source}=source_recipe{
            let original=self.original.borrow();let original=original.as_ref().ok_or_else(||bad("original implicit address lacks its complete body proof"))?;
            source=original.original_address_source(module,instance.definition,original_source,b)?;
            if source.is_none(){source=original.original_description_source(module,instance.definition,original_source,b)?;}
            expanded=original.type_recipe(module,instance.definition,original_source,environment,b)?;&expanded
        }else{source_recipe};
        let (ordinal,description)=match source_recipe{
            SemanticTypeRecipe::Address{lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal}),..}=>(*ordinal,false),
            SemanticTypeRecipe::Desc{lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal}),..}=>(*ordinal,true),
            _=>return Ok(()),
        };
        if description&&source.is_none(){return Ok(());}
        b.spend(definition.implicit_lifetimes.len())?;
        if !definition.implicit_lifetimes.iter().any(|row|row.ordinal==ordinal&&row.occurrence_path==recipe.occurrence_path){return Ok(());}
        let SemanticOperand::Value(mut value)=*subject else{return Ok(());};
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target else{return Err(bad("implicit occurrence belongs to a nonexecutable scope"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("implicit occurrence actual function is absent"))?;
        if let Some(source)=source{self.intrinsic_buffer_source(module,instance,f,value,source,b)?;}
        // A view can be stored in a real local environment and loaded before
        // its exact expression borrow. The complete original Place/source
        // proof above checks every actual alias/store and its sealed creator.
        // A real reborrow with that proof is already the actual occurrence;
        // do not lose it by demanding a second direct-SSA route to the root.
        // A bare Load still grants no fresh address occurrence identity.
        if !description {
        let mut seen=std::collections::BTreeSet::new();
        loop {
            b.spend(1)?;if !seen.insert(value){return Err(bad("implicit address producer cycle"));}
            let mut producer=None;
            for block in &f.blocks {b.spend(block.instructions.len())?;for instruction in &block.instructions {if instruction.result==Some(value) {if producer.is_some(){return Err(bad("implicit address has several actual SSA definitions"));}producer=Some(instruction);}}}
            let instruction=producer.ok_or_else(||bad("implicit address actual SSA definition is absent"))?;
            match &instruction.kind {
                InstructionKind::ReborrowAddress{..}|InstructionKind::AddressSlice{..}if source.is_some()=>break,
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressSlice{address,..}=>value=*address,
                InstructionKind::BorrowLocal{..}|InstructionKind::BorrowLocalMut{..}if !description=>break,
                InstructionKind::AddressField{..}|InstructionKind::AddressIndex{..}|InstructionKind::BorrowPackageField{..}|InstructionKind::BorrowCapture{..}if !description&&source.is_some()=>break,
                InstructionKind::DescribeLocal{..}|InstructionKind::DescribePackageField{..}|InstructionKind::DescribeCapture{..}|InstructionKind::DescribeAddress{..}if description=>break,
                _=>return Ok(()),
            }
        }
        }
        if actual.ty!=expected.ty{return Ok(());}
        let Some(a)=actual.life.slots.iter().find(|row|row.path.is_empty()) else{return Ok(());};
        let Some(e)=expected.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==a.mode) else{return Ok(());};
        let actual=actual.classes.get(a.binder as usize).ok_or_else(||bad("real implicit borrow has no outer class"))?;
        let expected=expected.classes.get(e.binder as usize).ok_or_else(||bad("original implicit borrow has no outer class"))?;
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let original=ClassKey::Implicit{scope:frame.key,ordinal};
        if expected!=&original {return Err(bad("implicit occurrence resolved to another original lifetime"));}
        // This original declaration names this exact actual borrow occurrence;
        // it is not equality inferred from a shared outside image or header.
        b.spend(1)?;
        b.spend(1)?;self.invalidate_closed_frames();
        self.premises.borrow_mut().entry(frame.key).or_default().relations.equalities.push((original,actual.clone()));
        Ok(())
    }
    fn site_expected(&self,module:&TypedModule,instance:&SemanticInstance,site:&SemanticSite,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType> {
        let definition=self.definition(module,instance.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&definition.body else{return Err(bad("semantic site has no original function"));};
        b.spend(sites.len())?;
        let recipe=sites.iter().find(|recipe|recipe.id==site.recipe).ok_or_else(||bad("semantic site has no original recipe"))?;
        let construction=matches!(recipe.kind,SemanticSiteRecipeKind::Construct{..});
        let recipe=match &recipe.kind {SemanticSiteRecipeKind::Construct{result}|SemanticSiteRecipeKind::DeclareLocal{expected:result}|SemanticSiteRecipeKind::TypeUse{expected:result}=>result,_=>return Err(bad("site role has no direct expected type recipe"))};
        if construction{if let SemanticTypeRecipe::OriginalType{scope,source}=recipe{
            let frame=self.frame(environment,*scope,b)?;
            let recipe=self.original.borrow().as_ref().ok_or_else(||bad("actual construction has no complete original payload proof"))?.construction_recipe(module,frame.definition,source,environment,b)?;
            return self.evaluate_type(module,&recipe,environment,external,active,b);
        }}
        self.evaluate_type(module,recipe,environment,external,active,b)
    }
    fn expression_origin(&self,expression:&SemanticClassExpr,environment:&EvalEnv,b:&mut Budget)->Check<Option<IntrinsicOrigin>> {
        let SemanticClassExpr::IntrinsicClass{operand,path,class}=expression else {return Ok(None);};
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let (scope,operand)=match operand {
            SemanticOperand::Parameter(index)=>(frame.key,OperandKey::Parameter(*index)),
            SemanticOperand::Capture(index)=>(frame.key,OperandKey::Capture(*index)),
            SemanticOperand::Receiver=>(frame.key,OperandKey::Receiver),
            SemanticOperand::Value(value)=>(frame.key,OperandKey::Value(*value)),
            SemanticOperand::Local(local)=>(frame.key,OperandKey::Local{local:*local,at:environment.at.ok_or_else(||bad("Local origin needs an actual CFG point"))?}),
            SemanticOperand::TypeArgument{scope,ordinal}=>{let frame=self.frame(environment,*scope,b)?;(frame.key,OperandKey::TypeArgument{scope:frame.key,ordinal:*ordinal})},
            SemanticOperand::Environment(ordinal)=>(frame.key,OperandKey::Environment{scope:frame.key,ordinal:*ordinal}),
            SemanticOperand::CoreExpression{..}=>return Err(bad("Typed origin retains a Core expression")),
        };
        b.spend(path.len().saturating_add(1))?;
        Ok(Some(IntrinsicOrigin::Existing{scope,operand,path:path.clone(),class:*class}))
    }
    pub(super) fn claim(&self,module:&TypedModule,claim:&SemanticTypeValue,expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {self.claim_scoped(module,claim,expected,environment,false,b)}
    fn claim_type_use(&self,module:&TypedModule,claim:&SemanticTypeValue,source:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<()> {self.claim_scoped(module,claim,source,environment,true,b)}
    /// A TypeUse may advertise an allowed outer view of this exact source.
    /// The source occurrence has already been bound to its real SSA producer;
    /// this comparison neither replaces that source nor adds any equality.
    fn claim_scoped(&self,module:&TypedModule,claim:&SemanticTypeValue,expected:&CheckedType,environment:&EvalEnv,root_view:bool,b:&mut Budget)->Check<()> {
        let (ty,life,expressions)=resolved(claim)?;
        let mut classes=Vec::new();for expression in expressions {classes.push(self.class(module,expression,environment,b)?);}
        // Metadata-only Task frame construction compares the same complete
        // claim, but cannot re-enter the full Task instruction/WF consumer.
        // Its later frame/site obligations remain mandatory at module exit.
        let mut actual=self.construct_value(module,ty,life,classes,b)?;
        let closure_cursor=self.original_closure_claim(&actual,expected,b)?;
        for (index,expression) in expressions.iter().enumerate() {
            let source=self.expression_origin(expression,environment,b)?;
            let expected_origin=expected.origins.get(index).ok_or_else(||bad(&format!("nominal claim original domain is incomplete [at {:?}, original {:?}, claim type {:?}, claim domain {}, claim classes {}, expected type {:?}, expected domain {}, expected classes {}, expected origins {}]",environment.at,environment.lexical.first().map(|frame|frame.definition),ty,life.binder_count,expressions.len(),expected.ty,expected.life.binder_count,expected.classes.len(),expected.origins.len())))?;
            if let Some(origin)=self.nominal_claim_origin(&actual.origins[index],expected_origin,source.as_ref(),environment,b)?{actual.origins[index]=origin;}
            else if let Some(origin)=source{
                if closure_cursor&&!self.same_origin(environment,&origin,expected_origin,b)?{return Err(bad("closure claim changes its exact original intrinsic occurrence"));}
                actual.origins[index]=origin;
            }
            else if closure_cursor{
                // An Implicit/Formal image is not an assertion that this κ is
                // a new outer Life identity. Keep the exact original closure
                // Receiver index, backed by the mandatory complete cursor.
                super::nominal_origins::charge_origin(expected_origin,b)?;
                actual.origins[index]=expected_origin.clone();
            }
        }
        self.claim_static_obligations(&mut actual,expected,b)?;
        let same=self.same_type(environment,&actual,expected,b)?;
        let view=!same&&root_view&&self.compatible(environment,expected,&actual,b)?;
        if !same&&!view {return Err(bad("site complete type/class claim differs from its original scoped recipe").with_note(format!("original {:?}, actual point {:?}; domains {}/{}, class images {:?}/{:?}",environment.lexical.first().map(|frame|frame.definition),environment.at,actual.life.binder_count,expected.life.binder_count,&actual.classes[..actual.classes.len().min(6)],&expected.classes[..expected.classes.len().min(6)])).with_note(format!("claim failure components: raw_type_equal {}, complete_shape_equal {}, raw_class_images_equal {}, raw_origins_equal {}, statics_equal {}; root_view {root_view}; actual type {:?}, original type {:?}; actual shape {:?}, original shape {:?}; actual origins {:?}, original origins {:?}; actual statics {:?}, original statics {:?}; original claim classes {:?}",actual.ty==expected.ty,actual.life==expected.life,actual.classes==expected.classes,actual.origins==expected.origins,actual.statics==expected.statics,actual.ty,expected.ty,actual.life,expected.life,actual.origins,expected.origins,actual.statics,expected.statics,expressions)));}
        Ok(())
    }
}
fn value_at(values:&[Option<CheckedType>],id:ValueId)->Check<&CheckedType> {values.get(id.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual SSA use has no independent semantic definition"))}
fn atomic(ty:&Type,b:&mut Budget)->Check<CheckedType> {b.spend(1)?;if !ty.is_b0_scalar() {return Err(bad("actual scalar instruction contains a borrowed/opaque semantic carrier"));}Ok(CheckedType{type_cell: crate::semantic::CheckedTypeData::new(ty.clone()),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]})}

#[cfg(test)]
mod type_use_view_tests {
    use super::*;
    #[test]
    fn type_use_views_follow_the_proved_source_direction_without_equating_roles(){
        let limits=Limits::default();let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/phantom-closure-complete-creator.wire"),&limits).unwrap();let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let instance=&module.semantic.as_ref().unwrap().instances[0];let scope=ScopeKey::Instance(instance.id);
        let input=ClassKey::Formal{scope,ordinal:0};let source=ClassKey::Implicit{scope,ordinal:0};
        // A scoped relation kernel control: these frames are not an admission
        // proof for an SSA value, source expression, header or resource owner.
        let env=EvalEnv{lexical:vec![LexicalFrame{definition:instance.definition,key:scope,actuals:vec![CheckedActual::Life(input.clone()),CheckedActual::Life(source.clone())],environment:vec![],self_type:None}],assumptions:vec![scope],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let ty=Type::Address{mutable:false,inner:Box::new(Type::I32)};let life=StorageLifeShape{binder_count:1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}],retained:vec![],nominal_arguments:vec![]};
        let value=|class:ClassKey|CheckedType{type_cell:CheckedTypeData::new(ty.clone()),life:life.clone(),origins:vec![IntrinsicOrigin::DerivedClass{source:class.clone()}],classes:vec![class],statics:vec![]};
        let claim=|ordinal,ty:Type|SemanticTypeValue::Resolved{ty,life:life.clone(),classes:vec![SemanticClassExpr::Formal{scope:SemanticScopeRef::Ancestor(0),ordinal}]};
        context.premises.borrow_mut().entry(scope).or_default().relations.outlives.push((source.clone(),input.clone()));
        assert!(context.claim(&module,&claim(0,ty.clone()),&value(source.clone()),&env,&mut b).is_err(),"ordinary complete type claims remain exact");
        context.claim_type_use(&module,&claim(0,ty.clone()),&value(source.clone()),&env,&mut b).unwrap();
        assert!(!context.same(&env,&source,&input,&mut b).unwrap());
        assert!(context.claim_type_use(&module,&claim(1,ty.clone()),&value(input.clone()),&env,&mut b).is_err());
        assert!(context.claim_type_use(&module,&claim(0,Type::Address{mutable:false,inner:Box::new(Type::I64)}),&value(source.clone()),&env,&mut b).is_err());
        context.premises.borrow_mut().entry(scope).or_default().relations.outlives=vec![(input,source.clone())];
        assert!(context.claim_type_use(&module,&claim(0,ty.clone()),&value(source),&env,&mut b).is_err(),"an upper bound cannot be reversed into a root view");
    }
}
