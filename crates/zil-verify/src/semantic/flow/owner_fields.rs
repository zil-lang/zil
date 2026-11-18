//! Ghost transport for actual class-free owning captures and product views.
//! The existing opcode and responsibility kernels remain mandatory; a closed
//! shape is not a live field, a cleanup ticket or a retained source proof.
use super::*;

impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::flow) fn owner_field_instruction(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,state:&mut State,values:&[Option<CheckedType>],environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        match &instruction.kind{
            InstructionKind::BindPackageCapture{local,index}|InstructionKind::EndPackageCapture{local,index}=>{
                let occurrences=self.occurrences.borrow();let source=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual package capture has no completely sealed creator operand"))?;
                ghost_captures::complete_environment(module,source,b)?;
                if f.locals.get(local.0 as usize).is_none_or(|target|target.ty!=source.ty){return Err(bad("package capture local differs from its full sealed environment type"));}
                dynamic_metadata::ty(&source.ty,b)?;b.spend(source.origins.len())?;
                let target=state.locals.get_mut(local.0 as usize).ok_or_else(||bad("package capture local is absent"))?;
                if matches!(instruction.kind,InstructionKind::BindPackageCapture{..}){if target.is_some(){return Err(bad("package capture rebinds an initialized lexical view"));}*target=Some(source.clone());}
                else{if target.as_ref().is_none_or(|value|value.ty!=source.ty){return Err(bad("package capture end has no matching initialized view"));}*target=None;}
                Ok(None)
            },
            InstructionKind::ReadPackageField{local,index}|InstructionKind::TakePackageField{local,index}|InstructionKind::DescribePackageField{local,index}|InstructionKind::BorrowPackageField{local,index,..}=>{
                let parent=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual package field has no initialized complete parent view"))?;

                let storage=match &parent.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                let Type::Package{fields,..}=storage else{return Err(bad("actual package field parent has another nominal storage"));};
                let(_, _ty)=fields.get(*index as usize).ok_or_else(||bad("actual package field index is absent"))?;
                let child=self.child_value(module,parent,&[LifeTypeProjection::Place(PlaceProjection::Field(*index))],b)?;
                match instruction.kind{
                    InstructionKind::DescribePackageField{..}=>self.owner_description(module,&child,instruction,scope,environment,active,b).map(Some),
                    InstructionKind::BorrowPackageField{mutable,..}=>self.owner_address(&child,instruction,scope,mutable,environment,active,b).map(Some),
                    _=>Ok(Some(child)),
                }
            },
            InstructionKind::DescribeCapture{index}|InstructionKind::BorrowCapture{index,..}=>{
                let occurrences=self.occurrences.borrow();let child=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual captured owner has no sealed creator source"))?;
                ghost_captures::complete_environment(module,child,b)?;
                match instruction.kind{InstructionKind::DescribeCapture{..}=>self.owner_description(module,child,instruction,scope,environment,active,b).map(Some),InstructionKind::BorrowCapture{mutable,..}=>self.owner_address(child,instruction,scope,mutable,environment,active,b).map(Some),_=>unreachable!()}
            },
            InstructionKind::DropPackageField{local,index}|InstructionKind::StorePackageField{local,index,..}=>{
                let parent=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual package field replacement has no complete parent domain"))?;
                if f.locals.get(local.0 as usize).is_none_or(|row|row.ty!=parent.ty){return Err(bad("actual package field replacement changes its exact parent local type"));}
                let storage=match &parent.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                let Type::Package{fields,..}=storage else{return Err(bad("actual package field replacement has another owning product kind"));};
                if fields.get(*index as usize).is_none(){return Err(bad("actual package field replacement selects no original field"));}
                let child=self.child_value(module,parent,&[LifeTypeProjection::Place(PlaceProjection::Field(*index))],b)?;
                match instruction.kind{
                    InstructionKind::StorePackageField{value,..}=>{
                        if !self.compatible(environment,value_at(values,value)?,&child,b)?{return Err(bad("actual package field replacement changes its complete original child domain"));}
                    },
                    InstructionKind::DropPackageField{..}=>{
                        // This is a conditional cleanup of the selected field,
                        // including the empty slot after an actual FnOnce take.
                        // The owning Product kernel retains the live/vacant
                        // debt and loan checks; its parent ghost domain stays
                        // intact for an exact same-domain field refill.
                        ghost_captures::complete_environment(module,&child,b)?;
                    },
                    _=>unreachable!(),
                }
                Ok(None)
            },
            InstructionKind::WriteCapture{index,value}=>{
                let value=value_at(values,*value)?;
                {let occurrences=self.occurrences.borrow();let old=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual capture replacement has no sealed destination"))?;if !self.same_type(environment,old,value,b)?{return Err(bad("actual capture replacement changes its complete source domain/type"));}}
                self.record(scope,OperandKey::Capture(*index),value,b)?;Ok(None)
            },
            InstructionKind::DropPackage{local}=>{
                let target=state.locals.get_mut(local.0 as usize).ok_or_else(||bad("package cleanup local is absent"))?;
                if let Some(value)=target.as_ref(){ghost_captures::complete_environment(module,value,b)?;}
                *target=None;Ok(None)
            },
            InstructionKind::ExtractField{aggregate,index}=>{
                let parent=value_at(values,*aggregate)?;
                self.child_value(module,parent,&[LifeTypeProjection::Place(PlaceProjection::Field(*index))],b).map(Some)
            },
            InstructionKind::AddressField{address,index}=>{
                let parent=value_at(values,*address)?;let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual field address lacks a typed parent address"));};
                actual_field(inner,*index,b)?;
                // An actual resource-head Address contains the Managed
                // position, whose payload has its own complete child domain.
                // The raw kernel above proves this exact canonical head role;
                // keep both projections rather than erasing the owner layer.
                let mut path=vec![LifeTypeProjection::Pointee];
                if matches!(inner.as_ref(),Type::Managed(_)){path.push(LifeTypeProjection::Pointee);}
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(*index)));
                let child=self.child_value(module,parent,&path,b)?;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                let slot=parent.life.slots.iter().find(|slot|slot.path.is_empty()&&slot.mode==mode).ok_or_else(||bad("actual field address has no complete parent head occurrence"))?;
                let key=parent.classes.get(slot.binder as usize).cloned().ok_or_else(||bad("actual field parent head class is absent"))?;
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},&[child],active,b).map(Some)
            },
            InstructionKind::DescribeAddress{address}=>{
                let parent=value_at(values,*address)?;let Type::Address{..}=&parent.ty else{return Err(bad("actual described resource has no typed head address"));};
                let child=self.child_value(module,parent,&[LifeTypeProjection::Pointee],b)?;self.owner_description(module,&child,instruction,scope,environment,active,b).map(Some)
            },
            InstructionKind::ReadDesc{desc,index}=>{
                let parent=value_at(values,*desc)?;
                self.child_value(module,parent,&[LifeTypeProjection::Pointee,LifeTypeProjection::Place(PlaceProjection::Field(*index))],b).map(Some)
            },
            InstructionKind::WriteDesc{desc,index,value}=>{
                let parent=value_at(values,*desc)?;
                let child=self.child_value(module,parent,&[LifeTypeProjection::Pointee,LifeTypeProjection::Place(PlaceProjection::Field(*index))],b)?;
                if !child.ty.is_copy_value()||!self.same_type(environment,value_at(values,*value)?,&child,b)?{
                    return Err(bad("description write requires the exact Copy child domain"));
                }
                self.description_field_write(module,instance,f,instruction,*desc,*index,*value,b)?;
                Ok(None)
            },
            _=>Err(bad("another opcode reached the owning field ghost transport")),
        }
    }
    fn owner_description(&self,module:&TypedModule,owner:&CheckedType,instruction:&Instruction,scope:ScopeKey,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let Type::Managed(_payload)=&owner.ty else{return Err(bad("actual owner description does not describe a complete Managed field"));};
        let child=self.child_value(module,owner,&[LifeTypeProjection::Pointee],b)?;let result=instruction.result.ok_or_else(||bad("actual owner description result is absent"))?;
        let mode=StoredBorrowMode::WritableDescription;let key=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
        dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},&[child],active,b)
    }
    fn owner_address(&self,child:&CheckedType,instruction:&Instruction,scope:ScopeKey,mutable:bool,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let result=instruction.result.ok_or_else(||bad("actual owning field borrow result is absent"))?;
        let mode=if mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};let key=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
        dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},std::slice::from_ref(child),active,b)
    }
}
fn actual_field<'a>(mut ty:&'a Type,index:u32,b:&mut Budget)->Check<&'a Type>{
    loop{b.spend(1)?;match ty{Type::BorrowedStorage{storage,..}|Type::Managed(storage)=>ty=storage,_=>break,}}
    match ty{Type::Record{fields,..}|Type::Package{fields,..}=>fields.get(index as usize).map(|(_,ty)|ty),Type::Tuple(fields)=>fields.get(index as usize),_=>None}.ok_or_else(||bad("actual field index selects no complete product child"))
}
