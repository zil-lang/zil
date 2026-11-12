//! Producer proposals from complete Typed CFGs. Neither safety kernel consumes
//! this solver's internal state; they must reconstruct actual target relations.
use super::*;
type Path = Vec<ContractProjection>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Origins {
    inputs: BTreeSet<StoragePosition>,
    local: bool,
}
type Row = BTreeMap<Path, Origins>;
fn cost(row: &Row, left: &mut usize, span: Span) -> R<()> {
    spend(left, row.len(), span)?;
    for (path, origins) in row {
        spend(left, path.len().saturating_add(origins.inputs.len()), span)?;
        for source in &origins.inputs {
            spend(left, source.path.len(), span)?;
        }
    }
    Ok(())
}
fn join(to: &mut Row, from: &Row, left: &mut usize, span: Span) -> R<bool> {
    cost(to, left, span)?;
    cost(from, left, span)?;
    let mut changed = false;
    for (path, origins) in from {
        let at = to.entry(path.clone()).or_default();
        let before = (at.inputs.len(), at.local);
        at.inputs.extend(origins.inputs.iter().cloned());
        at.local |= origins.local;
        changed |= before != (at.inputs.len(), at.local);
    }
    Ok(changed)
}
fn copy(row: &Row, left: &mut usize, span: Span) -> R<Row> {
    cost(row, left, span)?;
    Ok(row.clone())
}
fn leaves(
    module: &TypedModule,
    ty: &Type,
    depth: usize,
    path: &mut Path,
    out: &mut Vec<Path>,
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<()> {
    spend(left, path.len().saturating_add(1), span)?;
    if depth > limit {
        return Err(unsupported(
            span,
            "returned capture type has no finite leaf tree",
        ));
    }
    match ty {
        Type::Address { .. } | Type::Desc(_) => out.push(path.clone()),
        Type::BorrowedStorage { life, .. } => {
            for slot in &life.slots {
                spend(
                    left,
                    path.len().saturating_add(slot.path.len()).saturating_add(1),
                    span,
                )?;
                let mut leaf = path.clone();
                leaf.extend(slot.path.iter().copied().map(ContractProjection::Place));
                out.push(leaf);
            }
        }
        Type::Closure(id) => {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| unsupported(span, "returned capture definition absent"))?;
            for (index, ty) in d.captures.iter().enumerate() {
                path.push(ContractProjection::Capture(index as u32));
                leaves(module, ty, depth + 1, path, out, left, span, limit)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (index, ty) in fields.iter().enumerate() {
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                leaves(module, ty, depth + 1, path, out, left, span, limit)?;
                path.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                leaves(module, ty, depth + 1, path, out, left, span, limit)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}
fn seed(
    module: &TypedModule,
    ty: &Type,
    base: Option<StorageBase>,
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<Row> {
    let mut paths = Vec::new();
    leaves(
        module,
        ty,
        0,
        &mut Vec::new(),
        &mut paths,
        left,
        span,
        limit,
    )?;
    let mut out = Row::new();
    for path in paths {
        spend(left, path.len().saturating_mul(2).saturating_add(1), span)?;
        out.insert(
            path.clone(),
            Origins {
                inputs: base
                    .map(|base| BTreeSet::from([StoragePosition { base, path }]))
                    .unwrap_or_default(),
                local: base.is_none(),
            },
        );
    }
    Ok(out)
}
fn select(row: &Row, prefix: &[ContractProjection], left: &mut usize, span: Span) -> R<Row> {
    cost(row, left, span)?;
    spend(left, prefix.len().saturating_mul(row.len()), span)?;
    Ok(row
        .iter()
        .filter_map(|(path, o)| path.strip_prefix(prefix).map(|p| (p.to_vec(), o.clone())))
        .collect())
}
fn wrap(row: &Row, step: ContractProjection, left: &mut usize, span: Span) -> R<Row> {
    cost(row, left, span)?;
    Ok(row
        .iter()
        .map(|(path, o)| {
            let mut p = vec![step];
            p.extend_from_slice(path);
            (p, o.clone())
        })
        .collect())
}
fn pointed(row: &Row, path: &[ContractProjection], left: &mut usize, span: Span) -> R<Row> {
    let mut out = Row::new();
    if let Some(o) = row.get(&Vec::new()) {
        spend(
            left,
            path.len().saturating_mul(o.inputs.len().saturating_add(1)),
            span,
        )?;
        let mut o = o.clone();
        o.inputs = o
            .inputs
            .into_iter()
            .map(|mut source| {
                source.path.extend_from_slice(path);
                source
            })
            .collect();
        out.insert(Vec::new(), o);
    }
    Ok(out)
}
fn solve(
    module: &TypedModule,
    f: &TypedFunction,
    summaries: &[Row],
    left: &mut usize,
    limit: usize,
) -> R<Row> {
    let span = f.span;
    spend(
        left,
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(f.blocks.len().saturating_mul(3)),
        span,
    )?;
    let mut values = vec![Row::new(); f.values.len()];
    let mut outputs = vec![vec![Row::new(); f.locals.len()]; f.blocks.len()];
    let mut predecessors = vec![Vec::new(); f.blocks.len()];
    let mut reachable = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry.0 as usize];
    while let Some(index) = pending.pop() {
        spend(left, 1, span)?;
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        let targets = match f.blocks[index].terminator {
            Terminator::Jump(to) => vec![to],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            _ => vec![],
        };
        for target in targets {
            spend(left, 1, span)?;
            predecessors[target.0 as usize].push(index);
            pending.push(target.0 as usize);
        }
    }
    let mut initial = vec![Row::new(); f.locals.len()];
    for (index, local) in f.parameters.iter().enumerate() {
        initial[local.0 as usize] = seed(
            module,
            &f.locals[local.0 as usize].ty,
            Some(StorageBase::Parameter(index as u32)),
            left,
            span,
            limit,
        )?;
    }
    loop {
        let mut changed = false;
        for (index, block) in f.blocks.iter().enumerate() {
            spend(left, 1, span)?;
            if !reachable[index] {
                continue;
            }
            let mut state = vec![Row::new(); f.locals.len()];
            if index == f.entry.0 as usize {
                for (to, from) in state.iter_mut().zip(&initial) {
                    join(to, from, left, span)?;
                }
            }
            for predecessor in &predecessors[index] {
                for (to, from) in state.iter_mut().zip(&outputs[*predecessor]) {
                    join(to, from, left, span)?;
                }
            }
            for instruction in &block.instructions {
                spend(left, 1, instruction.span)?;
                let i = &instruction.kind;
                let at = instruction.span;
                if let Some(result) = instruction.result {
                    let row = match i {
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            copy(&state[local.0 as usize], left, at)?
                        }
                        InstructionKind::BorrowLocal { .. }
                        | InstructionKind::BorrowLocalMut { .. }
                        | InstructionKind::BorrowPlaceLocal { .. }
                        | InstructionKind::BorrowFieldLocal { .. } => {
                            seed(module, &f.values[result.0 as usize], None, left, at, limit)?
                        }
                        InstructionKind::ReborrowAddress { address, .. }
                        | InstructionKind::DescribeAddress { address } => {
                            copy(&values[address.0 as usize], left, at)?
                        }
                        InstructionKind::AddressField { address, index } => pointed(
                            &values[address.0 as usize],
                            &[ContractProjection::Place(PlaceProjection::Field(*index))],
                            left,
                            at,
                        )?,
                        InstructionKind::BorrowPlaceAddress { address, path, .. } => pointed(
                            &values[address.0 as usize],
                            &path
                                .iter()
                                .copied()
                                .map(ContractProjection::Place)
                                .collect::<Vec<_>>(),
                            left,
                            at,
                        )?,
                        InstructionKind::LoadStoredBorrow { storage, path, .. } => pointed(
                            &values[storage.0 as usize],
                            &path
                                .iter()
                                .copied()
                                .map(ContractProjection::Place)
                                .collect::<Vec<_>>(),
                            left,
                            at,
                        )?,
                        InstructionKind::MakeClosure { captures, .. } => {
                            let mut r = Row::new();
                            for (index, value) in captures.iter().enumerate() {
                                join(
                                    &mut r,
                                    &wrap(
                                        &values[value.0 as usize],
                                        ContractProjection::Capture(index as u32),
                                        left,
                                        at,
                                    )?,
                                    left,
                                    at,
                                )?;
                            }
                            r
                        }
                        InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeOwnedTuple { fields } => {
                            let mut r = Row::new();
                            for (index, value) in fields.iter().enumerate() {
                                join(
                                    &mut r,
                                    &wrap(
                                        &values[value.0 as usize],
                                        ContractProjection::Place(PlaceProjection::Field(
                                            index as u32,
                                        )),
                                        left,
                                        at,
                                    )?,
                                    left,
                                    at,
                                )?;
                            }
                            r
                        }
                        InstructionKind::ExtractField { aggregate, index } => select(
                            &values[aggregate.0 as usize],
                            &[ContractProjection::Place(PlaceProjection::Field(*index))],
                            left,
                            at,
                        )?,
                        InstructionKind::ReadPackageField { local, index }
                        | InstructionKind::TakePackageField { local, index } => select(
                            &state[local.0 as usize],
                            &[ContractProjection::Place(PlaceProjection::Field(*index))],
                            left,
                            at,
                        )?,
                        InstructionKind::TakeTupleField { local, path }
                        | InstructionKind::TakeVariantField { local, path } => select(
                            &state[local.0 as usize],
                            &path
                                .iter()
                                .copied()
                                .map(ContractProjection::Place)
                                .collect::<Vec<_>>(),
                            left,
                            at,
                        )?,
                        InstructionKind::ReadCapture { index }
                        | InstructionKind::TakeCapture { index } => seed(
                            module,
                            &f.values[result.0 as usize],
                            Some(StorageBase::Capture(*index)),
                            left,
                            at,
                            limit,
                        )?,
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let mut r = Row::new();
                            for (path, origins) in &summaries[function.0 as usize] {
                                let mut o = Origins {
                                    local: origins.local,
                                    ..Origins::default()
                                };
                                for source in &origins.inputs {
                                    let StorageBase::Parameter(index) = source.base else {
                                        o.local = true;
                                        continue;
                                    };
                                    let actual = &values[arguments[index as usize].0 as usize];
                                    let selected = if source.path.is_empty() {
                                        select(actual, &[], left, at)?
                                    } else {
                                        let direct = select(actual, &source.path, left, at)?;
                                        if direct.is_empty() {
                                            pointed(actual, &source.path, left, at)?
                                        } else {
                                            direct
                                        }
                                    };
                                    if let Some(from) = selected.get(&Vec::new()) {
                                        o.inputs.extend(from.inputs.iter().cloned());
                                        o.local |= from.local;
                                    } else {
                                        o.local = true;
                                    }
                                }
                                r.insert(path.clone(), o);
                            }
                            r
                        }
                        _ => seed(module, &f.values[result.0 as usize], None, left, at, limit)?,
                    };
                    changed |= join(&mut values[result.0 as usize], &row, left, at)?;
                }
                match i {
                    InstructionKind::Store { local, value } => {
                        state[local.0 as usize] = copy(&values[value.0 as usize], left, at)?
                    }
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local) => state[local.0 as usize].clear(),
                    _ => {}
                }
            }
            for (to, from) in outputs[index].iter_mut().zip(state) {
                changed |= join(to, &from, left, span)?;
            }
        }
        if !changed {
            break;
        }
    }
    let mut returned = Row::new();
    for (block, live) in f.blocks.iter().zip(reachable) {
        if live && let Terminator::Return(Some(value)) = block.terminator {
            join(&mut returned, &values[value.0 as usize], left, span)?;
        }
    }
    Ok(returned)
}

pub(crate) fn populate(module: &mut TypedModule, left: &mut usize, limits: &Limits) -> R<()> {
    // Existing self-contained factories keep their old work profile.
    let active = module.functions.iter().any(|f| {
        f.callable_return_contract
            .as_ref()
            .is_some_and(|c| !c.hold.is_empty())
    });
    if !active {
        return Ok(());
    }
    spend(left, module.functions.len(), Span::default())?;
    let mut summaries = vec![Row::new(); module.functions.len()];
    let mut relevant = Vec::with_capacity(module.functions.len());
    for f in &module.functions {
        let mut result_leaves = Vec::new();
        leaves(
            module,
            &f.return_type,
            0,
            &mut Vec::new(),
            &mut result_leaves,
            left,
            f.span,
            limits.max_call_depth,
        )?;
        relevant.push(!result_leaves.is_empty());
    }
    loop {
        let mut changed = false;
        for f in &module.functions {
            if f.foreign.is_none() && relevant[f.id.0 as usize] {
                let row = solve(module, f, &summaries, left, limits.max_call_depth)?;
                changed |= join(&mut summaries[f.id.0 as usize], &row, left, f.span)?;
            }
        }
        if !changed {
            break;
        }
    }
    for f in &mut module.functions {
        let Some(contract) = &mut f.callable_return_contract else {
            continue;
        };
        let mut bindings = Vec::new();
        let mut hold = BTreeSet::new();
        for (path, origins) in &summaries[f.id.0 as usize] {
            if origins.local
                || origins.inputs.is_empty()
                || origins
                    .inputs
                    .iter()
                    .any(|s| !matches!(s.base, StorageBase::Parameter(_)))
            {
                return Err(error(
                    "E-CALLABLE-RETURN-ORIGIN",
                    f.span,
                    "returned environment retains local or ungrounded storage",
                ));
            }
            let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
                return Err(error(
                    "E-CALLABLE-RETURN-ORIGIN",
                    f.span,
                    "returned environment has no actual capture leaf",
                ));
            };
            hold.insert(*index);
            bindings.push(StorageSourceBinding {
                position: StoragePosition {
                    base: StorageBase::Capture(*index),
                    path: tail.to_vec(),
                },
                sources: origins.inputs.iter().cloned().collect(),
            });
        }
        contract.capture_sources = Some(bindings);
        contract.hold = hold.iter().copied().collect();
        contract.call = hold.into_iter().collect();
        contract.writes = contract
            .hold
            .iter()
            .copied()
            .filter(|index| {
                matches!(
                    module.closures[contract.closure.0 as usize].captures[*index as usize],
                    Type::Address { mutable: true, .. } | Type::Desc(_)
                )
            })
            .collect();
    }
    Ok(())
}
