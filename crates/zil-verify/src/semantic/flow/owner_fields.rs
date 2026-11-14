//! Ghost transport for actual class-free owning captures and product views.
//! The existing opcode and responsibility kernels remain mandatory; a closed
//! shape is not a live field, a cleanup ticket or a retained source proof.
use super::*;

impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::flow) fn owner_field_instruction(&self,module:&TypedModule,_instance:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,state:&mut State,values:&[Option<CheckedType>],environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        crate::check_instruction(module,f,instruction,b)?;
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let span=Some(instruction.span);
        match &instruction.kind{
            InstructionKind::BindPackageCapture{local,index}|InstructionKind::EndPackageCapture{local,index}=>{
                let occurrences=self.occurrences.borrow();let source=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual package capture has no completely sealed creator operand"))?;
                if !source.classes.is_empty()||!source.statics.is_empty(){return Err(unsupported("package capture retains an unresolved full class/source domain",span));}
                self.closed_value(module,&source.ty,b)?;
                if f.locals.get(local.0 as usize).is_none_or(|target|target.ty!=source.ty){return Err(bad("package capture local differs from its full sealed environment type"));}
                dynamic_metadata::ty(&source.ty,b)?;b.spend(source.origins.len())?;
                let target=state.locals.get_mut(local.0 as usize).ok_or_else(||bad("package capture local is absent"))?;
                if matches!(instruction.kind,InstructionKind::BindPackageCapture{..}){if target.is_some(){return Err(bad("package capture rebinds an initialized lexical view"));}*target=Some(source.clone());}
                else{if target.as_ref().is_none_or(|value|value.ty!=source.ty){return Err(bad("package capture end has no matching initialized view"));}*target=None;}
                Ok(None)
            },
            InstructionKind::ReadPackageField{local,index}|InstructionKind::TakePackageField{local,index}|InstructionKind::DescribePackageField{local,index}|InstructionKind::BorrowPackageField{local,index,..}=>{
                let parent=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual package field has no initialized complete parent view"))?;
                if !parent.classes.is_empty()||!parent.statics.is_empty(){return Err(unsupported("package field needs its full parent class/source projection",span));}
                let storage=match &parent.ty{Type::BorrowedStorage{storage,..}=>storage.as_ref(),other=>other};
                let Type::Package{fields,..}=storage else{return Err(bad("actual package field parent has another nominal storage"));};
                let(_,ty)=fields.get(*index as usize).ok_or_else(||bad("actual package field index is absent"))?;
                let child=self.closed_value(module,ty,b)?;
                match instruction.kind{
                    InstructionKind::DescribePackageField{..}=>self.owner_description(module,&child,instruction,scope,environment,active,b).map(Some),
                    InstructionKind::BorrowPackageField{mutable,..}=>self.owner_address(&child,instruction,scope,mutable,environment,active,b).map(Some),
                    _=>Ok(Some(child)),
                }
            },
            InstructionKind::DescribeCapture{index}|InstructionKind::BorrowCapture{index,..}=>{
                let occurrences=self.occurrences.borrow();let child=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual captured owner has no sealed creator source"))?;
                if !child.classes.is_empty()||!child.statics.is_empty(){return Err(unsupported("captured owner requires full retained source and parent transport",span));}
                self.closed_value(module,&child.ty,b)?;
                match instruction.kind{InstructionKind::DescribeCapture{..}=>self.owner_description(module,child,instruction,scope,environment,active,b).map(Some),InstructionKind::BorrowCapture{mutable,..}=>self.owner_address(child,instruction,scope,mutable,environment,active,b).map(Some),_=>unreachable!()}
            },
            InstructionKind::WriteCapture{index,value}=>{
                let value=value_at(values,*value)?;self.closed_value(module,&value.ty,b)?;
                if !value.classes.is_empty()||!value.statics.is_empty(){return Err(unsupported("capture replacement needs complete post-source/parent transport",span));}
                {let occurrences=self.occurrences.borrow();let old=occurrences.get(&(scope,OperandKey::Capture(*index))).ok_or_else(||bad("actual capture replacement has no sealed destination"))?;if old.ty!=value.ty||!old.classes.is_empty()||!old.statics.is_empty(){return Err(bad("actual capture replacement changes its complete source domain/type"));}}
                self.record(scope,OperandKey::Capture(*index),value,b)?;Ok(None)
            },
            InstructionKind::DropPackage{local}=>{
                let target=state.locals.get_mut(local.0 as usize).ok_or_else(||bad("package cleanup local is absent"))?;
                if let Some(value)=target.as_ref(){if !value.classes.is_empty()||!value.statics.is_empty(){return Err(unsupported("package cleanup requires full logical field responsibility/source transport",span));}self.closed_value(module,&value.ty,b)?;}
                *target=None;Ok(None)
            },
            InstructionKind::ExtractField{aggregate,index}=>{
                let parent=value_at(values,*aggregate)?;if !parent.classes.is_empty()||!parent.statics.is_empty(){return Err(unsupported("field extraction needs the complete child class map",span));}
                let ty=actual_field(&parent.ty,*index,b)?;Ok(Some(self.closed_value(module,ty,b)?))
            },
            InstructionKind::AddressField{address,index}=>{
                let parent=value_at(values,*address)?;let Type::Address{mutable,inner}=&parent.ty else{return Err(bad("actual field address lacks a typed parent address"));};
                let child=self.closed_value(module,actual_field(inner,*index,b)?,b)?;
                let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
                let slot=parent.life.slots.iter().find(|slot|slot.path.is_empty()&&slot.mode==mode).ok_or_else(||bad("actual field address has no complete parent head occurrence"))?;
                let key=parent.classes.get(slot.binder as usize).cloned().ok_or_else(||bad("actual field parent head class is absent"))?;
                dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(key)},&[child],active,b).map(Some)
            },
            InstructionKind::DescribeAddress{address}=>{
                let parent=value_at(values,*address)?;let Type::Address{inner,..}=&parent.ty else{return Err(bad("actual described resource has no typed head address"));};
                let child=self.closed_value(module,inner,b)?;self.owner_description(module,&child,instruction,scope,environment,active,b).map(Some)
            },
            _=>Err(bad("another opcode reached the owning field ghost transport")),
        }
    }
    fn owner_description(&self,module:&TypedModule,owner:&CheckedType,instruction:&Instruction,scope:ScopeKey,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let Type::Managed(payload)=&owner.ty else{return Err(bad("actual owner description does not describe a complete Managed field"));};
        let child=self.closed_value(module,payload,b)?;let result=instruction.result.ok_or_else(||bad("actual owner description result is absent"))?;
        let mode=StoredBorrowMode::ReadOnlyDescription;let key=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
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
