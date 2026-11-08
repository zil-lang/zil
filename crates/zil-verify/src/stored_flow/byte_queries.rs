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
fn outside() -> Diagnostic {
    unsupported(
        "Byte query requires a complete Direct actual description/read proof",
        None,
    )
}
pub(super) fn inventory(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    if !b.byte_queries {
        return Ok(());
    }
    b.spend(module.functions.len().saturating_mul(2))?;
    let mut queries = vec![false; module.functions.len()];
    loop {
        let mut progressed = false;
        for (index, function) in module.functions.iter().enumerate().rev() {
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
    for (index, function) in module.functions.iter().enumerate() {
        b.spend(1)?;
        let companion = if function.storage_life.is_some() {
            b.spend(function.parameters.len())?;
            function
                .parameters
                .iter()
                .any(|p| is_description(&function.locals[p.0 as usize].ty))
        } else {
            false
        };
        if companion && !queries[index] {
            return Err(outside());
        }
        if queries[index]
            && (function.foreign.is_some()
                || function.environment.is_some()
                || function.call_kind != CallKind::Direct
                || mask.is_some_and(|rows| !rows[index]))
        {
            return Err(outside());
        }
        for block in &function.blocks {
            b.spend(block.instructions.len())?;
            for operation in &block.instructions {
                if let InstructionKind::FunctionRef { function } = operation.kind
                    && queries[function.0 as usize]
                {
                    return Err(outside());
                }
            }
        }
    }
    b.byte_query_functions = queries;
    Ok(())
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
            heap_element: None,
        }],
        loans: Vec::new(),
    })
}
pub(super) struct Flow {
    values: Vec<Vec<Permission>>,
}
impl Flow {
    pub(super) fn solve(
        f: &TypedFunction,
        graph: &FunctionGraph,
        actual: &[Payload],
        b: &mut Budget,
    ) -> Check<Option<Self>> {
        if !b.byte_queries {
            return Ok(None);
        }
        b.spend(1)?;
        if !b.byte_query_functions[f.id.0 as usize] {
            return Ok(None);
        }
        b.spend(f.values.len().saturating_add(f.locals.len()))?;
        let mut values = vec![Vec::new(); f.values.len()];
        let mut entry = vec![Vec::new(); f.locals.len()];
        b.spend(f.parameters.len())?;
        for (position, local) in f.parameters.iter().enumerate() {
            if is_description(&f.locals[local.0 as usize].ty) {
                entry[local.0 as usize].push(seed_origin(
                    HoldRoot::Parameter(position as u32),
                    Vec::new(),
                    b,
                )?);
            }
        }
        b.spend(amount(f.blocks.len(), f.locals.len().saturating_add(1))?)?;
        let mut exits = vec![vec![Vec::new(); f.locals.len()]; f.blocks.len()];
        loop {
            let mut changed = false;
            // Independent reverse CFG batches retain producer alternatives.
            let mut next = Vec::new();
            b.spend(exits.len())?;
            for exit in &exits {
                b.spend(exit.len())?;
                let mut row = Vec::new();
                for p in exit {
                    row.push(clone_alternatives(p, b)?);
                }
                next.push(row);
            }
            for bi in (0..f.blocks.len()).rev() {
                if !graph.reachable[bi] {
                    continue;
                }
                b.spend(f.locals.len())?;
                let mut state = vec![Vec::new(); f.locals.len()];
                if bi == f.entry.0 as usize {
                    for (dst, src) in state.iter_mut().zip(&entry) {
                        extend_alternatives(dst, src, b)?;
                    }
                }
                b.spend(graph.predecessors[bi].len())?;
                for pred in &graph.predecessors[bi] {
                    if graph.reachable[*pred] {
                        b.spend(f.locals.len())?;
                        for (dst, src) in state.iter_mut().zip(&exits[*pred]) {
                            extend_alternatives(dst, src, b)?;
                        }
                    }
                }
                b.spend(f.blocks[bi].instructions.len())?;
                for operation in &f.blocks[bi].instructions {
                    match &operation.kind {
                        InstructionKind::Store { local, value }
                            if is_description(&f.locals[local.0 as usize].ty) =>
                        {
                            state[local.0 as usize] =
                                clone_alternatives(&values[value.0 as usize], b)?
                        }
                        InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local) => state[local.0 as usize].clear(),
                        _ => {}
                    }
                    if let Some(result) = operation.result
                        && is_description(vt(f, result)?)
                    {
                        let mut produced = Vec::new();
                        match &operation.kind {
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                extend_alternatives(&mut produced, &state[local.0 as usize], b)?;
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
                                if !fields.get(*index as usize).is_some_and(|(_,ty)|matches!(ty,Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) {return Err(outside())}
                                produced.push(seed_origin(
                                    HoldRoot::Local(*local),
                                    vec![PlaceProjection::Field(*index)],
                                    b,
                                )?);
                            }
                            InstructionKind::DescribeAddress { address } if matches!(vt(f,*address)?,Type::Address {inner,..} if matches!(inner.as_ref(),Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) =>
                            {
                                let origin = &actual[address.0 as usize].address;
                                if !origin.sources.is_empty() {
                                    produced.push(clone_alternative(origin, b)?);
                                }
                            }
                            _ => return Err(outside()),
                        }
                        changed |=
                            extend_alternatives(&mut values[result.0 as usize], &produced, b)?;
                    }
                }
                b.spend(f.locals.len())?;
                for (dst, src) in next[bi].iter_mut().zip(&state) {
                    changed |= extend_alternatives(dst, src, b)?;
                }
            }
            exits = next;
            if !changed {
                break;
            }
        }
        Ok(Some(Self { values }))
    }
    fn at(&self, value: ValueId) -> &[Permission] {
        &self.values[value.0 as usize]
    }
    pub(super) fn instantiate(
        &self,
        source: &Source,
        callee: &TypedFunction,
        arguments: &[ValueId],
        values: &[Payload],
        state: &Frame,
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
            let alternatives = clone_alternatives(self.at(arguments[position as usize]), b)?;
            if alternatives.is_empty() {
                return Err(err("Byte descriptor input lacks actual weak origins"));
            }
            Ok(alternatives)
        } else if matches!(ty, Type::Address { .. }) {
            let actual = super::instantiate(source, arguments, values, state, b)?;
            if actual.sources.is_empty() {
                return Err(err("Byte separation lacks complete actual address origins"));
            }
            Ok(vec![actual])
        } else {
            Err(outside())
        }
    }
    pub(super) fn reads(
        &self,
        module: &TypedModule,
        operation: &Instruction,
        actual: &[Payload],
        state: &Frame,
        summaries: &[Summary],
        b: &mut Budget,
    ) -> Check<Vec<Permission>> {
        b.spend(1)?;
        if let Some(value) = query(&operation.kind) {
            let alternatives = clone_alternatives(self.at(value), b)?;
            if alternatives.is_empty() {
                return Err(err("Byte query has no actual weak head origin"));
            }
            return Ok(alternatives);
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
        // Only a fixed-point seed. Final coverage is checked independently.
        if !prior.byte_covered {
            return Ok(Vec::new());
        }
        b.spend(prior.byte_reads.len())?;
        let mut alternatives = Vec::new();
        for source in &prior.byte_reads {
            let incoming = self.instantiate(
                source,
                &module.functions[function.0 as usize],
                arguments,
                actual,
                state,
                b,
            )?;
            extend_alternatives(&mut alternatives, &incoming, b)?;
        }
        Ok(alternatives)
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
