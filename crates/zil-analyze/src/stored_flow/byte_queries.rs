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
fn closed() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "Byte query requires a complete Direct actual description/read proof",
        None,
    )]
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(buffer) = operand(&i.kind) else {
        return Ok(false);
    };
    fee(b, module.resources.len().saturating_add(3))?;
    if !description(value_type(f, buffer)?)
        || i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&Type::I64)
    {
        return Err(failure(
            "Byte query requires actual Desc<ByteBuffer> and i64 result",
        ));
    }
    let mut sealed = false;
    for resource in &module.resources {
        fee(b, resource.name.len().saturating_add(19))?;
        sealed |= resource.name == "@builtin:ByteBuffer"
            && resource.payload == Type::ByteBuffer
            && resource.teardown.is_none();
    }
    if !sealed || !module.target.is_supported_x64_llvm() {
        return Err(failure(
            "Byte query requires the actual canonical sealed ByteBuffer resource",
        ));
    }
    Ok(true)
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
    for (index, f) in module.functions.iter().enumerate() {
        fee(b, 1)?;
        if marked[index]
            && (f.foreign.is_some()
                || f.environment.is_some()
                || f.call_kind != CallKind::Direct
                || !b.composition_stored.is_empty() && !b.composition_stored[index])
        {
            return Err(closed());
        }
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                if let InstructionKind::FunctionRef { function } = i.kind
                    && marked[function.0 as usize]
                {
                    return Err(closed());
                }
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

/// Sparse alternative flow is rebuilt from the completed strong pointer solve.
/// Each alternative keeps its own parents even when two targets are equal.
pub(super) struct Flow {
    values: Vec<Vec<Permission>>,
}
impl Flow {
    pub(super) fn solve(
        f: &TypedFunction,
        preds: &[Vec<usize>],
        reachable: &[bool],
        actual: &[Value],
        b: &mut Budget,
    ) -> CompileResult<Option<Self>> {
        if !b.byte_queries {
            return Ok(None);
        }
        fee(b, 1)?;
        if !b.byte_query_functions[f.id.0 as usize] {
            return Ok(None);
        }
        fee(b, f.values.len().saturating_add(f.locals.len()))?;
        let mut values = vec![Vec::new(); f.values.len()];
        let mut entry = vec![Vec::new(); f.locals.len()];
        fee(b, f.parameters.len())?;
        for (position, local) in f.parameters.iter().enumerate() {
            if description(&f.locals[local.0 as usize].ty) {
                entry[local.0 as usize].push(new_origin(
                    HoldRoot::Parameter(position as u32),
                    Vec::new(),
                    b,
                )?);
            }
        }
        b.product(
            f.blocks.len(),
            f.locals.len().saturating_add(1),
            1,
            "Byte weak CFG states",
        )?;
        let mut outputs = vec![vec![Vec::new(); f.locals.len()]; f.blocks.len()];
        loop {
            let mut changed = false;
            for (bi, block) in f.blocks.iter().enumerate() {
                if !reachable[bi] {
                    continue;
                }
                fee(b, f.locals.len())?;
                let mut state = vec![Vec::new(); f.locals.len()];
                if bi == f.entry.0 as usize {
                    for (dst, src) in state.iter_mut().zip(&entry) {
                        add(dst, src, b)?;
                    }
                }
                fee(b, preds[bi].len())?;
                for pred in &preds[bi] {
                    if reachable[*pred] {
                        fee(b, f.locals.len())?;
                        for (dst, src) in state.iter_mut().zip(&outputs[*pred]) {
                            add(dst, src, b)?;
                        }
                    }
                }
                fee(b, block.instructions.len())?;
                for i in &block.instructions {
                    let mut out = Vec::new();
                    match &i.kind {
                        InstructionKind::Store { local, value }
                            if description(&f.locals[local.0 as usize].ty) =>
                        {
                            state[local.0 as usize] = copy_origins(&values[value.0 as usize], b)?;
                        }
                        InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local) => state[local.0 as usize].clear(),
                        _ => {}
                    }
                    if let Some(result) = i.result
                        && description(value_type(f, result)?)
                    {
                        match &i.kind {
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                add(&mut out, &state[local.0 as usize], b)?;
                            }
                            InstructionKind::DescribeLocal { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if p.as_ref()==&Type::ByteBuffer) =>
                            {
                                out.push(new_origin(HoldRoot::Local(*local), Vec::new(), b)?);
                            }
                            InstructionKind::DescribePackageField { local, index } => {
                                let Type::Package { fields, .. } =
                                    super::super::resources::package_storage(
                                        &f.locals[local.0 as usize].ty,
                                    )
                                else {
                                    return Err(closed());
                                };
                                fee(b, 2)?;
                                if !fields.get(*index as usize).is_some_and(|(_, ty)| matches!(ty, Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) { return Err(closed()) }
                                out.push(new_origin(
                                    HoldRoot::Local(*local),
                                    vec![PlaceProjection::Field(*index)],
                                    b,
                                )?);
                            }
                            InstructionKind::DescribeAddress { address } if matches!(value_type(f, *address)?, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(p) if p.as_ref()==&Type::ByteBuffer)) => {
                                if !actual[address.0 as usize].pointer.targets.is_empty() {
                                    out.push(copy_permission(
                                        &actual[address.0 as usize].pointer,
                                        b,
                                    )?);
                                }
                            }
                            _ => return Err(closed()),
                        }
                        changed |= add(&mut values[result.0 as usize], &out, b)?;
                    }
                }
                fee(b, f.locals.len())?;
                for (dst, src) in outputs[bi].iter_mut().zip(&state) {
                    changed |= add(dst, src, b)?;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(Some(Self { values }))
    }
    pub(super) fn actual(&self, value: ValueId) -> &[Permission] {
        &self.values[value.0 as usize]
    }
    pub(super) fn substitute(
        &self,
        source: &Target,
        callee: &TypedFunction,
        arguments: &[ValueId],
        values: &[Value],
        state: &State,
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
            let out = copy_origins(self.actual(arguments[position as usize]), b)?;
            if out.is_empty() {
                return Err(failure("Byte descriptor input lacks actual weak origins"));
            }
            Ok(out)
        } else if matches!(ty, Type::Address { .. }) {
            Ok(vec![super::substitute(
                source, arguments, values, state, b,
            )?])
        } else {
            Err(closed())
        }
    }
    pub(super) fn reads(
        &self,
        module: &TypedModule,
        i: &Instruction,
        values: &[Value],
        state: &State,
        summaries: &[Facts],
        b: &mut Budget,
    ) -> CompileResult<Vec<Permission>> {
        if let Some(value) = operand(&i.kind) {
            let out = copy_origins(self.actual(value), b)?;
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
        // This is an iteration seed, never a final proof of an empty effect.
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
