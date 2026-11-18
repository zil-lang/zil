//! Weak ByteBuffer origins retain delegation per producer alternative. They are
//! neither active loans nor public Hold facts; only actual queries read them.
use super::*;

pub(super) fn description(ty: &Type) -> bool {
    matches!(ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
}
pub(super) fn operand(kind: &InstructionKind) -> Option<ValueId> {
    match *kind {
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer } => Some(buffer),
        _ => None,
    }
}
/// The existing borrowed-Task signature checker already has its own complete
/// companion rules; it must not be reclassified as this new consumer.
pub(super) fn legacy_task_signature(f: &TypedFunction) -> bool {
    matches!(&f.return_type,Type::ErrorUnion {result,..}
        if matches!(result.as_ref(),Type::Managed(payload)
            if matches!(payload.as_ref(),Type::Task {retained_life,..} if !retained_life.ports.is_empty())))
}

fn closed() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "Byte query requires a complete Direct actual description/read proof",
        None,
    )]
}
pub(super) fn instruction(
    _module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(buffer) = operand(&i.kind) else {
        return Ok(false);
    };
    fee(b, 3)?;
    if !description(value_type(f, buffer)?)
        || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::I64)
    {
        return Err(failure(
            "Byte query requires actual Desc<ByteBuffer> and i64 result",
        ));
    }
    Ok(true)
}

/// Authenticate once before stored executable-value admission, so a missing
/// sealed definition is rejected as malformed IR rather than an opaque profile.
pub(super) fn authenticate(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if !b.byte_queries {
        return Ok(());
    }
    fee(b, module.resources.len().saturating_add(1))?;
    let mut count = 0usize;
    for resource in &module.resources {
        fee(b, resource.name.len().saturating_add(19))?;
        if resource.name == "@builtin:ByteBuffer" || resource.payload == Type::ByteBuffer {
            if resource.name != "@builtin:ByteBuffer"
                || resource.payload != Type::ByteBuffer
                || resource.teardown.is_some()
            {
                return Err(failure(
                    "Byte query requires the actual canonical sealed ByteBuffer resource",
                ));
            }
            count += 1;
        }
    }
    if count != 1 || !module.target.is_supported_x64_llvm() {
        return Err(failure(
            "Byte query requires one actual canonical sealed ByteBuffer resource",
        ));
    }
    Ok(())
}

fn covered(mask: &[bool], index: usize) -> bool {
    mask.is_empty() || mask[index]
}
fn indirect_arguments(kind: &InstructionKind) -> Option<&[ValueId]> {
    match kind {
        InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. }
        | InstructionKind::CallDynSlot { arguments, .. } => Some(arguments),
        _ => None,
    }
}

/// Presence is only scheduling. Missing coverage is never an empty effect.
// A zero-input ordinary entry may call a stored scalar body only after
// that body's independently solved query effect is proved complete and empty.
pub(super) struct PendingEdge {
    caller: usize,
    block: usize,
    instruction: usize,
    callee: usize,
}

fn zero_input_scalar(
    module: &TypedModule,
    caller: usize,
    callee: usize,
    b: &mut Budget,
) -> CompileResult<bool> {
    fee(b, 8)?;
    let from = module.functions.get(caller).ok_or_else(closed)?;
    let to = module.functions.get(callee).ok_or_else(closed)?;
    Ok(task_calls::body(from, b)?
        && from.environment.is_none()
        && from.foreign.is_none()
        && from.callable_contract.is_none()
        && from.callable_return_contract.is_none()
        && task_calls::body(to, b)?
        && to.environment.is_none()
        && to.foreign.is_none()
        && to.callable_contract.is_none()
        && to.callable_return_contract.is_none()
        && to.parameters.is_empty()
        && to.return_type.is_b0_scalar()
        && to.result_mode == PassingMode::Infer)
}
fn matches_edge(module: &TypedModule, edge: &PendingEdge, b: &mut Budget) -> CompileResult<bool> {
    fee(b, 4)?;
    if !zero_input_scalar(module, edge.caller, edge.callee, b)? {
        return Ok(false);
    }
    let caller = module.functions.get(edge.caller).ok_or_else(closed)?;
    let operation = caller
        .blocks
        .get(edge.block)
        .and_then(|block| block.instructions.get(edge.instruction))
        .ok_or_else(closed)?;
    let (function, arguments) = match &operation.kind {
        InstructionKind::Call { function, arguments }
        | InstructionKind::SuspendCall { function, arguments }
            if task_calls::call(&operation.kind, b)? => (function, arguments),
        _ => return Ok(false),
    };
    let target = &module.functions[edge.callee];
    Ok(function.0 as usize == edge.callee
        && arguments.is_empty()
        && (operation
            .result
            .and_then(|value| caller.values.get(value.0 as usize))
            == Some(&target.return_type)
            || (operation.result.is_none() && target.return_type == Type::Unit)))
}
pub(super) fn inventory(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<PendingEdge>> {
    if !b.byte_queries {
        return Ok(Vec::new());
    }
    fee(b, module.functions.len().saturating_mul(2))?;
    let mut marked = vec![false; module.functions.len()];
    let mut pending = Vec::new();
    let mut edges = Vec::new();
    for (index, f) in module.functions.iter().enumerate() {
        fee(b, f.blocks.len())?;
        for (block, live) in f.blocks.iter().zip(&graphs[index].1) {
            if !live {
                continue;
            }
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                marked[index] |= operand(&i.kind).is_some();
                if let (InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. }) = i.kind
                    && task_calls::call(&i.kind, b)? {
                    fee(b, 2)?;
                    edges.push((index, function.0 as usize));
                }
                if let Some((closure, _)) =
                    super::super::stored_callables::called(module, f, &i.kind)
                {
                    fee(b, 2)?;
                    edges.push((index, closure.function.0 as usize));
                }
            }
        }
    }
    loop {
        let mut changed = false;
        fee(b, edges.len())?;
        for &(caller, callee) in &edges {
            if marked[callee] && !marked[caller] {
                marked[caller] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if !b.composition_stored.is_empty() {
        fee(b, edges.len())?;
        for &(caller, callee) in &edges {
            if marked[callee]
                && covered(&b.composition_stored, caller) != covered(&b.composition_stored, callee)
                && (covered(&b.composition_stored, caller)
                    || !covered(&b.composition_stored, callee)
                    || !zero_input_scalar(module, caller, callee, b)?)
            {
                return Err(closed());
            }
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        fee(b, 1)?;
        if marked[index]
            && covered(&b.composition_stored, index)
            && (f.foreign.is_some() || !task_calls::body(f, b)?)
        {
            return Err(closed());
        }
        fee(b, f.blocks.len())?;
        for (block_index, block) in f.blocks.iter().enumerate() {
            fee(b, block.instructions.len())?;
            for (instruction_index, i) in block.instructions.iter().enumerate() {
                if let (InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. }) = i.kind
                    && task_calls::call(&i.kind, b)?
                    && marked[function.0 as usize]
                    && covered(&b.composition_stored, index)
                        != covered(&b.composition_stored, function.0 as usize)
                {
                    let edge = PendingEdge {
                        caller: index,
                        block: block_index,
                        instruction: instruction_index,
                        callee: function.0 as usize,
                    };
                    if covered(&b.composition_stored, index) || !matches_edge(module, &edge, b)? {
                        return Err(closed());
                    }
                    fee(b, 4)?;
                    pending.push(edge);
                }
                if let InstructionKind::FunctionRef { function } = i.kind
                    && marked[function.0 as usize]
                    && (covered(&b.composition_stored, index)
                        || covered(&b.composition_stored, function.0 as usize)
                        || b.store_option_results)
                {
                    return Err(closed());
                }
                if covered(&b.composition_stored, index) {
                    if let Some(arguments) = indirect_arguments(&i.kind) {
                        fee(b, arguments.len())?;
                        if super::super::stored_callables::called(module, f, &i.kind).is_none()
                            && arguments
                                .iter()
                                .any(|id| description(&f.values[id.0 as usize]))
                        {
                            return Err(closed());
                        }
                    }
                    // Constructing an environment does not execute its body.
                    // Actual invocation edges above carry its read effects.
                }
            }
        }
    }
    fee(b, module.dyn_vtables.len())?;
    for table in &module.dyn_vtables {
        fee(b, table.entries.len().saturating_add(1))?;
        for entry in table.entries.iter().chain(table.cleanup.iter()) {
            let index = entry.function.0 as usize;
            if marked.get(index).copied().ok_or_else(closed)?
                && covered(&b.composition_stored, index)
            {
                return Err(closed());
            }
        }
    }
    b.byte_query_functions = marked;
    Ok(pending)
}

fn permission_work(p: &Permission, b: &mut Budget) -> CompileResult<()> {
    target_work(&p.targets, b)?;
    fee(b, p.loans.len().saturating_add(1))
}
fn copy_permission(p: &Permission, b: &mut Budget) -> CompileResult<Permission> {
    permission_work(p, b)?;
    Ok(p.clone())
}
fn add(to: &mut Vec<Permission>, from: &[Permission], b: &mut Budget) -> CompileResult<bool> {
    fee(b, from.len())?;
    let mut changed = false;
    for candidate in from {
        let mut present = false;
        for prior in to.iter() {
            permission_work(prior, b)?;
            permission_work(candidate, b)?;
            if prior == candidate {
                present = true;
                break;
            }
        }
        if !present {
            to.push(copy_permission(candidate, b)?);
            changed = true;
        }
    }
    Ok(changed)
}
fn copy_origins(from: &[Permission], b: &mut Budget) -> CompileResult<Vec<Permission>> {
    let mut out = Vec::new();
    add(&mut out, from, b)?;
    Ok(out)
}
fn new_origin(
    base: HoldRoot,
    place: Vec<PlaceProjection>,
    b: &mut Budget,
) -> CompileResult<Permission> {
    fee(b, place.len().saturating_add(2))?;
    Ok(Permission {
        targets: BTreeSet::from([Target {
            retained_port: None,
            base,
            stored: Vec::new(),
            place,
            heap_parents: Vec::new(),
            heap_element: None,
        }]),
        loans: Loans::new(),
    })
}

/// These closed grants belong to the current execution of a loan site. They
/// never go through the strong solver's function-wide ancestry relation.
#[derive(Default, PartialEq, Eq)]
pub(super) struct Point {
    values: BTreeMap<ValueId, Vec<Permission>>,
    locals: BTreeMap<LocalId, Vec<Permission>>,
    views: BTreeMap<LocalId, u32>,
    weak_value_captures: BTreeMap<ValueId, WeakCaptures>,
    weak_local_captures: BTreeMap<LocalId, WeakCaptures>,
}
type WeakCaptures = BTreeMap<u32, Vec<Permission>>;

fn join_weak_captures<K: Copy + Ord>(
    to: &mut BTreeMap<K, WeakCaptures>,
    from: &BTreeMap<K, WeakCaptures>,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, from.len())?;
    for (key, fields) in from {
        ordered(b, to.len(), 1)?;
        let actual = to.entry(*key).or_default();
        for (index, row) in fields {
            ordered(b, actual.len(), 1)?;
            add(actual.entry(*index).or_default(), row, b)?;
        }
    }
    Ok(())
}
fn copy_weak_captures(fields: Option<&WeakCaptures>, b: &mut Budget) -> CompileResult<WeakCaptures> {
    let mut out = BTreeMap::new();
    if let Some(fields) = fields {
        for (index, row) in fields {
            ordered(b, out.len(), 1)?;
            out.insert(*index, copy_origins(row, b)?);
        }
    }
    Ok(out)
}
fn tracked(ty: &Type) -> bool {
    description(ty) || matches!(ty, Type::Address { .. })
}
fn point_work(p: &Point, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        p.values
            .len()
            .saturating_add(p.locals.len())
            .saturating_add(p.views.len())
            .saturating_add(p.weak_value_captures.len())
            .saturating_add(p.weak_local_captures.len()),
    )?;
    for row in p.values.values().chain(p.locals.values()).chain(
        p.weak_value_captures.values().chain(p.weak_local_captures.values()).flat_map(BTreeMap::values),
    ) {
        fee(b, row.len())?;
        for permission in row {
            permission_work(permission, b)?;
        }
    }
    Ok(())
}
fn point_join(to: &mut Point, from: &Point, b: &mut Budget) -> CompileResult<()> {
    fee(b, from.values.len().saturating_add(from.locals.len()))?;
    for (key, row) in &from.values {
        ordered(b, to.values.len(), 1)?;
        add(to.values.entry(*key).or_default(), row, b)?;
    }
    for (key, row) in &from.locals {
        ordered(b, to.locals.len(), 1)?;
        add(to.locals.entry(*key).or_default(), row, b)?;
    }
    join_weak_captures(&mut to.weak_value_captures, &from.weak_value_captures, b)?;
    join_weak_captures(&mut to.weak_local_captures, &from.weak_local_captures, b)?;
    for (local, capture) in &from.views {
        ordered(b, to.views.len(), 1)?;
        if to
            .views
            .insert(*local, *capture)
            .is_some_and(|old| old != *capture)
        {
            return Err(failure(
                "Byte query capture view has ambiguous actual sources",
            ));
        }
    }
    Ok(())
}
impl Point {
    fn value(&self, value: ValueId, b: &mut Budget) -> CompileResult<&[Permission]> {
        ordered(b, self.values.len(), 1)?;
        Ok(self.values.get(&value).map_or(&[], Vec::as_slice))
    }
    fn local(&self, local: LocalId, b: &mut Budget) -> CompileResult<&[Permission]> {
        ordered(b, self.locals.len(), 1)?;
        Ok(self.locals.get(&local).map_or(&[], Vec::as_slice))
    }
    fn revoke(&mut self, id: usize, b: &mut Budget) -> CompileResult<()> {
        point_work(self, b)?;
        for row in self.values.values_mut().chain(self.locals.values_mut()).chain(
            self.weak_value_captures.values_mut().chain(self.weak_local_captures.values_mut())
                .flat_map(BTreeMap::values_mut),
        ) {
            for p in row.iter_mut() {
                ordered(b, p.loans.len(), 1)?;
                p.loans.remove(&id);
            }
            let mut distinct = Vec::new();
            add(&mut distinct, row, b)?;
            *row = distinct;
        }
        Ok(())
    }
}
fn retarget(
    from: &[Permission],
    targets: &Targets,
    b: &mut Budget,
) -> CompileResult<Vec<Permission>> {
    let mut out = Vec::new();
    fee(b, from.len())?;
    for p in from {
        target_work(targets, b)?;
        fee(b, p.loans.len())?;
        let permission = Permission {
            targets: targets.clone(),
            loans: p.loans.clone(),
        };
        add(&mut out, &[permission], b)?;
    }
    Ok(out)
}
fn weak_capture_definition<'a>(
    module: &'a TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Option<&'a ClosureDefinition>> {
    let Type::Closure(id) = ty else {
        return Ok(None);
    };
    let closure = module.closures.get(id.0 as usize)
        .filter(|closure| closure.id == *id)
        .ok_or_else(closed)?;
    fee(b, closure.captures.len().saturating_add(1))?;
    Ok(closure.captures.iter().any(description).then_some(closure))
}

fn weak_capture_step(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    point: &mut Point,
    b: &mut Budget,
) -> CompileResult<()> {
    match i.kind {
        InstructionKind::Store { local, value }
            if weak_capture_definition(module, value_type(f, value)?, b)?.is_some() =>
        {
            ordered(b, point.weak_value_captures.len(), 1)?;
            let captures = copy_weak_captures(point.weak_value_captures.get(&value), b)?;
            ordered(b, point.weak_local_captures.len(), 1)?;
            point.weak_local_captures.insert(local, captures);
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
            if !point.weak_local_captures.is_empty() =>
        {
            ordered(b, point.weak_local_captures.len(), 1)?;
            point.weak_local_captures.remove(&local);
        }
        _ => {}
    }
    let Some(result) = i.result else {
        return Ok(());
    };
    let Some(definition) = weak_capture_definition(module, value_type(f, result)?, b)? else {
        return Ok(());
    };
    let captures = match &i.kind {
        InstructionKind::MakeClosure { closure, captures } => {
            if definition.id != *closure || definition.captures.len() != captures.len() {
                return Err(closed());
            }
            fee(b, captures.len())?;
            let mut out = WeakCaptures::new();
            for (index, (ty, value)) in definition.captures.iter().zip(captures).enumerate() {
                if description(ty) {
                    if value_type(f, *value)? != ty {
                        return Err(failure("Byte environment capture changes its complete weak type"));
                    }
                    ordered(b, out.len(), 1)?;
                    out.insert(index as u32, copy_origins(point.value(*value, b)?, b)?);
                }
            }
            out
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            ordered(b, point.weak_local_captures.len(), 1)?;
            let out = copy_weak_captures(point.weak_local_captures.get(local), b)?;
            if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                point.weak_local_captures.remove(local);
            }
            out
        }
        _ => return Err(closed()),
    };
    ordered(b, point.weak_value_captures.len(), 1)?;
    point.weak_value_captures.insert(result, captures);
    Ok(())
}

/// A definition replaces its SSA row. Reexecuting a real strong loan site
/// revokes the old generation in every saved SSA value and local first.
fn step(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    actual: &[Value],
    summaries: &[Facts],
    point: &mut Point,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let result = i.result.filter(|r| tracked(&f.values[r.0 as usize]));
    let fresh = if let Some(result) = result {
        let p = &actual[result.0 as usize].pointer;
        ordered(b, p.loans.len(), 1)?;
        matches!(value_type(f, result)?, Type::Address { .. })
            && p.loans.contains(&(result.0 as usize))
    } else {
        false
    };
    if fresh {
        point.revoke(result.unwrap().0 as usize, b)?;
    }
    weak_capture_step(module, f, i, point, b)?;
    match i.kind {
        InstructionKind::BindPackageCapture { local, index } => {
            ordered(b, point.views.len(), 1)?;
            point.views.insert(local, index);
        }
        InstructionKind::EndPackageCapture { local, .. } => {
            ordered(b, point.views.len(), 1)?;
            point.views.remove(&local);
        }
        InstructionKind::Store { local, value } if tracked(&f.locals[local.0 as usize].ty) => {
            let row = copy_origins(point.value(value, b)?, b)?;
            ordered(b, point.locals.len(), 1)?;
            point.locals.insert(local, row);
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            ordered(b, point.locals.len(), 1)?;
            point.locals.remove(&local);
            point.views.remove(&local);
        }
        _ => {}
    }
    let Some(result) = result else { return Ok(()) };
    let mut out = Vec::new();
    if description(value_type(f, result)?) {
        match &i.kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                add(&mut out, point.local(*local, b)?, b)?
            }
            InstructionKind::DescribeLocal { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if p.as_ref()==&Type::ByteBuffer) =>
            {
                out.push(new_origin(HoldRoot::Local(*local), Vec::new(), b)?);
                false
            }
            InstructionKind::ReadCapture { index } => {
                let actual = f.environment.and_then(|id| module.closures.get(id.0 as usize))
                    .filter(|closure| closure.function == f.id)
                    .and_then(|closure| closure.captures.get(*index as usize));
                if !actual.is_some_and(description) {
                    return Err(closed());
                }
                out.push(new_origin(HoldRoot::Capture(*index), Vec::new(), b)?);
                false
            }
            InstructionKind::DescribeCapture { index } => {
                let capture = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .filter(|closure| closure.function == f.id)
                    .and_then(|closure| closure.captures.get(*index as usize))
                    .ok_or_else(closed)?;
                if !matches!(capture, Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer)
                {
                    return Err(closed());
                }
                out.push(new_origin(HoldRoot::Capture(*index), Vec::new(), b)?);
                false
            }
            InstructionKind::DescribePackageField { local, index } => {
                let Type::Package { fields, .. } =
                    super::super::resources::package_storage(&f.locals[local.0 as usize].ty)
                else {
                    return Err(closed());
                };
                fee(b, 2)?;
                if !fields.get(*index as usize).is_some_and(
                    |(_, ty)| matches!(ty, Type::Managed(p) if p.as_ref()==&Type::ByteBuffer),
                ) {
                    return Err(closed());
                }
                out.push(new_origin(
                    point
                        .views
                        .get(local)
                        .copied()
                        .map_or(HoldRoot::Local(*local), HoldRoot::Capture),
                    vec![PlaceProjection::Field(*index)],
                    b,
                )?);
                false
            }
            InstructionKind::DescribeAddress { address } if matches!(value_type(f, *address)?, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) => {
                add(&mut out, point.value(*address, b)?, b)?
            }
            _ => return Err(closed()),
        };
    } else {
        let targets = &actual[result.0 as usize].pointer.targets;
        if !targets.is_empty() {
            match &i.kind {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    out = retarget(point.local(*local, b)?, targets, b)?
                }
                InstructionKind::ReborrowAddress { address, .. }
                | InstructionKind::AddressField { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. } => {
                    out = retarget(point.value(*address, b)?, targets, b)?
                }
                InstructionKind::Call { function, arguments }
                | InstructionKind::SuspendCall { function, arguments }
                    if task_calls::call(&i.kind, b)? => {
                    let summary = &summaries[function.0 as usize];
                    if summary.complete {
                        ordered(b, summary.returned.len(), 0)?;
                        if let Some(sources) = summary.returned.get(&Vec::new()) {
                            fee(b, sources.len())?;
                            for source in sources {
                                fee(b, target_width(source).saturating_add(1))?;
                                if let HoldRoot::Parameter(position) = source.base
                                    && source.retained_port.is_none()
                                    && source.stored.is_empty()
                                    && source.heap_element.is_none()
                                    && module.functions[function.0 as usize]
                                        .parameters
                                        .get(position as usize)
                                        .is_some_and(|p| {
                                            matches!(
                                                module.functions[function.0 as usize].locals
                                                    [p.0 as usize]
                                                    .ty,
                                                Type::Address { .. }
                                            )
                                        })
                                    && arguments.get(position as usize).is_some_and(|v| {
                                        matches!(f.values[v.0 as usize], Type::Address { .. })
                                    })
                                {
                                    let inherited = retarget(
                                        point.value(arguments[position as usize], b)?,
                                        targets,
                                        b,
                                    )?;
                                    add(&mut out, &inherited, b)?;
                                } else {
                                    target_work(targets, b)?;
                                    add(
                                        &mut out,
                                        &[Permission {
                                            targets: targets.clone(),
                                            loans: Loans::new(),
                                        }],
                                        b,
                                    )?;
                                }
                            }
                        }
                    }
                }
                _ => {
                    target_work(targets, b)?;
                    out.push(Permission {
                        targets: targets.clone(),
                        loans: Loans::new(),
                    });
                }
            }
            if fresh {
                fee(b, out.len())?;
                for p in &mut out {
                    ordered(b, p.loans.len(), 1)?;
                    p.loans.insert(result.0 as usize);
                }
            }
        }
    }
    ordered(b, point.values.len(), 1)?;
    point.values.insert(result, out);
    Ok(())
}
pub(super) struct Flow {
    entry: Point,
    outputs: Vec<Point>,
}
impl Flow {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn solve(
        module: &TypedModule,
        f: &TypedFunction,
        preds: &[Vec<usize>],
        reachable: &[bool],
        actual_entry: &State,
        actual: &[Value],
        summaries: &[Facts],
        b: &mut Budget,
    ) -> CompileResult<Option<Self>> {
        if !b.byte_queries {
            return Ok(None);
        }
        fee(b, 1)?;
        if !b.byte_query_functions[f.id.0 as usize] {
            return Ok(None);
        }
        let mut entry = Point::default();
        fee(b, f.parameters.len())?;
        for (position, local) in f.parameters.iter().enumerate() {
            let ty = &f.locals[local.0 as usize].ty;
            if weak_capture_definition(module, ty, b)?.is_some() {
                return Err(closed());
            }
            let row = if description(ty) {
                vec![new_origin(
                    HoldRoot::Parameter(position as u32),
                    Vec::new(),
                    b,
                )?]
            } else if matches!(ty, Type::Address { .. }) {
                ordered(b, actual_entry.locals.len(), 1)?;
                let actual = &actual_entry
                    .locals
                    .get(local)
                    .ok_or_else(|| failure("Byte address input has no actual entry"))?
                    .pointer;
                vec![copy_permission(actual, b)?]
            } else {
                continue;
            };
            ordered(b, entry.locals.len(), 1)?;
            entry.locals.insert(*local, row);
        }
        fee(b, f.blocks.len())?;
        let mut flow = Self {
            entry,
            outputs: (0..f.blocks.len()).map(|_| Point::default()).collect(),
        };
        loop {
            let mut changed = false;
            fee(b, f.blocks.len())?;
            for (bi, block) in f.blocks.iter().enumerate() {
                if !reachable[bi] {
                    continue;
                }
                let mut point = flow.input(f, bi, preds, reachable, b)?;
                fee(b, block.instructions.len())?;
                for i in &block.instructions {
                    step(module, f, i, actual, summaries, &mut point, b)?;
                }
                point_work(&point, b)?;
                point_work(&flow.outputs[bi], b)?;
                if point != flow.outputs[bi] {
                    flow.outputs[bi] = point;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(Some(flow))
    }
    pub(super) fn input(
        &self,
        f: &TypedFunction,
        bi: usize,
        preds: &[Vec<usize>],
        reachable: &[bool],
        b: &mut Budget,
    ) -> CompileResult<Point> {
        let mut point = Point::default();
        if bi == f.entry.0 as usize {
            point_join(&mut point, &self.entry, b)?;
        }
        fee(b, preds[bi].len())?;
        for pred in &preds[bi] {
            if reachable[*pred] {
                point_join(&mut point, &self.outputs[*pred], b)?;
            }
        }
        Ok(point)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        actual: &[Value],
        summaries: &[Facts],
        point: &mut Point,
        b: &mut Budget,
    ) -> CompileResult<()> {
        step(module, f, i, actual, summaries, point, b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn substitute(
        &self,
        source: &Target,
        callee: &TypedFunction,
        arguments: &[ValueId],
        values: &[Value],
        state: &State,
        point: &Point,
        b: &mut Budget,
    ) -> CompileResult<Vec<Permission>> {
        let HoldRoot::Parameter(position) = source.base else {
            return Err(closed());
        };
        fee(b, target_width(source).saturating_add(2))?;
        let ty = &callee.locals[callee.parameters[position as usize].0 as usize].ty;
        if description(ty) {
            if source.retained_port.is_some()
                || !source.stored.is_empty()
                || !source.place.is_empty()
                || source.heap_element.is_some()
            {
                return Err(closed());
            }
            let out = copy_origins(point.value(arguments[position as usize], b)?, b)?;
            if out.is_empty() {
                return Err(failure("Byte descriptor input lacks actual weak origins"));
            }
            Ok(out)
        } else if matches!(ty, Type::Address { .. }) {
            let actual = super::substitute(source, arguments, values, state, b)?;
            if actual.targets.is_empty() {
                return Err(failure(
                    "Byte separation lacks complete actual address origins",
                ));
            }
            let out = if source.retained_port.is_none()
                && source.stored.is_empty()
                && source.heap_element.is_none()
            {
                retarget(
                    point.value(arguments[position as usize], b)?,
                    &actual.targets,
                    b,
                )?
            } else {
                vec![Permission {
                    targets: actual.targets,
                    loans: Loans::new(),
                }]
            };
            if out.is_empty() {
                return Err(failure("Byte address input lacks current delegation proof"));
            }
            Ok(out)
        } else {
            Err(closed())
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reads(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        values: &[Value],
        state: &State,
        point: &Point,
        summaries: &[Facts],
        b: &mut Budget,
    ) -> CompileResult<Vec<Permission>> {
        fee(b, 1)?;
        if let Some(value) = operand(&i.kind) {
            let out = copy_origins(point.value(value, b)?, b)?;
            if out.is_empty() {
                return Err(failure("Byte query has no actual weak head origin"));
            }
            return Ok(out);
        }
        let (function, arguments, closure) = match &i.kind {
            InstructionKind::Call { function, arguments }
            | InstructionKind::SuspendCall { function, arguments }
                if task_calls::call(&i.kind, b)? => (*function, arguments.as_slice(), None),
            _ => {
                let Some((closure, arguments)) =
                    super::super::stored_callables::called(module, f, &i.kind)
                else {
                    return Ok(Vec::new());
                };
                (closure.function, arguments, Some(closure))
            }
        };
        fee(b, 1)?;
        if !b.byte_query_functions[function.0 as usize] {
            return Ok(Vec::new());
        }
        let prior = &summaries[function.0 as usize];
        if !prior.byte_covered {
            return Ok(Vec::new());
        }
        fee(b, prior.byte_reads.len())?;
        let mut out = Vec::new();
        for source in &prior.byte_reads {
            let actual = if matches!(source.base, HoldRoot::Capture(_)) {
                let closure = closure.ok_or_else(closed)?;
                captured_read(closure, f, i, source, values, state, point, b)?
            } else {
                self.substitute(
                    source,
                    &module.functions[function.0 as usize],
                    arguments,
                    values,
                    state,
                    point,
                    b,
                )?
            };
            add(&mut out, &actual, b)?;
        }
        Ok(out)
    }
}

/// A closure's query effect is relative to its real environment, never to a
/// type-only owner. Preserve every receiver alternative and its current loan
/// delegation before selecting the actual capture's payload.
#[allow(clippy::too_many_arguments)]
fn captured_read(
    closure: &ClosureDefinition,
    f: &TypedFunction,
    i: &Instruction,
    source: &Target,
    values: &[Value],
    state: &State,
    point: &Point,
    b: &mut Budget,
) -> CompileResult<Vec<Permission>> {
    let HoldRoot::Capture(index) = source.base else {
        return Err(closed());
    };
    if source.retained_port.is_some()
        || !source.stored.is_empty()
        || source.heap_element.is_some()
        || !source.heap_parents.is_empty()
    {
        return Err(closed());
    }
    let mut ty = closure.captures.get(index as usize).ok_or_else(closed)?;
    for projection in &source.place {
        fee(b, 1)?;
        let (PlaceProjection::Field(field), Type::Package { fields, .. }) =
            (projection, super::super::resources::package_storage(ty))
        else {
            return Err(closed());
        };
        ty = &fields.get(*field as usize).ok_or_else(closed)?.1;
    }
    if description(ty) && source.place.is_empty() {
        return weak_capture_read(i, point, index, b);
    }
    if !matches!(ty, Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer) {
        return Err(closed());
    }
    let mut heads = match i.kind {
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            copy_origins(point.value(callee, b)?, b)?
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            if !state.locals.contains_key(&callee) {
                return Err(closed());
            }
            vec![new_origin(
                local_capture_base(state, callee, b)?,
                Vec::new(),
                b,
            )?]
        }
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => {
            // The consumed value must name a real whole environment producer;
            // no temporary root is invented from its Closure type.
            let mut producer = None;
            for block in &f.blocks {
                fee(b, block.instructions.len().saturating_add(1))?;
                for instruction in &block.instructions {
                    if instruction.result == Some(callee)
                        && producer.replace(&instruction.kind).is_some()
                    {
                        return Err(closed());
                    }
                }
            }
            let base = match producer.ok_or_else(closed)? {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    local_capture_base(state, *local, b)?
                }
                InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index } => {
                    HoldRoot::Capture(*index)
                }
                _ => return Err(closed()),
            };
            let mut head = new_origin(base, Vec::new(), b)?;
            fee(b, values[callee.0 as usize].pointer.loans.len())?;
            head.loans = values[callee.0 as usize].pointer.loans.clone();
            vec![head]
        }
        _ => return Err(closed()),
    };
    if heads.is_empty() {
        return Err(failure(
            "Byte query environment lacks its actual receiver origin",
        ));
    }
    for head in &mut heads {
        let mut targets = Targets::new();
        for root in &head.targets {
            fee(
                b,
                target_width(root)
                    .saturating_add(source.place.len())
                    .saturating_add(2),
            )?;
            let mut selected = root.clone();
            selected.place.push(PlaceProjection::Field(index));
            selected.place.extend_from_slice(&source.place);
            targets.insert(selected);
        }
        head.targets = targets;
    }
    Ok(heads)
}
fn weak_capture_read(
    i: &Instruction,
    point: &Point,
    index: u32,
    b: &mut Budget,
) -> CompileResult<Vec<Permission>> {
    let mut out = Vec::new();
    let mut read_local = |local, b: &mut Budget| -> CompileResult<()> {
        ordered(b, point.weak_local_captures.len(), 1)?;
        let fields = point.weak_local_captures.get(&local).ok_or_else(closed)?;
        ordered(b, fields.len(), 1)?;
        add(&mut out, fields.get(&index).ok_or_else(closed)?, b)?;
        Ok(())
    };
    match i.kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => read_local(callee, b)?,
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            for origin in point.value(callee, b)? {
                for target in &origin.targets {
                    fee(b, target_width(target))?;
                    let HoldRoot::Local(local) = target.base else {
                        return Err(closed());
                    };
                    if target.retained_port.is_some() || !target.place.is_empty()
                        || !target.stored.is_empty() || !target.heap_parents.is_empty()
                        || target.heap_element.is_some()
                    {
                        return Err(closed());
                    }
                    read_local(local, b)?;
                }
            }
        }
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => {
            ordered(b, point.weak_value_captures.len(), 1)?;
            let fields = point.weak_value_captures.get(&callee).ok_or_else(closed)?;
            ordered(b, fields.len(), 1)?;
            add(&mut out, fields.get(&index).ok_or_else(closed)?, b)?;
        }
        _ => return Err(closed()),
    }
    if out.is_empty() {
        return Err(failure("Byte query captured description has no actual reaching owner"));
    }
    Ok(out)
}

pub(super) fn record(
    reads: &mut Vec<Target>,
    permission: &Permission,
    b: &mut Budget,
) -> CompileResult<()> {
    for source in &permission.targets {
        fee(b, target_width(source).saturating_add(1))?;
        match source.base {
            HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                let mut found = false;
                for prior in reads.iter() {
                    fee(
                        b,
                        target_width(prior)
                            .saturating_add(target_width(source))
                            .saturating_add(1),
                    )?;
                    if prior == source {
                        found = true;
                        break;
                    }
                }
                if !found {
                    fee(b, target_width(source).saturating_add(1))?;
                    reads.push(source.clone());
                }
            }
            HoldRoot::Local(_) => {}
            _ => return Err(closed()),
        }
    }
    Ok(())
}
pub(super) fn work(reads: &[Target], b: &mut Budget) -> CompileResult<()> {
    fee(b, reads.len())?;
    for source in reads {
        fee(b, target_width(source))?;
    }
    Ok(())
}

/// Rich signatures can be checked before the ordinary instruction inventory.
/// Only the exact new input companion pays this bounded presence scan.
pub(super) fn signature_presence(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    fee(b, 1)?;
    if b.byte_queries || b.byte_query_presence_checked {
        return Ok(b.byte_queries);
    }
    fee(b, module.functions.len())?;
    for function in &module.functions {
        fee(b, function.blocks.len())?;
        for block in &function.blocks {
            fee(b, block.instructions.len())?;
            for instruction in &block.instructions {
                if operand(&instruction.kind).is_some() {
                    b.byte_queries = true;
                    b.byte_query_presence_checked = true;
                    return Ok(true);
                }
            }
        }
    }
    b.byte_query_presence_checked = true;
    Ok(false)
}

pub(super) fn certify_edges(
    module: &TypedModule,
    pending: &[PendingEdge],
    summaries: &[Facts],
    b: &mut Budget,
) -> CompileResult<()> {
    if pending.is_empty() {
        return Ok(());
    }
    fee(b, pending.len())?;
    for edge in pending {
        fee(b, 4)?;
        if covered(&b.composition_stored, edge.caller)
            || !covered(&b.composition_stored, edge.callee)
            || !matches_edge(module, edge, b)?
        {
            return Err(closed());
        }
        let proof = summaries.get(edge.callee).ok_or_else(closed)?;
        if !proof.complete || !proof.byte_covered || !proof.byte_reads.is_empty() {
            return Err(closed());
        }
    }
    Ok(())
}
