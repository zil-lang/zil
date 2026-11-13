//! Proposal for a genuinely class-free local callback returning literal bytes.
//! No missing scope or unrecognized producer is treated as an empty domain.
use super::*;

struct LiteralGraph<'a> {
    definitions: Vec<Option<&'a InstructionKind>>,
    stores: BTreeMap<LocalId, Vec<ValueId>>,
    returns: Vec<ValueId>,
}
impl LiteralGraph<'_> {
    fn literal(
        &self,
        value: ValueId,
        active: &mut BTreeSet<ValueId>,
        left: &mut usize,
        span: Span,
        depth: usize,
        limit: usize,
    ) -> R<bool> {
        spend(left, active.len().saturating_add(1), span)?;
        if depth > limit {
            return Err(Diagnostic::limit(
                Phase::Types,
                "local static result graph exceeds depth",
                Some(span),
            ));
        }
        if !active.insert(value) {
            return Ok(false);
        }
        let result = match self.definitions.get(value.0 as usize).copied().flatten() {
            Some(InstructionKind::StaticUtf8 { .. }) => true,
            Some(InstructionKind::ReborrowAddress {
                address,
                mutable: false,
            }) => self.literal(*address, active, left, span, depth + 1, limit)?,
            Some(InstructionKind::Load(local) | InstructionKind::TakeLocal { local }) => {
                let Some(values) = self.stores.get(local) else {
                    active.remove(&value);
                    return Ok(false);
                };
                spend(left, values.len(), span)?;
                let mut complete = !values.is_empty();
                for source in values {
                    complete &= self.literal(*source, active, left, span, depth + 1, limit)?;
                }
                complete
            }
            _ => false,
        };
        active.remove(&value);
        Ok(result)
    }
}
fn graph<'a>(function: &'a TypedFunction, left: &mut usize) -> R<LiteralGraph<'a>> {
    spend(
        left,
        function.values.len().saturating_add(function.blocks.len()),
        function.span,
    )?;
    let mut graph = LiteralGraph {
        definitions: vec![None; function.values.len()],
        stores: BTreeMap::new(),
        returns: vec![],
    };
    let mut visited = vec![false; function.blocks.len()];
    let mut pending = vec![function.entry.0 as usize];
    while let Some(index) = pending.pop() {
        spend(left, 1, function.span)?;
        let row = visited
            .get_mut(index)
            .ok_or_else(|| unsupported(function.span, "static result CFG target absent"))?;
        if *row {
            continue;
        }
        *row = true;
        let block = &function.blocks[index];
        for instruction in &block.instructions {
            spend(left, 1, instruction.span)?;
            if let Some(value) = instruction.result {
                let slot = graph
                    .definitions
                    .get_mut(value.0 as usize)
                    .ok_or_else(|| unsupported(instruction.span, "static result SSA absent"))?;
                if slot.replace(&instruction.kind).is_some() {
                    return Err(unsupported(
                        instruction.span,
                        "static result SSA defined twice",
                    ));
                }
            }
            if let InstructionKind::Store { local, value } = instruction.kind {
                spend(left, graph.stores.len().saturating_add(1), instruction.span)?;
                graph.stores.entry(local).or_default().push(value);
            }
        }
        match block.terminator {
            Terminator::Return(Some(value)) => graph.returns.push(value),
            Terminator::Jump(block) => pending.push(block.0 as usize),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block.0 as usize);
                pending.push(else_block.0 as usize);
            }
            _ => {}
        }
    }
    Ok(graph)
}
pub(crate) fn populate(module: &mut TypedModule, left: &mut usize, limits: &Limits) -> R<()> {
    let mut changes = Vec::new();
    for definition in &module.closures {
        spend(left, 1, Span::default())?;
        if definition.storage_life.is_some()
            || !definition.captures.is_empty()
            || !matches!(&definition.result,Type::Address{mutable:false,inner}if matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))
            || !definition
                .parameters
                .iter()
                .all(|ty| ty.is_b0_scalar() || *ty == Type::Unit)
        {
            continue;
        }
        let function = &module.functions[definition.function.0 as usize];
        let sources = graph(function, left)?;
        let mut complete = !sources.returns.is_empty();
        for value in &sources.returns {
            inference::charge_type_snapshot(
                &function.values[value.0 as usize],
                function.span,
                limits,
                left,
                1,
            )?;
            complete &= function.values[value.0 as usize] == definition.result
                && sources.literal(
                    *value,
                    &mut BTreeSet::new(),
                    left,
                    function.span,
                    0,
                    limits.max_call_depth,
                )?;
        }
        if complete {
            changes.push((
                definition.id,
                StorageLifeContract {
                    fixed_binder_count: 0,
                    invocation_binder_count: 0,
                    bindings: vec![],
                    static_bindings: vec![StoragePosition {
                        base: StorageBase::Result,
                        path: vec![],
                    }],
                    classes: vec![],
                },
            ));
        }
    }
    for (id, life) in changes {
        let entry = module.closures[id.0 as usize].function;
        borrowed_fp::charge_contract(
            &life,
            |work| spend(left, work, module.functions[entry.0 as usize].span),
            2,
        )?;
        module.closures[id.0 as usize].storage_life = Some(life.clone());
        module.functions[entry.0 as usize].storage_life = Some(life);
        inference::charge_type_snapshot(
            &module.closures[id.0 as usize].result,
            module.functions[entry.0 as usize].span,
            limits,
            left,
            1,
        )?;
        module.functions[entry.0 as usize].retained_return_sources =
            Some(vec![StorageSourceBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: vec![],
                },
                sources: vec![StorageSource::Static {
                    leaf: module.closures[id.0 as usize].result.clone(),
                    mode: StoredBorrowMode::SharedAddress,
                }],
            }]);
    }
    Ok(())
}
