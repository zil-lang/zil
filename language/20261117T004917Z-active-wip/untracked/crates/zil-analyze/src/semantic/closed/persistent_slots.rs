//! Exact representation witnesses for original closure bindings stored in an
//! owning one-element array. A witness grants no initialization or drop right.
use super::*;
use crate::semantic::value::{Class, Value};

pub(super) struct Slot {
    local: LocalId,
    live: (BlockId, usize),
    store: (BlockId, usize),
    array: ValueId,
    initial: ValueId,
    logical: Value,
}

impl Slot {
    pub(super) fn local(&self) -> LocalId { self.local }
    pub(super) fn logical(&self) -> &Value { &self.logical }
    pub(super) fn declaration(&self) -> (BlockId, usize) { self.live }
    pub(super) fn initial_store(&self) -> (BlockId, usize) { self.store }
    pub(super) fn array(&self) -> ValueId { self.array }
    pub(super) fn initial(&self) -> ValueId { self.initial }
}

impl Check<'_, '_> {
    pub(super) fn persistent_slot(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        local: LocalId, b: &mut Budget,
    ) -> CompileResult<Option<Slot>> {
        b.spend(1, WORK)?;
        let physical = &function.locals.get(local.0 as usize)
            .filter(|entry| entry.id == local)
            .ok_or_else(|| fail("persistent slot has no exact physical local"))?.ty;
        let Type::Array { element, length: 1 } = physical else { return Ok(None); };
        if !matches!(element.as_ref(), Type::Closure(_)) { return Ok(None); }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("persistent slot has no actual function world"));
        };
        if self.inventory.function(target, b)?.id != function.id {
            return Err(fail("persistent slot changes its exact function world"));
        }
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body else {
            return Err(fail("persistent slot has no complete original declaration body"));
        };
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id { continue; }
            let SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(actual), expected,
            } = &site.action else { continue; };
            if *actual != local { continue; }
            b.spend(recipes.len(), WORK)?;
            let recipe = recipes.iter().find(|recipe| recipe.id == site.recipe)
                .ok_or_else(|| fail("persistent slot lacks its original declaration recipe"))?;
            let SemanticSiteRecipeKind::DeclareLocal { expected: original } = &recipe.kind else {
                return Err(fail("persistent slot replaces its original declaration operation"));
            };
            let logical = self.recipe_value(instance, original, b, 0)?;
            // An ordinary user array declaration does not establish this bridge.
            if !matches!(logical.ty, Type::Closure(_)) { continue; }
            if !geometry::same_type(&logical.ty, element, b)? {
                return Err(fail("persistent slot changes its complete original closure type"));
            }
            self.original_claim(instance, expected, &logical, b)?;
            let SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After } = site.at else {
                return Err(fail("persistent slot lacks its exact After StorageLive boundary"));
            };
            let live = instruction as usize;
            let actual = function.blocks.get(block.0 as usize)
                .filter(|entry| entry.id == block)
                .ok_or_else(|| fail("persistent slot has no actual declaration block"))?;
            if !matches!(actual.instructions.get(live).map(|entry| &entry.kind),
                Some(InstructionKind::StorageLive(target)) if *target == local) {
                return Err(fail("persistent slot declaration changes its actual activation"));
            }
            let store = live.checked_add(1).ok_or_else(|| fail("persistent slot boundary overflow"))?;
            let Some(InstructionKind::Store { local: target, value: array }) =
                actual.instructions.get(store).map(|entry| &entry.kind) else {
                    return Err(fail("persistent slot has no immediate initialized shell Store"));
                };
            if *target != local || function.values.get(array.0 as usize) != Some(physical) {
                return Err(fail("persistent slot initialization changes its exact shell"));
            }
            operations::available_before(function, (block, store, SemanticSiteEdge::Before), *array, b)?;
            let (_, _, producer) = self.defining(function, *array, b)?;
            let InstructionKind::MakeOwnedArray { elements } = &producer.kind else {
                return Err(fail("persistent slot initializer is not a real owning array construction"));
            };
            let [initial] = elements.as_slice() else {
                return Err(fail("persistent slot initializer changes its single element"));
            };
            if function.values.get(initial.0 as usize) != Some(&logical.ty) {
                return Err(fail("persistent slot initializer changes its complete child type"));
            }
            if selected.is_some() {
                return Err(unknown("persistent slot has competing original activation witnesses"));
            }
            selected = Some(Slot { local, live: (block, live), store: (block, store),
                array: *array, initial: *initial, logical });
        }
        Ok(selected)
    }

    /// Reconstruct the logical element from every initialized reaching write.
    /// Array geometry alone is never a live-element fact.
    pub(super) fn persistent_slot_value(
        &self, instance: &SemanticInstance, function: &TypedFunction, slot: &Slot,
        at: (BlockId, usize), b: &mut Budget, depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("persistent slot exceeds its finite value ancestry"));
        }
        operations::initialized_local_before(function, at, slot.local, b)?;
        let initialization = crate::shared_initialization(function, b)?;
        let mut pending = vec![at];
        let mut seen = BTreeSet::new();
        let mut selected: Option<Value> = None;
        while let Some((block, end)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((block, end)) { continue; }
            let actual = function.blocks.get(block.0 as usize)
                .filter(|entry| entry.id == block && end <= entry.instructions.len())
                .ok_or_else(|| fail("persistent slot read has no exact CFG boundary"))?;
            let mut source = None;
            for (index, instruction) in actual.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                match &instruction.kind {
                    InstructionKind::StoreOwnedArrayElement { local, index: element, value }
                        if *local == slot.local => {
                        if *element != 0 { return Err(fail("persistent slot write selects another element")); }
                        source = Some(*value);
                        break;
                    }
                    InstructionKind::Store { local, value } if *local == slot.local => {
                        if (block, index) != slot.store || *value != slot.array {
                            return Err(fail("persistent slot read crosses an unrelated shell replacement"));
                        }
                        source = Some(slot.initial);
                        break;
                    }
                    InstructionKind::TakeOwnedArrayElement { local, index }
                    | InstructionKind::DropOwnedArrayElement { local, index } if *local == slot.local => {
                        if *index != 0 { return Err(fail("persistent slot removal selects another element")); }
                        return Err(fail("persistent slot read crosses a vacant element"));
                    }
                    InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local } | InstructionKind::DropOwnedArray { local }
                        if *local == slot.local => {
                        return Err(fail("persistent slot read crosses a vacant activation"));
                    }
                    _ => {}
                }
            }
            if let Some(source) = source {
                let value = self.ssa_value(instance, function, source, b, depth + 1)?;
                if !value.equal_paid(&slot.logical, b)? {
                    return Err(fail("persistent slot write changes its complete original child domain"));
                }
                if let Some(old) = &selected {
                    if !old.equal_paid(&value, b)? {
                        return Err(fail("persistent slot join changes its complete child classes"));
                    }
                }
                selected = Some(value);
                continue;
            }
            if block == function.entry {
                return Err(fail("persistent slot reaches entry without an initialized element"));
            }
            let mut incoming = false;
            for predecessor in &initialization.predecessors[block.0 as usize] {
                b.spend(1, WORK)?;
                if !initialization.reachable[*predecessor] { continue; }
                let previous = &function.blocks[*predecessor];
                b.spend(pending.len().saturating_add(1), WORK)?;
                pending.push((previous.id, previous.instructions.len()));
                incoming = true;
            }
            if !incoming { return Err(fail("persistent slot read has no initialized predecessor")); }
        }
        selected.ok_or_else(|| fail("persistent slot cycle has no initialized element origin"))
    }

    /// Physical shell geometry preserves the whole child forest, including
    /// phantom rows. This creates no access, provenance, epoch or cleanup fact.
    pub(super) fn persistent_slot_array_value(
        &self, function: &TypedFunction, slot: &Slot, child: &Value, b: &mut Budget,
    ) -> CompileResult<Value> {
        if !child.equal_paid(&slot.logical, b)? || !child.life.slots.is_empty()
            || child.classes.len() != child.life.binder_count as usize {
            return Err(fail("persistent shell changes its complete owning closure domain"));
        }
        let mut life = value::empty();
        life.binder_count = child.life.binder_count;
        for class in 0..child.life.binder_count {
            b.spend(life.retained.len().saturating_add(2), WORK)?;
            life.retained.push(StorageRetainedBinding {
                path: vec![LifeTypeProjection::Element], class, binder: class,
            });
        }
        for row in &child.life.retained {
            b.spend(row.path.len().saturating_add(life.retained.len()).saturating_add(2), WORK)?;
            let mut path = vec![LifeTypeProjection::Element];
            path.extend_from_slice(&row.path);
            life.retained.push(StorageRetainedBinding { path, class: row.class, binder: row.binder });
        }
        for row in &child.life.nominal_arguments {
            b.spend(row.path.len().saturating_add(life.nominal_arguments.len()).saturating_add(2), WORK)?;
            let mut path = vec![LifeTypeProjection::Element];
            path.extend_from_slice(&row.path);
            life.nominal_arguments.push(NominalLifeArgument { path, declaration: row.declaration,
                argument: row.argument, binder: row.binder });
        }
        b.spend(child.classes.len().saturating_add(1), WORK)?;
        let classes: Vec<Class> = child.classes.clone();
        let physical = &function.locals.get(slot.local.0 as usize)
            .filter(|entry| entry.id == slot.local)
            .ok_or_else(|| fail("persistent shell loses its actual local"))?.ty;
        value::borrowed_full(physical, &life, classes, self.inventory, b)
    }
}
