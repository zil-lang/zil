//! Rebuild external capture leaves by following actual SSA definitions and all
//! reachable writes. No Types proposal or Candidate Hold row supplies roots.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
type Path = Vec<ContractProjection>;
type Sources = BTreeSet<StoragePosition>;
fn pay(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "returned environment actual source proof")
}
fn err(text: &str) -> Vec<Diagnostic> {
    bad(format!("returned capture source: {text}"))
}
fn leaves(
    module: &TypedModule,
    ty: &Type,
    path: &mut Path,
    out: &mut Vec<Path>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<()> {
    pay(b, path.len().saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(err("capture leaf tree exceeds its actual finite bound"));
    }
    match ty {
        Type::Address { .. } | Type::Desc(_) => out.push(path.clone()),
        Type::BorrowedStorage { life, .. } => {
            for slot in &life.slots {
                pay(
                    b,
                    path.len().saturating_add(slot.path.len()).saturating_add(1),
                )?;
                let mut p = path.clone();
                p.extend(slot.path.iter().copied().map(ContractProjection::Place));
                out.push(p)
            }
        }
        Type::Closure(id) => {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("capture definition absent"))?;
            for (index, ty) in d.captures.iter().enumerate() {
                path.push(ContractProjection::Capture(index as u32));
                leaves(module, ty, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (index, ty) in fields.iter().enumerate() {
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                leaves(module, ty, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                leaves(module, ty, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}
struct Context<'a> {
    module: &'a TypedModule,
    f: &'a TypedFunction,
    defs: Vec<Option<&'a InstructionKind>>,
    stores: BTreeMap<LocalId, Vec<ValueId>>,
    returned: Vec<ValueId>,
}
fn context<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    b: &mut Budget,
) -> CompileResult<Context<'a>> {
    pay(
        b,
        f.values
            .len()
            .saturating_add(f.blocks.len().saturating_mul(2)),
    )?;
    if f.foreign.is_some() || f.blocks.is_empty() {
        return Err(err("external capture source has no ordinary actual body"));
    }
    let mut reachable = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry.0 as usize];
    while let Some(index) = pending.pop() {
        pay(b, 1)?;
        let visited = reachable
            .get_mut(index)
            .ok_or_else(|| err("source CFG target is outside the actual block table"))?;
        if *visited {
            continue;
        }
        *visited = true;
        match f.blocks[index].terminator {
            Terminator::Jump(to) => pending.push(to.0 as usize),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(then_block.0 as usize);
                pending.push(else_block.0 as usize)
            }
            _ => {}
        }
    }
    let mut c = Context {
        module,
        f,
        defs: vec![None; f.values.len()],
        stores: BTreeMap::new(),
        returned: Vec::new(),
    };
    for (block, live) in f.blocks.iter().zip(reachable) {
        if !live {
            continue;
        }
        for i in &block.instructions {
            pay(b, 1)?;
            if let Some(v) = i.result {
                *c.defs
                    .get_mut(v.0 as usize)
                    .ok_or_else(|| err("actual source definition is outside the value table"))? =
                    Some(&i.kind)
            }
            if let InstructionKind::Store { local, value } = i.kind {
                pay(b, c.stores.len().saturating_add(1))?;
                c.stores.entry(local).or_default().push(value)
            }
        }
        if let Terminator::Return(Some(v)) = block.terminator {
            c.returned.push(v)
        }
    }
    Ok(c)
}
fn extend(to: &mut Sources, from: Sources, b: &mut Budget) -> CompileResult<()> {
    pay(b, to.len().saturating_add(from.len()))?;
    for s in &from {
        pay(b, s.path.len())?;
    }
    to.extend(from);
    Ok(())
}
fn append(
    mut sources: Sources,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(
        b,
        sources.len().saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut out = Sources::new();
    for mut source in sources.pop_first().into_iter().chain(sources) {
        source.path.extend_from_slice(path);
        out.insert(source);
    }
    Ok(out)
}
fn resolve(
    c: &Context<'_>,
    v: ValueId,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(b, path.len().saturating_add(active.len()).saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(err("actual source traversal exceeds depth bound"));
    }
    let key = (c.f.id, v, path.to_vec());
    if !active.insert(key.clone()) {
        return Err(err(
            "cyclic address source has no established external base",
        ));
    }
    let result = match c
        .defs
        .get(v.0 as usize)
        .copied()
        .flatten()
        .ok_or_else(|| err("source value has no actual definition"))?
    {
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            local_sources(c, *local, path, active, depth + 1, b)
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::DescribeAddress { address } => {
            resolve(c, *address, path, active, depth + 1, b)
        }
        InstructionKind::AddressField { address, index } => {
            let mut p = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
            p.extend_from_slice(path);
            append(resolve(c, *address, &[], active, depth + 1, b)?, &p, b)
        }
        InstructionKind::BorrowPlaceAddress {
            address,
            path: projection,
            ..
        } => {
            let mut p = projection
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            p.extend_from_slice(path);
            append(resolve(c, *address, &[], active, depth + 1, b)?, &p, b)
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path: projection,
            ..
        } => {
            let mut p = projection
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            p.extend_from_slice(path);
            append(resolve(c, *storage, &[], active, depth + 1, b)?, &p, b)
        }
        InstructionKind::MakeClosure { captures, .. } => {
            let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
                return Err(err("closure leaf lacks actual capture selector"));
            };
            resolve(
                c,
                *captures
                    .get(*index as usize)
                    .ok_or_else(|| err("capture operand absent"))?,
                tail,
                active,
                depth + 1,
                b,
            )
        }
        InstructionKind::MakePackage { fields }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeOwnedTuple { fields } => {
            let Some((ContractProjection::Place(PlaceProjection::Field(index)), tail)) =
                path.split_first()
            else {
                return Err(err("Product leaf lacks actual field selector"));
            };
            resolve(
                c,
                *fields
                    .get(*index as usize)
                    .ok_or_else(|| err("Product operand absent"))?,
                tail,
                active,
                depth + 1,
                b,
            )
        }
        InstructionKind::ExtractField { aggregate, index } => {
            let mut p = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
            p.extend_from_slice(path);
            resolve(c, *aggregate, &p, active, depth + 1, b)
        }
        InstructionKind::ReadPackageField { local, index }
        | InstructionKind::TakePackageField { local, index } => {
            let mut p = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
            p.extend_from_slice(path);
            local_sources(c, *local, &p, active, depth + 1, b)
        }
        InstructionKind::TakeTupleField {
            local,
            path: projection,
        }
        | InstructionKind::TakeVariantField {
            local,
            path: projection,
        } => {
            let mut p = projection
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            p.extend_from_slice(path);
            local_sources(c, *local, &p, active, depth + 1, b)
        }
        InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index } => {
            Ok(Sources::from([StoragePosition {
                base: StorageBase::Capture(*index),
                path: path.to_vec(),
            }]))
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let target = c
                .module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("callee absent"))?;
            let callee = context(c.module, target, b)?;
            let mut out = Sources::new();
            for returned in &callee.returned {
                for source in resolve(&callee, *returned, path, active, depth + 1, b)? {
                    let StorageBase::Parameter(index) = source.base else {
                        return Err(err("callee result is not rooted in an actual input"));
                    };
                    let actual = arguments
                        .get(index as usize)
                        .ok_or_else(|| err("callee actual input absent"))?;
                    extend(
                        &mut out,
                        resolve(c, *actual, &source.path, active, depth + 1, b)?,
                        b,
                    )?;
                }
            }
            Ok(out)
        }
        _ => Err(err(
            "returned environment retains local storage or an unsupported actual origin",
        )),
    };
    active.remove(&key);
    result
}
fn local_sources(
    c: &Context<'_>,
    local: LocalId,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(
        b,
        c.f.parameters
            .len()
            .saturating_add(c.stores.len())
            .saturating_add(path.len()),
    )?;
    let mut out = Sources::new();
    if let Some(index) = c.f.parameters.iter().position(|l| *l == local) {
        let ty = &c.f.locals[local.0 as usize].ty;
        let mut leaves_at = Vec::new();
        leaves(c.module, ty, &mut Vec::new(), &mut leaves_at, 0, b)?;
        if matches!(ty, Type::Address { .. } | Type::Desc(_)) || leaves_at.iter().any(|p| p == path)
        {
            out.insert(StoragePosition {
                base: StorageBase::Parameter(index as u32),
                path: path.to_vec(),
            });
        }
    }
    if let Some(values) = c.stores.get(&local) {
        for value in values {
            extend(&mut out, resolve(c, *value, path, active, depth + 1, b)?, b)?;
        }
    }
    if out.is_empty() {
        return Err(err(
            "local leaf is uninitialized or is not an external input",
        ));
    }
    Ok(out)
}
fn endpoint<'a>(
    module: &'a TypedModule,
    mut ty: &'a Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<(&'a Type, Option<bool>)> {
    let mut indirect = None;
    for step in path {
        pay(b, 1)?;
        if let Type::Address { mutable, inner } = ty {
            indirect = Some(*mutable);
            ty = inner;
        }
        if let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        ty = match (step, ty) {
            (ContractProjection::Capture(index), Type::Closure(id)) => module
                .closures
                .get(id.0 as usize)
                .and_then(|d| d.captures.get(*index as usize)),
            (
                ContractProjection::Place(PlaceProjection::Field(index)),
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            (ContractProjection::Place(PlaceProjection::Field(index)), Type::Tuple(fields)) => {
                fields.get(*index as usize)
            }
            (
                ContractProjection::Place(PlaceProjection::Variant(p)),
                Type::Enum { variants, .. },
            ) => variants
                .get(p.variant as usize)
                .and_then(|(_, fields)| fields.get(p.index as usize)),
            (
                ContractProjection::Place(PlaceProjection::ErrorArm(is_error)),
                Type::ErrorUnion { error, result },
            ) => Some(if *is_error {
                error.as_ref()
            } else {
                result.as_ref()
            }),
            _ => None,
        }
        .ok_or_else(|| err("source upper crosses an absent or differently typed actual field"))?;
    }
    Ok((ty, indirect))
}
fn compatible(expected: &Type, source: &Type, indirect: Option<bool>) -> bool {
    match (expected, source) {
        (
            Type::Address {
                mutable: need,
                inner: wanted,
            },
            Type::Address {
                mutable: have,
                inner: actual,
            },
        ) => (!*need || *have) && wanted == actual,
        (
            Type::Address {
                mutable: need,
                inner: wanted,
            },
            actual,
        ) => indirect.is_some_and(|have| (!*need || have) && wanted.as_ref() == actual),
        (Type::Desc(wanted), Type::Desc(actual)) => wanted == actual,
        _ => false,
    }
}

pub(super) fn check(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    let contract = f
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| err("factory contract absent"))?;
    let proposed = contract
        .capture_sources
        .as_ref()
        .ok_or_else(|| err("external source relation unknown"))?;
    let mut paths = Vec::new();
    leaves(module, &f.return_type, &mut Vec::new(), &mut paths, 0, b)?;
    if paths.len() != proposed.len() {
        return Err(err(
            "source relation does not cover every actual external capture leaf",
        ));
    }
    let c = context(module, f, b)?;
    for path in paths {
        let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
            return Err(err("result leaf has no actual capture"));
        };
        let position = StoragePosition {
            base: StorageBase::Capture(*index),
            path: tail.to_vec(),
        };
        let claims = proposed
            .iter()
            .filter(|row| row.position == position)
            .collect::<Vec<_>>();
        pay(
            b,
            proposed
                .len()
                .saturating_mul(position.path.len().saturating_add(1)),
        )?;
        if claims.len() != 1 || claims[0].sources.is_empty() {
            return Err(err(
                "external capture leaf lacks one nonempty source upper bound",
            ));
        }
        let claim = claims[0];
        let (expected, _) = endpoint(module, &f.return_type, &path, b)?;
        if claim.sources.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(err(
                "source upper must contain distinct canonical actual inputs",
            ));
        }
        for source in &claim.sources {
            if !matches!(source.base, StorageBase::Parameter(_)) {
                return Err(err("external source upper is not an actual input"));
            }
            let StorageBase::Parameter(index) = source.base else {
                unreachable!()
            };
            let local = f
                .parameters
                .get(index as usize)
                .ok_or_else(|| err("source upper input is outside the actual parameter table"))?;
            let (actual, indirect) =
                endpoint(module, &f.locals[local.0 as usize].ty, &source.path, b)?;
            if !compatible(expected, actual, indirect) {
                return Err(err(
                    "source upper cannot supply the actual captured leaf type or permission",
                ));
            }
        }
        for returned in &c.returned {
            let actual = resolve(&c, *returned, &path, &mut BTreeSet::new(), 0, b)?;
            if actual.is_empty() || actual.iter().any(|s| !claim.sources.contains(s)) {
                return Err(err(
                    "actual capture source exceeds its declared input upper bound",
                ));
            }
        }
    }
    Ok(())
}
