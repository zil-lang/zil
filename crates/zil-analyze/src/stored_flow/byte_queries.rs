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
pub(super) fn inventory(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.byte_queries {
        return Ok(());
    }
    fee(b, module.functions.len().saturating_mul(2))?;
    let mut marked = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    for (index, f) in module.functions.iter().enumerate() {
        for (block, live) in f.blocks.iter().zip(&graphs[index].1) {
            if !live {
                continue;
            }
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                marked[index] |= operand(&i.kind).is_some();
                if let InstructionKind::Call { function, .. } = i.kind {
                    fee(b, 2)?;
                    edges.push((index, function.0 as usize));
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
            {
                return Err(closed());
            }
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        fee(b, 1)?;
        let companion = if f.storage_life.is_some() && !legacy_task_signature(f) {
            fee(b, f.parameters.len())?;
            f.parameters
                .iter()
                .any(|p| description(&f.locals[p.0 as usize].ty))
        } else {
            false
        };
        if companion && covered(&b.composition_stored, index) && !marked[index] {
            return Err(closed());
        }
        if marked[index]
            && covered(&b.composition_stored, index)
            && (f.foreign.is_some() || f.environment.is_some() || f.call_kind != CallKind::Direct)
        {
            return Err(closed());
        }
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
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
                        if arguments
                            .iter()
                            .any(|id| description(&f.values[id.0 as usize]))
                        {
                            return Err(closed());
                        }
                    }
                    if let InstructionKind::MakeClosure { closure, .. } = i.kind {
                        fee(b, 1)?;
                        let entry = module
                            .closures
                            .get(closure.0 as usize)
                            .ok_or_else(closed)?
                            .function;
                        if marked[entry.0 as usize] {
                            return Err(closed());
                        }
                    }
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
    Ok(())
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
}
fn tracked(ty: &Type) -> bool {
    description(ty) || matches!(ty, Type::Address { .. })
}
fn point_work(p: &Point, b: &mut Budget) -> CompileResult<()> {
    fee(b, p.values.len().saturating_add(p.locals.len()))?;
    for row in p.values.values().chain(p.locals.values()) {
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
        for row in self.values.values_mut().chain(self.locals.values_mut()) {
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
    match i.kind {
        InstructionKind::Store { local, value } if tracked(&f.locals[local.0 as usize].ty) => {
            let row = copy_origins(point.value(value, b)?, b)?;
            ordered(b, point.locals.len(), 1)?;
            point.locals.insert(local, row);
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            ordered(b, point.locals.len(), 1)?;
            point.locals.remove(&local);
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
                    HoldRoot::Local(*local),
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
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
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
        let InstructionKind::Call {
            function,
            arguments,
        } = &i.kind
        else {
            return Ok(Vec::new());
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
            let actual = self.substitute(
                source,
                &module.functions[function.0 as usize],
                arguments,
                values,
                state,
                point,
                b,
            )?;
            add(&mut out, &actual, b)?;
        }
        Ok(out)
    }
}
pub(super) fn record(
    reads: &mut Vec<Target>,
    permission: &Permission,
    b: &mut Budget,
) -> CompileResult<()> {
    for source in &permission.targets {
        fee(b, target_width(source).saturating_add(1))?;
        match source.base {
            HoldRoot::Parameter(_) => {
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
