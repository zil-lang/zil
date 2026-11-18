//! Lowered finite Copy products, including a CT tuple materialized in the
//! runtime CFG, do not acquire nominal, borrowed or owning authority from the
//! absence of a source constructor site.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn package_repack(
        &self, module: &TypedModule, instance: &SemanticInstance, f: &TypedFunction,
        instruction: &Instruction, fields: &[ValueId], state: &State, b: &mut Budget,
    ) -> Check<CheckedType> {
        let result = instruction.result.ok_or_else(|| bad("package repack has no actual result"))?;
        let (block, ordinal, actual) = self.site_index.producers.get(f, result, b)?;
        let body = f.blocks.get(block.0 as usize).filter(|body| body.id == block)
            .ok_or_else(|| bad("package repack selects an absent actual block"))?;
        if body.instructions.get(ordinal as usize).is_none_or(|row| !std::ptr::eq(row, actual)) {
            return Err(bad("package repack has no canonical actual instruction coordinate"));
        }
        let program = module.semantic.as_ref().ok_or_else(|| bad("package repack has no original operation ledger"))?;
        let definition = self.definition(module, instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(bad("package repack has no complete original body"));
        };
        b.spend(program.sites.len().saturating_add(site_nodes.len()))?;
        let mut sites = program.sites.iter().filter(|site| {
            site.instance == instance.id
                && matches!(site.at, SemanticSiteAddress::Typed { block: actual, instruction: at, .. }
                    if actual == block && at >= ordinal)
                && matches!(site.action, SemanticSiteAction::TypeUse { subject: SemanticOperand::Value(actual), .. }
                    if actual == result)
        });
        let site = sites.next().ok_or_else(|| bad("package repack lacks its exact original whole-place TypeUse"))?;
        if sites.next().is_some() {
            return Err(bad("package repack ambiguously selects several original whole-place occurrences"));
        }
        let node = site_nodes.iter().find(|row| row.site == site.recipe).and_then(|row| {
            if let SemanticOriginalNode::Expression(node) = row.node { Some(node) } else { None }
        }).ok_or_else(|| bad("package repack TypeUse has no original expression"))?;
        let (root, path) = self.original.borrow().as_ref()
            .ok_or_else(|| bad("package repack has no completed original Place proof"))?
            .buffer_place(instance.definition, node, b)?;
        if !path.is_empty() {
            return Err(bad("package repack TypeUse selects a field of the original place"));
        }
        let storage = f.values.get(result.0 as usize)
            .ok_or_else(|| bad("package repack has no actual whole result type"))?;
        let layout = if let Type::BorrowedStorage { storage, .. } = storage { storage.as_ref() } else { storage };
        let Type::Package { fields: layout, .. } = layout else {
            return Err(bad("package repack has no complete actual Package layout"));
        };
        let first = (ordinal as usize).checked_sub(fields.len()).ok_or_else(|| bad("package repack omits its actual field preparation"))?;
        if fields.len() != layout.len() || fields.is_empty() {
            return Err(bad("package repack does not carry every actual field"));
        }
        let mut source = None;
        for (index, (value, (_, ty))) in fields.iter().zip(layout).enumerate() {
            b.spend(1)?;
            let prepared = &body.instructions[first + index];
            let local = match prepared.kind {
                InstructionKind::TakePackageField { local, index: actual }
                    if actual as usize == index && !ty.is_copy_value() => local,
                InstructionKind::ReadPackageField { local, index: actual }
                    if actual as usize == index && ty.is_copy_value() => local,
                _ => return Err(bad("package repack changes a field's exact ordinal or transfer mode")),
            };
            if prepared.result != Some(*value) || source.is_some_and(|source| source != local) {
                return Err(bad("package repack mixes actual source locals or changes a prepared field"));
            }
            source = Some(local);
        }
        let local = source.ok_or_else(|| bad("package repack has no actual owning source"))?;
        self.projected_local_root(module, instance, f, local, root, node, b)?;
        crate::check_instruction(module, f, instruction, b)?;
        let parent = state.locals.get(local.0 as usize).and_then(Option::as_ref)
            .ok_or_else(|| bad("package repack source has no complete initialized parent domain"))?;
        if &parent.ty != storage {
            return Err(bad("package repack changes its source's complete nominal type"));
        }
        parent.charge_clone(b)?;
        Ok(parent.clone())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn internal_copy_aggregate(&self,module:&TypedModule,f:&TypedFunction,instruction:&Instruction,fields:&[ValueId],values:&[Option<CheckedType>],reified:bool,b:&mut Budget)->Check<CheckedType>{
        if !matches!(instruction.kind,InstructionKind::MakeAggregate{..}){return Err(bad("owning or nominal construction has no exact original semantic site"));}
        let result=instruction.result.ok_or_else(||bad("internal Copy aggregate has no defining result"))?;
        let ty=f.values.get(result.0 as usize).ok_or_else(||bad("internal Copy aggregate result type is absent"))?;
        dynamic_metadata::ty(ty,b)?;
        // Nominal records require an authored constructor or an exact
        // original CT result with an independently checked producer tree.
        if !(matches!(ty,Type::Tuple(_)|Type::Array{..}) || reified && matches!(ty,Type::Record{..}))
            || !ty.is_copy_value(){return Err(bad("actual aggregate construction has no original semantic site"));}
        crate::check_instruction(module,f,instruction,b)?;
        let value=self.closed_value(module,ty,b)?;
        if !value.classes.is_empty()||!value.statics.is_empty(){return Err(bad("internal Copy aggregate has an unresolved class/static domain"));}
        b.spend(fields.len())?;
        for field in fields{
            let field=value_at(values,*field)?;
            dynamic_metadata::ty(&field.ty,b)?;
            if !field.classes.is_empty()||!field.statics.is_empty()||!field.ty.is_copy_value(){return Err(bad("internal Copy aggregate imports a borrowed or owning child"));}
            self.closed_value(module,&field.ty,b)?;
        }
        Ok(value)
    }
}
