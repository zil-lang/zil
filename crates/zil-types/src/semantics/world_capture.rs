//! Reconstruct implicit captured heads from the real child CFG and its original
//! Name observation. A header row alone cannot seal a physical capture input.
use super::*;
use std::collections::{BTreeSet, VecDeque};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Other,
    Head(u32),
    Data(u32),
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    locals: Vec<Option<Origin>>,
    values: Vec<Option<Origin>>,
    scopes: Vec<ScopeId>,
    owners: BTreeMap<LocalId, ScopeId>,
}
impl State {
    fn cost(&self) -> usize {
        self.locals
            .len()
            .saturating_add(self.values.len())
            .saturating_add(self.scopes.len())
            .saturating_add(self.owners.len())
            .saturating_add(1)
    }
}
fn value(values: &[Option<Origin>], id: ValueId) -> R<Origin> {
    values
        .get(id.0 as usize)
        .and_then(|v| *v)
        .ok_or_else(|| bad("capture observation has no reaching initialized SSA"))
}
fn local(locals: &[Option<Origin>], id: LocalId) -> R<Origin> {
    locals
        .get(id.0 as usize)
        .and_then(|v| *v)
        .ok_or_else(|| bad("capture observation reads an uninitialized local"))
}
fn offer(
    world: &mut World<'_, '_>,
    cfg: &flow::Cfg,
    target: BlockId,
    mut state: State,
    seen: &mut [Vec<State>],
    work: &mut VecDeque<(usize, usize)>,
) -> R<()> {
    world.pay(state.cost())?;
    let index = cfg.index(target)?;
    cfg.retain_dominating(index, &mut state.values);
    for old in &seen[index] {
        world.pay(state.cost().saturating_add(old.cost()))?;
        if old == &state {
            return Ok(());
        }
    }
    world.pay(1)?;
    let ordinal = seen[index].len();
    seen[index].push(state);
    work.push_back((index, ordinal));
    Ok(())
}
fn observe(
    world: &mut World<'_, '_>,
    child: &SemanticInstance,
    definition: &SemanticDefinition,
    ordinal: u32,
    block: BlockId,
    index: usize,
    edge: SemanticSiteEdge,
    state: &State,
    found: &mut BTreeSet<u32>,
) -> R<()> {
    world.pay(
        definition
            .implicit_lifetimes
            .len()
            .saturating_add(world.program.sites.len()),
    )?;
    let occurrence = definition
        .implicit_lifetimes
        .iter()
        .find(|row| row.ordinal == ordinal)
        .ok_or_else(|| bad("captured Fixed class names an absent original implicit occurrence"))?;
    for site in &world.program.sites {
        if site.instance != child.id
            || !matches!(site.at, SemanticSiteAddress::Typed { block:b,instruction:i,edge:e } if b==block && i as usize==index && e==edge)
        {
            continue;
        }
        let recipe = world.original_recipe(definition, site.recipe)?;
        if recipe.occurrence_path != occurrence.occurrence_path {
            continue;
        }
        let (SemanticSiteRecipeKind::TypeUse { .. }, SemanticSiteAction::TypeUse { subject, .. }) =
            (&recipe.kind, &site.action)
        else {
            continue;
        };
        let origin = match subject {
            SemanticOperand::Value(id) => value(&state.values, *id)?,
            SemanticOperand::Local(id) => local(&state.locals, *id)?,
            _ => {
                return Err(bad(
                    "implicit capture observation has no actual local or SSA operand",
                ))
            }
        };
        let Origin::Data(capture) = origin else {
            return Err(bad(
                "implicit capture occurrence does not read its real capture data",
            ));
        };
        found.insert(capture);
    }
    Ok(())
}
pub(super) fn input(world: &mut World<'_, '_>, child: &SemanticInstance, ordinal: u32) -> R<u32> {
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = child.target
    else {
        return Err(bad("implicit capture child has no actual entry"));
    };
    let function = world.function(function)?;
    let definition = world.definition(child.definition)?;
    let cfg = flow::Cfg::new(world, function)?;
    world.pay(
        function
            .locals
            .len()
            .saturating_add(function.values.len())
            .saturating_add(function.blocks.len()),
    )?;
    let mut initial = State {
        locals: vec![None; function.locals.len()],
        values: vec![None; function.values.len()],
        scopes: Vec::new(),
        owners: BTreeMap::new(),
    };
    for parameter in &function.parameters {
        *initial
            .locals
            .get_mut(parameter.0 as usize)
            .ok_or_else(|| bad("capture entry parameter is absent"))? = Some(Origin::Other);
    }
    let mut seen: Vec<Vec<State>> = (0..function.blocks.len()).map(|_| Vec::new()).collect();
    let mut work = VecDeque::new();
    let mut found = BTreeSet::new();
    offer(world, &cfg, function.entry, initial, &mut seen, &mut work)?;
    while let Some((block_index, state_index)) = work.pop_front() {
        world.pay(seen[block_index][state_index].cost())?;
        let mut state = seen[block_index][state_index].clone();
        let block = &function.blocks[block_index];
        for (index, instruction) in block.instructions.iter().enumerate() {
            observe(
                world,
                child,
                definition,
                ordinal,
                block.id,
                index,
                SemanticSiteEdge::Before,
                &state,
                &mut found,
            )?;
            world.pay(1)?;
            let produced = match &instruction.kind {
                InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index } => {
                    Some(Origin::Head(*index))
                }
                InstructionKind::Load(id) => Some(local(&state.locals, *id)?),
                InstructionKind::TakeLocal { local: id } => {
                    let old = local(&state.locals, *id)?;
                    state.locals[id.0 as usize] = None;
                    Some(old)
                }
                InstructionKind::Store {
                    local: id,
                    value: id_value,
                } => {
                    let origin = value(&state.values, *id_value)?;
                    *state
                        .locals
                        .get_mut(id.0 as usize)
                        .ok_or_else(|| bad("capture store local is absent"))? = Some(origin);
                    None
                }
                InstructionKind::StorageLive(id) | InstructionKind::StorageDead(id) => {
                    *state
                        .locals
                        .get_mut(id.0 as usize)
                        .ok_or_else(|| bad("capture activation local is absent"))? = None;
                    state.owners.remove(id);
                    if matches!(instruction.kind, InstructionKind::StorageLive(_)) {
                        if let Some(scope) = state.scopes.last() {
                            state.owners.insert(*id, *scope);
                        }
                    }
                    None
                }
                InstructionKind::ReadAddress { address } => {
                    Some(match value(&state.values, *address)? {
                        Origin::Head(index) => Origin::Data(index),
                        _ => Origin::Other,
                    })
                }
                InstructionKind::ReborrowAddress { address, .. } => {
                    Some(value(&state.values, *address)?)
                }
                InstructionKind::ScopeEnter { scope } => {
                    if state.scopes.len() >= world.limits.max_call_depth {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "capture scope depth exceeded",
                            None,
                        ));
                    }
                    if state.scopes.contains(scope) {
                        return Err(bad("capture CFG reenters an active scope"));
                    }
                    state.scopes.push(*scope);
                    None
                }
                InstructionKind::ScopeExit { scope, .. } => {
                    if state.scopes.pop() != Some(*scope) {
                        return Err(bad("capture CFG exits another active scope"));
                    }
                    world.pay(state.owners.len())?;
                    for (id, owner) in &state.owners {
                        if owner == scope {
                            state.locals[id.0 as usize] = None;
                        }
                    }
                    state.owners.retain(|_, owner| owner != scope);
                    None
                }
                InstructionKind::DropManaged { local: id }
                | InstructionKind::DropClosure { local: id }
                | InstructionKind::DropMutexGuard { local: id }
                | InstructionKind::DropPackage { local: id }
                | InstructionKind::DropTuple { local: id }
                | InstructionKind::DropEnum { local: id }
                | InstructionKind::DropErrorUnion { local: id }
                | InstructionKind::DropOwnedArray { local: id }
                | InstructionKind::DropDynOwned { local: id }
                | InstructionKind::EndPackageCapture { local: id, .. } => {
                    *state
                        .locals
                        .get_mut(id.0 as usize)
                        .ok_or_else(|| bad("capture cleanup local is absent"))? = None;
                    None
                }
                InstructionKind::BindPackageCapture { local: id, .. } => {
                    *state
                        .locals
                        .get_mut(id.0 as usize)
                        .ok_or_else(|| bad("capture view local is absent"))? = Some(Origin::Other);
                    None
                }
                InstructionKind::UnpackTuple { local: id, fields } => {
                    local(&state.locals, *id)?;
                    state.locals[id.0 as usize] = None;
                    world.pay(fields.len())?;
                    for field in fields {
                        *state
                            .locals
                            .get_mut(field.0 as usize)
                            .ok_or_else(|| bad("capture tuple destination is absent"))? =
                            Some(Origin::Other);
                    }
                    None
                }
                _ => instruction.result.map(|_| Origin::Other),
            };
            if let Some(id) = instruction.result {
                *state
                    .values
                    .get_mut(id.0 as usize)
                    .ok_or_else(|| bad("capture producer SSA is absent"))? = Some(
                    produced.ok_or_else(|| bad("capture SSA producer has no reaching equation"))?,
                );
            }
            observe(
                world,
                child,
                definition,
                ordinal,
                block.id,
                index,
                SemanticSiteEdge::After,
                &state,
                &mut found,
            )?;
        }
        observe(
            world,
            child,
            definition,
            ordinal,
            block.id,
            block.instructions.len(),
            SemanticSiteEdge::Before,
            &state,
            &mut found,
        )?;
        let successors = match block.terminator {
            Terminator::Jump(id) => vec![id],
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                value(&state.values, condition)?;
                vec![then_block, else_block]
            }
            Terminator::Return(_) | Terminator::Trap => Vec::new(),
            _ => return Err(bad("capture CFG has an unproved reachable terminator")),
        };
        for successor in successors {
            world.pay(state.cost())?;
            offer(world, &cfg, successor, state.clone(), &mut seen, &mut work)?;
        }
    }
    if found.len() != 1 {
        return Err(bad(
            "one implicit captured head has no unique real capture input",
        ));
    }
    found
        .iter()
        .next()
        .copied()
        .ok_or_else(|| bad("implicit captured input disappeared"))
}
