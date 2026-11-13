//! A real reborrow can satisfy one call's raw outer role. This is neither a
//! lifetime equality nor a permission/loan proof; both kernels still check the
//! actual operand, live roots, epochs and exclusivity independently.
use super::*;
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Root {
    pub local: LocalId,
    pub path: Vec<PlaceProjection>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Edge {
    pub parent: ValueId,
    pub source: roles::Source,
}
#[derive(Clone)]
pub(super) struct Witness {
    pub caller: SemanticInstanceId,
    pub callee: SemanticInstanceId,
    pub recipe: SemanticSiteId,
    pub block: BlockId,
    pub instruction: u32,
    pub parameter: u32,
    pub operand: ValueId,
    pub parents: Vec<ValueId>,
    pub roots: BTreeSet<Root>,
}
pub(super) fn copy_roots(world: &mut World<'_, '_>, roots: &BTreeSet<Root>) -> R<BTreeSet<Root>> {
    world.pay(roots.len())?;
    for root in roots {
        world.pay(root.path.len())?;
    }
    Ok(roots.clone())
}
pub(super) fn roots(
    world: &mut World<'_, '_>,
    operand: ValueId,
    value: &SemanticTypeValue,
) -> R<BTreeSet<Root>> {
    if let Some(roots) = world.value_roots.get(&operand) {
        let limits = world.limits;
        let left = &mut *world.left;
        *left = left.checked_sub(roots.len()).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "argument root snapshot work exceeded", None)
        })?;
        for root in roots {
            *left = left.checked_sub(root.path.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "argument root path work exceeded", None)
            })?;
            if root.path.len() > limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "argument root path depth exceeded",
                    None,
                ));
            }
        }
        return Ok(roots.clone());
    }
    let source = roles::outer(world, value)?;
    if let roles::Source::Class(SemanticClassExpr::OuterOccurrence {
        operand: SemanticOperand::Value(head),
        ..
    }) = source
    {
        if let Some(roots) = world.value_roots.get(&head) {
            let left = &mut *world.left;
            *left = left.checked_sub(roots.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "argument head root snapshot exceeded", None)
            })?;
            for root in roots {
                *left = left.checked_sub(root.path.len()).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "argument head path snapshot exceeded", None)
                })?;
            }
            return Ok(roots.clone());
        }
    }
    Ok(BTreeSet::new())
}
pub(super) fn satisfy(
    world: &mut World<'_, '_>,
    caller: &SemanticInstance,
    callee: &SemanticInstance,
    site: &SemanticSite,
    parameter: usize,
    operand: ValueId,
    actual: &SemanticTypeValue,
    expected: &SemanticTypeValue,
    locals: &[Option<SemanticTypeValue>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let actual = world.normalize(caller.id, actual, implicit)?;
    let expected = world.normalize(caller.id, expected, implicit)?;
    if actual == expected {
        return Ok(());
    }
    let actual_ty = World::type_of(&actual)?;
    let expected_ty = World::type_of(&expected)?;
    if actual_ty != expected_ty || !matches!(actual_ty, Type::Address { .. } | Type::Desc(_)) {
        return Err(bad(
            "actual call argument changes an exact whole type or intrinsic class domain",
        ));
    }
    let inner = world.pointee(&actual)?;
    let expected_inner = world.pointee(&expected)?;
    world.equal(&inner, &expected_inner)?;
    let mut source = roles::outer(world, &actual)?;
    let target = roles::outer(world, &expected)?;
    let actual_roots = roots(world, operand, &actual)?;
    world.pay(
        actual_roots
            .len()
            .saturating_mul(world.vacant.len().saturating_add(1)),
    )?;
    for root in &actual_roots {
        if locals
            .get(root.local.0 as usize)
            .and_then(|value| value.as_ref())
            .is_none()
        {
            return Err(bad(
                "call outer-role adapter has an inactive actual parent storage root",
            ));
        }
        world.pay(root.path.len())?;
        if world.vacant.iter().any(|(local, path)| {
            *local == root.local && (path.starts_with(&root.path) || root.path.starts_with(path))
        }) {
            return Err(bad(
                "call outer-role adapter has a vacant actual parent projection",
            ));
        }
    }
    let mut parents = Vec::new();
    let mut visited = BTreeSet::new();
    while source != target {
        world.pay(1)?;
        if parents.len() >= world.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "call outer-role reborrow chain depth exceeded",
                None,
            ));
        }
        let roles::Source::Class(SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(head),
            path,
            ..
        }) = &source
        else {
            return Err(bad(
                "call outer role has no real reborrow path to its original argument role",
            ));
        };
        if !path.is_empty() || !visited.insert(*head) {
            return Err(bad(
                "call outer-role adapter contains a cyclic or non-root occurrence",
            ));
        }
        let edge = world.reborrows.get(head).ok_or_else(|| {
            bad("call outer-role adapter is not a real reaching ReborrowAddress edge")
        })?;
        parents.push(edge.parent);
        source = roles::snapshot_source(&edge.source, world.limits, world.left)?;
    }
    if parents.is_empty() {
        return Err(bad(
            "call role adaptation changes a class without an actual reborrow",
        ));
    }
    let SemanticSiteAddress::Typed {
        block, instruction, ..
    } = site.at
    else {
        return Err(bad("call role adapter has no real Typed call site"));
    };
    world.pay(parents.len().saturating_add(1))?;
    world.argument_witnesses.push(Witness {
        caller: caller.id,
        callee: callee.id,
        recipe: site.recipe,
        block,
        instruction,
        parameter: parameter as u32,
        operand,
        parents,
        roots: actual_roots,
    });
    Ok(())
}

pub(super) fn audit(world: &mut World<'_, '_>) -> R<()> {
    world.pay(world.argument_witnesses.len())?;
    for index in 0..world.argument_witnesses.len() {
        let witness = &world.argument_witnesses[index];
        let caller_id = witness.caller;
        let callee_id = witness.callee;
        let block_id = witness.block;
        let instruction = witness.instruction;
        let recipe = witness.recipe;
        let parameter = witness.parameter;
        let operand = witness.operand;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(caller)) =
            world.instance(caller_id)?.target
        else {
            return Err(bad("argument role witness has no real caller function"));
        };
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(callee)) =
            world.instance(callee_id)?.target
        else {
            return Err(bad("argument role witness has no real callee function"));
        };
        let function = world.function(caller)?;
        world.pay(function.blocks.len())?;
        let block = function
            .blocks
            .iter()
            .find(|block| block.id == block_id)
            .ok_or_else(|| bad("argument role witness names an absent block"))?;
        let actual = block
            .instructions
            .get(instruction as usize)
            .ok_or_else(|| bad("argument role witness names an absent instruction"))?;
        let actual_arguments = match &actual.kind {
            InstructionKind::Call {
                function: target,
                arguments,
            } if *target == callee => arguments,
            InstructionKind::CallClosure { arguments, .. }
            | InstructionKind::CallClosureOnce { arguments, .. }
            | InstructionKind::CallClosureBorrowed { arguments, .. }
            | InstructionKind::CallClosureShared { arguments, .. }
            | InstructionKind::CallClosureMut { arguments, .. } => arguments,
            _ => {
                return Err(bad(
                    "argument role witness does not belong to its actual call opcode",
                ))
            }
        };
        if actual_arguments.get(parameter as usize) != Some(&operand) {
            return Err(bad("argument role witness changes the real argument SSA"));
        }
        world.pay(world.program.sites.len())?;
        if !world.program.sites.iter().any(|site|site.instance==caller_id&&site.recipe==recipe&&matches!(site.at,SemanticSiteAddress::Typed{block,instruction:i,..}if block==block_id&&i==instruction)){return Err(bad("argument role witness has no exact original actual call site"));}
        let witness = &world.argument_witnesses[index];
        *world.left = world
            .left
            .checked_sub(witness.parents.len().saturating_add(witness.roots.len()))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "argument role audit work exceeded", None)
            })?;
        for parent in &witness.parents {
            if function.values.get(parent.0 as usize).is_none() {
                return Err(bad("argument role witness has an absent actual parent SSA"));
            }
        }
        for root in &witness.roots {
            *world.left = world.left.checked_sub(root.path.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "argument role audit path exceeded", None)
            })?;
            if function.locals.get(root.local.0 as usize).is_none() {
                return Err(bad(
                    "argument role witness has an absent parent storage root",
                ));
            }
        }
    }
    Ok(())
}
