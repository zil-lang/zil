//! Kernel-local descriptor access proof. No producer facts or permission labels
//! are imported: actual address modes seed restrictions and executable calls
//! substitute independently rebuilt per-position requirements.
use super::*;
type CapturePath = Vec<u32>;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Obligation {
    ReadOnly,
    Input(u32, CapturePath),
    Environment(u32, CapturePath),
}
type Constraints = Vec<Obligation>;
type Rows = Vec<(CapturePath, Constraints)>;
#[derive(Clone, Default, PartialEq, Eq)]
struct Contract {
    returned: Rows,
    writes: Constraints,
}
fn charge(b: &mut Budget, n: usize) -> Check<()> {
    b.spend(n)
}
fn constraints_cost(v: &Constraints, b: &mut Budget) -> Check<()> {
    charge(b, v.len())?;
    for q in v {
        charge(
            b,
            match q {
                Obligation::ReadOnly => 1,
                Obligation::Input(_, p) | Obligation::Environment(_, p) => {
                    p.len().saturating_add(1)
                }
            },
        )?;
    }
    Ok(())
}
fn rows_cost(v: &Rows, b: &mut Budget) -> Check<()> {
    charge(b, v.len())?;
    for (p, c) in v {
        charge(b, p.len())?;
        constraints_cost(c, b)?;
    }
    Ok(())
}
fn duplicate(v: &Rows, b: &mut Budget) -> Check<Rows> {
    rows_cost(v, b)?;
    Ok(v.clone())
}
fn merge_constraints(to: &mut Constraints, from: &Constraints, b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for q in from {
        constraints_cost(to, b)?;
        charge(
            b,
            match q {
                Obligation::ReadOnly => 1,
                Obligation::Input(_, path) | Obligation::Environment(_, path) => {
                    path.len().saturating_add(1)
                }
            },
        )?;
        if !to.contains(q) {
            to.push(q.clone());
            changed = true;
        }
    }
    Ok(changed)
}
fn merge(to: &mut Rows, from: &Rows, b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for (p, c) in from {
        rows_cost(to, b)?;
        charge(b, p.len())?;
        if let Some((_, row)) = to.iter_mut().find(|(path, _)| path == p) {
            changed |= merge_constraints(row, c, b)?;
        } else {
            constraints_cost(c, b)?;
            to.push((p.clone(), c.clone()));
            changed = true;
        }
    }
    Ok(changed)
}
fn at(v: &Rows, p: &[u32], b: &mut Budget) -> Check<Constraints> {
    rows_cost(v, b)?;
    charge(b, p.len())?;
    if let Some((_, c)) = v.iter().find(|(path, _)| path == p) {
        constraints_cost(c, b)?;
        Ok(c.clone())
    } else {
        Ok(vec![])
    }
}
fn seed(
    module: &TypedModule,
    ty: &Type,
    input: Option<u32>,
    environment: Option<u32>,
    b: &mut Budget,
) -> Check<Rows> {
    let mut result = vec![];
    let mut pending = vec![(ty, vec![])];
    while let Some((ty, path)) = pending.pop() {
        charge(b, path.len().saturating_add(1))?;
        match ty {
            Type::Desc(_) => {
                let term = if let Some(i) = input {
                    Obligation::Input(i, path.clone())
                } else {
                    Obligation::Environment(
                        environment.ok_or_else(|| {
                            reject("E-VERIFY-HEAD-ACCESS", None, "missing capture position")
                        })?,
                        path.clone(),
                    )
                };
                merge(&mut result, &vec![(path, vec![term])], b)?;
            }
            Type::Address { mutable, inner } => {
                let term = if let Some(i) = input {
                    Obligation::Input(i, path.clone())
                } else {
                    Obligation::Environment(
                        environment.ok_or_else(|| {
                            reject("E-VERIFY-HEAD-ACCESS", None, "missing capture position")
                        })?,
                        path.clone(),
                    )
                };
                merge(&mut result, &vec![(path.clone(), vec![term])], b)?;
                if !mutable {
                    merge(
                        &mut result,
                        &vec![(path.clone(), vec![Obligation::ReadOnly])],
                        b,
                    )?;
                }
                if matches!(inner.as_ref(), Type::Closure(_)) {
                    pending.push((inner, path));
                }
            }
            Type::Closure(id) => {
                let definition = module.closures.get(id.0 as usize).ok_or_else(|| {
                    reject("E-VERIFY-HEAD-ACCESS", None, "missing closure definition")
                })?;
                charge(b, definition.captures.len())?;
                for (i, t) in definition.captures.iter().enumerate() {
                    charge(b, path.len().saturating_add(1))?;
                    let mut child = path.clone();
                    child.push(i as u32);
                    pending.push((t, child));
                }
            }
            _ => {}
        }
    }
    Ok(result)
}
fn replace(
    q: &Obligation,
    args: &[ValueId],
    env: Option<&Rows>,
    values: &[Rows],
    b: &mut Budget,
) -> Check<Constraints> {
    match q {
        Obligation::ReadOnly => Ok(vec![Obligation::ReadOnly]),
        Obligation::Input(n, p) => {
            let id = args.get(*n as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-HEAD-ACCESS",
                    None,
                    "callee input outside actual arguments",
                )
            })?;
            at(&values[id.0 as usize], p, b)
        }
        Obligation::Environment(n, p) => {
            let rows = env.ok_or_else(|| {
                reject("E-VERIFY-HEAD-ACCESS", None, "callee environment is absent")
            })?;
            charge(b, p.len().saturating_add(1))?;
            let mut path = vec![*n];
            path.extend(p);
            at(rows, &path, b)
        }
    }
}
fn apply(
    c: &Contract,
    args: &[ValueId],
    env: Option<&Rows>,
    values: &[Rows],
    written: &mut Constraints,
    b: &mut Budget,
) -> Check<Rows> {
    let mut output = vec![];
    rows_cost(&c.returned, b)?;
    constraints_cost(&c.writes, b)?;
    for q in &c.writes {
        merge_constraints(written, &replace(q, args, env, values, b)?, b)?;
    }
    for (path, row) in &c.returned {
        let mut terms = vec![];
        for q in row {
            merge_constraints(&mut terms, &replace(q, args, env, values, b)?, b)?;
        }
        if !terms.is_empty() {
            charge(b, path.len())?;
            output.push((path.clone(), terms));
        }
    }
    Ok(output)
}
fn closure_entry(module: &TypedModule, ty: &Type) -> Check<FunctionId> {
    let inner = match ty {
        Type::Address { inner, .. } => inner.as_ref(),
        other => other,
    };
    let Type::Closure(id) = inner else {
        return Err(reject(
            "E-VERIFY-HEAD-ACCESS",
            None,
            "invocation has no actual closure",
        ));
    };
    module
        .closures
        .get(id.0 as usize)
        .map(|d| d.function)
        .ok_or_else(|| {
            reject(
                "E-VERIFY-HEAD-ACCESS",
                None,
                "invocation closure definition absent",
            )
        })
}
fn body(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    contracts: &[Contract],
    b: &mut Budget,
) -> Check<Contract> {
    charge(
        b,
        f.blocks
            .len()
            .saturating_mul(f.locals.len())
            .saturating_add(f.values.len()),
    )?;
    let mut values = vec![Rows::new(); f.values.len()];
    let mut outputs = vec![vec![Rows::new(); f.locals.len()]; f.blocks.len()];
    let mut initial = vec![Rows::new(); f.locals.len()];
    for (index, l) in f.parameters.iter().enumerate() {
        initial[l.0 as usize] = seed(
            module,
            &f.locals[l.0 as usize].ty,
            Some(index as u32),
            None,
            b,
        )?;
    }
    let mut result = Contract::default();
    loop {
        let mut progressed = false;
        for block_id in (0..f.blocks.len()).rev() {
            charge(b, 1)?;
            if !graph.reachable[block_id] {
                continue;
            }
            charge(b, f.locals.len())?;
            let mut locals = vec![Rows::new(); f.locals.len()];
            if block_id == f.entry.0 as usize {
                for (index, row) in initial.iter().enumerate() {
                    locals[index] = duplicate(row, b)?;
                }
            }
            for &pred in &graph.predecessors[block_id] {
                if graph.reachable[pred] {
                    for (index, row) in outputs[pred].iter().enumerate() {
                        merge(&mut locals[index], row, b)?;
                    }
                }
            }
            let block = &f.blocks[block_id];
            for operation in &block.instructions {
                charge(b, 1)?;
                let mut generated = Rows::new();
                match &operation.kind {
                    InstructionKind::Store { local, value } => {
                        locals[local.0 as usize] = duplicate(&values[value.0 as usize], b)?;
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        generated = duplicate(&locals[local.0 as usize], b)?;
                        if matches!(operation.kind, InstructionKind::TakeLocal { .. }) {
                            locals[local.0 as usize].clear();
                        }
                    }
                    InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                        locals[local.0 as usize].clear()
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        generated = duplicate(&locals[local.0 as usize], b)?;
                    }
                    InstructionKind::ByteBufferBorrow {
                        buffer: address, ..
                    }
                    | InstructionKind::OwnedStoreBorrow { store: address, .. }
                    | InstructionKind::OwnedStoreBorrowMut { store: address, .. }
                    | InstructionKind::DescribeAddress { address }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::AddressIndex { address, .. } => {
                        generated = duplicate(&values[address.0 as usize], b)?;
                    }
                    InstructionKind::BorrowCapture { index, .. }
                    | InstructionKind::ReadCapture { index } => {
                        let id = f.environment.ok_or_else(|| {
                            reject(
                                "E-VERIFY-HEAD-ACCESS",
                                Some(operation.span),
                                "capture without an environment",
                            )
                        })?;
                        generated = seed(
                            module,
                            &module.closures[id.0 as usize].captures[*index as usize],
                            None,
                            Some(*index),
                            b,
                        )?;
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        for (index, id) in captures.iter().enumerate() {
                            for (path, terms) in &values[id.0 as usize] {
                                charge(b, path.len().saturating_add(1))?;
                                let mut child = vec![index as u32];
                                child.extend(path);
                                constraints_cost(terms, b)?;
                                merge(&mut generated, &vec![(child, terms.clone())], b)?;
                            }
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        generated = apply(
                            &contracts[function.0 as usize],
                            arguments,
                            None,
                            &values,
                            &mut result.writes,
                            b,
                        )?
                    }
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments }
                    | InstructionKind::CallClosureBorrowed {
                        callee, arguments, ..
                    } => {
                        let target = closure_entry(module, &f.values[callee.0 as usize])?;
                        generated = apply(
                            &contracts[target.0 as usize],
                            arguments,
                            Some(&values[callee.0 as usize]),
                            &values,
                            &mut result.writes,
                            b,
                        )?;
                    }
                    InstructionKind::CallClosureShared { callee, arguments }
                    | InstructionKind::CallClosureMut { callee, arguments } => {
                        let target = closure_entry(module, &f.locals[callee.0 as usize].ty)?;
                        generated = apply(
                            &contracts[target.0 as usize],
                            arguments,
                            Some(&locals[callee.0 as usize]),
                            &values,
                            &mut result.writes,
                            b,
                        )?;
                    }
                    InstructionKind::CallIndirect { arguments, .. } => {
                        for id in arguments {
                            merge(&mut generated, &values[id.0 as usize], b)?;
                            if matches!(
                                f.values[id.0 as usize],
                                Type::Desc(_) | Type::Address { mutable: true, .. }
                            ) {
                                merge_constraints(
                                    &mut result.writes,
                                    &at(&values[id.0 as usize], &[], b)?,
                                    b,
                                )?;
                            }
                        }
                    }
                    InstructionKind::WithForeignCallback { body, .. }
                    | InstructionKind::WithScopedThread { body, .. } => {
                        let target = closure_entry(module, &f.values[body.0 as usize])?;
                        generated = apply(
                            &contracts[target.0 as usize],
                            &[],
                            Some(&values[body.0 as usize]),
                            &values,
                            &mut result.writes,
                            b,
                        )?;
                    }
                    _ => {}
                }
                let access = match &operation.kind {
                    InstructionKind::WriteDesc { desc, .. } => Some(*desc),
                    other => byte_buffers::access(other)
                        .or_else(|| dynamic_store::access(other))
                        .and_then(|(id, write)| write.then_some(id)),
                };
                if let Some(id) = access {
                    merge_constraints(&mut result.writes, &at(&values[id.0 as usize], &[], b)?, b)?;
                }
                if let Some(id) = operation.result {
                    if matches!(
                        f.values[id.0 as usize],
                        Type::Address { mutable: false, .. }
                    ) {
                        merge(
                            &mut generated,
                            &vec![(vec![], vec![Obligation::ReadOnly])],
                            b,
                        )?;
                    }
                    progressed |= merge(&mut values[id.0 as usize], &generated, b)?;
                }
            }
            if let Terminator::Return(Some(id)) = block.terminator {
                progressed |= merge(&mut result.returned, &values[id.0 as usize], b)?;
            }
            charge(b, locals.len().saturating_add(outputs[block_id].len()))?;
            if locals != outputs[block_id] {
                outputs[block_id] = locals;
                progressed = true;
            }
        }
        if !progressed {
            return Ok(result);
        }
    }
}
pub(super) fn check(module: &TypedModule, graphs: &[FunctionGraph], b: &mut Budget) -> Check<()> {
    if !b.head_descriptions {
        return Ok(());
    }
    charge(b, module.functions.len())?;
    let mut contracts = vec![Contract::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for id in (0..module.functions.len()).rev() {
            let new = body(module, &module.functions[id], &graphs[id], &contracts, b)?;
            changed |= merge(&mut contracts[id].returned, &new.returned, b)?;
            changed |= merge_constraints(&mut contracts[id].writes, &new.writes, b)?;
        }
        if !changed {
            break;
        }
    }
    for c in contracts {
        constraints_cost(&c.writes, b)?;
        if c.writes.contains(&Obligation::ReadOnly) {
            return Err(reject(
                "E-VERIFY-HEAD-PERMISSION",
                None,
                "actual shared resource head reaches a mutating access through descriptor forwarding or captures",
            ));
        }
    }
    Ok(())
}
