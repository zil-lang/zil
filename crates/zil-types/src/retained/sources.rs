//! Producer proposals from complete Typed CFGs. Neither safety kernel consumes
//! this solver's internal state; they must reconstruct actual target relations.
use super::*;
#[path = "helper_classes.rs"]
mod helper_classes;
#[path = "mutable_sources.rs"]
mod mutable_sources;
type Path = Vec<ContractProjection>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Origins {
    inputs: BTreeSet<StoragePosition>,
    // Pointer storage is distinct from the targets stored inside a holder.
    places: BTreeSet<(LocalId, Path)>,
    local: bool,
    unknown: bool,
}
type Row = BTreeMap<Path, Origins>;
#[derive(Default)]
struct Summary {
    returned: Row,
    captures: Row,
    external: BTreeMap<u32, Row>,
}

fn cost(row: &Row, left: &mut usize, span: Span) -> R<()> {
    spend(left, row.len(), span)?;
    for (path, origins) in row {
        spend(left, path.len().saturating_add(origins.inputs.len()), span)?;
        spend(left, origins.places.len(), span)?;
        for (_, path) in &origins.places {
            spend(left, path.len(), span)?;
        }
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
        let before = (at.inputs.len(), at.places.len(), at.local, at.unknown);
        at.inputs.extend(origins.inputs.iter().cloned());
        at.places.extend(origins.places.iter().cloned());
        at.local |= origins.local;
        at.unknown |= origins.unknown;
        changed |= before != (at.inputs.len(), at.places.len(), at.local, at.unknown);
    }
    Ok(changed)
}
fn copy(row: &Row, left: &mut usize, span: Span) -> R<Row> {
    cost(row, left, span)?;
    Ok(row.clone())
}
#[allow(clippy::too_many_arguments)]
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
    let paths = if matches!(ty, Type::Closure(_)) {
        mutable_sources::effect_paths(module, ty, left, span, limit)?
    } else {
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
        paths
    };
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
                unknown: base.is_none(),
                places: BTreeSet::new(),
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
        o.places = o
            .places
            .into_iter()
            .map(|(local, mut at)| {
                at.extend_from_slice(path);
                (local, at)
            })
            .collect();
        out.insert(Vec::new(), o);
    }
    Ok(out)
}
fn stored_read(
    row: &Row,
    path: &[ContractProjection],
    state: &[Row],
    left: &mut usize,
    span: Span,
) -> R<Row> {
    let mut out = Row::new();
    if let Some(origin) = row.get(&Vec::new()) {
        let mut external = copy(row, left, span)?;
        if let Some(at) = external.get_mut(&Vec::new()) {
            at.places.clear();
            at.local &= origin.places.is_empty();
        }
        if origin.places.is_empty() || origin.unknown {
            join(&mut out, &pointed(&external, path, left, span)?, left, span)?;
        }
        for (local, prefix) in &origin.places {
            spend(left, prefix.len().saturating_add(path.len()), span)?;
            let mut full = prefix.clone();
            full.extend_from_slice(path);
            join(
                &mut out,
                &select(&state[local.0 as usize], &full, left, span)?,
                left,
                span,
            )?;
        }
    }
    Ok(out)
}

fn solve(
    module: &TypedModule,
    f: &TypedFunction,
    summaries: &[Summary],
    left: &mut usize,
    limit: usize,
) -> R<Summary> {
    let span = f.span;
    let captures = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .map_or(&[][..], |definition| definition.captures.as_slice());
    let mut external_cells = BTreeMap::new();
    for (position, local) in f.parameters.iter().enumerate() {
        if let Type::Address { inner, .. } = &f.locals[local.0 as usize].ty
            && matches!(
                inner.as_ref(),
                Type::Closure(_) | Type::BorrowedStorage { .. }
            )
        {
            spend(left, 1, span)?;
            external_cells.insert(
                position,
                LocalId((f.locals.len() + captures.len() + external_cells.len()) as u32),
            );
        }
    }
    let mut captured_cells = BTreeMap::new();
    for (index, ty) in captures.iter().enumerate() {
        if let Type::Address { inner, .. } = ty
            && matches!(inner.as_ref(), Type::BorrowedStorage { .. })
        {
            spend(left, 1, span)?;
            captured_cells.insert(
                index,
                LocalId(
                    (f.locals.len() + captures.len() + external_cells.len() + captured_cells.len())
                        as u32,
                ),
            );
        }
    }
    let width = f
        .locals
        .len()
        .saturating_add(captures.len())
        .saturating_add(external_cells.len())
        .saturating_add(captured_cells.len());

    spend(
        left,
        f.values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(f.blocks.len().saturating_mul(3)),
        span,
    )?;
    let mut values = vec![Row::new(); f.values.len()];
    spend(
        left,
        captures
            .len()
            .saturating_add(external_cells.len())
            .saturating_add(captured_cells.len())
            .saturating_mul(f.blocks.len().saturating_add(1)),
        span,
    )?;
    let mut outputs = vec![vec![Row::new(); width]; f.blocks.len()];
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
    let mut initial = vec![Row::new(); width];
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
    for (index, ty) in captures.iter().enumerate() {
        initial[f.locals.len() + index] = seed(
            module,
            ty,
            Some(StorageBase::Capture(index as u32)),
            left,
            span,
            limit,
        )?;
    }
    for (position, cell) in &external_cells {
        let local = f.parameters[*position];
        let Type::Address { inner, .. } = &f.locals[local.0 as usize].ty else {
            unreachable!()
        };
        initial[cell.0 as usize] = seed(
            module,
            inner,
            Some(StorageBase::Parameter(*position as u32)),
            left,
            span,
            limit,
        )?;
        if let Some(head) = initial[local.0 as usize].get_mut(&Vec::new()) {
            head.places.insert((*cell, Vec::new()));
        }
    }
    for (index, cell) in &captured_cells {
        let Type::Address { inner, .. } = &captures[*index] else {
            unreachable!()
        };
        initial[cell.0 as usize] = seed(
            module,
            inner,
            Some(StorageBase::Capture(*index as u32)),
            left,
            span,
            limit,
        )?;
        if let Some(head) = initial[f.locals.len() + *index].get_mut(&Vec::new()) {
            head.places.insert((*cell, Vec::new()));
        }
    }
    loop {
        let mut changed = false;
        for (index, block) in f.blocks.iter().enumerate() {
            spend(left, 1, span)?;
            if !reachable[index] {
                continue;
            }
            let mut state = vec![Row::new(); width];
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
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            let mut r =
                                seed(module, &f.values[result.0 as usize], None, left, at, limit)?;
                            if let Some(origin) = r.get_mut(&Vec::new()) {
                                origin.unknown = false;
                                origin.places.insert((*local, Vec::new()));
                            }
                            r
                        }
                        InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                            let mut r =
                                seed(module, &f.values[result.0 as usize], None, left, at, limit)?;
                            if let Some(origin) = r.get_mut(&Vec::new()) {
                                origin.unknown = false;
                                origin.places.insert((
                                    *local,
                                    path.iter()
                                        .copied()
                                        .map(ContractProjection::Place)
                                        .collect(),
                                ));
                            }
                            r
                        }
                        InstructionKind::BorrowFieldLocal { local, path, .. } => {
                            let mut r =
                                seed(module, &f.values[result.0 as usize], None, left, at, limit)?;
                            if let Some(origin) = r.get_mut(&Vec::new()) {
                                origin.unknown = false;
                                origin.places.insert((
                                    *local,
                                    path.iter()
                                        .map(|i| {
                                            ContractProjection::Place(PlaceProjection::Field(*i))
                                        })
                                        .collect(),
                                ));
                            }
                            r
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
                        InstructionKind::LoadStoredBorrow { storage, path, .. } => stored_read(
                            &values[storage.0 as usize],
                            &path
                                .iter()
                                .copied()
                                .map(ContractProjection::Place)
                                .collect::<Vec<_>>(),
                            &state,
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
                                if let Type::Address { inner, .. } = &f.values[value.0 as usize]
                                    && let Type::BorrowedStorage { life, .. } = inner.as_ref()
                                {
                                    for slot in &life.slots {
                                        spend(left, slot.path.len(), at)?;
                                        let path = slot
                                            .path
                                            .iter()
                                            .copied()
                                            .map(ContractProjection::Place)
                                            .collect::<Vec<_>>();
                                        let nested = mutable_sources::current(
                                            &values[value.0 as usize],
                                            &path,
                                            &state,
                                            left,
                                            at,
                                        )?;
                                        for (tail, origin) in nested {
                                            spend(
                                                left,
                                                path.len()
                                                    .saturating_add(tail.len())
                                                    .saturating_add(1),
                                                at,
                                            )?;
                                            let mut full =
                                                vec![ContractProjection::Capture(index as u32)];
                                            full.extend_from_slice(&path);
                                            full.extend(tail);
                                            join(&mut r, &Row::from([(full, origin)]), left, at)?;
                                        }
                                    }
                                }
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
                        | InstructionKind::TakeCapture { index } => {
                            copy(&state[f.locals.len() + *index as usize], left, at)?
                        }
                        InstructionKind::BorrowCapture { index, .. } => {
                            let mut row =
                                seed(module, &f.values[result.0 as usize], None, left, at, limit)?;
                            if let Some(head) = row.get_mut(&Vec::new()) {
                                head.unknown = false;
                                head.places.insert((
                                    LocalId((f.locals.len() + *index as usize) as u32),
                                    Vec::new(),
                                ));
                            }
                            row
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let mut r = Row::new();
                            for (path, origins) in &summaries[function.0 as usize].returned {
                                let mut o = Origins {
                                    local: origins.local,
                                    unknown: origins.unknown,
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
                                        mutable_sources::current(
                                            actual,
                                            &source.path,
                                            &state,
                                            left,
                                            at,
                                        )?
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
                    InstructionKind::BindPackageCapture { local, index } => {
                        state[local.0 as usize] =
                            copy(&state[f.locals.len() + *index as usize], left, at)?
                    }
                    InstructionKind::EndPackageCapture { local, index } => {
                        state[f.locals.len() + *index as usize] =
                            copy(&state[local.0 as usize], left, at)?
                    }
                    InstructionKind::WriteCapture { index, value } => {
                        state[f.locals.len() + *index as usize] =
                            copy(&values[value.0 as usize], left, at)?
                    }
                    InstructionKind::StoreStoredBorrow {
                        storage,
                        path,
                        value,
                    } => {
                        let prefix = path
                            .iter()
                            .copied()
                            .map(ContractProjection::Place)
                            .collect::<Vec<_>>();
                        let address = pointed(&values[storage.0 as usize], &prefix, left, at)?;
                        mutable_sources::publish(
                            &address,
                            &values[value.0 as usize],
                            &mut state,
                            left,
                            at,
                        )?;
                    }
                    InstructionKind::StorePackageField {
                        local,
                        index,
                        value,
                    } => mutable_sources::replace(
                        &mut state[local.0 as usize],
                        &[ContractProjection::Place(PlaceProjection::Field(*index))],
                        &values[value.0 as usize],
                        false,
                        left,
                        at,
                    )?,
                    InstructionKind::CallClosureMut { callee, arguments } => {
                        if let Type::Closure(id) = f.locals[callee.0 as usize].ty {
                            let updates = mutable_sources::invoke(
                                module,
                                id,
                                &state[callee.0 as usize],
                                arguments,
                                &values,
                                &state,
                                summaries,
                                left,
                                at,
                                limit,
                            )?;
                            let publications = mutable_sources::context_updates(
                                module,
                                id,
                                &state[callee.0 as usize],
                                &updates,
                                &state,
                                left,
                                at,
                                limit,
                            )?;
                            for (address, fields) in publications {
                                mutable_sources::publish(&address, &fields, &mut state, left, at)?;
                            }
                            mutable_sources::replace(
                                &mut state[callee.0 as usize],
                                &[],
                                &updates,
                                false,
                                left,
                                at,
                            )?;
                        }
                    }
                    InstructionKind::CallClosureBorrowed {
                        callee,
                        kind: CallableKind::FnMut,
                        arguments,
                    } => {
                        if let Type::Address { inner, .. } = &f.values[callee.0 as usize]
                            && let Type::Closure(id) = inner.as_ref()
                        {
                            let updates = mutable_sources::invoke(
                                module,
                                *id,
                                &values[callee.0 as usize],
                                arguments,
                                &values,
                                &state,
                                summaries,
                                left,
                                at,
                                limit,
                            )?;
                            let publications = mutable_sources::context_updates(
                                module,
                                *id,
                                &values[callee.0 as usize],
                                &updates,
                                &state,
                                left,
                                at,
                                limit,
                            )?;
                            for (address, fields) in publications {
                                mutable_sources::publish(&address, &fields, &mut state, left, at)?;
                            }
                            mutable_sources::publish(
                                &values[callee.0 as usize],
                                &updates,
                                &mut state,
                                left,
                                at,
                            )?;
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let summary = &summaries[function.0 as usize];
                        let callee = &module.functions[function.0 as usize];
                        let mut publications = Vec::new();
                        for (parameter, local) in callee.parameters.iter().enumerate() {
                            let Type::Address {
                                mutable: true,
                                inner,
                            } = &callee.locals[local.0 as usize].ty
                            else {
                                continue;
                            };
                            if !matches!(
                                inner.as_ref(),
                                Type::Closure(_) | Type::BorrowedStorage { .. }
                            ) {
                                continue;
                            }
                            let paths =
                                mutable_sources::effect_paths(module, inner, left, at, limit)?;
                            let mut updates = Row::new();
                            for path in paths {
                                let origin = if let Some(actual) = summary
                                    .external
                                    .get(&(parameter as u32))
                                    .and_then(|row| row.get(&path))
                                {
                                    mutable_sources::substitute(
                                        actual, None, arguments, &values, &state, left, at,
                                    )?
                                } else {
                                    Origins::default()
                                };
                                updates.insert(path, origin);
                            }
                            publications.push((parameter, updates));
                        }
                        for (parameter, updates) in publications {
                            mutable_sources::publish(
                                &values[arguments[parameter].0 as usize],
                                &updates,
                                &mut state,
                                left,
                                at,
                            )?;
                        }
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
    let mut summary = Summary::default();
    for (index, (block, live)) in f.blocks.iter().zip(reachable).enumerate() {
        if live && matches!(block.terminator, Terminator::Return(_)) {
            if let Terminator::Return(Some(value)) = block.terminator {
                join(&mut summary.returned, &values[value.0 as usize], left, span)?;
            }
            for capture in 0..captures.len() {
                let row = wrap(
                    &outputs[index][f.locals.len() + capture],
                    ContractProjection::Capture(capture as u32),
                    left,
                    span,
                )?;
                join(&mut summary.captures, &row, left, span)?;
            }
            for (capture, cell) in &captured_cells {
                let row = wrap(
                    &outputs[index][cell.0 as usize],
                    ContractProjection::Capture(*capture as u32),
                    left,
                    span,
                )?;
                join(&mut summary.captures, &row, left, span)?;
            }
            for (parameter, cell) in &external_cells {
                let local = f.parameters[*parameter];
                if matches!(
                    f.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, .. }
                ) {
                    let row = copy(&outputs[index][cell.0 as usize], left, span)?;
                    join(
                        summary.external.entry(*parameter as u32).or_default(),
                        &row,
                        left,
                        span,
                    )?;
                }
            }
        }
    }
    Ok(summary)
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
    let mut summaries = (0..module.functions.len())
        .map(|_| Summary::default())
        .collect::<Vec<_>>();
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
        let mutable_environment = f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .is_some_and(|d| d.kind == CallableKind::FnMut);
        let mutable_input=f.parameters.iter().any(|local|matches!(&f.locals[local.0 as usize].ty,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Closure(_)|Type::BorrowedStorage{..})));
        relevant.push(!result_leaves.is_empty() || mutable_environment || mutable_input);
    }
    loop {
        let mut changed = false;
        for f in &module.functions {
            if f.foreign.is_none() && relevant[f.id.0 as usize] {
                let actual = solve(module, f, &summaries, left, limits.max_call_depth)?;
                let summary = &mut summaries[f.id.0 as usize];
                changed |= join(&mut summary.returned, &actual.returned, left, f.span)?;
                changed |= join(&mut summary.captures, &actual.captures, left, f.span)?;
                for (parameter, row) in actual.external {
                    changed |= join(
                        summary.external.entry(parameter).or_default(),
                        &row,
                        left,
                        f.span,
                    )?;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut public_return_paths = BTreeMap::new();
    for f in &module.functions {
        if f.callable_return_contract.is_some() {
            let mut paths = Vec::new();
            leaves(
                module,
                &f.return_type,
                0,
                &mut Vec::new(),
                &mut paths,
                left,
                f.span,
                limits.max_call_depth,
            )?;
            public_return_paths.insert(f.id, paths);
        }
    }
    for f in &mut module.functions {
        let Some(contract) = &mut f.callable_return_contract else {
            continue;
        };
        let mut bindings = Vec::new();
        let mut hold = BTreeSet::new();
        for (path, origins) in &summaries[f.id.0 as usize].returned {
            let paths = &public_return_paths[&f.id];
            spend(
                left,
                paths.len().saturating_mul(path.len().saturating_add(1)),
                f.span,
            )?;
            if !paths.contains(path) {
                continue;
            }
            if origins.local
                || origins.unknown
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
                ) || matches!(&module.closures[contract.closure.0 as usize].captures[*index as usize],
                    Type::BorrowedStorage { life, .. } if life.slots.iter().any(|slot|
                        matches!(slot.mode, StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription)))
            })
            .collect();
    }
    loop {
        helper_classes::factory_inputs(module, left)?;
        if !populate_creator_fixed(module, left, limits.max_call_depth)? {
            break;
        }
        helper_classes::populate(module, left, limits)?;
    }
    Ok(())
}

// A proposal from the actual creator header. This never supplies targets: both
// kernels must check the MakeClosure class equation and every source leaf.
fn source_class(
    module: &TypedModule,
    f: &TypedFunction,
    source: &StoragePosition,
    left: &mut usize,
) -> R<StorageBinder> {
    spend(left, source.path.len().saturating_add(1), f.span)?;
    let StorageBase::Parameter(index) = source.base else {
        return Err(unsupported(
            f.span,
            "capture class source is not an actual creator input",
        ));
    };
    let local = *f
        .parameters
        .get(index as usize)
        .ok_or_else(|| unsupported(f.span, "capture class input absent"))?;
    let life = f.storage_life.as_ref().ok_or_else(|| {
        unsupported(
            f.span,
            "capture class needs its complete actual creator header",
        )
    })?;
    if source.path.is_empty() {
        spend(left, life.bindings.len(), f.span)?;
        if let Some(binding) = life
            .bindings
            .iter()
            .find(|binding| binding.position == *source)
        {
            return Ok(binding.binder);
        }
    }
    let (ty, pointer) = match &f.locals[local.0 as usize].ty {
        Type::Address { inner, .. } => (inner.as_ref(), true),
        ty => (ty, false),
    };
    if let Type::Closure(id) = ty {
        let class = capture_class(module, *id, &source.path, 0, left, f.span)?;
        return header_class(
            f,
            index,
            if pointer {
                vec![LifeTypeProjection::Pointee]
            } else {
                Vec::new()
            },
            class,
            left,
        );
    }
    let Type::BorrowedStorage { life: shape, .. } = ty else {
        return Err(unsupported(
            f.span,
            "capture class source has no actual qualified owner",
        ));
    };
    spend(
        left,
        shape
            .slots
            .len()
            .saturating_mul(source.path.len().saturating_add(1)),
        f.span,
    )?;
    let class = shape
        .slots
        .iter()
        .find(|slot| {
            source
                .path
                .iter()
                .copied()
                .eq(slot.path.iter().copied().map(ContractProjection::Place))
        })
        .map(|slot| slot.binder)
        .ok_or_else(|| {
            unsupported(
                f.span,
                "capture class source does not select an actual stored leaf",
            )
        })?;
    header_class(
        f,
        index,
        if pointer {
            vec![LifeTypeProjection::Pointee]
        } else {
            Vec::new()
        },
        class,
        left,
    )
}
fn capture_class(
    module: &TypedModule,
    id: ClosureId,
    path: &[ContractProjection],
    depth: usize,
    left: &mut usize,
    span: Span,
) -> R<u32> {
    spend(left, path.len().saturating_add(1), span)?;
    if depth > module.closures.len() {
        return Err(unsupported(span, "captured class graph is cyclic"));
    }
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| unsupported(span, "captured class child absent"))?;
    let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
        return Err(unsupported(
            span,
            "nested class source omits its capture selector",
        ));
    };
    let ty = d
        .captures
        .get(*index as usize)
        .ok_or_else(|| unsupported(span, "captured class field absent"))?;
    let life = d
        .storage_life
        .as_ref()
        .ok_or_else(|| unsupported(span, "captured class child relation absent"))?;
    let base = StorageBase::Capture(*index);
    let actual = match ty {
        Type::Address { .. } | Type::Desc(_) if tail.is_empty() => {
            spend(left, life.bindings.len(), span)?;
            life.bindings
                .iter()
                .find(|row| row.position.base == base && row.position.path.is_empty())
                .map(|row| row.binder)
        }
        Type::BorrowedStorage { life: shape, .. } => {
            spend(
                left,
                shape
                    .slots
                    .len()
                    .saturating_mul(tail.len().saturating_add(1))
                    .saturating_add(life.classes.len()),
                span,
            )?;
            let class = shape
                .slots
                .iter()
                .find(|slot| {
                    tail.iter().copied().eq(slot
                        .path
                        .iter()
                        .copied()
                        .map(ContractProjection::Place))
                })
                .map(|slot| slot.binder)
                .ok_or_else(|| {
                    unsupported(span, "nested class source is not an actual stored leaf")
                })?;
            life.classes
                .iter()
                .find(|row| {
                    row.position.base == base
                        && row.position.path.is_empty()
                        && row.position.class == class
                })
                .map(|row| row.binder)
        }
        Type::Closure(child) => {
            let class = capture_class(module, *child, tail, depth + 1, left, span)?;
            spend(left, life.classes.len(), span)?;
            life.classes
                .iter()
                .find(|row| {
                    row.position.base == base
                        && row.position.path.is_empty()
                        && row.position.class == class
                })
                .map(|row| row.binder)
        }
        _ => None,
    }
    .ok_or_else(|| unsupported(span, "captured leaf has no exact child class binding"))?;
    match actual {
        StorageBinder::Fixed(class) => Ok(class),
        _ => Err(unsupported(span, "captured child leaf is not Fixed")),
    }
}
fn header_class(
    f: &TypedFunction,
    parameter: u32,
    path: Vec<LifeTypeProjection>,
    class: u32,
    left: &mut usize,
) -> R<StorageBinder> {
    let header = f
        .storage_life
        .as_ref()
        .ok_or_else(|| unsupported(f.span, "capture class header absent"))?;
    spend(
        left,
        header
            .classes
            .len()
            .saturating_mul(path.len().saturating_add(1)),
        f.span,
    )?;
    let position = SignatureLifeClass {
        base: StorageBase::Parameter(parameter),
        path,
        class,
    };
    header
        .classes
        .iter()
        .find(|row| row.position == position)
        .map(|row| row.binder)
        .ok_or_else(|| unsupported(f.span, "actual creator omits the captured nominal class"))
}
fn captured_parameter(
    module: &TypedModule,
    f: &TypedFunction,
    closure: ClosureId,
    capture: usize,
    left: &mut usize,
) -> R<u32> {
    spend(
        left,
        f.blocks
            .len()
            .saturating_add(f.values.len().saturating_mul(2)),
        f.span,
    )?;
    let mut definitions = vec![None; f.values.len()];
    let mut stores = BTreeMap::<LocalId, Vec<ValueId>>::new();
    let mut pending = Vec::new();
    for block in &f.blocks {
        spend(left, block.instructions.len(), f.span)?;
        for instruction in &block.instructions {
            if let Some(value) = instruction.result {
                definitions[value.0 as usize] = Some(&instruction.kind);
            }
            if let InstructionKind::Store { local, value } = instruction.kind {
                spend(left, stores.len().saturating_add(1), f.span)?;
                stores.entry(local).or_default().push(value);
            }
            if let InstructionKind::MakeClosure {
                closure: actual,
                captures,
            } = &instruction.kind
                && *actual == closure
            {
                pending.push(
                    *captures
                        .get(capture)
                        .ok_or_else(|| unsupported(f.span, "whole capture operand absent"))?,
                );
            }
        }
    }
    let mut seen = vec![false; f.values.len()];
    let mut parameter = None;
    while let Some(value) = pending.pop() {
        spend(left, 1, f.span)?;
        if seen[value.0 as usize] {
            continue;
        }
        seen[value.0 as usize] = true;
        match definitions[value.0 as usize] {
            Some(InstructionKind::Load(local) | InstructionKind::TakeLocal { local }) => {
                spend(
                    left,
                    f.parameters.len().saturating_add(stores.len()),
                    f.span,
                )?;
                if let Some(index) = f.parameters.iter().position(|input| input == local) {
                    let actual = index as u32;
                    if parameter.replace(actual).is_some_and(|old| old != actual) {
                        return Err(unsupported(
                            f.span,
                            "whole captured class joins different actual input instances",
                        ));
                    }
                } else if let Some(values) = stores.get(local) {
                    spend(left, values.len(), f.span)?;
                    pending.extend(values);
                } else {
                    return Err(unsupported(
                        f.span,
                        "whole captured class has no input origin",
                    ));
                }
            }
            Some(InstructionKind::Call {
                function,
                arguments,
            }) => {
                let Some(CallableReturnContract {
                    source: CallableReturnSource::Parameter(index),
                    ..
                }) = &module.functions[function.0 as usize].callable_return_contract
                else {
                    return Err(unsupported(
                        f.span,
                        "whole captured class requires an actual forwarded input",
                    ));
                };
                pending.push(
                    *arguments.get(*index as usize).ok_or_else(|| {
                        unsupported(f.span, "whole captured forwarded input absent")
                    })?,
                );
            }
            _ => {
                return Err(unsupported(
                    f.span,
                    "whole captured class is not transported from an input",
                ));
            }
        }
    }
    parameter
        .ok_or_else(|| unsupported(f.span, "whole captured class has no grounded actual input"))
}
fn relative_capture<'a>(
    position: &'a StoragePosition,
    prefix: &[ContractProjection],
) -> Option<(u32, &'a [ContractProjection])> {
    let StorageBase::Capture(index) = position.base else {
        return None;
    };
    if prefix.is_empty() {
        return Some((index, &position.path));
    }
    if prefix.first() != Some(&ContractProjection::Capture(index))
        || !position.path.starts_with(&prefix[1..])
    {
        return None;
    }
    let (ContractProjection::Capture(index), tail) =
        position.path[prefix.len() - 1..].split_first()?
    else {
        return None;
    };
    Some((*index, tail))
}
fn populate_creator_fixed(module: &mut TypedModule, left: &mut usize, limit: usize) -> R<bool> {
    let mut updates = Vec::new();
    for f in &module.functions {
        let Some(contract) = &f.callable_return_contract else {
            continue;
        };
        if contract.source != CallableReturnSource::Fresh {
            continue;
        }
        let mut pending = vec![(contract.closure, Vec::<ContractProjection>::new())];
        let mut candidates = Vec::new();
        while let Some((id, prefix)) = pending.pop() {
            spend(left, prefix.len().saturating_add(1), f.span)?;
            if prefix.len() > limit {
                return Err(unsupported(
                    f.span,
                    "nested owning class graph exceeds finite depth",
                ));
            }
            let d = &module.closures[id.0 as usize];
            spend(left, d.captures.len(), f.span)?;
            for (index, capture) in d.captures.iter().enumerate() {
                if let Type::Closure(child) = capture {
                    spend(left, prefix.len().saturating_add(1), f.span)?;
                    let mut child_path = prefix.clone();
                    child_path.push(ContractProjection::Capture(index as u32));
                    pending.push((*child, child_path));
                }
            }
            candidates.push((id, prefix));
        }
        for (id, prefix) in candidates {
            if !prefix.is_empty() {
                // An input F's child was constructed by its own creator. Its
                // type alone cannot borrow this factory's header classes.
                let mut constructed = false;
                spend(left, f.blocks.len(), f.span)?;
                for block in &f.blocks {
                    spend(left, block.instructions.len(), f.span)?;
                    constructed |= block.instructions.iter().any(|i|matches!(i.kind,InstructionKind::MakeClosure{closure,..} if closure==id));
                }
                if !constructed {
                    continue;
                }
            }
            let definition = &module.closures[id.0 as usize];
            if definition.storage_life.is_some()
            || !definition.captures.iter().any(|ty|matches!(ty,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Package{..})) || matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::BorrowedStorage{..})) || matches!(ty,Type::Closure(id) if module.closures[id.0 as usize].storage_life.as_ref().is_some_and(|life|life.fixed_binder_count>0)))
        { continue; }
            if !definition.result.is_copy_value()
                || definition.parameters.iter().any(|ty| !ty.is_copy_value())
            {
                return Err(unsupported(
                    f.span,
                    "captured owning classes need the complete callable input/result class relation",
                ));
            }
            let mut keys = BTreeMap::new();
            let mut proposed = StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: Vec::new(),
                classes: Vec::new(),
            };
            let rows = contract
                .capture_sources
                .as_ref()
                .ok_or_else(|| unsupported(f.span, "capture class source relation unknown"))?;
            for (index, ty) in definition.captures.iter().enumerate() {
                let class_count = match ty {
                    Type::BorrowedStorage { life, .. } => life.binder_count,
                    Type::Closure(id) => module.closures[id.0 as usize]
                        .storage_life
                        .as_ref()
                        .map_or(0, |life| life.fixed_binder_count),
                    _ => 0,
                };
                let pointed_classes = match ty {
                    Type::Address { inner, .. } => match inner.as_ref() {
                        Type::BorrowedStorage { life, .. } => life.binder_count,
                        _ => 0,
                    },
                    _ => 0,
                };
                let count = if matches!(ty, Type::Address { .. }) {
                    1 + pointed_classes
                } else {
                    class_count
                };
                for class in 0..count {
                    let mut actual = None;
                    for row in rows {
                        spend(
                            left,
                            row.position
                                .path
                                .len()
                                .saturating_add(prefix.len())
                                .saturating_add(1),
                            f.span,
                        )?;
                        let Some((capture, tail)) = relative_capture(&row.position, &prefix) else {
                            continue;
                        };
                        if capture != index as u32 {
                            continue;
                        }
                        let selected = match ty {
                            Type::BorrowedStorage { life, .. } => life.slots.iter().any(|slot| {
                                slot.binder == class
                                    && tail.iter().copied().eq(slot
                                        .path
                                        .iter()
                                        .copied()
                                        .map(ContractProjection::Place))
                            }),
                            Type::Address { .. } => tail.is_empty(),
                            Type::Closure(id) => {
                                capture_class(module, *id, tail, 0, left, f.span)? == class
                            }
                            _ => false,
                        };
                        if !selected {
                            continue;
                        }
                        for source in &row.sources {
                            let key = if pointed_classes > 0 && class > 0 {
                                let StorageBase::Parameter(input) = source.base else {
                                    return Err(unsupported(
                                        f.span,
                                        "pointed capture class has no actual input",
                                    ));
                                };
                                if !source.path.is_empty() {
                                    return Err(unsupported(
                                        f.span,
                                        "pointed capture class requires its complete input projection",
                                    ));
                                }
                                header_class(
                                    f,
                                    input,
                                    vec![LifeTypeProjection::Pointee],
                                    class - 1,
                                    left,
                                )?
                            } else {
                                source_class(module, f, source, left)?
                            };
                            if actual.replace(key).is_some_and(|old| old != key) {
                                return Err(unsupported(
                                    f.span,
                                    "captured class has no single proved creator relation",
                                ));
                            }
                        }
                    }
                    let actual = if let Some(actual) = actual {
                        actual
                    } else {
                        header_class(
                            f,
                            captured_parameter(module, f, definition.id, index, left)?,
                            Vec::new(),
                            class,
                            left,
                        )?
                    };
                    spend(left, keys.len().saturating_add(1), f.span)?;
                    let next = keys.len() as u32;
                    let fixed = *keys.entry(actual).or_insert(next);
                    if matches!(ty, Type::Address { .. }) && class == 0 {
                        proposed.bindings.push(StorageLifeBinding {
                            position: StoragePosition {
                                base: StorageBase::Capture(index as u32),
                                path: Vec::new(),
                            },
                            binder: StorageBinder::Fixed(fixed),
                        });
                    } else {
                        proposed.classes.push(SignatureLifeClassBinding {
                            position: SignatureLifeClass {
                                base: StorageBase::Capture(index as u32),
                                path: if pointed_classes > 0 {
                                    vec![LifeTypeProjection::Pointee]
                                } else {
                                    Vec::new()
                                },
                                class: if pointed_classes > 0 {
                                    class - 1
                                } else {
                                    class
                                },
                            },
                            binder: StorageBinder::Fixed(fixed),
                        });
                    }
                }
            }
            proposed.fixed_binder_count = keys.len() as u32;
            let mut result_classes = Vec::new();
            spend(left, keys.len(), f.span)?;
            for (creator, fixed) in keys {
                result_classes.push(SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Result,
                        path: Vec::new(),
                        class: fixed,
                    },
                    binder: creator,
                });
            }
            result_classes.sort_by_key(|row| row.position.class);
            updates.push((
                definition.id,
                definition.function,
                prefix.is_empty().then_some(f.id),
                proposed,
                result_classes,
            ));
        }
    }
    let changed = !updates.is_empty();
    for (id, function, creator, proposal, result_classes) in updates {
        spend(
            left,
            borrowed_fp::metadata_work(&proposal).saturating_mul(2),
            module.functions[function.0 as usize].span,
        )?;
        module.closures[id.0 as usize].storage_life = Some(proposal.clone());
        module.functions[function.0 as usize].storage_life = Some(proposal);
        let Some(creator) = creator else {
            continue;
        };
        let span = module.functions[creator.0 as usize].span;
        let header = module.functions[creator.0 as usize]
            .storage_life
            .as_mut()
            .ok_or_else(|| unsupported(span, "actual creator class header absent"))?;
        spend(
            left,
            header.classes.len().saturating_add(result_classes.len()),
            span,
        )?;
        if header
            .classes
            .iter()
            .any(|row| row.position.base == StorageBase::Result)
        {
            return Err(unsupported(
                span,
                "creator already declares a different result class relation",
            ));
        }
        header.classes.extend(result_classes);
    }
    Ok(changed)
}

pub(crate) fn check_contexts(module: &TypedModule, left: &mut usize) -> R<()> {
    spend(left, module.closures.len(), Span::default())?;
    for d in &module.closures {
        spend(
            left,
            d.captures.len(),
            module.functions[d.function.0 as usize].span,
        )?;
        if d.storage_life.is_none() && d.captures.iter().any(|ty|matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::BorrowedStorage{..}))) {
            return Err(unsupported(module.functions[d.function.0 as usize].span,"qualified capture has no completed actual creator class relation"));
        }
    }
    Ok(())
}
