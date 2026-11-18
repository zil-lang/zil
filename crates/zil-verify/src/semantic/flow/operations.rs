//! Actual operational transfers reuse the independent instruction kernels and
//! then reconstruct ghost values. Unresolved retained sources never become a
//! closed envelope merely because an operation has a result type.
use super::*;
use original::intrinsics::Use;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_operation(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        instruction: &Instruction,
        site: SemanticSiteId,
        use_: Use,
        values: &[Option<CheckedType>],
        environment: &EvalEnv,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let span = Some(instruction.span);
        let raw = match &instruction.kind {
            InstructionKind::OwnedStoreNew { .. }
            | InstructionKind::OwnedStoreReserve { .. }
            | InstructionKind::OwnedStorePush { .. }
            | InstructionKind::OwnedStorePop { .. }
            | InstructionKind::OwnedStoreLen { .. }
            | InstructionKind::OwnedStoreCapacity { .. }
            | InstructionKind::OwnedStoreBorrow { .. }
            | InstructionKind::OwnedStoreBorrowMut { .. } => {
                crate::dynamic_store::check_instruction(module, f, instruction, b)?
            }
            InstructionKind::TaskCreate {
                worker, arguments, ..
            } => {
                self.operation_target(
                    module,
                    *worker,
                    use_.worker
                        .ok_or_else(|| bad("Task creation has no original worker identity"))?,
                    b,
                )?;
                for argument in arguments {
                    value_at(values, *argument)?;
                }
                crate::tasks::instruction(module, f, instruction, b)?
            }
            InstructionKind::TaskValid { .. }
            | InstructionKind::TaskPoll { .. }
            | InstructionKind::TaskRequestCancel { .. }
            | InstructionKind::TaskPollClose { .. }
            | InstructionKind::TaskFinishDestroy { .. } => {
                crate::tasks::instruction(module, f, instruction, b)?
            }
            InstructionKind::MutexCreate { .. }
            | InstructionKind::MutexLock { .. }
            | InstructionKind::MutexValue { .. }
            | InstructionKind::MutexUnlock { .. } => {
                crate::generic_mutex::instruction(module, f, instruction, b)?
            }
            InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                callback,
                ..
            } => {
                let providers = use_.providers.ok_or_else(|| {
                    bad("async creation has no complete original provider identities")
                })?;
                for (actual, expected) in [*register, *request_close, *poll_drain]
                    .into_iter()
                    .zip(providers)
                {
                    self.operation_target(module, actual, expected, b)?;
                }
                value_at(values, *callback)?;
                let source = self.original.borrow().as_ref()
                    .ok_or_else(|| bad("registration installation lacks its original callback operand"))?
                    .intrinsic_receiver(caller.definition, site, b)?;
                self.assignment_value_source(module, caller, f, *callback, source, b)
                    .map_err(|error| error.with_note("at the original registration callback operand"))?;
                crate::registration::instruction(module, f, instruction, b)?
            }
            InstructionKind::AsyncRegistrationValid { .. }
            | InstructionKind::AsyncRegistrationRequestClose { .. }
            | InstructionKind::AsyncRegistrationPollClose { .. }
            | InstructionKind::AsyncRegistrationFinishDestroy { .. }
            | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } => {
                crate::registration::instruction(module, f, instruction, b)?
            }
            InstructionKind::ByteBufferBorrow { .. } => {
                crate::byte_buffers::check_instruction(module, f, instruction, b)?
            }
            _ => {
                return Err(unsupported(
                    "operational scheme lacks its exact actual kernel consumer",
                    span,
                ));
            }
        }
        .ok_or_else(|| bad("operational result has no independent instruction type"))?;
        let head = match instruction.kind {
            InstructionKind::OwnedStoreReserve { store, .. }
            | InstructionKind::OwnedStorePush { store, .. }
            | InstructionKind::OwnedStorePop { store }
            | InstructionKind::OwnedStoreLen { store }
            | InstructionKind::OwnedStoreCapacity { store }
            | InstructionKind::OwnedStoreBorrow { store, .. }
            | InstructionKind::OwnedStoreBorrowMut { store, .. } => Some(store),
            InstructionKind::ByteBufferBorrow { buffer, .. } => Some(buffer),
            InstructionKind::MutexLock { mutex } => Some(mutex),
            InstructionKind::MutexValue { guard } | InstructionKind::MutexUnlock { guard } => Some(guard),
            InstructionKind::TaskValid { task }
            | InstructionKind::TaskPoll { task }
            | InstructionKind::TaskRequestCancel { task }
            | InstructionKind::TaskPollClose { task } => Some(task),
            InstructionKind::AsyncRegistrationValid { registration }
            | InstructionKind::AsyncRegistrationRequestClose { registration }
            | InstructionKind::AsyncRegistrationPollClose { registration } => Some(registration),
            _ => None,
        };
        if let Some(source) = use_.buffer {
            let head = head.ok_or_else(|| bad("operation loses its original owner argument"))?;
            value_at(values, head)?;
            self.intrinsic_buffer_source(module, caller, f, head, source, b)?;
        }
        match instruction.kind {
            InstructionKind::MutexValue{guard}=>{
                let borrow=value_at(values,guard)?;
                let guard=self.child_value(module,borrow,&[LifeTypeProjection::Pointee],b)?;
                self.mutex_guard_value(module,&guard,b)?;
                let mut heads=borrow.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==StoredBorrowMode::ExclusiveAddress);
                if heads.next().is_none()||heads.next().is_some(){return Err(bad("guard payload borrowing lacks its one exact exclusive head occurrence"));}
            },
            InstructionKind::MutexUnlock{guard}=>self.mutex_guard_value(module,value_at(values,guard)?,b)?,
            _=>{},
        }
        if let Type::Address { mutable, inner } = &raw {
            let head = value_at(
                values,
                head.ok_or_else(|| bad("owned-region borrow has no actual parent head"))?,
            )?;
            let mut slots = head.life.slots.iter().filter(|row| row.path.is_empty());
            let row = slots.next().ok_or_else(|| {
                bad("owned-region parent has no independently grounded variable head")
            })?;
            if slots.next().is_some() {
                return Err(bad("owned-region parent head is ambiguous"));
            }
            let mut key = head
                .classes
                .get(row.binder as usize)
                .cloned()
                .ok_or_else(|| bad("owned-region head class is absent"))?;
            if let InstructionKind::ByteBufferBorrow { buffer, mutable } = instruction.kind {
                let recipe = self
                    .original
                    .borrow()
                    .as_ref()
                    .ok_or_else(|| bad("byte view lacks its original operation proof"))?
                    .byte_view_receiving(module, caller.definition, site, environment, b)?;
                if let Some(recipe) = recipe {
                    // Path and owner were checked above. The actual address
                    // supplies the upper for this fresh description occurrence.
                    let selected = self.life(module, &recipe, environment, b)?;
                    let (_, _, descriptor) = self.site_index.producers.get(f, buffer, b)?;
                    let InstructionKind::DescribeAddress { address } = descriptor.kind else {
                        return Err(bad("received byte view has no actual borrowed manager head"));
                    };
                    let lender = value_at(values, address)?;
                    let Type::Address {
                        mutable: writable,
                        inner,
                    } = &lender.ty
                    else {
                        return Err(bad("received byte view manager is not an actual resource address"));
                    };
                    if (mutable && !*writable)
                        || !matches!(
                            inner.as_ref(),
                            Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer
                        )
                    {
                        return Err(bad("received byte view changes its actual manager family or access mode"));
                    }
                    let mode = if *writable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    };
                    b.spend(lender.life.slots.len())?;
                    let mut heads = lender
                        .life
                        .slots
                        .iter()
                        .filter(|slot| slot.path.is_empty() && slot.mode == mode);
                    let head = heads.next().ok_or_else(|| bad("received byte view lacks its exact manager lifetime occurrence"))?;
                    let class = lender
                        .classes
                        .get(head.binder as usize)
                        .ok_or_else(|| bad("received byte view manager lifetime is absent"))?;
                    if heads.next().is_some() || !self.outlives(environment, class, &selected, b)? {
                        return Err(bad("received byte view exceeds its actual manager lifetime upper"));
                    }
                    key = selected;
                }
            }
            let child = self.closed_pointee(module, inner, b)?;
            return dynamic_witness::semantic_external::construct(
                self,
                environment,
                dynamic_witness::semantic_external::Constructor::Address {
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    outer: Some(key),
                },
                &[child],
                active,
                b,
            );
        }
        if let Type::MutexGuard {
            payload,
            retained_life,
        } = &raw
        {
            let parent = value_at(
                values,
                head.ok_or_else(|| bad("guard has no exact actual manager description"))?,
            )?;
            let mut heads=parent.life.slots.iter().filter(|row|row.path.is_empty()&&matches!(row.mode,StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription));
            let row=heads.next().ok_or_else(||bad("guard manager has no exact actual description occurrence"))?;
            if heads.next().is_some(){return Err(bad("guard manager repeats its actual description occurrence"));}
            let key = parent
                .classes
                .get(row.binder as usize)
                .cloned()
                .ok_or_else(|| bad("guard manager class is absent"))?;
            self.closed_value(module, payload, b)?;
            crate::generic_mutex::guard_shape(payload, retained_life, b)?;
            let mut value = self.checked_value(
                module,
                &raw,
                &StorageLifeShape {
                    binder_count: 1,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                vec![key.clone()],
                b,
            )?;
            value.origins = vec![IntrinsicOrigin::DerivedClass { source: key }];
            self.mutex_guard_value(module,&value,b)?;
            return Ok(value);
        }
        self.closed_value(module, &raw, b)
    }
    fn operation_target(
        &self,
        module: &TypedModule,
        function: FunctionId,
        expected: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<()> {
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("operational target has no original instance directory"))?;
        b.spend(program.instances.len())?;
        let mut targets=program.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function));
        let target = targets
            .next()
            .ok_or_else(|| bad("operational target has no actual original definition"))?;
        if targets.next().is_some() || target.definition != expected {
            return Err(bad(
                "operational worker/provider substitutes another original definition",
            ));
        }
        Ok(())
    }
}
impl CheckedContext {
    pub(super) fn mutex_guard_value(&self,module:&TypedModule,value:&CheckedType,b:&mut Budget)->Check<()>{
        let Type::MutexGuard{payload,retained_life}=&value.ty else{return Err(bad("guard lifecycle selects another complete semantic value kind"));};
        crate::generic_mutex::guard_shape(payload,retained_life,b)?;
        let life=StorageLifeShape{binder_count:1,slots:vec![],retained:vec![],nominal_arguments:vec![]};
        if value.life!=life||value.classes.len()!=1||value.origins.len()!=1||!value.statics.is_empty(){return Err(bad("guard lifecycle loses its complete one-manager domain/occurrence"));}
        self.closed_value(module,payload,b)?;
        dynamic_witness::structural_actual(module,payload,DynStructuralBound::Value,b)?;
        dynamic_witness::structural_actual(module,payload,DynStructuralBound::SyncDrop,b)
    }
    pub(super) fn drop_mutex_guard(&self,module:&TypedModule,f:&TypedFunction,instruction:&Instruction,state:&mut State,b:&mut Budget)->Check<()>{
        let InstructionKind::DropMutexGuard{local}=instruction.kind else{return Err(bad("guard cleanup selects another actual opcode"));};
        if crate::generic_mutex::instruction(module,f,instruction,b)?.is_some(){return Err(bad("guard cleanup invents a result value"));}
        let declared=f.locals.get(local.0 as usize).ok_or_else(||bad("guard cleanup local identity is absent"))?;
        let occupant=state.locals.get_mut(local.0 as usize).ok_or_else(||bad("guard cleanup semantic local identity is absent"))?;
        if let Some(value)=occupant.as_ref(){
            if value.ty!=declared.ty{return Err(bad("guard cleanup changes its exact initialized original local type"));}
            self.mutex_guard_value(module,value,b)?;
        }
        // Empty after a real move/unlock is a no-op. The independent ordinary
        // loan/epoch ledger still checks every actual cleanup edge and rejects
        // invalid outstanding payload borrows; no shape row supplies a loan.
        *occupant=None;Ok(())
    }
}

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_scoped_callback(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,
        site:&SemanticSite,values:&[Option<CheckedType>],environment:&EvalEnv,
        external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget,
    )->Check<CheckedType> {
        let InstructionKind::WithForeignCallback { register, close, callback, body, .. } = instruction.kind else {
            return Err(bad("scoped callback source selects a different actual opcode"));
        };
        let source = self.original.borrow().as_ref().ok_or_else(|| bad("scoped callback lacks an original body"))?
            .scoped_callback_sources(caller.definition, site.recipe, b)?;
        for (function, definition) in [register, close].into_iter().zip(source.providers) {
            self.operation_target(module, function, definition, b)?;
        }
        self.intrinsic_buffer_source(module, caller, f, callback, source.callback, b)?;
        let head = value_at(values, callback)?;
        if !matches!(&head.ty, Type::Address { mutable: true, inner } if matches!(inner.as_ref(), Type::Closure(_))) {
            return Err(bad("scoped callback must refer to the actual exclusive closure environment"));
        }
        self.call_input_source(module, caller, f, body, source.body_node, source.body_place, b)?;
        let expected = self.evaluate_type(module, &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(source.body_node),
        }, environment, external, active, b)?;
        if !self.same_type(environment, value_at(values, body)?, &expected, b)? {
            return Err(bad("scoped callback body changes its original callable type or environment"));
        }
        let result = crate::scoped_callback::instruction(module, f, instruction, b)?
            .ok_or_else(|| bad("scoped callback does not produce the registration result"))?;
        self.closed_value(module, &result, b)
    }
}
