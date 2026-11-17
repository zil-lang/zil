//! The existing private callback cell is a representation boundary. Its
//! element retains the exact original Closure domain; the array shell never
//! becomes a callable, an environment capture, or a new source of equality.
use super::*;

pub(super) fn element(ty:&Type)->Option<&Type>{
    match ty{Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))=>Some(element),_=>None}
}

impl CheckedContext{
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

    /// Locate the complete actual initialization, then bind its single operand
    /// to the initializer of the exact original DeclareLocal occurrence.
    pub(super) fn persistent_initialization(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,b:&mut Budget)->Check<(ValueId,ValueId)>{
        let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("persistent callback local is absent"))?;
        if !declaration.mutable||element(&declaration.ty).is_none()||f.parameters.contains(&local){return Err(bad("private callback cell changes its original local role"));}
        let programme=module.semantic.as_ref().ok_or_else(||bad("private callback cell lacks its original programme"))?;
        let mut declared=None;
        for site in &programme.sites{
            b.spend(1)?;
            if site.instance==caller.id&&matches!(site.action,SemanticSiteAction::DeclareLocal{local:SemanticLocalTarget::Typed(actual),..}if actual==local){
                if declared.replace(site).is_some(){return Err(bad("private callback cell has ambiguous original declarations"));}
            }
        }
        let site=declared.ok_or_else(||bad("private callback cell has no original logical declaration"))?;
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
        let(_,_,producer)=super::intrinsics::owner_sources::producer(f,array,b)?;
        let InstructionKind::MakeOwnedArray{elements}=&producer.kind else{return Err(bad("private callback initializer is not its actual one-element constructor"));};
        let [input]=elements.as_slice()else{return Err(bad("private callback initializer changes its single original element"));};
        crate::check_instruction(module,f,producer,b)?;
        if f.values.get(array.0 as usize)!=Some(&declaration.ty){return Err(bad("private callback constructor changes its exact local carrier"));}
        self.assignment_value_source(module,caller,f,*input,initializer,b)?;
        Ok((array,*input))
    }

    pub(super) fn persistent_constraint(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,logical:&CheckedType,b:&mut Budget)->Check<CheckedType>{
        let actual=f.locals.get(local.0 as usize).ok_or_else(||bad("private callback declaration local is absent"))?;
        if element(&actual.ty)!=Some(&logical.ty){return Err(bad("private callback cell element changes its complete logical Closure type"));}
        self.persistent_initialization(module,caller,f,local,b)?;
        self.persistent_array(module,logical,b)
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
        let parent=state.locals.get(local.0 as usize).and_then(Option::as_ref);
        if matches!(instruction.kind,InstructionKind::DropOwnedArray{..}){
            if let Some(parent)=parent{if element(&parent.ty).is_none(){return Err(bad("private callback cleanup selects another initialized carrier"));}let child=self.child_value(module,parent,&[LifeTypeProjection::Element],b)?;ghost_captures::complete_environment(module,&child,b)?;}
            state.locals[local.0 as usize]=None;return Ok(None);
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
                    if original.compound.is_some()||original.span!=instruction.span{continue;}
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
