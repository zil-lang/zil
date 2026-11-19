//! Only the closed original scheme selects an intrinsic. The actual opcode,
//! descriptor source and whole operand types are checked again here. These
//! semantic facts never replace the ordinary resource/loan/epoch kernels.
use super::*;
use original::intrinsics::{BufferSource, Intrinsic};
use zil_base::{DiagnosticIrLocation, DiagnosticRole};
#[path = "owner_fields.rs"]
mod owner_fields;
#[path = "owner_sources.rs"]
pub(super) mod owner_sources;
#[path = "assignments.rs"]
mod assignments;

impl CheckedContext {
    pub(super) fn call_input_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,node:zil_base::NodeId,source:Option<BufferSource>,b:&mut Budget)->Check<()>{
        match source{
            // An argument observes the pointer/description value at this
            // exact original Place. This does not turn it into movable data.
            Some(BufferSource::Place{definition,node})=>self.original_projection_source(module,caller,f,value,definition,node,true,b),
            Some(source)=>self.intrinsic_buffer_source(module,caller,f,value,source,b),
            None=>self.exact_expression_value(module,caller,f,value,node,b),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_intrinsic(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        instruction: &Instruction,
        sites: &[&SemanticSite],
        values: &[Option<CheckedType>],
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Option<CheckedType>> {
        let mut candidates = sites
            .iter()
            .filter(|row| matches!(row.action, SemanticSiteAction::BodyCall { .. }));
        let site = candidates
            .next()
            .ok_or_else(|| bad("actual intrinsic has no original exact Call site"))?;
        if candidates.next().is_some() {
            return Err(bad("actual intrinsic has several original Call claims"));
        }
        let SemanticSiteAction::BodyCall { actuals } = &site.action else {
            unreachable!()
        };
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("actual intrinsic lacks universal original proof"))?
            .intrinsic_recipes(module,caller.definition,site.recipe,environment,b)?;
        if recipes.len()!=actuals.len(){return Err(bad("operational mixed actual vector differs from its complete original scheme arity"));}
        for(recipe,actual)in recipes.iter().zip(actuals){
            let expected=self.evaluate_actual(module,recipe,environment,external,active,b)?;
            self.actual_claim(module,actual,&expected,environment,b)?;
        }
        let use_ = self
            .original
            .borrow()
            .as_ref()
            .ok_or_else(|| bad("actual intrinsic lacks universal original proof"))?
            .intrinsic(caller.definition, site.recipe, b)?;
        if Intrinsic::actual(f,&instruction.kind) != Some(use_.kind) {
            return Err(bad(
                "actual opcode differs from its independently resolved original intrinsic",
            ));
        }
        if use_.kind==Intrinsic::SliceLen{return self.actual_slice_length(module,caller,f,instruction,site,use_.buffer,values,environment,b).map(Some);}
        if use_.kind==Intrinsic::OwnedDyn{return self.actual_dyn_owned(module,caller,f,instruction,site,values,environment,external,active,b).map(Some);}
        if use_.kind==Intrinsic::CCallback{
            let target=use_.worker.ok_or_else(||bad("c_callback has no independently resolved original static target"))?;
            return self.actual_c_callback(module,instruction,target,environment,external,active,b).map(Some);
        }
        if use_.kind == Intrinsic::ScopedCallback {
            return self.actual_scoped_callback(module,caller,f,instruction,site,values,environment,external,active,b).map(Some);
        }
        if matches!(use_.kind,Intrinsic::SyncCellScope(_)|Intrinsic::SyncCellOp(..)|Intrinsic::SyncCellCompareExchange){
            return self.actual_scoped_cell(module,caller,f,instruction,site,use_.buffer,b).map(Some);
        }
        if use_.kind==Intrinsic::ThreadScope{
            return self.actual_scoped_thread(module,caller,f,instruction,site,values,environment,external,active,b).map(Some);
        }
        if let Intrinsic::Convert(target) = use_.kind {
            let value = match instruction.kind {
                InstructionKind::ConvertInteger { value, .. }
                | InstructionKind::ConvertNumber { value, .. } => value,
                _ => return Err(bad("numeric scheme selects another actual operation")),
            };
            let source = &value_at(values, value)?.ty;
            if !source.is_integer() && !source.is_float() {
                return Err(bad(
                    "actual conversion lacks its independently defined numeric input",
                ));
            }
            return Ok(Some(atomic(&target.ty(), b)?));
        }
        if use_.kind == Intrinsic::TaskYield {
            if f.call_kind != CallKind::Suspend {
                return Err(bad(
                    "actual task_yield changes its fixed Suspend convention",
                ));
            }
            return Ok(Some(atomic(&Type::Unit, b)?));
        }
        if matches!(
            use_.kind,
            Intrinsic::TaskCreate
                | Intrinsic::TaskCreateOwned
                | Intrinsic::TaskValid
                | Intrinsic::TaskPoll
                | Intrinsic::TaskRequest
                | Intrinsic::TaskPollClose
                | Intrinsic::TaskFinish
        ) {
            return self
                .actual_task_operation(
                    module,
                    caller,
                    f,
                    instruction,
                    site,
                    use_,
                    values,
                    environment,
                    external,
                    active,
                    b,
                )
                .map(Some);
        }
        if !matches!(
            use_.kind,
            Intrinsic::BytesNew
                | Intrinsic::BytesReserve
                | Intrinsic::BytesPush
                | Intrinsic::BytesTruncate
                | Intrinsic::BytesLen
                | Intrinsic::BytesCapacity
        ) {
            return self
                .actual_operation(
                    module,
                    caller,
                    f,
                    instruction,
                    site.recipe,
                    use_,
                    values,
                    environment,
                    active,
                    b,
                )
                .map(Some);
        }
        let actual = crate::byte_buffers::check_instruction(module, f, instruction, b)?
            .ok_or_else(|| bad("byte intrinsic has no checked result type"))?;
        let buffer = match &instruction.kind {
            InstructionKind::ByteBufferReserve { buffer, capacity }
            | InstructionKind::ByteBufferTruncate {
                buffer,
                length: capacity,
            } => {
                if value_at(values, *capacity)?.ty != Type::I64 {
                    return Err(bad("actual byte length/capacity has another carrier"));
                }
                Some(*buffer)
            }
            InstructionKind::ByteBufferPush { buffer, value } => {
                if value_at(values, *value)?.ty != Type::U8 {
                    return Err(bad("actual byte append has another element type"));
                }
                Some(*buffer)
            }
            InstructionKind::ByteBufferLen { buffer }
            | InstructionKind::ByteBufferCapacity { buffer } => Some(*buffer),
            InstructionKind::ByteBufferNew => None,
            _ => return Err(bad("non-byte opcode reached the byte scheme")),
        };
        match (buffer, use_.buffer) {
            (Some(buffer), Some(source)) => {
                if value_at(values, buffer)?.ty != Type::Desc(Box::new(Type::ByteBuffer)) {
                    return Err(bad(
                        "byte intrinsic has no independently defined description value",
                    ));
                }
                self.intrinsic_buffer_source(module, caller, f, buffer, source, b).map_err(|mut error| {
                    error.relate(DiagnosticRole::Input, Some(instruction.span), intrinsic_location(f, &site.at), format!(
                        "byte operation owner argument 0 uses descriptor SSA {:?}", buffer
                    ));
                    error
                })?;
            }
            (None, None) => {}
            _ => {
                return Err(bad(
                    "intrinsic owner source differs from its complete original arity",
                ));
            }
        }
        let definition = self.definition(module, caller.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(bad("intrinsic original body is absent"));
        };
        b.spend(site_nodes.len())?;
        let mut nodes = site_nodes.iter().filter(|row| row.site == site.recipe);
        let SemanticOriginalNode::Expression(node) = nodes
            .next()
            .ok_or_else(|| bad("intrinsic source operation is absent"))?
            .node
        else {
            return Err(bad("intrinsic original Call is not an expression"));
        };
        if nodes.next().is_some() {
            return Err(bad("intrinsic source operation is ambiguous"));
        }
        let recipe = self.original.borrow().as_ref().unwrap().construction_recipe(
            module,
            caller.definition,
            &SemanticOriginalTypeSource::Expression(node),
            environment,
            b,
        )?;
        let expected = self.evaluate_type(module, &recipe, environment, external, active, b)?;
        if expected.ty != actual || !expected.classes.is_empty() || !expected.statics.is_empty() {
            let mut diagnostic = bad("closed byte result differs from its original operation payload");
            diagnostic.relate(DiagnosticRole::Result, Some(instruction.span), intrinsic_location(f, &site.at), format!(
                "actual intrinsic result {actual:?}; original payload {:?} at expression {node:?}",
                expected.ty
            ));
            return Err(diagnostic);
        }
        Ok(Some(expected))
    }
    pub(super) fn intrinsic_buffer_source(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        value: ValueId,
        source: BufferSource,
        b: &mut Budget,
    ) -> Check<()> {
        if let BufferSource::Place { definition, node } = source {
            return self.projected_buffer_source(module, caller, f, value, definition, node, b);
        }
        if let BufferSource::Address { definition, node } = source {
            return self.address_place_source(module, caller, f, value, definition, node, b);
        }
        if let BufferSource::TemporaryOwner { definition, node } = source {
            return self.temporary_description_source(module, caller, f, value, definition, node, b);
        }
        let local = self.buffer_source_local(module, caller, f, source, b)?;
        let mut value = value;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            b.spend(1)?;
            if !seen.insert(value) {
                return Err(bad("operational owner reborrow has a producer cycle"));
            }
            if let Some(source)=self.prepared_closure_source(module,caller,f,value,b)?{value=source;continue;}
            let (_, _, producer) = self.site_index.producers.get(f, value, b)?;
            match &producer.kind {
                InstructionKind::DescribeLocal { local: actual }
                | InstructionKind::Load(actual)
                | InstructionKind::TakeLocal { local: actual }
                | InstructionKind::BorrowLocal { local: actual }
                | InstructionKind::BorrowLocalMut { local: actual }
                    if *actual == local =>
                {
                    return Ok(());
                }
                InstructionKind::DescribePackageField { local: actual, index }
                    | InstructionKind::TakePackageField { local: actual, index }
                    | InstructionKind::BorrowPackageField { local: actual, index, .. }
                    if *actual == local && self.manager_cell_projection(module,caller,f,local,*index,b)? => return Ok(()),
                InstructionKind::ReborrowAddress { address, .. } | InstructionKind::AddressSlice { address, .. } => value = *address,
                _ => {
                    return Err(bad(
                        "intrinsic descriptor selects another original buffer value",
                    ));
                }
            }
        }
    }
    fn temporary_description_source(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        descriptor: ValueId,
        definition: SemanticDefinitionId,
        node: zil_base::NodeId,
        b: &mut Budget,
    ) -> Check<()> {
        if definition != caller.definition {
            return Err(bad("temporary description belongs to another original scope"));
        }
        let (describe_block, describe_at, describe) = self.site_index.producers.get(f, descriptor, b)?;
        let InstructionKind::DescribeLocal { local } = describe.kind else {
            return Err(bad("temporary description operand does not describe the actual owner slot"));
        };
        let declaration = f.locals.get(local.0 as usize).ok_or_else(|| bad("temporary description owner slot is absent"))?;
        b.spend(f.parameters.len().saturating_add(1))?;
        if f.parameters.contains(&local)
            || !matches!((f.values.get(descriptor.0 as usize), &declaration.ty),
                (Some(Type::Desc(desc)), Type::Managed(inner)) if desc == inner)
        {
            return Err(bad("temporary description operand selects another owner role"));
        }
        let mut initializer = None;
        for block in &f.blocks {
            b.spend(block.instructions.len().saturating_add(1))?;
            for (at, instruction) in block.instructions.iter().enumerate() {
                if let InstructionKind::Store { local: destination, value } = instruction.kind
                    && destination == local
                {
                    let store = SitePoint {
                        function: f.id,
                        block: block.id,
                        instruction: u32::try_from(at).map_err(|_| limit("temporary owner instruction exceeds the actual point domain"))?,
                        after: false,
                    };
                    let observation = SitePoint {
                        function: f.id, block: describe_block, instruction: describe_at, after: false,
                    };
                    if initializer.replace(value).is_some()
                        || !closure_boundaries::creation_dominates(f, store, observation, b)?
                    {
                        return Err(bad("temporary description does not have one dominating owner initializer"));
                    }
                }
            }
        }
        let initializer = initializer.ok_or_else(|| bad("temporary description owner initializer is absent"))?;
        let (producer_block, producer_at, producer) = self.site_index.producers.get(f, initializer, b)?;
        let original = self.definition(module, definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Err(bad("temporary description lacks its original function body"));
        };
        let program = module.semantic.as_ref().ok_or_else(|| bad("temporary description lacks its semantic program"))?;
        let mut matches = 0;
        for site in &program.sites {
            b.spend(1)?;
            let SemanticSiteAddress::Typed { block, instruction, edge } = site.at else {
                continue;
            };
            if site.instance != caller.id || block != producer_block {
                continue;
            }
            let produces_owner = match site.action {
                SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
                    | SemanticSiteAction::CallableCall { .. } =>
                    instruction == producer_at && edge == SemanticSiteEdge::After,
                SemanticSiteAction::Construct { result: SemanticOperand::Value(result), .. } =>
                    result == initializer && instruction == producer_at && edge == SemanticSiteEdge::After,
                SemanticSiteAction::TypeUse { subject: SemanticOperand::Value(result), .. } =>
                    result == initializer && producer_at.checked_add(1) == Some(instruction)
                        && edge == SemanticSiteEdge::Before && matches!(producer.kind,
                            InstructionKind::TakeLocal { .. } | InstructionKind::TakeCapture { .. }
                            | InstructionKind::TakePackageField { .. }),
                _ => false,
            };
            if !produces_owner {
                continue;
            }
            b.spend(site_nodes.len())?;
            matches += usize::from(site_nodes.iter().any(|row| row.site == site.recipe && row.node == SemanticOriginalNode::Expression(node)));
        }
        if matches != 1 {
            let mut error = bad("temporary description initializer is not its exact original owning producer").with_note(format!(
                "original {:?} expression {:?}, candidate initializer {:?}", definition, node, initializer
            ));
            error.relate(DiagnosticRole::Input, Some(describe.span), Some(DiagnosticIrLocation {
                function: f.id.0, block: Some(describe_block.0), instruction: Some(describe_at),
            }), format!("description {:?} refers to owner slot {:?}", descriptor, local));
            error.relate(DiagnosticRole::Result, Some(producer.span), Some(DiagnosticIrLocation {
                function: f.id.0, block: Some(producer_block.0), instruction: Some(producer_at),
            }), format!("actual producer of initializer {:?}", initializer));
            return Err(error);
        }
        if matches!(producer.kind,
            InstructionKind::TakeLocal { .. } | InstructionKind::TakeCapture { .. }
            | InstructionKind::TakePackageField { .. })
        {
            let operand = self.original.borrow().as_ref()
                .ok_or_else(|| bad("prepared transfer lacks its original body"))?
                .prepared_move_operand(module, definition, node, b)?
                .ok_or_else(|| bad("prepared transfer is not the actual original Give"))?;
            self.original_projection_source(module, caller, f, initializer, definition, operand, true, b)?;
        }
        crate::check_instruction(module, f, producer, b)?;
        Ok(())
    }

    pub(super) fn buffer_source_local(
        &self,
        module: &TypedModule,
        caller: &SemanticInstance,
        f: &TypedFunction,
        source: BufferSource,
        b: &mut Budget,
    ) -> Check<LocalId> {
        Ok(match source {
            BufferSource::Parameter {
                definition,
                ordinal,
            } => {
                if definition != caller.definition {
                    return Err(bad(
                        "intrinsic description uses another original parameter scope",
                    ));
                }
                *f.parameters
                    .get(ordinal as usize)
                    .ok_or_else(|| bad("intrinsic parameter ordinal is absent"))?
            }
            BufferSource::Local { definition, node } => {
                if definition != caller.definition {
                    return Err(bad(
                        "intrinsic description uses another original local scope",
                    ));
                }
                let range=self.original.borrow().as_ref().ok_or_else(||bad("range local lacks its complete original proof"))?.range_statement(module,definition,node,b)?;
                if let Some(range)=range {
                    b.spend(f.locals.len())?;
                    let mut locals=f.locals.iter().filter(|local|local.name==range.name&&local.span==range.span&&!local.mutable&&local.ty.is_integer());
                    let local=locals.next().ok_or_else(||bad("range binder has no actual iteration storage"))?;
                    if locals.next().is_some(){return Err(bad("range binder has several actual iteration storages"));}
                    self.range_local_root(module,caller,f,local.id,&range,b)?;
                    return Ok(local.id);
                }
                if !matches!(caller.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==f.id){return Err(bad("intrinsic local selects another actual caller function"));}
                let recipes=self.original.borrow().as_ref().ok_or_else(||bad("intrinsic local has no complete original proof"))?.original_declaration_sites(definition,node,b)?;
                let mut local = None;
                for recipe in recipes {
                    for site in self.actual_declarations(module,caller.id,recipe,b)? {
                        if let SemanticSiteAction::DeclareLocal {
                            local: SemanticLocalTarget::Typed(id),
                            ..
                        } = site.action
                        {
                            if local.replace(id).is_some() {
                                return Err(bad(
                                    "intrinsic original local has several actual declarations",
                                ));
                            }
                        }
                    }
                }
                local.ok_or_else(|| {
                    bad("intrinsic original buffer has no exact actual local declaration")
                })?
            }
            BufferSource::Place { .. } | BufferSource::Address { .. } | BufferSource::TemporaryOwner { .. } => {
                return Err(bad(
                    "projected buffer source was reduced to an unqualified local",
                ));
            }
        })
    }
}
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    fn actual_slice_length(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,site:&SemanticSite,source:Option<BufferSource>,values:&[Option<CheckedType>],_env:&EvalEnv,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::SliceLength{address}=instruction.kind else{return Err(bad("slice length original scheme selects another actual operation"));};
        if !matches!(&value_at(values,address)?.ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Slice{..})){return Err(bad("slice length has no actual complete slice address"));}
        crate::check_instruction(module,f,instruction,b)?;
        if let Some(source)=source{self.intrinsic_buffer_source(module,caller,f,address,source,b)?;}
        else{
            let node=self.original.borrow().as_ref().ok_or_else(||bad("slice receiver has no completed original proof"))?.intrinsic_receiver(caller.definition,site.recipe,b)?;
            let definition=self.definition(module,caller.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("slice receiver has no original body"));};
            let (block, index, _) = self.site_index.producers.get(f, address, b)?;
            let mut matched = false;
            for actual in self.site_index.at(module, caller.id, block, index, b)? {
                b.spend(site_nodes.len().saturating_add(1))?;
                if matches!(actual.action, SemanticSiteAction::BodyCall { .. } | SemanticSiteAction::Call { .. })
                    && site_nodes.iter().any(|row| row.site == actual.recipe && row.node == SemanticOriginalNode::Expression(node))
                {
                    matched = true;
                }
            }
            if !matched{return Err(bad("slice length temporary receiver changes its exact original producer occurrence"));}
        }
        atomic(&Type::I64,b)
    }
}

fn intrinsic_location(f: &TypedFunction, at: &SemanticSiteAddress) -> Option<DiagnosticIrLocation> {
    let SemanticSiteAddress::Typed { block, instruction, .. } = at else {
        return None;
    };
    Some(DiagnosticIrLocation { function: f.id.0, block: Some(block.0), instruction: Some(*instruction) })
}
