//! Actual operational transfers reuse the independent instruction kernels and
//! then reconstruct ghost values. Unresolved retained sources never become a
//! closed envelope merely because an operation has a result type.
use super::*;
use original::intrinsics::Use;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_operation(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,use_:Use,values:&[Option<CheckedType>],environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let span=Some(instruction.span);
        let raw=match &instruction.kind{
            InstructionKind::OwnedStoreNew{..}|InstructionKind::OwnedStoreReserve{..}|InstructionKind::OwnedStorePush{..}|InstructionKind::OwnedStorePop{..}|InstructionKind::OwnedStoreLen{..}|InstructionKind::OwnedStoreCapacity{..}|InstructionKind::OwnedStoreBorrow{..}|InstructionKind::OwnedStoreBorrowMut{..}=>crate::dynamic_store::check_instruction(module,f,instruction,b)?,
            InstructionKind::TaskCreate{worker,arguments,..}=>{self.operation_target(module,*worker,use_.worker.ok_or_else(||bad("Task creation has no original worker identity"))?,b)?;for argument in arguments{value_at(values,*argument)?;}crate::tasks::instruction(module,f,instruction,b)?},
            InstructionKind::TaskValid{..}|InstructionKind::TaskPoll{..}|InstructionKind::TaskRequestCancel{..}|InstructionKind::TaskPollClose{..}|InstructionKind::TaskFinishDestroy{..}=>crate::tasks::instruction(module,f,instruction,b)?,
            InstructionKind::MutexCreate{..}|InstructionKind::MutexLock{..}|InstructionKind::MutexValue{..}|InstructionKind::MutexUnlock{..}=>crate::generic_mutex::instruction(module,f,instruction,b)?,
            InstructionKind::AsyncRegistrationStart{register,request_close,poll_drain,callback,..}=>{
                let providers=use_.providers.ok_or_else(||bad("async creation has no complete original provider identities"))?;
                for(actual,expected)in [*register,*request_close,*poll_drain].into_iter().zip(providers){self.operation_target(module,actual,expected,b)?;}
                value_at(values,*callback)?;crate::registration::instruction(module,f,instruction,b)?
            },
            InstructionKind::AsyncRegistrationValid{..}|InstructionKind::AsyncRegistrationRequestClose{..}|InstructionKind::AsyncRegistrationPollClose{..}|InstructionKind::AsyncRegistrationFinishDestroy{..}|InstructionKind::AsyncRegistrationFinishDestroyChecked{..}=>crate::registration::instruction(module,f,instruction,b)?,
            InstructionKind::ByteBufferBorrow{..}=>crate::byte_buffers::check_instruction(module,f,instruction,b)?,
            _=>return Err(unsupported("operational scheme lacks its exact actual kernel consumer",span)),
        }.ok_or_else(||bad("operational result has no independent instruction type"))?;
        let head=match instruction.kind{
            InstructionKind::OwnedStoreReserve{store,..}|InstructionKind::OwnedStorePush{store,..}|InstructionKind::OwnedStorePop{store}|InstructionKind::OwnedStoreLen{store}|InstructionKind::OwnedStoreCapacity{store}|InstructionKind::OwnedStoreBorrow{store,..}|InstructionKind::OwnedStoreBorrowMut{store,..}=>Some(store),
            InstructionKind::ByteBufferBorrow{buffer,..}=>Some(buffer),
            InstructionKind::MutexLock{mutex}=>Some(mutex),InstructionKind::MutexValue{guard}=>Some(guard),
            InstructionKind::TaskValid{task}|InstructionKind::TaskPoll{task}|InstructionKind::TaskRequestCancel{task}|InstructionKind::TaskPollClose{task}=>Some(task),
            InstructionKind::AsyncRegistrationValid{registration}|InstructionKind::AsyncRegistrationRequestClose{registration}|InstructionKind::AsyncRegistrationPollClose{registration}=>Some(registration),
            _=>None,
        };
        if let Some(source)=use_.buffer{let head=head.ok_or_else(||bad("operation loses its original owner argument"))?;value_at(values,head)?;self.intrinsic_buffer_source(module,caller,f,head,source,b)?;}
        if let Type::Address{mutable,inner}=&raw{
            let head=value_at(values,head.ok_or_else(||bad("owned-region borrow has no actual parent head"))?)?;
            let mut slots=head.life.slots.iter().filter(|row|row.path.is_empty());let row=slots.next().ok_or_else(||bad("owned-region parent has no independently grounded variable head"))?;
            if slots.next().is_some(){return Err(bad("owned-region parent head is ambiguous"));}
            let key=head.classes.get(row.binder as usize).cloned().ok_or_else(||bad("owned-region head class is absent"))?;
            let child=self.closed_value(module,inner,b)?;
            return dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode:if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress},outer:Some(key)},&[child],active,b);
        }
        if let Type::MutexGuard{payload,retained_life}=&raw{
            let parent=value_at(values,head.ok_or_else(||bad("guard has no exact actual manager description"))?)?;
            let row=parent.life.slots.iter().find(|row|row.path.is_empty()).ok_or_else(||bad("guard manager has no actual lifetime occurrence"))?;
            let key=parent.classes.get(row.binder as usize).cloned().ok_or_else(||bad("guard manager class is absent"))?;
            self.closed_value(module,payload,b)?;crate::generic_mutex::guard_shape(payload,retained_life,b)?;
            let mut value=self.checked_value(module,&raw,&StorageLifeShape{binder_count:1,slots:vec![],retained:vec![],nominal_arguments:vec![]},vec![key.clone()],b)?;
            value.origins=vec![IntrinsicOrigin::DerivedClass{source:key}];return Ok(value);
        }
        self.closed_value(module,&raw,b)
    }
    fn operation_target(&self,module:&TypedModule,function:FunctionId,expected:SemanticDefinitionId,b:&mut Budget)->Check<()> {
        let program=module.semantic.as_ref().ok_or_else(||bad("operational target has no original instance directory"))?;
        b.spend(program.instances.len())?;let mut targets=program.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function));
        let target=targets.next().ok_or_else(||bad("operational target has no actual original definition"))?;
        if targets.next().is_some()||target.definition!=expected{return Err(bad("operational worker/provider substitutes another original definition"));}Ok(())
    }
}
