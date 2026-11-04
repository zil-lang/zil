//! Actual borrowed-head access restrictions, independent of candidate Hold data.
//! Descriptor inputs are symbolic requirements. Only executable shared-address
//! operations introduce Shared; call/capture substitution cannot erase it.
use super::*;
use std::collections::BTreeMap;
type Path = Vec<u32>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Requirement {
    Shared,
    Parameter(u32, Path),
    Capture(u32, Path),
}
type Row = BTreeSet<Requirement>;
type Value = BTreeMap<Path, Row>;
type State = BTreeMap<LocalId, Value>;
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub(super) result: Value,
    pub(super) writes: Row,
}
fn pay(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "borrowed resource head access proof")
}
fn row_cost(r: &Row, b: &mut Budget) -> CompileResult<()> {
    pay(b, r.len())?;
    for q in r {
        pay(
            b,
            match q {
                Requirement::Shared => 1,
                Requirement::Parameter(_, p) | Requirement::Capture(_, p) => {
                    p.len().saturating_add(1)
                }
            },
        )?;
    }
    Ok(())
}
fn cost(v: &Value, b: &mut Budget) -> CompileResult<()> {
    pay(b, v.len())?;
    for (p, r) in v {
        pay(b, p.len())?;
        row_cost(r, b)?;
    }
    Ok(())
}
fn copy(v: &Value, b: &mut Budget) -> CompileResult<Value> {
    cost(v, b)?;
    Ok(v.clone())
}
fn row_join(t: &mut Row, s: &Row, b: &mut Budget) -> CompileResult<bool> {
    row_cost(s, b)?;
    pay(b, t.len().saturating_add(s.len()))?;
    let n = t.len();
    t.extend(s.iter().cloned());
    Ok(t.len() != n)
}
fn join(t: &mut Value, s: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (p, r) in s {
        pay(b, p.len().saturating_add(t.len()))?;
        changed |= row_join(t.entry(p.clone()).or_default(), r, b)?;
    }
    Ok(changed)
}
fn state_copy(s: &State, b: &mut Budget) -> CompileResult<State> {
    pay(b, s.len())?;
    for v in s.values() {
        cost(v, b)?;
    }
    Ok(s.clone())
}
fn initial(
    module: &TypedModule,
    ty: &Type,
    parameter: Option<u32>,
    capture: Option<u32>,
    b: &mut Budget,
) -> CompileResult<Value> {
    let mut out = Value::new();
    let mut stack = vec![(ty, Path::new())];
    while let Some((ty, path)) = stack.pop() {
        pay(b, path.len().saturating_add(1))?;
        match ty {
            Type::Desc(_) => {
                let q = if let Some(i) = parameter {
                    Requirement::Parameter(i, path.clone())
                } else {
                    Requirement::Capture(capture.expect("capture source"), path.clone())
                };
                out.entry(path).or_default().insert(q);
            }
            Type::Address { mutable, inner } => {
                let q = if let Some(i) = parameter {
                    Requirement::Parameter(i, path.clone())
                } else {
                    Requirement::Capture(capture.expect("capture source"), path.clone())
                };
                out.entry(path.clone()).or_default().insert(q);
                if !mutable {
                    out.entry(path.clone())
                        .or_default()
                        .insert(Requirement::Shared);
                }
                if matches!(inner.as_ref(), Type::Closure(_)) {
                    stack.push((inner, path));
                }
            }
            Type::Closure(id) => {
                let c = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("head access capture definition absent"))?;
                pay(b, c.captures.len())?;
                for (i, t) in c.captures.iter().enumerate() {
                    pay(b, path.len().saturating_add(1))?;
                    let mut p = path.clone();
                    p.push(i as u32);
                    stack.push((t, p));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
fn select(v: &Value, path: &[u32], b: &mut Budget) -> CompileResult<Row> {
    pay(b, path.len().saturating_add(v.len()))?;
    v.get(path).map_or(Ok(Row::new()), |r| {
        row_cost(r, b)?;
        Ok(r.clone())
    })
}
fn substitute(
    q: &Requirement,
    args: &[ValueId],
    environment: Option<&Value>,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Row> {
    match q {
        Requirement::Shared => Ok(Row::from([Requirement::Shared])),
        Requirement::Parameter(i, p) => {
            let id = args
                .get(*i as usize)
                .ok_or_else(|| bad("head access actual input absent"))?;
            select(&values[id.0 as usize], p, b)
        }
        Requirement::Capture(i, p) => {
            let env = environment.ok_or_else(|| bad("head access actual environment absent"))?;
            pay(b, p.len().saturating_add(1))?;
            let mut path = vec![*i];
            path.extend(p);
            select(env, &path, b)
        }
    }
}
fn invoke(
    summary: &Summary,
    args: &[ValueId],
    environment: Option<&Value>,
    values: &[Value],
    effect: &mut Row,
    b: &mut Budget,
) -> CompileResult<Value> {
    let mut out = Value::new();
    cost(&summary.result, b)?;
    row_cost(&summary.writes, b)?;
    for q in &summary.writes {
        row_join(effect, &substitute(q, args, environment, values, b)?, b)?;
    }
    for (p, r) in &summary.result {
        let mut row = Row::new();
        for q in r {
            row_join(&mut row, &substitute(q, args, environment, values, b)?, b)?;
        }
        if !row.is_empty() {
            out.insert(p.clone(), row);
        }
    }
    Ok(out)
}
fn entry(module: &TypedModule, ty: &Type) -> Option<FunctionId> {
    let t = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::Closure(id) = t {
        module.closures.get(id.0 as usize).map(|d| d.function)
    } else {
        None
    }
}
fn scan(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    summaries: &[Summary],
    b: &mut Budget,
) -> CompileResult<Summary> {
    pay(
        b,
        f.blocks
            .len()
            .saturating_mul(2)
            .saturating_add(f.values.len()),
    )?;
    let mut outputs = vec![State::new(); f.blocks.len()];
    let mut values = vec![Value::new(); f.values.len()];
    let mut seed = State::new();
    for (i, l) in f.parameters.iter().enumerate() {
        let v = initial(module, &f.locals[l.0 as usize].ty, Some(i as u32), None, b)?;
        if !v.is_empty() {
            seed.insert(*l, v);
        }
    }
    let mut summary = Summary::default();
    loop {
        let mut changed = false;
        for (block_id, block) in f.blocks.iter().enumerate() {
            pay(b, 1)?;
            if !graph.1[block_id] {
                continue;
            }
            let mut state = if block_id == f.entry.0 as usize {
                state_copy(&seed, b)?
            } else {
                State::new()
            };
            for &p in &graph.0[block_id] {
                if !graph.1[p] {
                    continue;
                }
                pay(b, outputs[p].len())?;
                for (l, v) in &outputs[p] {
                    join(state.entry(*l).or_default(), v, b)?;
                }
            }
            for instruction in &block.instructions {
                pay(b, 1)?;
                let mut result = Value::new();
                match &instruction.kind {
                    InstructionKind::Load(l) | InstructionKind::TakeLocal { local: l } => {
                        if let Some(v) = state.get(l) {
                            result = copy(v, b)?;
                        }
                        if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                            state.remove(l);
                        }
                    }
                    InstructionKind::Store { local, value } => {
                        let v = copy(&values[value.0 as usize], b)?;
                        if v.is_empty() {
                            state.remove(local);
                        } else {
                            state.insert(*local, v);
                        }
                    }
                    InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
                        state.remove(l);
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        if let Some(v) = state.get(local) {
                            result = copy(v, b)?;
                        }
                    }
                    InstructionKind::ByteBufferBorrow {
                        buffer: address, ..
                    }
                    | InstructionKind::OwnedStoreBorrow { store: address, .. }
                    | InstructionKind::OwnedStoreBorrowMut { store: address, .. }
                    | InstructionKind::DescribeAddress { address }
                    | InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        result = copy(&values[address.0 as usize], b)?;
                    }
                    InstructionKind::ReadCapture { index }
                    | InstructionKind::BorrowCapture { index, .. } => {
                        let d = &module.closures[f
                            .environment
                            .ok_or_else(|| bad("head capture outside environment"))?
                            .0 as usize];
                        result =
                            initial(module, &d.captures[*index as usize], None, Some(*index), b)?;
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        for (i, id) in captures.iter().enumerate() {
                            for (p, r) in &values[id.0 as usize] {
                                pay(b, p.len().saturating_add(1))?;
                                let mut path = vec![i as u32];
                                path.extend(p);
                                row_join(result.entry(path).or_default(), r, b)?;
                            }
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        result = invoke(
                            &summaries[function.0 as usize],
                            arguments,
                            None,
                            &values,
                            &mut summary.writes,
                            b,
                        )?;
                    }
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments }
                    | InstructionKind::CallClosureBorrowed {
                        callee, arguments, ..
                    } => {
                        let id = entry(module, &f.values[callee.0 as usize])
                            .ok_or_else(|| bad("head call value is not a closure"))?;
                        result = invoke(
                            &summaries[id.0 as usize],
                            arguments,
                            Some(&values[callee.0 as usize]),
                            &values,
                            &mut summary.writes,
                            b,
                        )?;
                    }
                    InstructionKind::CallClosureMut { callee, arguments }
                    | InstructionKind::CallClosureShared { callee, arguments } => {
                        let id = entry(module, &f.locals[callee.0 as usize].ty)
                            .ok_or_else(|| bad("head call local is not a closure"))?;
                        let empty = Value::new();
                        result = invoke(
                            &summaries[id.0 as usize],
                            arguments,
                            Some(state.get(callee).unwrap_or(&empty)),
                            &values,
                            &mut summary.writes,
                            b,
                        )?;
                    }
                    InstructionKind::CallIndirect { arguments, .. } => {
                        for a in arguments {
                            let v = &values[a.0 as usize];
                            join(&mut result, v, b)?;
                            if matches!(
                                f.values[a.0 as usize],
                                Type::Desc(_) | Type::Address { mutable: true, .. }
                            ) {
                                row_join(&mut summary.writes, &select(v, &[], b)?, b)?;
                            }
                        }
                    }
                    InstructionKind::WithScopedThread { body, .. }
                    | InstructionKind::WithForeignCallback { body, .. } => {
                        let id = entry(module, &f.values[body.0 as usize])
                            .ok_or_else(|| bad("head scoped body is not a closure"))?;
                        result = invoke(
                            &summaries[id.0 as usize],
                            &[],
                            Some(&values[body.0 as usize]),
                            &values,
                            &mut summary.writes,
                            b,
                        )?;
                    }
                    _ => {}
                }
                let access = match &instruction.kind {
                    InstructionKind::WriteDesc { desc, .. } => Some(*desc),
                    InstructionKind::ByteBufferReserve { buffer, .. }
                    | InstructionKind::ByteBufferPush { buffer, .. }
                    | InstructionKind::ByteBufferTruncate { buffer, .. }
                    | InstructionKind::ByteBufferBorrow {
                        buffer,
                        mutable: true,
                    } => Some(*buffer),
                    kind => dynamic_store::access(kind).and_then(|(v, w)| w.then_some(v)),
                };
                if let Some(id) = access {
                    row_join(
                        &mut summary.writes,
                        &select(&values[id.0 as usize], &[], b)?,
                        b,
                    )?;
                }
                if let Some(id) = instruction.result {
                    if matches!(
                        f.values[id.0 as usize],
                        Type::Address { mutable: false, .. }
                    ) {
                        result
                            .entry(vec![])
                            .or_default()
                            .insert(Requirement::Shared);
                    }
                    changed |= join(&mut values[id.0 as usize], &result, b)?;
                }
            }
            if let Terminator::Return(Some(id)) = block.terminator {
                changed |= join(&mut summary.result, &values[id.0 as usize], b)?;
            }
            if outputs[block_id] != state {
                pay(b, outputs[block_id].len().saturating_add(state.len()))?;
                outputs[block_id] = state;
                changed = true;
            }
        }
        if !changed {
            return Ok(summary);
        }
    }
}
pub(super) fn check(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<Summary>> {
    if !b.head_descriptions {
        return Ok(Vec::new());
    }
    pay(b, module.functions.len())?;
    let mut summaries = vec![Summary::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            let next = scan(module, f, &graphs[index], &summaries, b)?;
            changed |= join(&mut summaries[index].result, &next.result, b)?;
            changed |= row_join(&mut summaries[index].writes, &next.writes, b)?;
        }
        if !changed {
            break;
        }
    }
    if summaries
        .iter()
        .any(|s| s.writes.contains(&Requirement::Shared))
    {
        return Err(bad(
            "shared borrowed resource head cannot authorize a write, including through forwarding or captures",
        ));
    }
    Ok(summaries)
}
