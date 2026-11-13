//! Rebuild external capture leaves by following actual SSA definitions and all
//! reachable writes. No Types proposal or Candidate Hold row supplies roots.
use super::*;
mod dynamic_static;
use std::collections::{BTreeMap, BTreeSet};
type Path = Vec<ContractProjection>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ActualSource {
    Input(StoragePosition),
    Static,
}
type Sources = BTreeSet<ActualSource>;
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
        Type::OwnedDyn { interface } | Type::Dyn { interface } => {
            pay(b, module.dyn_interfaces.len())?;
            let d = module
                .dyn_interfaces
                .iter()
                .find(|d| d.id == *interface)
                .ok_or_else(|| err("actual capture dyn interface absent"))?;
            pay(
                b,
                d.retained_life
                    .ports
                    .len()
                    .saturating_mul(path.len().saturating_add(2)),
            )?;
            for port in 0..d.retained_life.ports.len() {
                let mut leaf = path.clone();
                leaf.push(ContractProjection::RetainedPort(port as u32));
                out.push(leaf);
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
    capture_writes: Vec<(u32, ValueId)>,
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
        capture_writes: Vec::new(),
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
            if let InstructionKind::WriteCapture { index, value } = i.kind {
                pay(b, 1)?;
                c.capture_writes.push((index, value));
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
        pay(
            b,
            match s {
                ActualSource::Input(position) => position.path.len(),
                ActualSource::Static => 1,
            },
        )?;
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
        match &mut source {
            ActualSource::Input(position) => position.path.extend_from_slice(path),
            ActualSource::Static if path.is_empty() => {}
            ActualSource::Static => {
                return Err(err(
                    "static byte storage cannot supply an aggregate field path",
                ));
            }
        }
        out.insert(source);
    }
    Ok(out)
}
fn stored_read_sources(
    c: &Context<'_>,
    storage: ValueId,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(b, path.len().saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(err("stored source traversal exceeds depth bound"));
    }
    match c.defs.get(storage.0 as usize).copied().flatten() {
        Some(
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local },
        ) => local_sources(c, *local, path, active, depth + 1, b),
        Some(InstructionKind::BorrowPlaceLocal {
            local,
            path: prefix,
            ..
        }) => {
            pay(b, prefix.len().saturating_add(path.len()))?;
            let mut full = prefix
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            full.extend_from_slice(path);
            local_sources(c, *local, &full, active, depth + 1, b)
        }
        Some(InstructionKind::ReborrowAddress { address, .. }) => {
            stored_read_sources(c, *address, path, active, depth + 1, b)
        }
        Some(InstructionKind::AddressField { address, index }) => {
            pay(b, path.len().saturating_add(1))?;
            let mut full = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
            full.extend_from_slice(path);
            stored_read_sources(c, *address, &full, active, depth + 1, b)
        }
        Some(InstructionKind::Load(local) | InstructionKind::TakeLocal { local }) => {
            let mut out = Sources::new();
            if let Some(index) = c.f.parameters.iter().position(|p| p == local)
                && matches!(c.f.locals[local.0 as usize].ty, Type::Address { .. })
            {
                out.insert(ActualSource::Input(StoragePosition {
                    base: StorageBase::Parameter(index as u32),
                    path: path.to_vec(),
                }));
            }
            if let Some(writes) = c.stores.get(local) {
                for value in writes {
                    extend(
                        &mut out,
                        stored_read_sources(c, *value, path, active, depth + 1, b)?,
                        b,
                    )?;
                }
            }
            if out.is_empty() {
                Err(err("stored read has no actual initialized carrier"))
            } else {
                Ok(out)
            }
        }
        _ => append(resolve(c, storage, &[], active, depth + 1, b)?, path, b),
    }
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
        // Revisit the finite CFG equation at bottom, not as an established
        // origin. Every reachable Store is still joined by its caller and
        // each final returned leaf must have a nonempty external base.
        return Ok(Sources::new());
    }
    let result = match c
        .defs
        .get(v.0 as usize)
        .copied()
        .flatten()
        .ok_or_else(|| err("source value has no actual definition"))?
    {
        InstructionKind::StaticUtf8 { .. } => {
            if !path.is_empty()
                || !matches!(c.f.values.get(v.0 as usize),Some(Type::Address{mutable:false,inner})if matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))
            {
                return Err(err(
                    "StaticUtf8 source must be its complete immutable byte slice",
                ));
            }
            Ok(Sources::from([ActualSource::Static]))
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            local_sources(c, *local, path, active, depth + 1, b)
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::DescribeAddress { address } => {
            resolve(c, *address, path, active, depth + 1, b)
        }
        InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. }
            if path.is_empty() =>
        {
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
            stored_read_sources(c, *storage, &p, active, depth + 1, b)
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
            if let Some(definition) =
                c.f.environment
                    .and_then(|id| c.module.closures.get(id.0 as usize))
                && definition.storage_life.as_ref().is_some_and(|life| {
                    life.static_bindings.iter().any(|position| {
                        position.base == StorageBase::Capture(*index) && position.path == path
                    })
                })
            {
                capture_sources(c.module, definition, *index, path, active, depth + 1, b)
            } else {
                Ok(Sources::from([ActualSource::Input(StoragePosition {
                    base: StorageBase::Capture(*index),
                    path: path.to_vec(),
                })]))
            }
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
                    let ActualSource::Input(source) = source else {
                        out.insert(ActualSource::Static);
                        continue;
                    };
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
        kind => dynamic_static::resolve(c, kind, path, active, depth, b).unwrap_or_else(|| {
            Err(err(
                "returned environment retains local storage or an unsupported actual origin",
            ))
        }),
    };
    active.remove(&key);
    result
}

fn capture_sources(
    module: &TypedModule,
    definition: &ClosureDefinition,
    index: u32,
    path: &[ContractProjection],
    active: &mut BTreeSet<(FunctionId, ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Sources> {
    pay(b, module.functions.len().saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(err("constant capture source graph exceeds finite depth"));
    }
    let mut out = Sources::new();
    let mut constructed = false;
    for function in &module.functions {
        if function.foreign.is_some() || function.blocks.is_empty() {
            continue;
        }
        let c = context(module, function, b)?;
        for instruction in c.defs.iter().flatten() {
            pay(b, 1)?;
            if let InstructionKind::MakeClosure { closure, captures } = instruction
                && *closure == definition.id
            {
                constructed = true;
                let operand = *captures
                    .get(index as usize)
                    .ok_or_else(|| err("constant capture constructor omits an actual operand"))?;
                let actual = function
                    .values
                    .get(operand.0 as usize)
                    .ok_or_else(|| err("constant capture constructor operand is absent"))?;
                let expected = definition
                    .captures
                    .get(index as usize)
                    .ok_or_else(|| err("constant capture selector is absent"))?;
                b.type_work(actual, false, "constant constructor complete actual type")?;
                b.type_work(
                    expected,
                    false,
                    "constant constructor complete capture type",
                )?;
                if actual != expected {
                    return Err(err(
                        "constant capture constructor changes the complete actual leaf",
                    ));
                }
                extend(
                    &mut out,
                    resolve(&c, operand, path, active, depth + 1, b)?,
                    b,
                )?;
            }
        }
        if function.environment == Some(definition.id) {
            for (slot, value) in &c.capture_writes {
                if *slot == index {
                    extend(
                        &mut out,
                        resolve(&c, *value, path, active, depth + 1, b)?,
                        b,
                    )?;
                }
            }
        }
    }
    if !constructed {
        return Err(err("constant capture has no actual constructor grounding"));
    }
    Ok(out)
}

pub(super) fn check_static_occurrences(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    pay(b, module.functions.len())?;
    for function in &module.functions {
        let Some(life) = &function.storage_life else {
            continue;
        };
        for position in &life.static_bindings {
            pay(b, position.path.len().saturating_add(1))?;
            let c = context(module, function, b)?;
            let mut active = BTreeSet::new();
            let actual = match position.base {
                StorageBase::Capture(index) => {
                    let definition = function
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                        .ok_or_else(|| err("constant capture has no actual environment"))?;
                    capture_sources(module, definition, index, &position.path, &mut active, 0, b)?
                }
                StorageBase::Result => {
                    let mut sources = Sources::new();
                    for value in &c.returned {
                        extend(
                            &mut sources,
                            resolve(&c, *value, &position.path, &mut active, 0, b)?,
                            b,
                        )?;
                    }
                    sources
                }
                StorageBase::Parameter(_) => {
                    return Err(err(
                        "constant input requires a complete actual call-site source proof",
                    ));
                }
                StorageBase::Receiver => {
                    return Err(err("invocation receiver cannot be a constant static input"));
                }
            };
            if actual.is_empty() || actual.iter().any(|source| *source != ActualSource::Static) {
                return Err(err(
                    "constant occurrence retains an actual local, parameter, or unknown source",
                ));
            }
        }
    }
    Ok(())
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
            out.insert(ActualSource::Input(StoragePosition {
                base: StorageBase::Parameter(index as u32),
                path: path.to_vec(),
            }));
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
                ContractProjection::RetainedPort(port),
                Type::OwnedDyn { interface } | Type::Dyn { interface },
            ) => {
                pay(b, module.dyn_interfaces.len())?;
                module
                    .dyn_interfaces
                    .iter()
                    .find(|d| d.id == *interface)
                    .and_then(|d| d.retained_life.ports.get(*port as usize))
                    .map(|p| &p.leaf)
            }
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

// Resolve a logical retained leaf to the complete class occurrence of its
// actual carrier. Child class ordinals never become parent class ordinals
// without the enclosing definition's explicit occurrence mapping.
fn port_class(
    module: &TypedModule,
    ty: &Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<(Vec<LifeTypeProjection>, u32)> {
    pay(b, path.len().saturating_add(1))?;
    if path.len() > b.partial_depth {
        return Err(err("retained capture class path exceeds depth"));
    }
    match (ty, path.split_first()) {
        (
            Type::OwnedDyn { interface } | Type::Dyn { interface },
            Some((ContractProjection::RetainedPort(port), [])),
        ) => {
            pay(b, module.dyn_interfaces.len())?;
            let binder = module
                .dyn_interfaces
                .iter()
                .find(|d| d.id == *interface)
                .and_then(|d| d.retained_life.ports.get(*port as usize))
                .ok_or_else(|| err("retained capture class port absent"))?
                .binder;
            Ok((vec![], binder))
        }
        (Type::Closure(id), Some((ContractProjection::Capture(index), tail))) => {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("retained class environment absent"))?;
            let child = d
                .captures
                .get(*index as usize)
                .ok_or_else(|| err("retained class capture absent"))?;
            let (at, class) = port_class(module, child, tail, b)?;
            let life = d
                .storage_life
                .as_ref()
                .ok_or_else(|| err("retained class environment is Unknown"))?;
            pay(
                b,
                life.classes
                    .len()
                    .saturating_mul(at.len().saturating_add(1)),
            )?;
            let mut selected = life.classes.iter().filter(|row| {
                row.position.base == StorageBase::Capture(*index)
                    && row.position.path == at
                    && row.position.class == class
            });
            let row = selected
                .next()
                .ok_or_else(|| err("retained class child occurrence absent"))?;
            if selected.next().is_some() {
                return Err(err("retained class child occurrence repeated"));
            }
            let StorageBinder::Fixed(class) = row.binder else {
                return Err(err("retained capture class is not Fixed"));
            };
            Ok((vec![], class))
        }
        (Type::Address { inner, .. }, _) => {
            let (mut at, class) = port_class(module, inner, path, b)?;
            pay(b, at.len().saturating_add(1))?;
            at.insert(0, LifeTypeProjection::Pointee);
            Ok((at, class))
        }
        (
            Type::Tuple(fields),
            Some((ContractProjection::Place(PlaceProjection::Field(index)), tail)),
        ) => {
            let child = fields
                .get(*index as usize)
                .ok_or_else(|| err("retained class inline child absent"))?;
            let (mut at, class) = port_class(module, child, tail, b)?;
            pay(b, at.len().saturating_add(1))?;
            at.insert(0, LifeTypeProjection::Place(PlaceProjection::Field(*index)));
            Ok((at, class))
        }
        (
            Type::Record { fields, .. } | Type::Package { fields, .. },
            Some((ContractProjection::Place(PlaceProjection::Field(index)), tail)),
        ) => {
            let child = &fields
                .get(*index as usize)
                .ok_or_else(|| err("retained class named child absent"))?
                .1;
            let (mut at, class) = port_class(module, child, tail, b)?;
            pay(b, at.len().saturating_add(1))?;
            at.insert(0, LifeTypeProjection::Place(PlaceProjection::Field(*index)));
            Ok((at, class))
        }
        _ => Err(err(
            "retained class needs its complete actual carrier relation",
        )),
    }
}
fn header_port_class(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    ty: &Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let (at, class) = port_class(module, ty, path, b)?;
    let life = f
        .storage_life
        .as_ref()
        .ok_or_else(|| err("retained environment omits its complete function class relation"))?;
    pay(
        b,
        life.classes
            .len()
            .saturating_mul(at.len().saturating_add(1)),
    )?;
    let mut selected = life.classes.iter().filter(|row| {
        row.position.base == base && row.position.path == at && row.position.class == class
    });
    let row = selected
        .next()
        .ok_or_else(|| err("retained environment header class occurrence absent"))?;
    if selected.next().is_some() {
        return Err(err("retained environment header class occurrence repeated"));
    }
    Ok(row.binder)
}

fn mutable_calls(
    module: &TypedModule,
    root: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.id];
    while let Some(id) = pending.pop() {
        pay(b, seen.len().saturating_add(1))?;
        if !seen.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| err("source call target absent"))?;
        pay(b, f.blocks.len())?;
        for block in &f.blocks {
            pay(b, block.instructions.len())?;
            for instruction in &block.instructions {
                match instruction.kind {
                    InstructionKind::StoreStoredBorrow { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureBorrowed {
                        kind: CallableKind::FnMut,
                        ..
                    } => return Ok(true),
                    InstructionKind::Call { function, .. } => pending.push(function),
                    _ => {}
                }
            }
        }
    }
    Ok(false)
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
    let memory = mutable_calls(module, f, b)?;
    if memory && !paths.is_empty() {
        pay(b, b.pending_returned_source_proofs.len().saturating_add(1))?;
        b.pending_returned_source_proofs.insert(f.id);
        b.stored_borrows = true;
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
        for pair in claim.sources.windows(2) {
            let canonical = match (&pair[0], &pair[1]) {
                (StorageSource::Exact(left), StorageSource::Exact(right)) => left < right,
                (StorageSource::Exact(_), StorageSource::Static { .. }) => true,
                (StorageSource::Static { .. }, _) => false,
                _ => return Err(err("source upper uses an unsupported source expression")),
            };
            if !canonical {
                return Err(err(
                    "source upper must contain distinct canonical actual inputs",
                ));
            }
        }
        for atom in &claim.sources {
            if let StorageSource::Static { leaf, mode } = atom {
                b.type_work(leaf, false, "static capture upper complete leaf")?;
                if *mode != StoredBorrowMode::SharedAddress
                    || leaf != expected
                    || !matches!(leaf,Type::Address{mutable:false,inner}if **inner==Type::U8 || matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))
                {
                    return Err(err(
                        "static capture upper changes the complete immutable byte leaf or mode",
                    ));
                }
                continue;
            }
            let source = crate::exact_storage_source(atom)?;
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
            if matches!(path.last(), Some(ContractProjection::RetainedPort(_)))
                && header_port_class(module, f, StorageBase::Result, &f.return_type, &path, b)?
                    != header_port_class(
                        module,
                        f,
                        source.base,
                        &f.locals[local.0 as usize].ty,
                        &source.path,
                        b,
                    )?
            {
                return Err(err(
                    "retained capture source changes its actual complete lifetime class",
                ));
            }
        }
        if memory {
            continue;
        }
        for returned in &c.returned {
            let actual = resolve(&c, *returned, &path, &mut BTreeSet::new(), 0, b)?;
            let mut covered = !actual.is_empty();
            for source in &actual {
                covered &= match source {
                    ActualSource::Input(position) => claim
                        .sources
                        .iter()
                        .any(|atom| matches!(atom,StorageSource::Exact(upper)if upper==position)),
                    ActualSource::Static => {
                        pay(b, claim.sources.len())?;
                        claim
                            .sources
                            .iter()
                            .any(|atom| matches!(atom, StorageSource::Static { .. }))
                    }
                };
            }
            if !covered {
                return Err(err(
                    "actual capture source exceeds its declared input upper bound",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod static_grounding_tests {
    use super::*;
    #[test]
    fn static_uppers_do_not_ground_a_return_cycle_without_an_actual_literal() {
        let limits = Limits::default();
        let mut literal = crate::static_utf8_tests::module();
        let leaf = literal.functions[0].return_type.clone();
        literal.functions[0].storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
            static_bindings: vec![StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            }],
        });
        literal.functions[0].retained_return_sources = Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            sources: vec![StorageSource::Static {
                leaf,
                mode: StoredBorrowMode::SharedAddress,
            }],
        }]);
        let mut callee = literal.functions[0].clone();
        callee.id = FunctionId(1);
        callee.name = "actual_literal".into();
        literal.functions.push(callee);
        literal.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
            function: FunctionId(1),
            arguments: vec![],
        };
        let mut cycle = literal.clone();
        cycle.functions[1].blocks[0].instructions[0].kind = InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![],
        };
        for (module, accepted) in [(literal, true), (cycle, false)] {
            let check = |module: &TypedModule| {
                check_static_occurrences(module, &mut Budget::new(&limits)).is_ok()
            };
            assert_eq!(
                check(&module),
                accepted,
                "actual source proof must not use the public Static upper"
            );
            let bytes = zil_ir::wire::encode_module_candidate(
                &module,
                &CandidatePlan::empty_for(&module),
                &limits,
            )
            .unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(
                check(&decoded),
                accepted,
                "Wire must preserve the same least-literal source obligation"
            );
        }
    }
}

/// A pending output class must have a real literal derivation through the
/// actual returned SSA. Canonical public Static categories are not evidence.
pub(super) fn check_dyn_static_return(
    module: &TypedModule,
    function: &TypedFunction,
    value: ValueId,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    let c = context(module, function, b)?;
    let actual = resolve(&c, value, path, &mut BTreeSet::new(), 0, b)?;
    if actual.is_empty() || actual.iter().any(|source| *source != ActualSource::Static) {
        return Err(err(
            "pending dynamic Static output has no actual literal grounding",
        ));
    }
    Ok(())
}
