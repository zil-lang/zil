//! Only the closed original scheme selects an intrinsic. The actual opcode,
//! descriptor source and whole operand types are checked again here. These
//! semantic facts never replace the ordinary resource/loan/epoch kernels.
use super::*;
use original::intrinsics::{BufferSource, Intrinsic};
#[path = "owner_fields.rs"]
mod owner_fields;
#[path = "owner_sources.rs"]
mod owner_sources;

impl CheckedContext {
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
        if !actuals.is_empty() {
            return Err(unsupported(
                "operational mixed actual vector requires its complete original closed-scheme substitution",
                None,
            ));
        }
        let use_ = self
            .original
            .borrow()
            .as_ref()
            .ok_or_else(|| bad("actual intrinsic lacks universal original proof"))?
            .intrinsic(caller.definition, site.recipe, b)?;
        if Intrinsic::actual(&instruction.kind) != Some(use_.kind) {
            return Err(bad(
                "actual opcode differs from its independently resolved original intrinsic",
            ));
        }
        if use_.kind==Intrinsic::CCallback{
            let target=use_.worker.ok_or_else(||bad("c_callback has no independently resolved original static target"))?;
            return self.actual_c_callback(module,instruction,target,environment,external,active,b).map(Some);
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
                self.intrinsic_buffer_source(module, caller, f, buffer, source, b)?;
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
        let recipe = self.original.borrow().as_ref().unwrap().type_recipe(
            module,
            caller.definition,
            &SemanticOriginalTypeSource::Expression(node),
            environment,
            b,
        )?;
        let expected = self.evaluate_type(module, &recipe, environment, external, active, b)?;
        if expected.ty != actual || !expected.classes.is_empty() || !expected.statics.is_empty() {
            return Err(bad(
                "closed byte result differs from its original complete scheme",
            ));
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
        let local = self.buffer_source_local(module, caller, f, source, b)?;
        let mut value = value;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            b.spend(1)?;
            if !seen.insert(value) {
                return Err(bad("operational owner reborrow has a producer cycle"));
            }
            let mut producer = None;
            for block in &f.blocks {
                b.spend(block.instructions.len())?;
                for row in &block.instructions {
                    if row.result == Some(value) && producer.replace(row).is_some() {
                        return Err(bad("intrinsic buffer has several actual SSA definitions"));
                    }
                }
            }
            match &producer
                .ok_or_else(|| bad("intrinsic buffer has no defining instruction"))?
                .kind
            {
                InstructionKind::DescribeLocal { local: actual }
                | InstructionKind::Load(actual)
                | InstructionKind::TakeLocal { local: actual }
                | InstructionKind::BorrowLocal { local: actual }
                | InstructionKind::BorrowLocalMut { local: actual }
                    if *actual == local =>
                {
                    return Ok(());
                }
                InstructionKind::ReborrowAddress { address, .. } => value = *address,
                _ => {
                    return Err(bad(
                        "intrinsic descriptor selects another original buffer value",
                    ));
                }
            }
        }
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
                let definition = self.definition(module, definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body
                else {
                    return Err(bad("intrinsic local has no original body"));
                };
                let program = module
                    .semantic
                    .as_ref()
                    .ok_or_else(|| bad("intrinsic local has no semantic ledger"))?;
                b.spend(
                    site_nodes
                        .len()
                        .saturating_mul(program.sites.len().saturating_add(1)),
                )?;
                let mut local = None;
                for node in site_nodes
                    .iter()
                    .filter(|row| row.node == SemanticOriginalNode::Statement(node))
                {
                    for site in program
                        .sites
                        .iter()
                        .filter(|site| site.instance == caller.id && site.recipe == node.site)
                    {
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
            BufferSource::Place { .. } => {
                return Err(bad(
                    "projected buffer source was reduced to an unqualified local",
                ));
            }
        })
    }
}
