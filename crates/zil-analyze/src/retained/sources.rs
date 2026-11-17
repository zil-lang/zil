//! Rebuild external capture leaves by following actual SSA definitions and all
//! reachable writes. No Types proposal or Candidate Hold row supplies roots.
use super::*;
mod dynamic_static;
mod mutations;
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
        Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => {
            crate::tasks::require_closed_context(payload)?;
            let Type::Task { retained_life, .. } = payload.as_ref() else {
                unreachable!()
            };
            pay(
                b,
                retained_life
                    .ports
                    .len()
                    .saturating_mul(path.len().saturating_add(2)),
            )?;
            for (port, _) in retained_life.ports.iter().enumerate() {
                let mut leaf = path.clone();
                leaf.push(ContractProjection::RetainedPort(port as u32));
                out.push(leaf);
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
    mutations: mutations::Writes,
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
        mutations: mutations::Writes::default(),
    };
    for (block, live) in f.blocks.iter().zip(&reachable) {
        if !*live {
            continue;
        }
        for (ordinal, i) in block.instructions.iter().enumerate() {
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
                let definition = f.environment.and_then(|id| module.closures.get(id.0 as usize))
                    .ok_or_else(|| err("capture replacement has no actual environment"))?;
                let expected = definition.captures.get(index as usize)
                    .ok_or_else(|| err("capture replacement selects an absent complete field"))?;
                let actual = f.values.get(value.0 as usize)
                    .ok_or_else(|| err("capture replacement has no actual value"))?;
                crate::dyn_metadata::ty(expected, b)?;
                crate::dyn_metadata::ty(actual, b)?;
                if expected != actual { return Err(err("capture replacement changes its complete field Type")); }
                crate::semantic::source_value_available(f, (block.id, ordinal), value, b)?;
                c.capture_writes.push((index, value));
            }
        }
        if let Terminator::Return(Some(v)) = block.terminator {
            c.returned.push(v)
        }
    }
    c.mutations = mutations::collect(&c, &reachable, b)?;
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
            if out.is_empty() && !c.stores.get(local).is_some_and(|values| !values.is_empty()) {
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
    c.mutations.check_value(v, path, b)?;
    let key = (c.f.id, v, path.to_vec());
    if !active.insert(key.clone()) {
        // Revisit the finite CFG equation at bottom, not as an established
        // origin. Every reachable Store is still joined by its caller and
        // each final returned leaf must have a nonempty external base.
        return Ok(Sources::new());
    }
    let producer = c
        .defs
        .get(v.0 as usize)
        .copied()
        .flatten()
        .ok_or_else(|| err("source value has no actual definition"))?;
    let result = match producer {
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
        InstructionKind::UpdateField { aggregate, index, value } => {
            let at = crate::shared_definition(c.f, v, b)?;
            crate::semantic::source_value_available(c.f, (at.block, at.instruction), *aggregate, b)?;
            crate::semantic::source_value_available(c.f, (at.block, at.instruction), *value, b)?;
            let parent = c.f.values.get(aggregate.0 as usize)
                .ok_or_else(|| err("updated aggregate source is absent"))?;
            let result = c.f.values.get(v.0 as usize)
                .ok_or_else(|| err("updated aggregate result is absent"))?;
            let replacement = c.f.values.get(value.0 as usize)
                .ok_or_else(|| err("updated aggregate replacement is absent"))?;
            crate::dyn_metadata::ty(parent, b)?;
            crate::dyn_metadata::ty(result, b)?;
            crate::dyn_metadata::ty(replacement, b)?;
            let storage = if let Type::BorrowedStorage { storage, .. } = parent { storage.as_ref() } else { parent };
            let field = match storage {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } | Type::Package { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }.ok_or_else(|| err("updated aggregate source has no exact physical field"))?;
            if parent != result || field != replacement {
                return Err(err("updated aggregate changes its complete parent, result or field Type"));
            }
            let Some((ContractProjection::Place(PlaceProjection::Field(selected)), tail)) = path.split_first() else {
                return Err(err("updated aggregate Static leaf lacks its complete field selector"));
            };
            if selected == index { resolve(c, *value, tail, active, depth + 1, b) }
            else { resolve(c, *aggregate, path, active, depth + 1, b) }
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
        InstructionKind::Call { function, arguments }
        | InstructionKind::SuspendCall { function, arguments }
            if crate::stored_flow::synchronous_abi::ordinary_call(producer, b)? => {
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
            c.mutations.check(mutations::Root::Capture(index), path, b)?;
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
    c.mutations.check(mutations::Root::Local(local), path, b)?;
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
    if out.is_empty()
        && !c
            .stores
            .get(&local)
            .is_some_and(|values| !values.is_empty())
    {
        return Err(err(
            "local leaf is uninitialized or is not an external input",
        ));
    }
    // A real Store whose equation currently revisits an active call remains
    // bottom. Only the final occurrence check can establish a literal root.
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
            (ContractProjection::RetainedPort(port), Type::Managed(payload)) => {
                match payload.as_ref() {
                    Type::Task { retained_life, .. } => retained_life
                        .ports
                        .get(*port as usize)
                        .map(|port| &port.leaf),
                    _ => None,
                }
            }

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
        (Type::Managed(payload), Some((ContractProjection::RetainedPort(port), [])))
            if matches!(payload.as_ref(), Type::Task { .. }) => {
            // RetainedPort is a physical occurrence. Its ordinal need not be
            // its logical class, and the owner adds a Pointee boundary. Keep
            // the exact full Task directory, including unrelated/phantom N.
            crate::tasks::require_closed_context(payload)?;
            let Type::Task { retained_life, .. } = payload.as_ref() else { unreachable!() };
            crate::dyn_metadata::retained_shape(retained_life, b)?;
            let row = retained_life.ports.get(*port as usize)
                .ok_or_else(|| err("retained Task capture selects an absent physical port"))?;
            if row.binder >= retained_life.binder_count {
                return Err(err("retained Task port leaves its full intrinsic class domain"));
            }
            pay(b, 2)?;
            Ok((vec![LifeTypeProjection::Pointee], row.binder))
        }
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
        _ => {
            crate::dyn_metadata::ty(ty, b)?;
            pay(b, path.len().saturating_add(192))?;
            let mut errors = err("retained class needs its complete actual carrier relation");
            errors[0].notes.push(format!("actual retained carrier {ty:?}; physical leaf path {path:?}"));
            Err(errors)
        }
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
    let (at, class) = match port_class(module, ty, path, b) {
        Ok(selected) => selected,
        Err(mut errors) => {
            if errors.iter().any(|error| error.category == zil_base::ResultCategory::ResourceLimit) { return Err(errors); }
            pay(b, 192)?;
            for error in &mut errors {
                error.notes.push(format!("returned source header function {} base {base:?}", f.id.0));
            }
            return Err(errors);
        }
    };
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
                    InstructionKind::Call { function, .. }
                    | InstructionKind::SuspendCall { function, .. }
                        if crate::stored_flow::synchronous_abi::ordinary_call(&instruction.kind, b)? => pending.push(function),
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
                    || !matches!(leaf, Type::Address { mutable: false, .. })
                {
                    return Err(err(
                        "static capture upper changes the complete immutable leaf or mode",
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
            let actual = match resolve(&c, *returned, &path, &mut BTreeSet::new(), 0, b) {
                Ok(sources) => sources,
                Err(mut errors) => {
                    if errors.iter().any(|error| error.category == zil_base::ResultCategory::ResourceLimit) { return Err(errors); }
                    pay(b, path.len().saturating_add(192))?;
                    for error in &mut errors {
                        error.notes.push(format!("actual returned capture source function {} returned SSA {} full leaf path {path:?} hidden closure {}",
                            f.id.0, returned.0, contract.closure.0));
                    }
                    return Err(errors);
                }
            };
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

#[cfg(test)]
mod static_grounding_tests {
    use super::*;
    #[test]
    fn static_tuple_sources_reject_field_replacement_and_mutable_helper_aliases() {
        // A source-equation Raw/Wire control, not a complete module or Source
        // acceptance receipt. Every variant keeps the same old literal/header.
        let limits = Limits::default();
        for mode in 0..9 {
            let mut module = crate::static_utf8_tests::module();
            let leaf = module.functions[0].return_type.clone();
            let tuple = Type::Tuple(vec![leaf.clone(), Type::I32]);
            let scalar_write = mode == 1 || mode == 7;
            let target = if scalar_write { Type::I32 } else { leaf.clone() };
            let pointer = Type::Address { mutable: true, inner: Box::new(target.clone()) };
            let whole_pointer = Type::Address { mutable: true, inner: Box::new(tuple.clone()) };
            let span = module.functions[0].span;
            let instruction = |result: Option<u32>, kind| Instruction {
                result: result.map(ValueId), kind, span,
            };
            let function = &mut module.functions[0];
            function.storage_life = Some(StorageLifeContract { fixed_binder_count: 0,
                invocation_binder_count: 0, bindings: vec![], classes: vec![],
                static_bindings: vec![StoragePosition { base: StorageBase::Result, path: vec![] }] });
            function.locals = vec![
                LocalDecl { id: LocalId(0), name: "replacement_source".into(), ty: leaf.clone(), mutable: false, span },
                LocalDecl { id: LocalId(1), name: "tuple_slot".into(), ty: tuple.clone(), mutable: true, span },
                LocalDecl { id: LocalId(2), name: "pointer_temporary".into(), ty: pointer.clone(), mutable: false, span },
            ];
            function.parameters = vec![LocalId(0)];
            function.values = vec![leaf.clone(), Type::I32, tuple.clone(), pointer.clone(), target.clone(),
                tuple.clone(), leaf.clone(), pointer.clone(), pointer.clone(), whole_pointer, tuple.clone()];
            let mut body = function.blocks[0].instructions.clone();
            body.extend([
                instruction(Some(1), InstructionKind::Constant(Constant::I32(7))),
                instruction(Some(2), InstructionKind::MakeAggregate { fields: vec![ValueId(0), ValueId(1)] }),
                instruction(None, InstructionKind::StorageLive(LocalId(1))),
                instruction(None, InstructionKind::Store { local: LocalId(1), value: ValueId(2) }),
                instruction(Some(3), InstructionKind::BorrowPlaceLocal { local: LocalId(1),
                    path: vec![PlaceProjection::Field(if scalar_write { 1 } else { 0 })], mutable: true }),
                instruction(None, InstructionKind::StorageLive(LocalId(2))),
                instruction(None, InstructionKind::Store { local: LocalId(2), value: ValueId(3) }),
                instruction(Some(7), InstructionKind::Load(LocalId(2))),
                instruction(Some(8), InstructionKind::ReborrowAddress { address: ValueId(7), mutable: true }),
                instruction(Some(4), if scalar_write { InstructionKind::Constant(Constant::I32(9)) }
                    else { InstructionKind::Load(LocalId(0)) }),
            ]);
            match mode {
                0 => {},
                1 | 2 => body.push(instruction(None, InstructionKind::WriteAddress { address: ValueId(8), value: ValueId(4) })),
                3 => body.push(instruction(None, InstructionKind::Call { function: FunctionId(1), arguments: vec![ValueId(8), ValueId(4)] })),
                4 => {
                    body.push(instruction(Some(9), InstructionKind::BorrowLocalMut { local: LocalId(1) }));
                    body.push(instruction(None, InstructionKind::StoreStoredBorrow { storage: ValueId(9), path: vec![PlaceProjection::Field(0)], value: ValueId(4) }));
                }
                5 => {
                    body.push(instruction(None, InstructionKind::ReservePlaceLocal { reservation: zil_ir::ReservationId(0), local: LocalId(1), path: vec![PlaceProjection::Field(0)] }));
                    body.push(instruction(None, InstructionKind::CommitPlaceReplace { reservation: zil_ir::ReservationId(0), value: ValueId(4) }));
                }
                6 => {
                    body.push(instruction(Some(9), InstructionKind::BorrowLocalMut { local: LocalId(1) }));
                    body.push(instruction(None, InstructionKind::ReservePlaceAddress { reservation: zil_ir::ReservationId(0), address: ValueId(9), path: vec![PlaceProjection::Field(0)] }));
                    body.push(instruction(None, InstructionKind::CommitPlaceReplace { reservation: zil_ir::ReservationId(0), value: ValueId(4) }));
                }
                7 | 8 => {
                    body.push(instruction(Some(10), InstructionKind::UpdateField { aggregate: ValueId(2),
                        index: if scalar_write { 1 } else { 0 }, value: ValueId(4) }));
                    body.push(instruction(None, InstructionKind::Store { local: LocalId(1), value: ValueId(10) }));
                }
                _ => unreachable!(),
            }
            body.extend([
                instruction(Some(5), InstructionKind::Load(LocalId(1))),
                instruction(Some(6), InstructionKind::ExtractField { aggregate: ValueId(5), index: 0 }),
            ]);
            function.blocks[0].instructions = body;
            function.blocks[0].terminator = Terminator::Return(Some(ValueId(6)));
            let mut helper = function.clone();
            helper.id = FunctionId(1);
            helper.name = "replace_borrowed_field".into();
            helper.storage_life = None;
            helper.return_type = Type::Unit;
            helper.locals = vec![
                LocalDecl { id: LocalId(0), name: "target".into(), ty: pointer.clone(), mutable: false, span },
                LocalDecl { id: LocalId(1), name: "source".into(), ty: target.clone(), mutable: false, span },
            ];
            helper.parameters = vec![LocalId(0), LocalId(1)];
            helper.values = vec![pointer, target];
            helper.blocks[0].instructions = vec![
                instruction(Some(0), InstructionKind::Load(LocalId(0))),
                instruction(Some(1), InstructionKind::Load(LocalId(1))),
                instruction(None, InstructionKind::WriteAddress { address: ValueId(0), value: ValueId(1) }),
            ];
            helper.blocks[0].terminator = Terminator::Return(None);
            module.functions.push(helper);
            let check = |module: &TypedModule| check_static_occurrences(module, &mut Budget::new(&limits)).is_ok();
            assert_eq!(check(&module), mode <= 1 || mode == 7, "mutation mode {mode}");
            let bytes = zil_ir::wire::encode_module_candidate(&module, &CandidatePlan::empty_for(&module), &limits).unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(check(&decoded), mode <= 1 || mode == 7, "Wire mutation mode {mode}");
        }
    }
    #[test]
    fn nested_static_capture_requires_every_actual_constructor_to_retain_literal_sources() {
        // Source-independent trust-boundary control: an unused constructor with
        // the same complete physical Types must not inherit another literal's
        // Static proof. No semantic/source or native execution is claimed.
        let limits = Limits::default();
        let mut module = crate::static_utf8_tests::module();
        let leaf = module.functions[0].return_type.clone();
        let inner = Type::Tuple(vec![leaf.clone(), Type::I32]);
        let outer = Type::Tuple(vec![inner.clone(), Type::I32]);
        let path = vec![ContractProjection::Place(PlaceProjection::Field(0)),
            ContractProjection::Place(PlaceProjection::Field(0))];
        let span = module.functions[0].span;
        let instruction = |result, kind| Instruction { result: Some(ValueId(result)), kind, span };
        let mut entry = module.functions[0].clone();
        entry.id = FunctionId(1);
        entry.name = "nested_capture_entry".into();
        entry.environment = Some(ClosureId(0));
        entry.values = vec![outer.clone(), inner.clone(), leaf.clone()];
        let header = StorageLifeContract { fixed_binder_count: 0, invocation_binder_count: 0,
            bindings: vec![], classes: vec![], static_bindings: vec![
                StoragePosition { base: StorageBase::Capture(0), path: path.clone() },
                StoragePosition { base: StorageBase::Result, path: vec![] },
            ] };
        entry.storage_life = Some(header.clone());
        entry.blocks[0].instructions = vec![
            instruction(0, InstructionKind::ReadCapture { index: 0 }),
            instruction(1, InstructionKind::ExtractField { aggregate: ValueId(0), index: 0 }),
            instruction(2, InstructionKind::ExtractField { aggregate: ValueId(1), index: 0 }),
        ];
        entry.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        let maker = &mut module.functions[0];
        maker.return_type = Type::Closure(ClosureId(0));
        maker.locals.push(LocalDecl { id: LocalId(0), name: "unproved_other_source".into(),
            ty: leaf.clone(), mutable: false, span });
        maker.parameters.push(LocalId(0));
        maker.values = vec![leaf.clone(), Type::I32, inner.clone(), outer.clone(),
            Type::Closure(ClosureId(0))];
        maker.blocks[0].instructions.extend([
            instruction(1, InstructionKind::Constant(Constant::I32(7))),
            instruction(2, InstructionKind::MakeAggregate { fields: vec![ValueId(0), ValueId(1)] }),
            instruction(3, InstructionKind::MakeAggregate { fields: vec![ValueId(2), ValueId(1)] }),
            instruction(4, InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![ValueId(3)] }),
        ]);
        maker.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
        module.functions.push(entry);
        module.closures.push(ClosureDefinition { id: ClosureId(0), function: FunctionId(1),
            captures: vec![outer.clone()], parameters: vec![], result: leaf.clone(),
            kind: CallableKind::FnOnce, lending_captures: vec![0], storage_life: Some(header) });
        let mut forged = module.clone();
        let maker = &mut forged.functions[0];
        maker.values.extend([leaf, inner, outer, Type::Closure(ClosureId(0))]);
        maker.blocks[0].instructions.extend([
            instruction(5, InstructionKind::Load(LocalId(0))),
            instruction(6, InstructionKind::MakeAggregate { fields: vec![ValueId(5), ValueId(1)] }),
            instruction(7, InstructionKind::MakeAggregate { fields: vec![ValueId(6), ValueId(1)] }),
            instruction(8, InstructionKind::MakeClosure { closure: ClosureId(0), captures: vec![ValueId(7)] }),
        ]);
        let mut captured_replacement = module.clone();
        let pointer = Type::Address { mutable: true, inner: Box::new(
            captured_replacement.closures[0].captures[0].clone()) };
        let leaf = captured_replacement.closures[0].result.clone();
        let leaf_pointer = Type::Address { mutable: true, inner: Box::new(leaf.clone()) };
        captured_replacement.closures[0].parameters.push(leaf.clone());
        let entry = &mut captured_replacement.functions[1];
        entry.parameters.push(LocalId(0));
        entry.locals.push(LocalDecl { id: LocalId(0), name: "capture_replacement".into(),
            ty: leaf.clone(), mutable: false, span });
        entry.values.extend([pointer, leaf_pointer, leaf]);
        let mut replacement = vec![
            instruction(3, InstructionKind::BorrowCapture { index: 0, mutable: true }),
            instruction(4, InstructionKind::BorrowPlaceAddress { address: ValueId(3),
                path: vec![PlaceProjection::Field(0), PlaceProjection::Field(0)], mutable: true }),
            instruction(5, InstructionKind::Load(LocalId(0))),
            Instruction { result: None, kind: InstructionKind::WriteAddress { address: ValueId(4), value: ValueId(5) }, span },
        ];
        replacement.append(&mut entry.blocks[0].instructions);
        entry.blocks[0].instructions = replacement;
        for (module, accepted) in [(module, true), (forged, false), (captured_replacement, false)] {
            let check = |module: &TypedModule|
                check_static_occurrences(module, &mut Budget::new(&limits)).is_ok();
            assert_eq!(check(&module), accepted,
                "complete Static paths require all real constructors, including unused ones");
            let bytes = zil_ir::wire::encode_module_candidate(&module,
                &CandidatePlan::empty_for(&module), &limits).unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(check(&decoded), accepted,
                "Wire must preserve nested source selectors and the unproved constructor");
        }
    }
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

/// A possibility witness, distinct from the stronger all-Static result proof.
/// It follows actual instructions/returns and never reads a Static upper atom.
pub(super) fn has_static_source(module: &TypedModule, function: &TypedFunction,
    value: ValueId, path: &[ContractProjection], b: &mut Budget) -> CompileResult<bool> {
    let c = context(module, function, b)?;
    let sources = resolve(&c, value, path, &mut BTreeSet::new(), 0, b)?;
    Ok(sources.contains(&ActualSource::Static))
}

/// Every actual provider must have a nonempty independently derived return
/// directory. None remains Unknown; an advertised Static upper is not a seed.
pub(super) fn slot_static_source(module: &TypedModule, interface: DynInterfaceId,
    slot: u32, b: &mut Budget) -> CompileResult<Option<bool>> {
    let mut found = false;
    let mut literal = false;
    let mut complete = true;
    for table in &module.dyn_vtables {
        pay(b, 1)?;
        if table.interface != interface { continue; }
        pay(b, table.entries.len())?;
        let mut entries = table.entries.iter().filter(|entry| entry.slot == slot);
        let entry = entries.next().ok_or_else(|| err("Static slot source omits an actual provider entry"))?;
        if entries.next().is_some() { return Err(err("Static slot source repeats its provider entry")); }
        let function = module.functions.get(entry.function.0 as usize)
            .filter(|function| function.id == entry.function)
            .ok_or_else(|| err("Static slot source has no actual provider function"))?;
        let c = context(module, function, b)?;
        found = true;
        complete &= !c.returned.is_empty();
        for value in &c.returned {
            let sources = resolve(&c, *value, &[], &mut BTreeSet::new(), 0, b)?;
            complete &= !sources.is_empty();
            literal |= sources.contains(&ActualSource::Static);
        }
    }
    Ok((found && complete).then_some(literal))
}

pub(super) fn source_leaf<'a>(module: &'a TypedModule, ty: &'a Type,
    path: &[ContractProjection], b: &mut Budget) -> CompileResult<&'a Type> {
    endpoint(module, ty, path, b).map(|(ty, _)| ty)
}
