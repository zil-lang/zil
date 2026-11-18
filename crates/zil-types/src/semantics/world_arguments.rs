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
#[derive(Clone, Copy)]
pub(super) enum Boundary { Parameter(u32), Capture(u32) }
#[derive(Clone)]
pub(super) struct Witness {
    pub caller: SemanticInstanceId,
    pub callee: Option<SemanticInstanceId>,
    pub recipe: SemanticSiteId,
    pub block: BlockId,
    pub instruction: u32,
    pub boundary: Boundary,
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
    callee: Option<&SemanticInstance>,
    site: &SemanticSite,
    parameter: usize,
    operand: ValueId,
    actual: &SemanticTypeValue,
    expected: &SemanticTypeValue,
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let parameter = u32::try_from(parameter).map_err(|_| bad("call parameter ordinal overflows"))?;
    satisfy_boundary(world, caller, callee, site, Boundary::Parameter(parameter), operand, actual, expected, values, locals, implicit)
}
/// A constructor borrows from the same real reaching edge as a call argument.
/// Its expected persistent source remains unchanged; this adapter checks only
/// the actual capture operand's directed outer view at this exact mint.
pub(super) fn capture(
    world: &mut World<'_, '_>, caller: &SemanticInstance, child: &SemanticInstance,
    site: &SemanticSite, ordinal: u32, operand: ValueId,
    actual: &SemanticTypeValue, expected: &SemanticTypeValue,
    values: &[Option<SharedType>], locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    if child.parent != Some(caller.id) || site.instance != caller.id {
        return Err(bad("capture role adaptation changes its exact creator world"));
    }
    satisfy_boundary(world, caller, Some(child), site, Boundary::Capture(ordinal), operand, actual, expected, values, locals, implicit)
}
fn satisfy_boundary(
    world: &mut World<'_, '_>, caller: &SemanticInstance, callee: Option<&SemanticInstance>,
    site: &SemanticSite, boundary: Boundary, operand: ValueId,
    actual: &SemanticTypeValue, expected: &SemanticTypeValue,
    values: &[Option<SharedType>], locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let parameter = match boundary { Boundary::Parameter(ordinal) | Boundary::Capture(ordinal) => ordinal as usize };
    let actual = world.normalized(caller.id, actual, implicit)?;
    let expected = world.normalized(caller.id, expected, implicit)?;
    world.pay(1)?;
    if std::ptr::eq(actual.as_ref(), expected.as_ref()) {
        return Ok(());
    }
    crate::dyn_metadata::semantic_operand_snapshot(actual.as_ref(), world.limits, world.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(expected.as_ref(), world.limits, world.left, 1)?;
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
        if world.stale_locals.contains(&root.local)
            || locals
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
    if matches!(actual_ty, Type::Address { mutable: false, .. })
        && (matches!(source, roles::Source::Static)
            || matches!(
                world.value_role_sources.get(&operand).map(|value| &value.source),
                Some(roles::Source::Static)
            ))
    {
        // The actual SSA retains a proved program-static source, including
        // through a checked local role. It may satisfy this call's shorter
        // shared outer region. The whole pointee was independently compared
        // above, and the witness audit still pins the exact call/parameter/SSA.
        // No class identity, root, epoch or exclusive permission is invented.
        let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = site.at
        else {
            return Err(bad("static argument has no exact actual Typed call site"));
        };
        world.pay(actual_roots.len().saturating_add(1))?;
        world.argument_witnesses.push(Witness {
            caller: caller.id,
            callee: callee.map(|callee| callee.id),
            recipe: site.recipe,
            block,
            instruction,
            boundary,
            operand,
            parents: Vec::new(),
            roots: actual_roots,
        });
        return Ok(());
    }
    if matches!(boundary, Boundary::Parameter(_)) && super::receiver::permits(world, caller.id, site.recipe, operand, &source, &target)? {
        // This exact source binding/field path and actual producer establish
        // only this receiver loan. Its whole pointee domain was checked above.
        return Ok(());
    }
    if matches!(boundary, Boundary::Parameter(_)) && super::borrow_roles::call_region_argument(
        world, caller, site, parameter, operand, &expected, values, locals, implicit,
    )? {
        // The exact original input and reaching actual read/projection/borrow
        // were checked; no source type or class image is replaced here.
        let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = site.at
        else {
            return Err(bad("call region has no exact actual Typed call site"));
        };
        let mut parents = Vec::new();
        let mut cursor = operand;
        let mut seen = BTreeSet::new();
        loop {
            world.pay(index::search(world.reborrows.len()).saturating_add(1))?;
            if let Some(edge) = world.reborrows.get(&cursor) {
                if parents.len() >= world.limits.max_call_depth || !seen.insert(cursor) {
                    return Err(bad(
                        "call region has a cyclic or excessive real reborrow chain",
                    ));
                }
                parents.push(edge.parent);
                cursor = edge.parent;
            } else {
                break;
            }
        }
        world.argument_witnesses.push(Witness {
            caller: caller.id,
            callee: callee.map(|row| row.id),
            recipe: site.recipe,
            block,
            instruction,
            boundary,
            operand,
            parents,
            roots: actual_roots,
        });
        return Ok(());
    }
    let mut parents = Vec::new();
    let mut visited = BTreeSet::new();
    while source != target {
        world.pay(1)?;
        if !parents.is_empty()
            && matches!(actual_ty, Type::Address { mutable: false, .. })
            && matches!(source, roles::Source::Static)
        {
            // An actual ReborrowAddress can hide the static endpoint behind
            // one or more child occurrences. The paid reaching edges above
            // establish that endpoint just as the direct Static case does;
            // Static outlives this shared call region without being equal to
            // it. Keep every actual parent in the same call/operand witness.
            break;
        }
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
            world.pay(256)?;
            let mut error =
                bad("call outer role has no real reborrow path to its original argument role");
            error.notes.push(format!("caller {:?}, callee {:?}, site {:?}, parameter {}, operand {:?}; source {:?}, target {:?}",caller.id,callee.map(|callee|callee.id),site.recipe,parameter,operand,source,target));
            return Err(error);
        };
        if !path.is_empty() || !visited.insert(*head) {
            return Err(bad(
                "call outer-role adapter contains a cyclic or non-root occurrence",
            ));
        }
        let Some(edge) = world.reborrows.get(head) else {
            world.pay(256)?;
            let mut diagnostic =
                bad("call outer-role adapter is not a real reaching ReborrowAddress edge");
            diagnostic.notes.push(format!("exact outer-role boundary: point {:?}, caller {:?}, callee {:?}, original site {:?}, parameter {}, argument {:?}, mint {:?}; source {:?}, target {:?}",
                world.current_point, caller.id, callee.map(|row| row.id), site.recipe, parameter, operand, head, source, target));
            return Err(diagnostic);
        };
        parents.push(edge.parent);
        let incoming = roles::snapshot_source(&edge.source, world.limits, world.left)?;
        source = match incoming {
            // The edge keeps the source occupant's original role. A checked
            // contextual loan can name that role symbolically, while the call
            // target above has already been normalized in this same Gamma.
            // Normalize each real edge's image before comparing; this never
            // creates an edge or identifies either actual storage root.
            roles::Source::Class(image) => {
                roles::Source::Class(world.class(caller.id, &image, implicit)?)
            }
            roles::Source::Static => roles::Source::Static,
        };
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
        callee: callee.map(|callee| callee.id),
        recipe: site.recipe,
        block,
        instruction,
        boundary,
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
        let boundary = witness.boundary;
        let parameter = match boundary { Boundary::Parameter(ordinal) | Boundary::Capture(ordinal) => ordinal };
        let operand = witness.operand;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(caller)) =
            world.instance(caller_id)?.target
        else {
            return Err(bad("argument role witness has no real caller function"));
        };
        let callee = if let Some(callee_id) = callee_id {
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(callee)) =
                world.instance(callee_id)?.target
            else {
                return Err(bad("argument role witness has no real callee function"));
            };
            Some(callee)
        } else {
            None
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
        let argument_cost = match &actual.kind {
            InstructionKind::MakeClosure { captures, .. } => captures.len(),
            InstructionKind::Call { arguments, .. }
            | InstructionKind::ForeignCall { arguments, .. }
            | InstructionKind::SuspendCall { arguments, .. }
            | InstructionKind::CallIndirect { arguments, .. }
            | InstructionKind::CallClosure { arguments, .. }
            | InstructionKind::CallClosureOnce { arguments, .. }
            | InstructionKind::CallClosureBorrowed { arguments, .. }
            | InstructionKind::CallClosureShared { arguments, .. }
            | InstructionKind::CallClosureMut { arguments, .. } => arguments.len(),
            _ => super::intrinsics::operand_work(&actual.kind),
        };
        world.pay(argument_cost)?;
        let actual_arguments = if let Boundary::Capture(_) = boundary {
            let InstructionKind::MakeClosure { closure, captures } = &actual.kind else {
                return Err(bad("capture role witness does not belong to its real MakeClosure"));
            };
            let child_id = callee_id.ok_or_else(|| bad("capture role witness has no actual child world"))?;
            let (_, child) = world.child_at(*closure, child_id)?;
            if child.parent != Some(caller_id) {
                return Err(bad("capture role witness changes its true creator"));
            }
            let owner = world.instance(caller_id)?.definition;
            let original = world.definition(owner)?;
            if !matches!(world.original_recipe(original, recipe)?.kind,
                SemanticSiteRecipeKind::Closure { definition, .. } if definition == child.definition) {
                return Err(bad("capture role witness changes its original closure recipe"));
            }
            captures.clone()
        } else if let Some(callee) = callee {
            match &actual.kind {
                InstructionKind::Call {
                    function: target,
                    arguments,
                }
                | InstructionKind::ForeignCall {
                    function: target,
                    arguments,
                    ..
                }
                | InstructionKind::SuspendCall {
                    function: target,
                    arguments,
                } if *target == callee => arguments.clone(),
                InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. } => arguments.clone(),
                _ => {
                    return Err(bad(
                        "argument role witness does not belong to its actual call opcode",
                    ));
                }
            }
        } else {
            let ledger = world.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("sealed argument role has no complete original scheme"))?;
            let owner = world.instance(caller_id)?.definition;
            let scheme = proof
                .intrinsics
                .get(&(owner, recipe))
                .ok_or_else(|| bad("sealed argument role has no exact original call scheme"))?;
            super::intrinsics::operand_ids(&scheme.kind, &actual.kind)?
        };
        if actual_arguments.get(parameter as usize) != Some(&operand) {
            return Err(bad("argument role witness changes the real argument SSA"));
        }
        world.pay(world.program.sites.len())?;
        if !world.program.sites.iter().any(|site| {
            site.instance == caller_id && site.recipe == recipe
                && matches!(site.at, SemanticSiteAddress::Typed { block, instruction: i, .. } if block == block_id && i == instruction)
                && match boundary {
                    Boundary::Parameter(_) => true,
                    Boundary::Capture(_) => matches!((&site.action, &actual.kind),
                        (SemanticSiteAction::Closure { closure: SemanticClosureTarget::Typed(id), .. }, InstructionKind::MakeClosure { closure, .. }) if id == closure),
                }
        }) { return Err(bad("outer-role witness has no exact original actual boundary")); }
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
