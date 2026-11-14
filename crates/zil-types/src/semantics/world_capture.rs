//! Reconstruct implicit captured heads from the real child CFG and its original
//! Name observation. A header row alone cannot seal a physical capture input.
use super::*;
use std::collections::{BTreeSet, VecDeque};
#[path = "header_capture_uses.rs"]
mod first_use;

fn creator_input(
    world: &mut World<'_, '_>,
    child: &SemanticInstance,
    ordinal: u32,
) -> R<Option<u32>> {
    let original = world.definition(child.definition)?;
    let SemanticDefinitionBody::FunctionWithBody {
        sites, site_nodes, ..
    } = &original.body
    else {
        return Ok(None);
    };
    world.pay(
        original
            .implicit_lifetimes
            .len()
            .saturating_add(sites.len())
            .saturating_add(site_nodes.len()),
    )?;
    let occurrence = original
        .implicit_lifetimes
        .iter()
        .find(|row| row.ordinal == ordinal)
        .ok_or_else(|| bad("implicit capture has no original occurrence"))?;
    let mut node = None;
    for recipe in sites {
        if recipe.occurrence_path != occurrence.occurrence_path
            || !matches!(recipe.kind, SemanticSiteRecipeKind::TypeUse { .. })
        {
            continue;
        }
        world.pay(site_nodes.len())?;
        for binding in site_nodes
            .iter()
            .filter(|binding| binding.site == recipe.id)
        {
            let SemanticOriginalNode::Expression(expression) = binding.node else {
                return Err(bad(
                    "implicit capture occurrence is not its original expression",
                ));
            };
            if node.replace(expression).is_some() {
                return Err(bad(
                    "implicit capture occurrence has multiple original expressions",
                ));
            }
        }
    }
    let Some(node) = node else {
        world.pay(occurrence.occurrence_path.len().saturating_add(64))?;
        let mut diagnostic = bad("implicit captured head has no original type-use occurrence");
        diagnostic.notes.push(format!(
            "child {:?}, original {:?}, implicit {}, path {:?}",
            child.id, original.id, ordinal, occurrence.occurrence_path
        ));
        return Err(diagnostic);
    };
    let ledger = world.ledger;
    let proof = ledger.original.borrow();
    let proof = proof
        .as_ref()
        .ok_or_else(|| bad("implicit capture has no original body proof"))?;
    world.pay(
        proof
            .capture_names
            .len()
            .saturating_add(proof.closure_move.len()),
    )?;
    if proof.closure_move.get(&original.id) != Some(&false) {
        world.pay(64)?;
        let mut diagnostic =
            bad("implicit captured head has no original default-borrow capture mode");
        diagnostic.notes.push(format!(
            "child {:?}, original {:?}, implicit {}, source node {:?}, move mode {:?}",
            child.id,
            original.id,
            ordinal,
            node,
            proof.closure_move.get(&original.id)
        ));
        return Err(diagnostic);
    }
    let names = proof
        .capture_names
        .get(&original.id)
        .ok_or_else(|| bad("implicit capture has no original lexical capture directory"))?;
    let mut selected = None;
    for name in names {
        world.pay(name.len().saturating_add(1))?;
        if first_use::find(original, name, world.limits, world.left)? == Some(node) {
            if selected.replace(name).is_some() {
                return Err(bad(
                    "one implicit occurrence selects multiple original lexical captures",
                ));
            }
        }
    }
    let Some(name) = selected else {
        let mut diagnostic =
            bad("implicit captured head has no matching original first free capture use");
        world.pay(names.iter().fold(64usize, |work, name| {
            work.saturating_add(name.len()).saturating_add(32)
        }))?;
        let mut uses = Vec::new();
        for name in names {
            uses.push((
                name.as_str(),
                first_use::find(original, name, world.limits, world.left)?,
            ))
        }
        diagnostic.notes.push(format!(
            "child {:?}, original {:?}, implicit {}, source node {:?}, first free uses {:?}",
            child.id, original.id, ordinal, node, uses
        ));
        return Err(diagnostic);
    };
    let environment = child
        .environment
        .as_ref()
        .ok_or_else(|| bad("implicit capture has no actual environment"))?;
    let SemanticClosureTarget::Typed(closure) = environment.closure else {
        return Err(bad("implicit capture retains a Core environment"));
    };
    world.pay(ledger.closure_captures.len())?;
    let Some(receipts) = ledger.closure_captures.get(&closure) else {
        return Ok(None);
    };
    let mut index = None;
    for (slot, (candidate, _)) in receipts.iter().enumerate() {
        world.pay(name.len().saturating_add(candidate.len()).saturating_add(1))?;
        if candidate == name && index.replace(slot).is_some() {
            return Err(bad(
                "one original lexical capture has multiple actual input slots",
            ));
        }
    }
    let index =
        index.ok_or_else(|| bad("original captured name has no actual MakeClosure input"))?;
    let parent = world.instance(
        child
            .parent
            .ok_or_else(|| bad("implicit capture has no real creator instance"))?,
    )?;
    if original.parent != Some(parent.definition) {
        return Err(bad("implicit capture changes its original lexical parent"));
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = parent.target
    else {
        return Err(bad("implicit capture creator has no actual function"));
    };
    let creator = world.function(function)?;
    world.pay(ledger.frames.len())?;
    let frame = ledger
        .frames
        .get(&match child.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) => id,
            _ => return Err(bad("implicit capture child has no actual function")),
        })
        .ok_or_else(|| bad("implicit capture entry has no actual emitter frame"))?;
    if frame.parent != Some((function, closure)) {
        return Err(bad(
            "implicit capture entry changes its actual creator/environment",
        ));
    }
    world.pay(world.program.sites.len())?;
    let mut mint = None;
    for site in &world.program.sites {
        if site.instance == parent.id
            && matches!(site.action, SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(id), .. } if id == closure)
        {
            if mint.replace(site).is_some() {
                return Err(bad(
                    "implicit capture has multiple actual creator occurrences",
                ));
            }
        }
    }
    let mint =
        mint.ok_or_else(|| bad("implicit capture has no actual original creator occurrence"))?;
    let parent_original = world.definition(parent.definition)?;
    if !matches!(&world.original_recipe(parent_original, mint.recipe)?.kind, SemanticSiteRecipeKind::Closure { definition, .. } if *definition == child.definition)
    {
        return Err(bad(
            "implicit capture mint changes its original child definition",
        ));
    }
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::After,
    } = mint.at
    else {
        return Err(bad(
            "implicit capture mint is not an actual construction boundary",
        ));
    };
    world.pay(creator.blocks.len())?;
    let operation = creator
        .blocks
        .iter()
        .find(|row| row.id == block)
        .and_then(|row| row.instructions.get(instruction as usize))
        .ok_or_else(|| bad("implicit capture mint leaves its actual creator CFG"))?;
    let InstructionKind::MakeClosure {
        closure: actual,
        captures,
    } = &operation.kind
    else {
        return Err(bad("implicit capture mint is not its actual MakeClosure"));
    };
    world.pay(captures.len().saturating_add(receipts.len()))?;
    if *actual != closure
        || captures.len() != receipts.len()
        || captures
            .iter()
            .zip(receipts)
            .any(|(actual, (_, recorded))| actual != recorded)
    {
        return Err(bad(
            "implicit capture input differs from its exact actual lexical construction",
        ));
    }
    Ok(Some(
        u32::try_from(index).map_err(|_| bad("capture input ordinal overflows"))?,
    ))
}
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
                ));
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
    if let Some(input) = creator_input(world, child, ordinal)? {
        return Ok(input);
    }
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = child.target
    else {
        return Err(bad("implicit capture child has no actual entry"));
    };
    let function = world.function(function)?;
    let definition = world.definition(child.definition)?;
    let cfg = flow::Cfg::new(world, child, function)?;
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
