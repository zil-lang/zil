//! Actual environment leaves in the stored-target ledger. The physical Field
//! prefix is private: public Capture selectors are authenticated separately.
use super::*;

pub(super) fn closure(ty: &Type) -> Option<ClosureId> {
    match ty {
        Type::Closure(id) => Some(*id),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}
pub(super) fn leaves<'a>(
    module: &'a TypedModule,
    id: ClosureId,
    b: &mut Budget,
) -> Check<Vec<(Path, &'a Type)>> {
    let limits = b.limits.clone();
    let actual = crate::retained::sources::capture_leaves(module, id, &limits, b)?;
    b.spend(actual.len())?;
    let mut out = Vec::with_capacity(actual.len());
    let mut private = Vec::new();
    for leaf in actual {
        b.spend(leaf.position.path.len().saturating_add(1))?;
        let StorageBase::Capture(index) = leaf.position.base else {
            return Err(err("actual environment leaf lacks a capture"));
        };
        let mut path = Vec::with_capacity(leaf.position.path.len().saturating_add(1));
        path.push(PlaceProjection::Field(index));
        for step in leaf.position.path {
            path.push(match step {
                ContractProjection::Place(step) => step,
                ContractProjection::Capture(index) => PlaceProjection::Field(index),
                _ => {
                    return Err(err(
                        "environment leaf has no actual physical target transport",
                    ));
                }
            });
        }
        if let Type::Address { inner, .. } = leaf.ty
            && let Type::BorrowedStorage { storage, life } = inner.as_ref()
        {
            // Pointee dependencies are private memory facts. Capturing the
            // actual pointer carries its current slots; public Capture source
            // rows continue to select the pointer itself, never a fake GEP.
            b.spend(life.slots.len())?;
            for slot in &life.slots {
                b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                let ty = crate::dynamic_ports::physical_path(storage, &slot.path, b)?;
                let mut child = path.clone();
                child.extend(&slot.path);
                private.push((child, ty));
            }
        }
        out.push((path, leaf.ty));
    }
    // Preserve the original public directory first. Its exact contract check
    // zips only these rows; extra pointee facts are derived by the memory solver.
    b.spend(private.len())?;
    out.extend(private);
    Ok(out)
}
fn writes(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: true, .. } | Type::Desc(_))
}
pub(super) fn seed(
    module: &TypedModule,
    id: ClosureId,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    for (path, ty) in leaves(module, id, b)? {
        b.spend(path.len().saturating_mul(3).saturating_add(6))?;
        let source = Source {
            base,
            leaf: path.clone(),
            retained_port: None,
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let loan = loans.len();
        let exclusive = writes(ty);
        loans.push(Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive,
        });
        b.stored_exclusive_loans |= exclusive;
        add_leaf(
            out,
            &path,
            &Permission {
                sources: vec![source],
                loans: vec![loan],
            },
            b,
        )?;
    }
    Ok(())
}
pub(super) fn create(
    module: &TypedModule,
    id: ClosureId,
    captures: &[ValueId],
    values: &[Payload],
    state: &Frame,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    let d = &module.closures[id.0 as usize];
    for (path, _) in leaves(module, id, b)? {
        let Some((PlaceProjection::Field(index), tail)) = path.split_first() else {
            return Err(err("actual environment leaf has no capture field"));
        };
        let actual = &values[captures[*index as usize].0 as usize];
        let permission = if tail.is_empty() {
            permission_work(&actual.address, b)?;
            actual.address.clone()
        } else if addressed(&d.captures[*index as usize]).is_some() {
            select(state, &actual.address, tail, b)?
        } else if let Some(permission) = leaf(actual, tail, b)? {
            permission_work(permission, b)?;
            permission.clone()
        } else {
            Permission::default()
        };
        add_leaf(out, &path, &permission, b)?;
    }
    Ok(())
}
/// Each actual leaf has a stable loan site; invocation keeps the exact original
/// parents, while mutable leaves do not borrow authority from shared siblings.
pub(super) fn loan_count(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<usize> {
    let mut count = 0usize;
    b.spend(f.values.len())?;
    for ty in &f.values {
        if let Type::Closure(id) = ty {
            count = count
                .checked_add(leaves(module, *id, b)?.len())
                .ok_or_else(|| limit("environment loan arena overflow"))?;
        }
    }
    Ok(count)
}
pub(super) fn retain(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    out: &mut Payload,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<()> {
    let Type::Closure(id) = vt(f, value)? else {
        return Ok(());
    };
    let mut offset = f.values.len();
    b.spend(f.values.len())?;
    for ty in &f.values {
        if let Some((_, shape)) = qualified(ty) {
            offset = offset
                .checked_add(shape.slots.len())
                .ok_or_else(|| limit("environment loan offset overflow"))?;
        }
    }
    b.spend(value.0 as usize)?;
    for ty in &f.values[..value.0 as usize] {
        if let Type::Closure(id) = ty {
            offset = offset
                .checked_add(leaves(module, *id, b)?.len())
                .ok_or_else(|| limit("environment loan offset overflow"))?;
        }
    }
    for (ordinal, (path, ty)) in leaves(module, *id, b)?.into_iter().enumerate() {
        let Some(row) = out.leaves.position(&path, b)? else {
            continue;
        };
        let p = out.leaves.permission_mut(row);
        let loan = offset
            .checked_add(ordinal)
            .ok_or_else(|| limit("environment loan identity overflow"))?;
        let row = loans
            .get_mut(loan)
            .ok_or_else(|| err("environment loan outside reserved arena"))?;
        add_sources(&mut row.sources, &p.sources, b)?;
        add_loans(&mut row.parents, &p.loans, b)?;
        row.exclusive = writes(ty);
        b.stored_exclusive_loans |= row.exclusive;
        b.spend(1)?;
        p.loans = vec![loan];
    }
    Ok(())
}

fn actual_input_position(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StoragePosition> {
    let HoldRoot::Parameter(index) = source.base else {
        return Err(err(
            "returned environment depends on activation or capture storage",
        ));
    };
    if source.retained_port.is_some() || source.heap_element.is_some() {
        return Err(err(
            "returned environment source lacks an actual public target selector",
        ));
    }
    let local = *f
        .parameters
        .get(index as usize)
        .ok_or_else(|| err("environment source input absent"))?;
    let mut ty = &f.locals[local.0 as usize].ty;
    b.spend(
        source
            .leaf
            .len()
            .saturating_add(source.inline.len())
            .saturating_add(1),
    )?;
    let mut path = Vec::with_capacity(source.leaf.len().saturating_add(source.inline.len()));
    for physical in source.leaf.iter().chain(&source.inline) {
        b.spend(1)?;
        if let Type::Address { inner, .. } = ty {
            ty = inner;
        }
        if let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        let selector = if let Type::Closure(id) = ty {
            let PlaceProjection::Field(index) = physical else {
                return Err(err("environment input capture is not an actual field"));
            };
            ty = module
                .closures
                .get(id.0 as usize)
                .and_then(|d| d.captures.get(*index as usize))
                .ok_or_else(|| err("environment input capture is absent"))?;
            ContractProjection::Capture(*index)
        } else {
            ty = match (ty, physical) {
                (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(i),
                ) => fields.get(*i as usize).map(|(_, t)| t),
                (Type::Array { element, length }, PlaceProjection::Field(i)) if i < length => {
                    Some(element.as_ref())
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                    .get(p.variant as usize)
                    .and_then(|(_, fields)| fields.get(p.index as usize)),
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                    Some(if *e { error.as_ref() } else { result.as_ref() })
                }
                _ => None,
            }
            .ok_or_else(|| {
                err("environment source physical projection differs from actual input type")
            })?;
            ContractProjection::Place(*physical)
        };
        path.push(selector);
    }
    Ok(StoragePosition {
        base: StorageBase::Parameter(index),
        path,
    })
}
/// The completed memory solver, rather than an advertised relation, proves
/// every returned environment's actual inputs, including overwritten slots.
pub(super) fn check_summary(
    module: &TypedModule,
    f: &TypedFunction,
    summary: &Summary,
    solution: &Solution,
    graph: &FunctionGraph,
    b: &mut Budget,
) -> Check<()> {
    let Some(contract) = &f.callable_return_contract else {
        return Ok(());
    };
    let declared = contract
        .capture_sources
        .as_ref()
        .ok_or_else(|| err("environment sources unknown"))?;
    let limits = b.limits.clone();
    let actual_leaves =
        crate::retained::sources::capture_leaves(module, contract.closure, &limits, b)?;
    let physical = leaves(module, contract.closure, b)?;
    for (actual_leaf, (path, _)) in actual_leaves.iter().zip(&physical) {
        b.spend(
            summary
                .result
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let sources = summary
            .result
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, sources)| sources)
            .ok_or_else(|| err("returned environment leaf has no independently grounded row"))?;
        if sources.is_empty() {
            return Err(err("returned environment leaf has no actual source"));
        }
        b.spend(
            declared
                .len()
                .saturating_mul(actual_leaf.position.path.len().saturating_add(1)),
        )?;
        let upper = declared
            .iter()
            .find(|binding| binding.position == actual_leaf.position);
        for source in sources {
            if source.base == HoldRoot::Static {
                if !source.leaf.is_empty()
                    || !source.inline.is_empty()
                    || source.heap_element.is_some()
                    || source.retained_port.is_some()
                    || !source.heap_parents.is_empty()
                {
                    return Err(err("static environment source has an unproven projection"));
                }
                let Some(upper) = upper else {
                    return Err(err("actual static capture has no explicit source upper"));
                };
                type_work(actual_leaf.ty, &b.limits.clone(), b)?;
                let mut found = false;
                b.spend(upper.sources.len())?;
                for atom in &upper.sources {
                    if let StorageSource::Static { leaf, mode } = atom {
                        type_work(leaf, &b.limits.clone(), b)?;
                        found |= *mode == StoredBorrowMode::SharedAddress
                            && matches!(actual_leaf.ty, Type::Address { mutable: false, .. })
                            && leaf == actual_leaf.ty;
                    }
                }
                if !found {
                    return Err(err(
                        "actual static capture is omitted by its complete typed upper",
                    ));
                }
                static_parents(f, path, solution, graph, b)?;
                continue;
            }
            let actual = actual_input_position(module, f, source, b)?;
            b.spend(
                upper
                    .map_or(1, |binding| binding.sources.len())
                    .saturating_mul(actual.path.len().saturating_add(1)),
            )?;
            if !match upper {
                Some(binding) => binding.sources.iter().any(|atom| {
                    matches!(atom,
                    StorageSource::Exact(source) if *source == actual)
                }),
                None => false,
            } {
                return Err(err(
                    "environment source claim omits an actual stored target input",
                ));
            }
        }
    }
    b.spend(b.pending_callable_sources.len())?;
    b.pending_callable_sources
        .retain(|function| *function != f.id);
    Ok(())
}

/// Static target spelling cannot erase a retained activation or receiver head.
/// Reconstruct every real reachable Return instead of trusting the upper.
fn static_parents(
    f: &TypedFunction,
    path: &[PlaceProjection],
    solution: &Solution,
    graph: &FunctionGraph,
    b: &mut Budget,
) -> Check<()> {
    b.spend(f.blocks.len())?;
    for (index, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[index] {
            continue;
        }
        let Terminator::Return(Some(value)) = block.terminator else {
            continue;
        };
        let actual = leaf(&solution.values[value.0 as usize], path, b)?
            .ok_or_else(|| err("static capture Return has no actual leaf"))?;
        b.spend(actual.sources.len())?;
        if !actual
            .sources
            .iter()
            .any(|source| source.base == HoldRoot::Static)
        {
            continue;
        }
        for id in ancestry(&actual.loans, &solution.loans, b)? {
            let loan = solution
                .loans
                .get(id)
                .ok_or_else(|| err("static capture parent lies outside actual loan arena"))?;
            b.spend(loan.sources.len().saturating_add(1))?;
            if loan.receiver_head
                || loan.sources.is_empty()
                || loan
                    .sources
                    .iter()
                    .any(|source| source.base != HoldRoot::Static)
            {
                return Err(err(
                    "static capture retains a nonstatic or ungrounded parent",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn borrowed_child(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.returned_environment_sources {
        return Ok(false);
    }
    let InstructionKind::BorrowCapture { index, .. } = i.kind else {
        return Ok(false);
    };
    b.spend(1)?;
    Ok(f.environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .and_then(|d| d.captures.get(index as usize))
        .is_some_and(|ty| matches!(ty, Type::Closure(_))))
}

/// Capturing an address retains the pointer and its original parents. Its
/// pointee slots are memory, so each invocation reads the actual current row.
/// Already extracted SSA data pointers keep their immutable payload instead.
pub(super) fn current_leaf(
    definition: &ClosureDefinition,
    environment: &Payload,
    path: &[PlaceProjection],
    snapshot: &Permission,
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    if let Some((PlaceProjection::Field(index), tail)) = path.split_first()
        && !tail.is_empty()
        && matches!(
            definition.captures.get(*index as usize),
            Some(Type::Address { .. })
        )
    {
        let head = leaf(environment, &[PlaceProjection::Field(*index)], b)?
            .ok_or_else(|| err("captured pointee lacks its actual head"))?;
        return select(state, head, tail, b);
    }
    permission_work(snapshot, b)?;
    Ok(snapshot.clone())
}
