//! Exact representation witnesses for original closure bindings stored in an
//! owning one-element array. A witness grants no initialization or drop right.
use super::*;
use crate::semantic::value::{Class, Value};

pub(super) struct Slot {
    function: FunctionId,
    instance: SemanticInstanceId,
    local: LocalId,
    live: (BlockId, usize),
    store: (BlockId, usize),
    array: ValueId,
    initial: ValueId,
    logical: Value,
}

impl Slot {
    pub(super) fn local(&self) -> LocalId {
        self.local
    }
    pub(super) fn logical(&self) -> &Value {
        &self.logical
    }
    pub(super) fn declaration(&self) -> (BlockId, usize) {
        self.live
    }
    pub(super) fn initial_store(&self) -> (BlockId, usize) {
        self.store
    }
    pub(super) fn array(&self) -> ValueId {
        self.array
    }
    pub(super) fn initial(&self) -> ValueId {
        self.initial
    }
}

impl Check<'_, '_> {
    pub(super) fn persistent_array_slot(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<Option<Slot>> {
        if !matches!(function.values.get(value.0 as usize),
            Some(Type::Array { length: 1, element }) if matches!(element.as_ref(), Type::Closure(_)))
        {
            return Ok(None);
        }
        let mut selected = None;
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                let InstructionKind::Store {
                    local,
                    value: stored,
                } = instruction.kind
                else {
                    continue;
                };
                if stored != value {
                    continue;
                }
                let Some(slot) = self.persistent_slot(instance, function, local, b)? else {
                    continue;
                };
                if slot.array != value {
                    continue;
                }
                if selected.is_some() {
                    return Err(fail(
                        "persistent array has competing original slot destinations",
                    ));
                }
                selected = Some(slot);
            }
        }
        Ok(selected)
    }

    pub(super) fn persistent_slot(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        b: &mut Budget,
    ) -> CompileResult<Option<Slot>> {
        b.spend(1, WORK)?;
        let physical = &function
            .locals
            .get(local.0 as usize)
            .filter(|entry| entry.id == local)
            .ok_or_else(|| fail("persistent slot has no exact physical local"))?
            .ty;
        let Type::Array { element, length: 1 } = physical else {
            return Ok(None);
        };
        if !matches!(element.as_ref(), Type::Closure(_)) {
            return Ok(None);
        }
        let SemanticInstanceTarget::Function(target) = &instance.target else {
            return Err(fail("persistent slot has no actual function world"));
        };
        if self.inventory.function(target, b)?.id != function.id {
            return Err(fail("persistent slot changes its exact function world"));
        }
        let definition = self.inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites: recipes, .. } = &definition.body
        else {
            return Err(fail(
                "persistent slot has no complete original declaration body",
            ));
        };
        let mut selected = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id {
                continue;
            }
            let SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(actual),
                expected,
            } = &site.action
            else {
                continue;
            };
            if *actual != local {
                continue;
            }
            b.spend(recipes.len(), WORK)?;
            let recipe = recipes
                .iter()
                .find(|recipe| recipe.id == site.recipe)
                .ok_or_else(|| fail("persistent slot lacks its original declaration recipe"))?;
            let SemanticSiteRecipeKind::DeclareLocal { expected: original } = &recipe.kind else {
                return Err(fail(
                    "persistent slot replaces its original declaration operation",
                ));
            };
            let logical = self.recipe_value(instance, original, b, 0)?;
            // An ordinary user array declaration does not establish this bridge.
            if !matches!(logical.ty, Type::Closure(_)) {
                continue;
            }
            if !geometry::same_type(&logical.ty, element, b)? {
                return Err(fail(
                    "persistent slot changes its complete original closure type",
                ));
            }
            let claim = self.resolved(instance, expected, b, 0)?;
            if !self.capture_values_equal(&claim, &logical, b)? {
                return Err(fail(
                    "persistent slot claim changes its complete original closure value",
                ));
            }
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge: SemanticSiteEdge::After,
            } = site.at
            else {
                return Err(fail(
                    "persistent slot lacks its exact After StorageLive boundary",
                ));
            };
            let live = instruction as usize;
            let actual = function
                .blocks
                .get(block.0 as usize)
                .filter(|entry| entry.id == block)
                .ok_or_else(|| fail("persistent slot has no actual declaration block"))?;
            if !matches!(actual.instructions.get(live).map(|entry| &entry.kind),
                Some(InstructionKind::StorageLive(target)) if *target == local)
            {
                return Err(fail(
                    "persistent slot declaration changes its actual activation",
                ));
            }
            let store = live
                .checked_add(1)
                .ok_or_else(|| fail("persistent slot boundary overflow"))?;
            let Some(InstructionKind::Store {
                local: target,
                value: array,
            }) = actual.instructions.get(store).map(|entry| &entry.kind)
            else {
                return Err(fail(
                    "persistent slot has no immediate initialized shell Store",
                ));
            };
            if *target != local || function.values.get(array.0 as usize) != Some(physical) {
                return Err(fail(
                    "persistent slot initialization changes its exact shell",
                ));
            }
            operations::available_before(
                function,
                (block, store, SemanticSiteEdge::Before),
                *array,
                b,
            )?;
            let (producer_block, producer_index, producer) = self.defining(function, *array, b)?;
            let InstructionKind::MakeOwnedArray { elements } = &producer.kind else {
                return Err(fail(
                    "persistent slot initializer is not a real owning array construction",
                ));
            };
            let [initial] = elements.as_slice() else {
                return Err(fail(
                    "persistent slot initializer changes its single element",
                ));
            };
            if function.values.get(initial.0 as usize) != Some(&logical.ty) {
                return Err(fail(
                    "persistent slot initializer changes its complete child type",
                ));
            }
            operations::available_before(
                function,
                (producer_block, producer_index, SemanticSiteEdge::Before),
                *initial,
                b,
            )?;
            // The shell is an implementation detail, never an addressable or
            // transferred source value. Otherwise an untracked whole-array
            // alias could replace its element behind a logical source query.
            for owner_block in &function.blocks {
                for (ordinal, operation) in owner_block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    match &operation.kind {
                        InstructionKind::Load(target)
                        | InstructionKind::TakeLocal { local: target }
                        | InstructionKind::BorrowLocal { local: target }
                        | InstructionKind::BorrowLocalMut { local: target }
                        | InstructionKind::BorrowFieldLocal { local: target, .. }
                        | InstructionKind::BorrowPlaceLocal { local: target, .. }
                        | InstructionKind::ReservePlaceLocal { local: target, .. }
                            if *target == local =>
                        {
                            return Err(fail(
                                "persistent closure shell escapes its exact element operations",
                            ));
                        }
                        InstructionKind::Store {
                            local: target,
                            value,
                        } if *target == local
                            && ((owner_block.id, ordinal) != (block, store) || value != array) =>
                        {
                            return Err(fail(
                                "persistent closure shell has an unrelated whole replacement",
                            ));
                        }
                        InstructionKind::TakeOwnedArrayElement {
                            local: target,
                            index,
                        }
                        | InstructionKind::DropOwnedArrayElement {
                            local: target,
                            index,
                        }
                        | InstructionKind::StoreOwnedArrayElement {
                            local: target,
                            index,
                            ..
                        }
                        | InstructionKind::BorrowOwnedArrayElement {
                            local: target,
                            index,
                            ..
                        } if *target == local && *index != 0 => {
                            return Err(fail(
                                "persistent closure shell changes its exact element selector",
                            ));
                        }
                        _ => {}
                    }
                }
            }
            if selected.is_some() {
                return Err(unknown(
                    "persistent slot has competing original activation witnesses",
                ));
            }
            selected = Some(Slot {
                function: function.id,
                instance: instance.id,
                local,
                live: (block, live),
                store: (block, store),
                array: *array,
                initial: *initial,
                logical,
            });
        }
        Ok(selected)
    }

    /// Reconstruct the logical element from every initialized reaching write.
    /// Array geometry alone is never a live-element fact.
    pub(super) fn persistent_slot_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        slot: &Slot,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        // Always prove this exact read's initialization and reaching writes,
        // including when a transfer equation recursively reads the same slot.
        self.persistent_slot_sources(instance, function, slot, at, b, depth)?;
        let key = (instance.id, function.id, slot.local);
        {
            let active = self.slot_queries.borrow();
            b.lookup(active.len(), WORK)?;
            if let Some(seed) = active.get(&key) {
                return match seed {
                    Some(seed) => seed.copy(b),
                    None => Err(unknown(
                        "persistent slot has a cyclic initializer without an actual seed",
                    )),
                };
            }
        }
        {
            let mut active = self.slot_queries.borrow_mut();
            b.lookup(active.len(), WORK)?;
            if active.len() >= b.helper_nodes {
                return Err(vec![zil_base::Diagnostic::limit(
                    zil_base::Phase::Analyze,
                    "persistent slot value equations exceed their finite query limit",
                    None,
                )]);
            }
            // Prepay removal now: cleanup must preserve the original error
            // even when the transfer proof exhausts its remaining budget.
            b.lookup(active.len().saturating_add(1), WORK)?;
            active.insert(key, None);
        }
        let result = (|| {
            // This seed comes from the actual dominating constructor, not the
            // local's asserted type/class metadata. Mark it only after proof.
            let seed = self.ssa_value(instance, function, slot.initial, b, depth + 1)?;
            if !seed.equal_paid(&slot.logical, b)? {
                return Err(fail(
                    "persistent slot initial value changes its complete original domain",
                ));
            }
            let copy = seed.copy(b)?;
            {
                let mut active = self.slot_queries.borrow_mut();
                b.lookup(active.len(), WORK)?;
                active.insert(key, Some(copy));
            }
            // Induction over every real refill, not just the current CFG arm.
            // Recursive reads may use the proved seed only while all these
            // transfer equations remain required by the enclosing query.
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::StoreOwnedArrayElement {
                        local,
                        index: element,
                        value,
                    } = instruction.kind
                    else {
                        continue;
                    };
                    if local != slot.local {
                        continue;
                    }
                    if element != 0 {
                        return Err(fail("persistent slot refill changes its element selector"));
                    }
                    operations::available_before(
                        function,
                        (block.id, index, SemanticSiteEdge::Before),
                        value,
                        b,
                    )?;
                    let incoming = self.ssa_value(instance, function, value, b, depth + 1)?;
                    if !incoming.equal_paid(&seed, b)? {
                        return Err(fail(
                            "persistent slot refill changes its independently seeded whole value",
                        ));
                    }
                }
            }
            Ok(seed)
        })();
        let mut active = self.slot_queries.borrow_mut();
        // No provisional seed survives its complete proof transaction.
        active.remove(&key);
        result
    }

    fn persistent_slot_sources(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        slot: &Slot,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<BTreeSet<ValueId>> {
        b.spend(1, WORK)?;
        if function.id != slot.function || instance.id != slot.instance {
            return Err(fail(
                "persistent slot read changes its exact original function world",
            ));
        }
        if depth > b.partial_depth {
            return Err(unknown("persistent slot exceeds its finite value ancestry"));
        }
        operations::initialized_local_before(function, at, slot.local, b)?;
        let initialization = crate::shared_initialization(function, b)?;
        let mut pending = vec![at];
        let mut seen = BTreeSet::new();
        let mut selected = BTreeSet::new();
        while let Some((block, end)) = pending.pop() {
            b.lookup(seen.len(), WORK)?;
            if !seen.insert((block, end)) {
                continue;
            }
            let actual = function
                .blocks
                .get(block.0 as usize)
                .filter(|entry| entry.id == block && end <= entry.instructions.len())
                .ok_or_else(|| fail("persistent slot read has no exact CFG boundary"))?;
            let mut source = None;
            for (index, instruction) in actual.instructions.iter().enumerate().take(end).rev() {
                b.spend(1, WORK)?;
                self.head_projection_barrier(function, slot.local, &[], &instruction.kind, b)?;
                match &instruction.kind {
                    InstructionKind::StoreOwnedArrayElement {
                        local,
                        index: element,
                        value,
                    } if *local == slot.local => {
                        if *element != 0 {
                            return Err(fail("persistent slot write selects another element"));
                        }
                        operations::available_before(
                            function,
                            (block, index, SemanticSiteEdge::Before),
                            *value,
                            b,
                        )?;
                        source = Some(*value);
                        break;
                    }
                    InstructionKind::Store { local, value } if *local == slot.local => {
                        if (block, index) != slot.store || *value != slot.array {
                            return Err(fail(
                                "persistent slot read crosses an unrelated shell replacement",
                            ));
                        }
                        source = Some(slot.initial);
                        break;
                    }
                    InstructionKind::TakeOwnedArrayElement { local, index }
                    | InstructionKind::DropOwnedArrayElement { local, index }
                        if *local == slot.local =>
                    {
                        if *index != 0 {
                            return Err(fail("persistent slot removal selects another element"));
                        }
                        return Err(fail("persistent slot read crosses a vacant element"));
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropOwnedArray { local }
                        if *local == slot.local =>
                    {
                        return Err(fail("persistent slot read crosses a vacant activation"));
                    }
                    _ => {}
                }
            }
            if let Some(source) = source {
                let actual = function
                    .values
                    .get(source.0 as usize)
                    .ok_or_else(|| fail("persistent slot source has no complete actual type"))?;
                if !geometry::same_type(actual, &slot.logical.ty, b)? {
                    return Err(fail(
                        "persistent slot reaching write changes its complete child type",
                    ));
                }
                b.lookup(selected.len(), WORK)?;
                selected.insert(source);
                continue;
            }
            if block == function.entry {
                return Err(fail(
                    "persistent slot reaches entry without an initialized element",
                ));
            }
            let mut incoming = false;
            for predecessor in &initialization.predecessors[block.0 as usize] {
                b.spend(1, WORK)?;
                if !initialization.reachable[*predecessor] {
                    continue;
                }
                let previous = &function.blocks[*predecessor];
                b.spend(pending.len().saturating_add(1), WORK)?;
                pending.push((previous.id, previous.instructions.len()));
                incoming = true;
            }
            if !incoming {
                return Err(fail("persistent slot read has no initialized predecessor"));
            }
        }
        if selected.is_empty() {
            return Err(fail(
                "persistent slot cycle has no initialized element origin",
            ));
        }
        Ok(selected)
    }

    /// Physical shell geometry preserves the whole child forest, including
    /// phantom rows. This creates no access, provenance, epoch or cleanup fact.
    pub(super) fn persistent_slot_array_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        slot: &Slot,
        child: &Value,
        b: &mut Budget,
    ) -> CompileResult<Value> {
        if function.id != slot.function || instance.id != slot.instance {
            return Err(fail(
                "persistent shell changes its exact original function world",
            ));
        }
        if !child.equal_paid(&slot.logical, b)?
            || !child.life.slots.is_empty()
            || child.classes.len() != child.life.binder_count as usize
        {
            return Err(fail(
                "persistent shell changes its complete owning closure domain",
            ));
        }
        let mut life = value::empty();
        life.binder_count = child.life.binder_count;
        for class in 0..child.life.binder_count {
            b.spend(life.retained.len().saturating_add(2), WORK)?;
            life.retained.push(StorageRetainedBinding {
                path: vec![LifeTypeProjection::Element],
                class,
                binder: class,
            });
        }
        for row in &child.life.retained {
            b.spend(
                row.path
                    .len()
                    .saturating_add(life.retained.len())
                    .saturating_add(2),
                WORK,
            )?;
            let mut path = vec![LifeTypeProjection::Element];
            path.extend_from_slice(&row.path);
            life.retained.push(StorageRetainedBinding {
                path,
                class: row.class,
                binder: row.binder,
            });
        }
        for row in &child.life.nominal_arguments {
            b.spend(
                row.path
                    .len()
                    .saturating_add(life.nominal_arguments.len())
                    .saturating_add(2),
                WORK,
            )?;
            let mut path = vec![LifeTypeProjection::Element];
            path.extend_from_slice(&row.path);
            life.nominal_arguments.push(NominalLifeArgument {
                path,
                declaration: row.declaration,
                argument: row.argument,
                binder: row.binder,
            });
        }
        b.spend(child.classes.len().saturating_add(1), WORK)?;
        let classes: Vec<Class> = child.classes.clone();
        let physical = &function
            .locals
            .get(slot.local.0 as usize)
            .filter(|entry| entry.id == slot.local)
            .ok_or_else(|| fail("persistent shell loses its actual local"))?
            .ty;
        value::borrowed_full(physical, &life, classes, self.inventory, b)
    }
}
