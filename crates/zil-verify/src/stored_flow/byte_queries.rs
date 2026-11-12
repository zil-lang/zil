//! Independent weak ByteBuffer provenance. Sorted source permissions remain
//! correlated with their producer's parents; alternatives never share ancestry.
use super::*;

pub(super) fn is_description(ty: &Type) -> bool {
    matches!(ty, Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer)
}
pub(super) fn query(kind: &InstructionKind) -> Option<ValueId> {
    match kind {
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer } => Some(*buffer),
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

fn outside() -> Diagnostic {
    unsupported(
        "Byte query requires a complete Direct actual description/read proof",
        None,
    )
}
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
) -> Check<bool> {
    b.spend(8)?;
    let from = module.functions.get(caller).ok_or_else(outside)?;
    let to = module.functions.get(callee).ok_or_else(outside)?;
    Ok(from.call_kind == CallKind::Direct
        && from.environment.is_none()
        && from.foreign.is_none()
        && from.callable_contract.is_none()
        && from.callable_return_contract.is_none()
        && to.call_kind == CallKind::Direct
        && to.environment.is_none()
        && to.foreign.is_none()
        && to.callable_contract.is_none()
        && to.callable_return_contract.is_none()
        && to.parameters.is_empty()
        && to.return_type.is_b0_scalar()
        && to.result_mode == PassingMode::Infer)
}
fn matches_edge(module: &TypedModule, edge: &PendingEdge, b: &mut Budget) -> Check<bool> {
    b.spend(4)?;
    if !zero_input_scalar(module, edge.caller, edge.callee, b)? {
        return Ok(false);
    }
    let caller = module.functions.get(edge.caller).ok_or_else(outside)?;
    let operation = caller
        .blocks
        .get(edge.block)
        .and_then(|block| block.instructions.get(edge.instruction))
        .ok_or_else(outside)?;
    let InstructionKind::Call {
        function,
        arguments,
    } = &operation.kind
    else {
        return Ok(false);
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
    graphs: &[FunctionGraph],
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<Vec<PendingEdge>> {
    if !b.byte_queries {
        return Ok(Vec::new());
    }
    b.spend(module.functions.len())?;
    let mut queries = vec![false; module.functions.len()];
    let mut pending = Vec::new();
    loop {
        let mut progressed = false;
        b.spend(module.functions.len())?;
        for (index, function) in module.functions.iter().enumerate().rev() {
            b.spend(function.blocks.len())?;
            for (block, live) in function.blocks.iter().zip(&graphs[index].reachable) {
                if !live {
                    continue;
                }
                b.spend(block.instructions.len())?;
                for operation in &block.instructions {
                    let found = query(&operation.kind).is_some()
                        || matches!(operation.kind, InstructionKind::Call { function, .. } if queries[function.0 as usize]);
                    if found && !queries[index] {
                        queries[index] = true;
                        progressed = true;
                    }
                }
            }
        }
        if !progressed {
            break;
        }
    }
    let covered = |index: usize| mask.is_none_or(|rows| rows[index]);
    if mask.is_some() {
        b.spend(module.functions.len())?;
        for (caller, function) in module.functions.iter().enumerate() {
            b.spend(function.blocks.len())?;
            for (block_index, (block, live)) in function
                .blocks
                .iter()
                .zip(&graphs[caller].reachable)
                .enumerate()
            {
                if !live {
                    continue;
                }
                b.spend(block.instructions.len())?;
                for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                    if let InstructionKind::Call { function, .. } = instruction.kind
                        && queries[function.0 as usize]
                        && covered(caller) != covered(function.0 as usize)
                    {
                        let edge = PendingEdge {
                            caller,
                            block: block_index,
                            instruction: instruction_index,
                            callee: function.0 as usize,
                        };
                        if covered(caller)
                            || !covered(function.0 as usize)
                            || !matches_edge(module, &edge, b)?
                        {
                            return Err(outside());
                        }
                        b.spend(4)?;
                        pending.push(edge);
                    }
                }
            }
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        b.spend(1)?;
        let companion = if function.storage_life.is_some() && !legacy_task_signature(function) {
            b.spend(function.parameters.len())?;
            function
                .parameters
                .iter()
                .any(|p| is_description(&function.locals[p.0 as usize].ty))
        } else {
            false
        };
        if companion && covered(index) && !queries[index] {
            return Err(outside());
        }
        if queries[index]
            && covered(index)
            && (function.foreign.is_some()
                || function.environment.is_some()
                || function.call_kind != CallKind::Direct)
        {
            return Err(outside());
        }
        b.spend(function.blocks.len())?;
        for block in &function.blocks {
            b.spend(block.instructions.len())?;
            for operation in &block.instructions {
                if let InstructionKind::FunctionRef { function } = operation.kind
                    && queries[function.0 as usize]
                    && (covered(index) || covered(function.0 as usize) || b.store_option_results)
                {
                    return Err(outside());
                }
                if covered(index) {
                    if let Some(arguments) = indirect_arguments(&operation.kind) {
                        b.spend(arguments.len())?;
                        if arguments
                            .iter()
                            .any(|id| is_description(&function.values[id.0 as usize]))
                        {
                            return Err(outside());
                        }
                    }
                    if let InstructionKind::MakeClosure { closure, .. } = operation.kind {
                        b.spend(1)?;
                        let entry = module
                            .closures
                            .get(closure.0 as usize)
                            .ok_or_else(outside)?
                            .function;
                        if queries[entry.0 as usize] {
                            return Err(outside());
                        }
                    }
                }
            }
        }
    }
    b.spend(module.dyn_vtables.len())?;
    for table in &module.dyn_vtables {
        b.spend(table.entries.len().saturating_add(1))?;
        for entry in table.entries.iter().chain(table.cleanup.iter()) {
            let index = entry.function.0 as usize;
            if queries.get(index).copied().ok_or_else(outside)? && covered(index) {
                return Err(outside());
            }
        }
    }
    b.byte_query_functions = queries;
    Ok(pending)
}
fn clone_alternative(p: &Permission, b: &mut Budget) -> Check<Permission> {
    permission_work(p, b)?;
    b.spend(1)?;
    Ok(p.clone())
}
fn extend_alternatives(
    dst: &mut Vec<Permission>,
    src: &[Permission],
    b: &mut Budget,
) -> Check<bool> {
    b.spend(src.len())?;
    let mut changed = false;
    for incoming in src {
        let mut duplicate = false;
        for prior in dst.iter() {
            permission_work(prior, b)?;
            permission_work(incoming, b)?;
            b.spend(1)?;
            if prior == incoming {
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            dst.push(clone_alternative(incoming, b)?);
            changed = true;
        }
    }
    Ok(changed)
}
fn clone_alternatives(src: &[Permission], b: &mut Budget) -> Check<Vec<Permission>> {
    let mut dst = Vec::new();
    extend_alternatives(&mut dst, src, b)?;
    Ok(dst)
}
fn seed_origin(base: HoldRoot, inline: Vec<PlaceProjection>, b: &mut Budget) -> Check<Permission> {
    b.spend(inline.len().saturating_add(2))?;
    Ok(Permission {
        sources: vec![Source {
            retained_port: None,
            base,
            leaf: Vec::new(),
            inline,
            heap_parents: Vec::new(),
            heap_element: None,
        }],
        loans: Vec::new(),
    })
}
/// Sorted sparse rows use SSA indices first and local indices after them.
/// Every saved permission has a closed set of current-generation grants.
#[derive(Default, PartialEq, Eq)]
pub(super) struct Point {
    rows: Vec<(usize, Vec<Permission>)>,
}
fn tracked(ty: &Type) -> bool {
    is_description(ty) || matches!(ty, Type::Address { .. })
}
fn point_work(p: &Point, b: &mut Budget) -> Check<()> {
    b.spend(p.rows.len())?;
    for (_, row) in &p.rows {
        b.spend(row.len())?;
        for permission in row {
            permission_work(permission, b)?;
        }
    }
    Ok(())
}
impl Point {
    fn at(&self, index: usize, b: &mut Budget) -> Check<&[Permission]> {
        b.spend(self.rows.len().saturating_add(1))?;
        Ok(self
            .rows
            .iter()
            .find(|(key, _)| *key == index)
            .map_or(&[], |(_, row)| row.as_slice()))
    }
    fn put(&mut self, index: usize, row: Vec<Permission>, b: &mut Budget) -> Check<()> {
        b.spend(self.rows.len().saturating_add(1))?;
        match self.rows.binary_search_by_key(&index, |(key, _)| *key) {
            Ok(position) => self.rows[position].1 = row,
            Err(position) => self.rows.insert(position, (index, row)),
        }
        Ok(())
    }
    fn remove(&mut self, index: usize, b: &mut Budget) -> Check<()> {
        b.spend(self.rows.len().saturating_add(1))?;
        if let Ok(position) = self.rows.binary_search_by_key(&index, |(key, _)| *key) {
            self.rows.remove(position);
        }
        Ok(())
    }
    fn revoke(&mut self, id: usize, b: &mut Budget) -> Check<()> {
        point_work(self, b)?;
        for (_, row) in &mut self.rows {
            for permission in row.iter_mut() {
                b.spend(permission.loans.len())?;
                permission.loans.retain(|loan| *loan != id);
            }
            *row = clone_alternatives(row, b)?;
        }
        Ok(())
    }
}
fn join_point(to: &mut Point, from: &Point, b: &mut Budget) -> Check<()> {
    b.spend(from.rows.len())?;
    for (key, row) in &from.rows {
        let mut merged = clone_alternatives(to.at(*key, b)?, b)?;
        extend_alternatives(&mut merged, row, b)?;
        to.put(*key, merged, b)?;
    }
    Ok(())
}
fn retarget(origins: &[Permission], sources: &[Source], b: &mut Budget) -> Check<Vec<Permission>> {
    b.spend(origins.len())?;
    let mut out = Vec::new();
    for origin in origins {
        let mut permission = Permission::default();
        add_sources(&mut permission.sources, sources, b)?;
        add_loans(&mut permission.loans, &origin.loans, b)?;
        extend_alternatives(&mut out, &[permission], b)?;
    }
    Ok(out)
}
fn advance(
    module: &TypedModule,
    f: &TypedFunction,
    operation: &Instruction,
    actual: &[Payload],
    summaries: &[Summary],
    point: &mut Point,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    let result = operation
        .result
        .filter(|id| tracked(&f.values[id.0 as usize]));
    let fresh = if let Some(result) = result {
        let p = &actual[result.0 as usize].address;
        b.spend(p.loans.len().saturating_add(1))?;
        matches!(vt(f, result)?, Type::Address { .. }) && p.loans.contains(&(result.0 as usize))
    } else {
        false
    };
    if fresh {
        point.revoke(result.unwrap().0 as usize, b)?;
    }
    match operation.kind {
        InstructionKind::Store { local, value } if tracked(&f.locals[local.0 as usize].ty) => {
            let row = clone_alternatives(point.at(value.0 as usize, b)?, b)?;
            point.put(f.values.len() + local.0 as usize, row, b)?;
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            point.remove(f.values.len() + local.0 as usize, b)?
        }
        _ => {}
    }
    let Some(result) = result else { return Ok(()) };
    let mut produced = Vec::new();
    if is_description(vt(f, result)?) {
        match &operation.kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                produced = clone_alternatives(point.at(f.values.len() + local.0 as usize, b)?, b)?;
            }
            InstructionKind::DescribeLocal { local } if matches!(&f.locals[local.0 as usize].ty,Type::Managed(p) if p.as_ref()==&Type::ByteBuffer) => {
                produced.push(seed_origin(HoldRoot::Local(*local), Vec::new(), b)?)
            }
            InstructionKind::DescribePackageField { local, index } => {
                let ty = &f.locals[local.0 as usize].ty;
                let physical = if let Type::BorrowedStorage { storage, .. } = ty {
                    storage.as_ref()
                } else {
                    ty
                };
                let Type::Package { fields, .. } = physical else {
                    return Err(outside());
                };
                b.spend(2)?;
                if !fields.get(*index as usize).is_some_and(
                    |(_, ty)| matches!(ty,Type::Managed(p) if p.as_ref()==&Type::ByteBuffer),
                ) {
                    return Err(outside());
                }
                produced.push(seed_origin(
                    HoldRoot::Local(*local),
                    vec![PlaceProjection::Field(*index)],
                    b,
                )?);
            }
            InstructionKind::DescribeAddress { address } if matches!(vt(f,*address)?,Type::Address {inner,..} if matches!(inner.as_ref(),Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) =>
            {
                produced = clone_alternatives(point.at(address.0 as usize, b)?, b)?;
            }
            _ => return Err(outside()),
        }
    } else {
        let sources = &actual[result.0 as usize].address.sources;
        if !sources.is_empty() {
            match &operation.kind {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    produced =
                        retarget(point.at(f.values.len() + local.0 as usize, b)?, sources, b)?
                }
                InstructionKind::ReborrowAddress { address, .. }
                | InstructionKind::AddressField { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. } => {
                    produced = retarget(point.at(address.0 as usize, b)?, sources, b)?
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let summary = &summaries[function.0 as usize];
                    if summary.complete {
                        b.spend(summary.result.len())?;
                        if let Some((_, roots)) =
                            summary.result.iter().find(|(path, _)| path.is_empty())
                        {
                            b.spend(roots.len())?;
                            for source in roots {
                                b.spend(width(source).saturating_add(1))?;
                                if let HoldRoot::Parameter(position) = source.base
                                    && source.retained_port.is_none()
                                    && source.leaf.is_empty()
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
                                        point.at(arguments[position as usize].0 as usize, b)?,
                                        sources,
                                        b,
                                    )?;
                                    extend_alternatives(&mut produced, &inherited, b)?;
                                } else {
                                    let mut p = Permission::default();
                                    add_sources(&mut p.sources, sources, b)?;
                                    extend_alternatives(&mut produced, &[p], b)?;
                                }
                            }
                        }
                    }
                }
                _ => {
                    let mut p = Permission::default();
                    add_sources(&mut p.sources, sources, b)?;
                    produced.push(p);
                }
            }
            if fresh {
                b.spend(produced.len())?;
                for p in &mut produced {
                    add_loans(&mut p.loans, &[result.0 as usize], b)?;
                }
            }
        }
    }
    point.put(result.0 as usize, produced, b)
}
pub(super) struct Flow {
    initial: Point,
    exits: Vec<Point>,
}
impl Flow {
    pub(super) fn solve(
        module: &TypedModule,
        f: &TypedFunction,
        graph: &FunctionGraph,
        actual_entry: &Frame,
        actual: &[Payload],
        summaries: &[Summary],
        b: &mut Budget,
    ) -> Check<Option<Self>> {
        if !b.byte_queries {
            return Ok(None);
        }
        b.spend(1)?;
        if !b.byte_query_functions[f.id.0 as usize] {
            return Ok(None);
        }
        let mut initial = Point::default();
        b.spend(f.parameters.len())?;
        for (position, local) in f.parameters.iter().enumerate() {
            let ty = &f.locals[local.0 as usize].ty;
            let row = if is_description(ty) {
                vec![seed_origin(
                    HoldRoot::Parameter(position as u32),
                    Vec::new(),
                    b,
                )?]
            } else if matches!(ty, Type::Address { .. }) {
                vec![clone_alternative(
                    &actual_entry.locals[local.0 as usize].address,
                    b,
                )?]
            } else {
                continue;
            };
            initial.put(f.values.len() + local.0 as usize, row, b)?;
        }
        b.spend(f.blocks.len())?;
        let mut flow = Self {
            initial,
            exits: (0..f.blocks.len()).map(|_| Point::default()).collect(),
        };
        // In this new domain a DAG has final predecessor point states before
        // each instruction. Cycles retain the complete original fixed point,
        // including every fresh execution's full grant revocation.
        if b.ordinary_error_places
            && b.store_option_results
            && let Some(order) = super::acyclic_order(graph, b)?
        {
            for block in order {
                let mut point = flow.input(f, block, graph, b)?;
                b.spend(f.blocks[block].instructions.len())?;
                for operation in &f.blocks[block].instructions {
                    advance(module, f, operation, actual, summaries, &mut point, b)?;
                }
                flow.exits[block] = point;
            }
            return Ok(Some(flow));
        }
        loop {
            b.spend(flow.exits.len())?;
            let mut next = Vec::new();
            for exit in &flow.exits {
                let mut copy = Point::default();
                join_point(&mut copy, exit, b)?;
                next.push(copy);
            }
            let mut changed = false;
            for bi in (0..f.blocks.len()).rev() {
                if !graph.reachable[bi] {
                    continue;
                }
                let mut point = flow.input(f, bi, graph, b)?;
                b.spend(f.blocks[bi].instructions.len())?;
                for operation in &f.blocks[bi].instructions {
                    advance(module, f, operation, actual, summaries, &mut point, b)?;
                }
                point_work(&point, b)?;
                point_work(&flow.exits[bi], b)?;
                if point != flow.exits[bi] {
                    next[bi] = point;
                    changed = true;
                }
            }
            flow.exits = next;
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
        graph: &FunctionGraph,
        b: &mut Budget,
    ) -> Check<Point> {
        let mut point = Point::default();
        if bi == f.entry.0 as usize {
            join_point(&mut point, &self.initial, b)?;
        }
        b.spend(graph.predecessors[bi].len())?;
        for predecessor in &graph.predecessors[bi] {
            if graph.reachable[*predecessor] {
                join_point(&mut point, &self.exits[*predecessor], b)?;
            }
        }
        Ok(point)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        operation: &Instruction,
        actual: &[Payload],
        summaries: &[Summary],
        point: &mut Point,
        b: &mut Budget,
    ) -> Check<()> {
        advance(module, f, operation, actual, summaries, point, b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate(
        &self,
        module: &TypedModule,
        caller: &TypedFunction,
        source: &Source,
        callee: &TypedFunction,
        arguments: &[ValueId],
        values: &[Payload],
        state: &Frame,
        point: &Point,
        b: &mut Budget,
    ) -> Check<Vec<Permission>> {
        let HoldRoot::Parameter(position) = source.base else {
            return Err(outside());
        };
        b.spend(width(source).saturating_add(2))?;
        let ty = &callee.locals[callee.parameters[position as usize].0 as usize].ty;
        if is_description(ty) {
            if source.retained_port.is_some()
                || !source.leaf.is_empty()
                || !source.inline.is_empty()
                || source.heap_element.is_some()
            {
                return Err(outside());
            }
            let row = clone_alternatives(point.at(arguments[position as usize].0 as usize, b)?, b)?;
            if row.is_empty() {
                return Err(err("Byte descriptor input lacks actual weak origins"));
            }
            Ok(row)
        } else if matches!(ty, Type::Address { .. }) {
            let actual = super::instantiate(module, caller, source, arguments, values, state, b)?;
            if actual.sources.is_empty() {
                return Err(err("Byte separation lacks complete actual address origins"));
            }
            let row = if source.retained_port.is_none()
                && source.leaf.is_empty()
                && source.heap_element.is_none()
            {
                retarget(
                    point.at(arguments[position as usize].0 as usize, b)?,
                    &actual.sources,
                    b,
                )?
            } else {
                vec![Permission {
                    sources: actual.sources,
                    loans: Vec::new(),
                }]
            };
            if row.is_empty() {
                return Err(err("Byte address input lacks current delegation proof"));
            }
            Ok(row)
        } else {
            Err(outside())
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reads(
        &self,
        module: &TypedModule,
        caller: &TypedFunction,
        operation: &Instruction,
        actual: &[Payload],
        state: &Frame,
        point: &Point,
        summaries: &[Summary],
        b: &mut Budget,
    ) -> Check<Vec<Permission>> {
        b.spend(1)?;
        if let Some(value) = query(&operation.kind) {
            let row = clone_alternatives(point.at(value.0 as usize, b)?, b)?;
            if row.is_empty() {
                return Err(err("Byte query has no actual weak head origin"));
            }
            return Ok(row);
        }
        let InstructionKind::Call {
            function,
            arguments,
        } = &operation.kind
        else {
            return Ok(Vec::new());
        };
        b.spend(1)?;
        if !b.byte_query_functions[function.0 as usize] {
            return Ok(Vec::new());
        }
        let prior = &summaries[function.0 as usize];
        if !prior.byte_covered {
            return Ok(Vec::new());
        }
        b.spend(prior.byte_reads.len())?;
        let mut out = Vec::new();
        for source in &prior.byte_reads {
            let row = self.instantiate(
                module,
                caller,
                source,
                &module.functions[function.0 as usize],
                arguments,
                actual,
                state,
                point,
                b,
            )?;
            extend_alternatives(&mut out, &row, b)?;
        }
        Ok(out)
    }
}
pub(super) fn record(out: &mut Vec<Source>, permission: &Permission, b: &mut Budget) -> Check<()> {
    b.spend(permission.sources.len())?;
    for source in &permission.sources {
        match source.base {
            HoldRoot::Parameter(_) => {
                add_sources(out, std::slice::from_ref(source), b)?;
            }
            HoldRoot::Local(_) => {}
            _ => return Err(outside()),
        }
    }
    Ok(())
}

/// The early rich-signature checker runs before instruction admission. Only an
/// exact newly admitted companion pays this scan; later proofs grant authority.
pub(super) fn signature_presence(module: &TypedModule, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    if b.byte_queries || b.byte_query_presence_checked {
        return Ok(b.byte_queries);
    }
    b.spend(module.functions.len())?;
    for function in &module.functions {
        b.spend(function.blocks.len())?;
        for block in &function.blocks {
            b.spend(block.instructions.len())?;
            if block.instructions.iter().any(|i| query(&i.kind).is_some()) {
                b.byte_queries = true;
                b.byte_query_presence_checked = true;
                return Ok(true);
            }
        }
    }
    b.byte_query_presence_checked = true;
    Ok(false)
}

pub(super) fn certify_edges(
    module: &TypedModule,
    pending: &[PendingEdge],
    summaries: &[Summary],
    solved: &[Option<Solution>],
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    if pending.is_empty() {
        return Ok(());
    }
    b.spend(pending.len())?;
    let rows = mask.ok_or_else(outside)?;
    for edge in pending {
        b.spend(5)?;
        if rows.get(edge.caller).copied() != Some(false)
            || rows.get(edge.callee).copied() != Some(true)
            || solved.get(edge.callee).is_none_or(Option::is_none)
            || !matches_edge(module, edge, b)?
        {
            return Err(outside());
        }
        let proof = summaries.get(edge.callee).ok_or_else(outside)?;
        if !proof.complete || !proof.byte_covered || !proof.byte_reads.is_empty() {
            return Err(outside());
        }
    }
    Ok(())
}
