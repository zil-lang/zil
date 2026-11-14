//! Exact reaching worlds. The join is a union of whole states, never an
//! equality between two lifetime images. A repeated loop state is the fixed
//! point; every new state and comparison is paid from the request budget.
use super::*;
use std::collections::{BTreeSet, VecDeque};

#[derive(PartialEq, Eq)]
pub(super) struct State {
    pub locals: Vec<Option<SemanticTypeValue>>,
    pub values: Vec<Option<SemanticTypeValue>>,
    pub declared: BTreeMap<LocalId, SemanticTypeValue>,
    pub implicit: BTreeMap<u32, SemanticClassExpr>,
    pub vacant_captures: BTreeSet<(SemanticInstanceId, u32)>,
    pub stale_values: BTreeSet<ValueId>,
    pub stale_locals: BTreeSet<LocalId>,
    pub scopes: Vec<ScopeId>,
    pub unsafe_regions: Vec<u32>,
    pub owners: BTreeMap<LocalId, ScopeId>,
    pub vacant: BTreeSet<(LocalId, Vec<PlaceProjection>)>,
    pub role_sources: BTreeMap<LocalId, roles::Source>,
    pub value_role_sources: BTreeMap<ValueId, roles::Source>,
    pub reborrows: BTreeMap<ValueId, arguments::Edge>,
    pub value_roots: BTreeMap<ValueId, BTreeSet<arguments::Root>>,
    pub local_roots: BTreeMap<LocalId, BTreeSet<arguments::Root>>,
}
impl State {
    pub fn snapshot(&self, world: &mut World<'_, '_>) -> R<Self> {
        world.pay(
            self.locals
                .len()
                .saturating_add(self.values.len())
                .saturating_add(self.declared.len())
                .saturating_add(self.implicit.len())
                .saturating_add(self.stale_values.len())
                .saturating_add(self.vacant_captures.len())
                .saturating_add(self.stale_locals.len())
                .saturating_add(self.scopes.len())
                .saturating_add(self.unsafe_regions.len())
                .saturating_add(self.owners.len())
                .saturating_add(self.vacant.len())
                .saturating_add(self.role_sources.len())
                .saturating_add(self.value_role_sources.len())
                .saturating_add(self.reborrows.len())
                .saturating_add(self.value_roots.len())
                .saturating_add(self.local_roots.len())
                .saturating_add(1),
        )?;
        for (_, path) in &self.vacant {
            world.pay(path.len())?;
        }
        let mut locals = Vec::with_capacity(self.locals.len());
        for value in &self.locals {
            locals.push(value.as_ref().map(|v| world.copy(v)).transpose()?);
        }
        let mut values = Vec::with_capacity(self.values.len());
        for value in &self.values {
            values.push(value.as_ref().map(|v| world.copy(v)).transpose()?);
        }
        let mut declared = BTreeMap::new();
        for (local, value) in &self.declared {
            declared.insert(*local, world.copy(value)?);
        }
        for value in self.implicit.values() {
            crate::dyn_metadata::semantic_operand_snapshot(value, world.limits, world.left, 1)?;
        }
        let mut role_sources = BTreeMap::new();
        for (local, source) in &self.role_sources {
            role_sources.insert(*local, roles::copy_source(world, source)?);
        }
        let mut value_role_sources = BTreeMap::new();
        for (value, source) in &self.value_role_sources {
            value_role_sources.insert(*value, roles::copy_source(world, source)?);
        }
        let mut reborrows = BTreeMap::new();
        for (value, edge) in &self.reborrows {
            reborrows.insert(
                *value,
                arguments::Edge {
                    parent: edge.parent,
                    source: roles::copy_source(world, &edge.source)?,
                },
            );
        }
        let mut value_roots = BTreeMap::new();
        for (value, roots) in &self.value_roots {
            value_roots.insert(*value, arguments::copy_roots(world, roots)?);
        }
        let mut local_roots = BTreeMap::new();
        for (local, roots) in &self.local_roots {
            local_roots.insert(*local, arguments::copy_roots(world, roots)?);
        }
        Ok(Self {
            locals,
            values,
            declared,
            implicit: self.implicit.clone(),
            vacant_captures: self.vacant_captures.clone(),
            stale_values: self.stale_values.clone(),
            stale_locals: self.stale_locals.clone(),
            scopes: self.scopes.clone(),
            unsafe_regions: self.unsafe_regions.clone(),
            owners: self.owners.clone(),
            vacant: self.vacant.clone(),
            role_sources,
            value_role_sources,
            reborrows,
            value_roots,
            local_roots,
        })
    }
    fn charge_compare(&self, world: &mut World<'_, '_>) -> R<()> {
        world.pay(
            self.locals
                .len()
                .saturating_add(self.values.len())
                .saturating_add(self.declared.len())
                .saturating_add(self.implicit.len())
                .saturating_add(self.stale_locals.len())
                .saturating_add(self.stale_values.len())
                .saturating_add(self.vacant_captures.len())
                .saturating_add(self.scopes.len())
                .saturating_add(self.unsafe_regions.len())
                .saturating_add(self.owners.len())
                .saturating_add(self.vacant.len())
                .saturating_add(self.role_sources.len())
                .saturating_add(self.value_role_sources.len())
                .saturating_add(self.reborrows.len())
                .saturating_add(self.value_roots.len())
                .saturating_add(self.local_roots.len()),
        )?;
        for (_, path) in &self.vacant {
            world.pay(path.len())?;
        }
        for value in self
            .locals
            .iter()
            .chain(&self.values)
            .flatten()
            .chain(self.declared.values())
        {
            crate::dyn_metadata::semantic_operand_snapshot(value, world.limits, world.left, 1)?;
        }
        for value in self.implicit.values() {
            crate::dyn_metadata::semantic_operand_snapshot(value, world.limits, world.left, 1)?;
        }
        for source in self
            .role_sources
            .values()
            .chain(self.value_role_sources.values())
        {
            if let roles::Source::Class(image) = source {
                crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
            }
        }
        for edge in self.reborrows.values() {
            if let roles::Source::Class(image) = &edge.source {
                crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
            }
        }
        for roots in self.value_roots.values().chain(self.local_roots.values()) {
            world.pay(roots.len())?;
            for root in roots {
                world.pay(root.path.len())?;
            }
        }
        Ok(())
    }
}
pub(super) struct Cfg {
    blocks: BTreeMap<BlockId, usize>,
    definitions: Vec<usize>,
    dominators: Vec<BTreeSet<usize>>,
    reachable_from: Vec<BTreeSet<usize>>,
    instance: SemanticInstanceId,
}
impl Cfg {
    pub fn new(world: &mut World<'_, '_>, function: &TypedFunction) -> R<Self> {
        world.pay(world.program.instances.len())?;
        let mut instances = world.program.instances.iter().filter(|instance| matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function.id));
        let instance = instances
            .next()
            .ok_or_else(|| bad("class CFG has no real original instance"))?
            .id;
        if instances.next().is_some() {
            return Err(bad("class CFG has ambiguous original instances"));
        }
        let count = function.blocks.len();
        world.pay(
            count
                .saturating_mul(count)
                .saturating_mul(3)
                .saturating_add(function.values.len())
                .saturating_add(count),
        )?;
        let mut blocks = BTreeMap::new();
        let mut definitions = vec![usize::MAX; function.values.len()];
        for (index, block) in function.blocks.iter().enumerate() {
            if blocks.insert(block.id, index).is_some() {
                return Err(bad("class CFG repeats a block identity"));
            }
            world.pay(block.instructions.len())?;
            for instruction in &block.instructions {
                if let Some(result) = instruction.result {
                    let definition = definitions
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("class CFG defines an absent SSA value"))?;
                    if *definition != usize::MAX {
                        return Err(bad("class CFG repeats an SSA definition"));
                    }
                    *definition = index;
                }
            }
        }
        if definitions.contains(&usize::MAX) {
            return Err(bad("class CFG has an SSA value without a definition"));
        }
        let entry = *blocks
            .get(&function.entry)
            .ok_or_else(|| bad("class CFG entry is absent"))?;
        let mut predecessors = vec![Vec::new(); count];
        let mut successors = vec![Vec::new(); count];
        for (index, block) in function.blocks.iter().enumerate() {
            let targets = match block.terminator {
                Terminator::Jump(target) => vec![target],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block, else_block],
                _ => Vec::new(),
            };
            for target in targets {
                world.pay(1)?;
                let target = *blocks
                    .get(&target)
                    .ok_or_else(|| bad("class CFG successor is absent"))?;
                predecessors[target].push(index);
                successors[index].push(target);
            }
        }
        let mut reachable = BTreeSet::new();
        let mut work = vec![entry];
        while let Some(index) = work.pop() {
            world.pay(successors[index].len().saturating_add(1))?;
            if reachable.insert(index) {
                work.extend(successors[index].iter().copied());
            }
        }
        let mut dominators = vec![BTreeSet::new(); count];
        for index in &reachable {
            world.pay(reachable.len())?;
            dominators[*index] = if *index == entry {
                BTreeSet::from([entry])
            } else {
                reachable.clone()
            };
        }
        loop {
            let mut changed = false;
            for index in &reachable {
                if *index == entry {
                    continue;
                }
                world.pay(reachable.len())?;
                let mut common = reachable.clone();
                for predecessor in &predecessors[*index] {
                    if !reachable.contains(predecessor) {
                        continue;
                    }
                    world.pay(common.len())?;
                    common.retain(|value| dominators[*predecessor].contains(value));
                }
                common.insert(*index);
                world.pay(common.len().saturating_add(dominators[*index].len()))?;
                if common != dominators[*index] {
                    dominators[*index] = common;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut reachable_from = Vec::with_capacity(count);
        for start in 0..count {
            let mut reached = BTreeSet::new();
            let mut pending = vec![start];
            while let Some(block) = pending.pop() {
                world.pay(successors[block].len().saturating_add(1))?;
                if reached.insert(block) {
                    pending.extend(successors[block].iter().copied());
                }
            }
            reachable_from.push(reached);
        }
        Ok(Self {
            blocks,
            definitions,
            dominators,
            reachable_from,
            instance,
        })
    }
    pub fn index(&self, block: BlockId) -> R<usize> {
        self.blocks
            .get(&block)
            .copied()
            .ok_or_else(|| bad("capture CFG block is absent"))
    }
    pub fn retain_dominating<T>(&self, index: usize, values: &mut [Option<T>]) {
        for (value, definition) in values.iter_mut().zip(&self.definitions) {
            if *definition == index || !self.dominators[index].contains(definition) {
                *value = None;
            }
        }
    }
    pub fn check_sites(
        &self,
        world: &mut World<'_, '_>,
        function: &TypedFunction,
        sites: &[&SemanticSite],
    ) -> R<()> {
        world.pay(sites.len())?;
        for site in sites {
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge,
            } = site.at
            else {
                return Err(bad("Core site survives the actual class world"));
            };
            let index = *self
                .blocks
                .get(&block)
                .ok_or_else(|| bad("actual site block is absent"))?;
            let block = &function.blocks[index];
            let instruction = instruction as usize;
            if instruction > block.instructions.len() {
                return Err(bad("actual site lies outside its block"));
            }
            if matches!(site.action, SemanticSiteAction::TypeUse { .. }) {
                if edge != SemanticSiteEdge::Before {
                    return Err(bad("type use changes its actual Before boundary"));
                }
                continue;
            }
            if edge != SemanticSiteEdge::After || instruction == block.instructions.len() {
                return Err(bad(
                    "operation site has no actual After instruction boundary",
                ));
            }
            let actual = &block.instructions[instruction];
            let valid = match (&site.action, &actual.kind) {
                (
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(a),
                        ..
                    },
                    InstructionKind::StorageLive(b),
                ) => a == b,
                (
                    SemanticSiteAction::Construct {
                        result: SemanticOperand::Value(id),
                        ..
                    },
                    InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::MakeOwnedArray { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::MakeVariant { .. },
                ) => actual.result == Some(*id),
                (
                    SemanticSiteAction::Closure {
                        result: SemanticOperand::Value(id),
                        closure: SemanticClosureTarget::Typed(a),
                        ..
                    },
                    InstructionKind::MakeClosure { closure: b, .. },
                ) => a == b && actual.result == Some(*id),
                (
                    SemanticSiteAction::Call {
                        target: SemanticFunctionTarget::Typed(a),
                        ..
                    },
                    InstructionKind::Call { function: b, .. }
                    | InstructionKind::ForeignCall { function: b, .. }
                    | InstructionKind::SuspendCall { function: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::CallableCall {
                        callee: SemanticOperand::Local(a),
                    },
                    InstructionKind::CallClosureShared { callee: b, .. }
                    | InstructionKind::CallClosureMut { callee: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::CallableCall {
                        callee: SemanticOperand::Value(a),
                    },
                    InstructionKind::CallClosure { callee: b, .. }
                    | InstructionKind::CallIndirect { callee: b, .. }
                    | InstructionKind::CallClosureOnce { callee: b, .. }
                    | InstructionKind::CallClosureBorrowed { callee: b, .. },
                ) => a == b,
                (
                    SemanticSiteAction::BodyCall { .. },
                    InstructionKind::Call { .. }
                    | InstructionKind::ForeignCall { .. }
                    | InstructionKind::SuspendCall { .. }
                    | InstructionKind::CallIndirect { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. },
                ) => true,
                (SemanticSiteAction::BodyCall { .. }, actual) => {
                    let owner = world.instance(site.instance)?.definition;
                    let ledger = world.ledger;
                    let proof = ledger.original.borrow();
                    if let Some(kind) = proof
                        .as_ref()
                        .and_then(|p| p.intrinsics.get(&(owner, site.recipe)))
                        .map(|c| &c.kind)
                    {
                        super::intrinsic_operation_matches(kind, actual, world.left)?
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !valid {
                return Err(bad(
                    "original site does not match its actual operation and operand",
                ));
            }
        }
        Ok(())
    }
    fn loop_occurrence(
        &self,
        world: &mut World<'_, '_>,
        entry: usize,
        value: Option<&SemanticTypeValue>,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<bool> {
        let Some(SemanticTypeValue::Resolved { classes, .. }) = value else {
            return Ok(false);
        };
        world.pay(classes.len())?;
        for image in classes {
            if self.loop_image(world, entry, image, implicit)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn loop_image(
        &self,
        world: &mut World<'_, '_>,
        entry: usize,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<bool> {
        world.pay(1)?;
        if let SemanticClassExpr::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = image
        {
            if let Some((unknown, sources)) =
                roles::loop_sources(world, self.instance, *ordinal, implicit)?
            {
                if unknown {
                    return Ok(true);
                }
                for source in sources {
                    if self.loop_image(world, entry, &source, implicit)? {
                        return Ok(true);
                    }
                }
            }
        }
        if let SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(id),
            ..
        }
        | SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Value(id),
            ..
        } = image
        {
            let definition = *self
                .definitions
                .get(id.0 as usize)
                .ok_or_else(|| bad("loop role source selects an absent SSA occurrence"))?;
            return Ok(self.reachable_from[entry].contains(&definition)
                && self.reachable_from[definition].contains(&entry));
        }
        Ok(false)
    }
    pub fn offer(
        &self,
        world: &mut World<'_, '_>,
        source: Option<usize>,
        target: BlockId,
        mut state: State,
        reaching: &mut [Vec<State>],
        work: &mut VecDeque<(usize, usize)>,
    ) -> R<()> {
        world.pay(state.values.len().saturating_add(1))?;
        let index = *self
            .blocks
            .get(&target)
            .ok_or_else(|| bad("class CFG successor is absent"))?;
        // A loop may carry an old physical occurrence while reexecuting its
        // producer. Its public SSA spelling cannot make the two epochs equal.
        // Mark the occupant, not the spelling: replacement clears this mark.
        if source.is_some_and(|source| {
            self.reachable_from[index].contains(&source)
                && (self.dominators[source].contains(&index) || index <= source)
        }) {
            for (ordinal, value) in state.locals.iter().enumerate() {
                if self.loop_occurrence(world, index, value.as_ref(), &state.implicit)? {
                    state.stale_locals.insert(LocalId(ordinal as u32));
                }
            }
            for (ordinal, value) in state.values.iter().enumerate() {
                if self.loop_occurrence(world, index, value.as_ref(), &state.implicit)? {
                    state.stale_values.insert(ValueId(ordinal as u32));
                }
            }
            for (local, source) in &state.role_sources {
                if let roles::Source::Class(image) = source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_locals.insert(*local);
                    }
                }
            }
            for (value, source) in &state.value_role_sources {
                if let roles::Source::Class(image) = source {
                    if self.loop_image(world, index, image, &state.implicit)? {
                        state.stale_values.insert(*value);
                    }
                }
            }
        }
        // Only definitions strictly dominating entry to this block survive.
        // In particular a previous loop iteration cannot initialize this
        // iteration's SSA, and a sibling branch cannot donate a same-numbered value.
        for (value, definition) in state.values.iter_mut().zip(&self.definitions) {
            if *definition == index || !self.dominators[index].contains(definition) {
                *value = None;
            }
        }
        state
            .stale_values
            .retain(|id| state.values[id.0 as usize].is_some());
        state
            .stale_locals
            .retain(|id| state.locals[id.0 as usize].is_some());
        state
            .role_sources
            .retain(|id, _| state.locals[id.0 as usize].is_some());
        state
            .value_role_sources
            .retain(|id, _| state.values[id.0 as usize].is_some());
        for old in &reaching[index] {
            state.charge_compare(world)?;
            old.charge_compare(world)?;
            if old == &state {
                return Ok(());
            }
        }
        world.pay(1)?;
        let ordinal = reaching[index].len();
        reaching[index].push(state);
        work.push_back((index, ordinal));
        Ok(())
    }
}
