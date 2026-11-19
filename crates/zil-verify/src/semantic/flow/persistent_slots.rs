//! A source binding carries its element's domain. A move can transfer a
//! manager cell through a real closure constructor. Its physical field then
//! belongs to that environment until an actual move, replacement or cleanup.
use super::*;

pub(super) fn element(ty:&Type)->Option<&Type>{
    match ty{Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))=>Some(element),_=>None}
}

impl CheckedContext{
    /// Existing EvalOnce staging is transparent only after its complete
    /// no-alias/one-Store/one-Take epoch has been checked. A real original
    /// binder remains an independent source occurrence, even at the same type.
    pub(super) fn prepared_closure_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<Option<ValueId>>{
        self.prepared_owning_source(module, caller, f, value, false, b)
    }

    pub(super) fn prepared_managed_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<Option<ValueId>>{
        self.prepared_owning_source(module, caller, f, value, true, b)
    }

    fn prepared_owning_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,managed:bool,b:&mut Budget)->Check<Option<ValueId>>{
        if !matches!((managed, f.values.get(value.0 as usize)), (false, Some(Type::Closure(_))) | (true, Some(Type::Managed(_)))) { return Ok(None); }
        let(block,at,instruction)=self.site_index.producers.get(f,value,b)?;
        let InstructionKind::TakeLocal{local}=instruction.kind else{return Ok(None);};
        if f.locals.get(local.0 as usize).is_none_or(|declaration|!declaration.mutable){return Ok(None);}
        let subjects = [sites::Subject::Declaration(local), sites::Subject::LocalTypeUse(local),
                        sites::Subject::ValueTypeUse(value)];
        if self.site_index.has_subject(module, caller.id, &subjects, b)? { return Ok(None); }
        if managed {
            super::intrinsics::owner_sources::scalar_preparations::managed_source(module,f,block,at,local,b)
        } else {
            super::intrinsics::owner_sources::scalar_preparations::owning_source(module,f,block,at,local,b)
        }
    }
    /// A complete occurrence map for one real element, including phantom κ.
    fn persistent_array(&self,module:&TypedModule,child:&CheckedType,b:&mut Budget)->Check<CheckedType>{
        if !matches!(child.ty,Type::Closure(_))||!child.life.slots.is_empty()||!child.life.retained.is_empty()||!child.life.nominal_arguments.is_empty(){return Err(bad("private callback element is not its complete opaque Closure domain"));}
        ghost_captures::complete_environment(module,child,b)?;
        let ty=Type::Array{element:Box::new(child.raw_type_copy(b)?),length:1};
        let mut life=empty_life();life.binder_count=child.life.binder_count;
        child.charge_clone(b)?;
        for class in 0..life.binder_count{life.retained.push(StorageRetainedBinding{path:vec![LifeTypeProjection::Element],class,binder:class});}
        let mut value=self.checked_value(module,&ty,&life,child.classes.clone(),b)?;
        value.origins=child.origins.clone();value.statics=child.statics.clone();Ok(value)
    }

    pub(super) fn binding_storage(
        &self, module: &TypedModule, storage: &Type, child: &CheckedType, environment: &EvalEnv, b: &mut Budget,
    ) -> Check<CheckedType> {
        let (element, selector) = storage.binding_cell_element()
            .ok_or_else(|| bad("source binding has no physical single-element carrier"))?;
        dynamic_metadata::ty(storage, b)?;
        if *element != child.ty { return Err(bad("physical binding carrier changes its logical element")); }
        if matches!(selector, LifeTypeProjection::Element) { return self.persistent_array(module, child, b); }
        let mut value = CheckedType {
            type_cell: crate::semantic::CheckedTypeData::new(storage.clone()), life: empty_life(),
            classes: Vec::new(), origins: Vec::new(), statics: Vec::new(),
        };
        self.attach_child(environment, &mut value, child, vec![selector], Some(PlaceProjection::Field(0)), b)?;
        value.life.binder_count = u32::try_from(value.classes.len()).map_err(|_| limit("binding lifetime domain exceeds u32"))?;
        Ok(value)
    }

    /// A lowered lexical cell has no source Package constructor. Select its
    /// unique declaration immediately after formation, then prove its original
    /// initializer independently from the actual stored field.
    pub(super) fn binding_constructor(
        &self, module: &TypedModule, caller: &SemanticInstance, f: &TypedFunction,
        block: &BasicBlock, ordinal: usize, instruction: &Instruction, values: &[Option<CheckedType>],
        environment: &EvalEnv, b: &mut Budget,
    ) -> Check<Option<CheckedType>> {
        let Some(result) = instruction.result else { return Ok(None); };
        let physical = f.values.get(result.0 as usize).ok_or_else(|| bad("constructor result is absent"))?;
        let Some(input) = physical.binding_cell_input(&instruction.kind) else { return Ok(None); };
        let next = ordinal.checked_add(1).ok_or_else(|| limit("binding construction position overflow"))?;
        let Some(Instruction { kind: InstructionKind::StorageLive(local), .. }) = block.instructions.get(next) else { return Ok(None); };
        let after = self.site_index.at(module, caller.id, block.id, next as u32, b)?;
        if !after.iter()
            .any(|site| matches!(&site.action, SemanticSiteAction::DeclareLocal { local: SemanticLocalTarget::Typed(id),
                expected: SemanticTypeValue::Resolved { ty: Type::Managed(_), .. } } if id == local)) { return Ok(None); }
        let (actual, incoming) = self.persistent_initialization(module,caller,f,*local,b)?;
        if actual != result || incoming != input { return Err(bad("lexical binding selects another aggregate initializer")); }
        self.binding_storage(module, physical, value_at(values,input)?, environment, b).map(Some)
    }

    /// Each constructor must have transferred the same source binder into
    /// this field. Occupancy and loans of its view stay with the CFG kernels.
    pub(super) fn captured_manager_cell(&self, child: SemanticInstanceId, field: u32, b: &mut Budget) -> Check<bool> {
        let boundaries = self.closure_boundaries.borrow();
        b.spend(boundaries.len())?;
        let mut ordinal = None;
        for row in boundaries.iter().filter(|row| row.child == child) {
            b.spend(row.moved_cells.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
            let Some(&actual) = row.moved_cells.get(&field) else { return Ok(false); };
            if ordinal.replace(actual).is_some_and(|old| old != actual) {
                return Err(bad("actual constructors select different original manager binders"));
            }
        }
        Ok(ordinal.is_some())
    }

    pub(super) fn manager_cell_projection(&self, module: &TypedModule, caller: &SemanticInstance,
        f: &TypedFunction, local: LocalId, index: u32, b: &mut Budget) -> Check<bool> {
        let ty=&f.locals.get(local.0 as usize).ok_or_else(||bad("binding projection local is absent"))?.ty;
        if index != 0 || !matches!(ty.binding_cell_element(), Some((Type::Managed(_), _))) { return Ok(false); }
        if let Some(field) = self.bound_package_capture(module, f, local, b)? {
            return self.captured_manager_cell(caller.id, field, b);
        }
        self.persistent_initialization(module,caller,f,local,b)?;
        Ok(true)
    }

    pub(super) fn bound_package_capture(&self, module: &TypedModule, f: &TypedFunction,
        local: LocalId, b: &mut Budget) -> Check<Option<u32>> {
        if f.environment.is_none() { return Ok(None); }
        let entry = f.blocks.get(f.entry.0 as usize)
            .filter(|block| block.id == f.entry).ok_or_else(|| bad("capture view has no entry block"))?;
        let mut field = None;
        for instruction in &entry.instructions {
            b.spend(1)?;
            if let InstructionKind::BindPackageCapture { local: target, index } = instruction.kind
                && target == local {
                crate::packages::check_instruction(module, f, instruction, b)?;
                if field.replace(index).is_some() { return Err(bad("capture view has competing entry bindings")); }
            }
        }
        Ok(field)
    }

    /// Locate the complete actual initialization, then bind its single operand
    /// to the initializer of the exact original DeclareLocal occurrence.
    pub(super) fn persistent_initialization(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,b:&mut Budget)->Check<(ValueId,ValueId)>{
        let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("persistent callback local is absent"))?;
        if !declaration.mutable||declaration.ty.binding_cell_element().is_none()||f.parameters.contains(&local){return Err(bad("private callback cell changes its original local role"));}
        let declared = self.site_index.subject(module, caller.id, sites::Subject::Declaration(local), b)?;
        let site = match declared.as_slice() {
            [site] => *site,
            [] => return Err(bad("private callback cell has no original logical declaration")),
            _ => return Err(bad("private callback cell has ambiguous original declarations")),
        };
        let (_,initializer)=self.original.borrow().as_ref().ok_or_else(||bad("private callback cell lacks its independent original body"))?.persistent_binding(module,caller.definition,site.recipe,b)?;
        let mut live=None;let mut stored=None;
        for block in &f.blocks{for(at,instruction)in block.instructions.iter().enumerate(){
            b.spend(1)?;
            match instruction.kind{
                InstructionKind::StorageLive(actual)if actual==local=>{if live.replace((block.id,at as u32)).is_some(){return Err(bad("private callback cell has multiple storage epochs"));}crate::check_instruction(module,f,instruction,b)?;},
                InstructionKind::Store{local:actual,value}if actual==local=>{if stored.replace((block.id,at as u32,value)).is_some(){return Err(bad("private callback cell has multiple whole-shell initializers"));}crate::check_instruction(module,f,instruction,b)?;},
                _=>{},
            }
        }}
        let live=live.ok_or_else(||bad("private callback cell has no actual StorageLive"))?;
        let(block,at,array)=stored.ok_or_else(||bad("private callback cell has no actual initializing Store"))?;
        let SemanticSiteAddress::Typed{block:site_block,instruction:site_at,..}=site.at else{return Err(bad("private callback cell retains a nonactual declaration"));};
        if (site_block,site_at)!=(block,at)&&(site_block,site_at)!=live{return Err(bad("private callback declaration selects another storage epoch or Store"));}
        let(_,_,producer)=self.site_index.producers.get(f,array,b)?;
        let input=declaration.ty.binding_cell_input(&producer.kind)
            .ok_or_else(||bad("private binding initializer changes its actual single operand or constructor"))?;
        crate::check_instruction(module,f,producer,b)?;
        if f.values.get(array.0 as usize)!=Some(&declaration.ty){return Err(bad("private callback constructor changes its exact local carrier"));}
        self.assignment_value_source(module,caller,f,input,initializer,b)?;
        Ok((array,input))
    }

    pub(super) fn persistent_constraint(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,logical:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<CheckedType>{
        let actual=f.locals.get(local.0 as usize).ok_or_else(||bad("private callback declaration local is absent"))?;
        if actual.ty.binding_cell_element().map(|row|row.0)!=Some(&logical.ty){return Err(bad("private callback cell element changes its complete logical Closure type"));}
        self.persistent_initialization(module,caller,f,local,b)?;
        self.binding_storage(module,&actual.ty,logical,environment,b)
    }

    pub(super) fn persistent_constructor(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,values:&[Option<CheckedType>],b:&mut Budget)->Check<CheckedType>{
        let result=instruction.result.ok_or_else(||bad("private callback constructor lacks its actual result"))?;
        let mut local=None;
        for block in &f.blocks{for operation in &block.instructions{
            b.spend(1)?;
            if let InstructionKind::Store{local:target,value}=operation.kind{if value==result&&local.replace(target).is_some(){return Err(bad("private callback constructor enters multiple shells"));}}
        }}
        let local=local.ok_or_else(||bad("private callback constructor has no actual local shell"))?;
        let(array,input)=self.persistent_initialization(module,caller,f,local,b)?;
        if array!=result{return Err(bad("private callback constructor differs from its original initializer"));}
        self.persistent_array(module,value_at(values,input)?,b)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn persistent_instruction(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,state:&mut State,values:&[Option<CheckedType>],environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        let(local,index)=match instruction.kind{
            InstructionKind::TakeOwnedArrayElement{local,index}|InstructionKind::DropOwnedArrayElement{local,index}|InstructionKind::StoreOwnedArrayElement{local,index,..}|InstructionKind::BorrowOwnedArrayElement{local,index,..}=>(local,Some(index)),
            InstructionKind::DropOwnedArray{local}=>(local,None),
            _=>return Err(bad("another operation reaches the private callback element transport")),
        };
        if index.is_some_and(|index|index!=0){return Err(bad("private callback cell selects another element"));}
        self.persistent_initialization(module,caller,f,local,b)?;
        let parent=state.locals.get(local.0 as usize).and_then(Option::as_deref);
        if matches!(instruction.kind,InstructionKind::DropOwnedArray{..}){
            if let Some(parent)=parent{if element(&parent.ty).is_none(){return Err(bad("private callback cleanup selects another initialized carrier"));}let child=self.child_value(module,parent,&[LifeTypeProjection::Element],b)?;ghost_captures::complete_environment(module,&child,b)?;}
            state.edit_locals(b)?[local.0 as usize]=None;return Ok(None);
        }
        let parent=parent.ok_or_else(||bad("private callback operation has no initialized shell on every incoming edge"))?;
        if f.locals.get(local.0 as usize).is_none_or(|decl|decl.ty!=parent.ty)||element(&parent.ty).is_none(){return Err(bad("private callback operation changes its exact initialized shell"));}
        let child=self.child_value(module,parent,&[LifeTypeProjection::Element],b)?;
        match instruction.kind{
            InstructionKind::TakeOwnedArrayElement{..}=>Ok(Some(child)),
            InstructionKind::DropOwnedArrayElement{..}=>{ghost_captures::complete_environment(module,&child,b)?;Ok(None)},
            InstructionKind::StoreOwnedArrayElement{value,..}=>{
                if !self.compatible(environment,value_at(values,value)?,&child,b)?{return Err(bad("private callback refill changes its complete original element domain"));}
                let originals=self.original.borrow().as_ref().ok_or_else(||bad("private callback refill lacks its original source proof"))?.assignments(module,caller.definition,b)?;
                let mut matched=false;
                for original in originals{
                    if original.compound.is_some()||original.span!=instruction.span&&original.statement_span!=instruction.span{continue;}
                    let(root,path)=self.original.borrow().as_ref().unwrap().buffer_place(caller.definition,original.target,b)?;
                    let original::intrinsics::BufferRoot::Local{node}=root else{continue;};
                    if !path.is_empty()||self.buffer_source_local(module,caller,f,original::intrinsics::BufferSource::Local{definition:caller.definition,node},b)?!=local{continue;}
                    if matched{return Err(bad("private callback refill ambiguously selects an original assignment"));}
                    self.assignment_value_source(module,caller,f,value,original.value,b)?;matched=true;
                }
                if !matched{return Err(bad("private callback refill has no exact original assignment and operand"));}Ok(None)
            },
            InstructionKind::BorrowOwnedArrayElement{mutable,..}=>{
                let result=instruction.result.ok_or_else(||bad("private callback borrow has no actual result"))?;
                let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
                let mode=if mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                let outer=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(outer)},&[child],active,b).map(Some)
            },
            _=>unreachable!(),
        }
    }
}
