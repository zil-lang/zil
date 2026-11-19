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
pub(super) type SharedRoots = Rc<BTreeSet<Root>>;

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
        for root in roots.iter() {
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
        return Ok(roots.as_ref().clone());
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
            for root in roots.iter() {
                *left = left.checked_sub(root.path.len()).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "argument head path snapshot exceeded", None)
                })?;
            }
            return Ok(roots.as_ref().clone());
        }
    }
    Ok(BTreeSet::new())
}

/// Check a physical tuple field against its original class domain. A real
/// reborrow retains its separate loan in the CFG, while its reaching SSA
/// parent can establish the original outer role of this construction.
pub(super) fn tuple_field(
    world: &mut World<'_, '_>,
    scope: SemanticInstanceId,
    operand: ValueId,
    expected: &SemanticTypeValue,
    values: &ssa::Values,
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let actual = world.get(values, operand)?;
    let actual = world.normalized(scope, &actual, implicit)?;
    let expected = world.normalized(scope, expected, implicit)?;
    world.pay(1)?;
    if crate::dyn_metadata::same_formed_value(&actual, &expected, world.limits, world.left)? {
        return Ok(());
    }
    let ty = World::type_of(&actual)?;
    if ty != World::type_of(&expected)? || !matches!(ty, Type::Address { .. }) {
        return world.equal(&actual, &expected);
    }
    let mut cursor = operand;
    let mut seen = BTreeSet::new();
    loop {
        world.pay(index::search(world.reborrows.len()).saturating_add(1))?;
        let Some(edge) = world.reborrows.get(&cursor) else { break; };
        if seen.len() >= world.limits.max_call_depth || !seen.insert(cursor) {
            return Err(bad("tuple field has a cyclic or excessive actual reborrow chain"));
        }
        cursor = edge.parent;
        let parent = world.get(values, cursor)?;
        let parent = world.normalized(scope, &parent, implicit)?;
        if crate::dyn_metadata::same_formed_value(&parent, &expected, world.limits, world.left)? {
            let inner = world.pointee(&actual)?;
            let expected_inner = world.pointee(&expected)?;
            return world.equal(&inner, &expected_inner);
        }
    }
    world.equal(&actual, &expected)
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
    values: &ssa::Values,
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
    values: &ssa::Values, locals: &[Option<SharedType>],
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
    values: &ssa::Values, locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    let parameter = match boundary { Boundary::Parameter(ordinal) | Boundary::Capture(ordinal) => ordinal as usize };
    let actual = world.normalized(caller.id, actual, implicit)?;
    let expected = world.normalized(caller.id, expected, implicit)?;
    world.pay(1)?;
    if std::ptr::eq(actual.as_ref(), expected.as_ref()) {
        return Ok(());
    }
    if crate::dyn_metadata::same_formed_value(&actual, &expected, world.limits, world.left)? {
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
            let mut diagnostic = places::place_diagnostic(
                world, "call outer-role adapter has an inactive actual parent storage root",
                root.local, &root.path, None,
            );
            diagnostic.notes.push(format!("call operand SSA {}", operand.0));
            return Err(diagnostic);
        }
        world.pay(root.path.len())?;
        if let Some((_, vacant)) = world.vacant.iter().find(|(local, path)| {
            *local == root.local && (path.starts_with(&root.path) || root.path.starts_with(path))
        }) {
            let mut diagnostic = places::place_diagnostic(
                world, "call outer-role adapter has a vacant actual parent projection",
                root.local, &root.path, Some(vacant),
            );
            diagnostic.notes.push(format!("call operand SSA {}", operand.0));
            return Err(diagnostic);
        }
    }
    let moved_capture = matches!(boundary, Boundary::Capture(_))
        && roles::moved_capture(world, caller.id, operand, &actual, &expected, implicit)?;
    if moved_capture || matches!(actual_ty, Type::Address { mutable: false, .. })
        && (matches!(source, roles::Source::Static)
            || matches!(
                world.value_role_sources.get(&operand).map(|value| &value.source),
                Some(roles::Source::Static)
            ))
    {
        // The actual SSA retains the proved input image moved at this mint,
        // or a program-static source that supplies a shorter shared view.
        // The whole pointee and the reaching roots were checked above.
        let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = site.at
        else {
            return Err(bad("outer-role adaptation has no exact actual Typed site"));
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
        let shape = flow::Shape::for_function(world, function)?;
        let (block, _) = shape.block_and_predecessors(world, function, block_id)?;
        let actual = block
            .instructions
            .get(instruction as usize)
            .ok_or_else(|| bad("argument role witness names an absent instruction"))?;
        world.pay(1)?;
        let parameter = parameter as usize;
        let actual_argument = if let Boundary::Capture(_) = boundary {
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
            captures.get(parameter).copied()
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
                } if *target == callee => arguments.get(parameter).copied(),
                InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. } => arguments.get(parameter).copied(),
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
            if let InstructionKind::CallIndirect { callee, arguments } = &actual.kind {
                world.pay(index::search(proof.callable_calls.len()))?;
                let scheme = proof.callable_calls.get(&(owner, recipe))
                    .ok_or_else(|| bad("indirect argument role has no exact original call scheme"))?;
                if scheme.kind != CallableKind::FunctionPointer
                    || !matches!(function.values.get(callee.0 as usize),
                        Some(Type::BorrowedFunctionPointer { life, .. })
                        if life.invocation_binder_count != 0
                            && life.invocation_binder_count == scheme.signature.invocation_lifetimes)
                {
                    return Err(bad("indirect argument role changes its original invocation frame"));
                }
                arguments.get(parameter).copied()
            } else {
                let scheme = proof
                    .intrinsics
                    .get(&(owner, recipe))
                    .ok_or_else(|| bad("sealed argument role has no exact original call scheme"))?;
                world.pay(super::intrinsics::operand_work(&actual.kind))?;
                super::intrinsics::operand_ids(&scheme.kind, &actual.kind)?
                    .get(parameter)
                    .copied()
            }
        };
        if actual_argument != Some(operand) {
            return Err(bad("argument role witness changes the real argument SSA"));
        }
        if !world.point_rows(caller_id, block_id, instruction as usize)?.any(|site| {
            site.recipe == recipe && match boundary {
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
