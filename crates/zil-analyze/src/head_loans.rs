//! Executable head ancestry and simultaneous access windows. This uses only the
//! producer's newly solved actual CFG roots; no public candidate is consumed.
use super::head_access::{Requirement, Summary};
use super::*;
use std::collections::BTreeMap;
type Loans = BTreeSet<usize>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LoanRoot {
    Local(LocalId),
    Parameter(u32, Vec<u32>),
    Capture(u32, Vec<u32>),
}
impl LoanRoot {
    fn from_hold(root: HoldRoot) -> Self {
        match root {
            HoldRoot::Local(l) => Self::Local(l),
            HoldRoot::Parameter(p) => Self::Parameter(p, vec![]),
            HoldRoot::Capture(c) => Self::Capture(c, vec![]),
        }
    }
    fn path(&self) -> &[u32] {
        match self {
            Self::Local(_) => &[],
            Self::Parameter(_, p) | Self::Capture(_, p) => p,
        }
    }
}
type Apart = BTreeSet<(LoanRoot, LoanRoot)>;
type Carrier = BTreeMap<Vec<u32>, Loans>;
type Locals = BTreeMap<LocalId, Carrier>;
struct Node {
    roots: Vec<LoanRoot>,
    exclusive: bool,
    weak: bool,
    parents: Loans,
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "actual borrowed head loan proof")
}
fn union(to: &mut Loans, from: &Loans, b: &mut Budget) -> CompileResult<bool> {
    fee(b, to.len().saturating_add(from.len()))?;
    let n = to.len();
    to.extend(from);
    Ok(to.len() != n)
}
fn merge(to: &mut Carrier, from: &Carrier, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (p, loans) in from {
        fee(b, p.len().saturating_add(to.len()))?;
        changed |= union(to.entry(p.clone()).or_default(), loans, b)?;
    }
    Ok(changed)
}
fn copy(v: &Carrier, b: &mut Budget) -> CompileResult<Carrier> {
    fee(b, v.len())?;
    for (p, row) in v {
        fee(b, p.len().saturating_add(row.len()))?;
    }
    Ok(v.clone())
}
fn state_copy(v: &Locals, b: &mut Budget) -> CompileResult<Locals> {
    fee(b, v.len())?;
    let mut out = Locals::new();
    for (&local, row) in v {
        out.insert(local, copy(row, b)?);
    }
    Ok(out)
}
fn direct(c: &Carrier, b: &mut Budget) -> CompileResult<Loans> {
    fee(b, c.len())?;
    match c.get(&Vec::new()) {
        Some(row) => {
            fee(b, row.len())?;
            Ok(row.clone())
        }
        None => Ok(Loans::new()),
    }
}
fn all(c: &Carrier, b: &mut Budget) -> CompileResult<Loans> {
    let mut out = Loans::new();
    for row in c.values() {
        union(&mut out, row, b)?;
    }
    Ok(out)
}
fn select(
    q: &Requirement,
    args: &[ValueId],
    env: Option<&Carrier>,
    values: &[Carrier],
    b: &mut Budget,
) -> CompileResult<Loans> {
    let (value, path) = match q {
        Requirement::Shared => return Ok(Loans::new()),
        Requirement::Parameter(i, p) => {
            fee(b, p.len())?;
            let id = args
                .get(*i as usize)
                .ok_or_else(|| bad("head loan actual argument absent"))?;
            (&values[id.0 as usize], p.clone())
        }
        Requirement::Capture(i, p) => {
            fee(b, p.len().saturating_add(1))?;
            let mut path = vec![*i];
            path.extend(p);
            (
                env.ok_or_else(|| bad("head loan actual environment absent"))?,
                path,
            )
        }
    };
    fee(b, value.len().saturating_add(path.len()))?;
    match value.get(&path) {
        Some(row) => {
            fee(b, row.len())?;
            Ok(row.clone())
        }
        None => Ok(Loans::new()),
    }
}
fn entry(module: &TypedModule, ty: &Type) -> Option<usize> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::Closure(id) = ty {
        module
            .closures
            .get(id.0 as usize)
            .map(|d| d.function.0 as usize)
    } else {
        None
    }
}
fn invocation<'a>(
    module: &TypedModule,
    f: &TypedFunction,
    i: &'a Instruction,
    locals: &'a Locals,
    values: &'a [Carrier],
) -> Option<(usize, &'a [ValueId], Option<&'a Carrier>)> {
    match &i.kind {
        InstructionKind::Call {
            function,
            arguments,
        } => Some((function.0 as usize, arguments, None)),
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        } => Some((
            entry(module, &f.values[callee.0 as usize])?,
            arguments,
            Some(&values[callee.0 as usize]),
        )),
        InstructionKind::CallClosureShared { callee, arguments }
        | InstructionKind::CallClosureMut { callee, arguments } => Some((
            entry(module, &f.locals[callee.0 as usize].ty)?,
            arguments,
            locals.get(callee),
        )),
        InstructionKind::WithScopedThread { body, .. }
        | InstructionKind::WithForeignCallback { body, .. } => Some((
            entry(module, &f.values[body.0 as usize])?,
            &[],
            Some(&values[body.0 as usize]),
        )),
        _ => None,
    }
}
fn transfer(
    locals: &mut Locals,
    i: &Instruction,
    values: &[Carrier],
    b: &mut Budget,
) -> CompileResult<()> {
    match i.kind {
        InstructionKind::Store { local, value } => {
            let c = copy(&values[value.0 as usize], b)?;
            if c.is_empty() {
                locals.remove(&local);
            } else {
                locals.insert(local, c);
            }
        }
        InstructionKind::StorageLive(l)
        | InstructionKind::StorageDead(l)
        | InstructionKind::TakeLocal { local: l }
        | InstructionKind::DropClosure { local: l } => {
            locals.remove(&l);
        }
        _ => {}
    }
    Ok(())
}
fn overlap(a: &[LoanRoot], c: &[LoanRoot], b: &mut Budget) -> CompileResult<bool> {
    fee(b, a.len().saturating_mul(c.len()))?;
    if a.is_empty() || c.is_empty() {
        return Ok(false);
    }
    let pa = a
        .iter()
        .fold(0usize, |n, r| n.saturating_add(r.path().len()));
    let pc = c
        .iter()
        .fold(0usize, |n, r| n.saturating_add(r.path().len()));
    fee(
        b,
        pa.saturating_mul(c.len())
            .saturating_add(pc.saturating_mul(a.len())),
    )?;
    Ok(a.iter().any(|r| c.contains(r)))
}
fn external(
    root: &LoanRoot,
    context: (&TypedModule, &TypedFunction),
    b: &mut Budget,
) -> CompileResult<bool> {
    let (module, f) = context;
    let (mut ty, path) = match root {
        LoanRoot::Local(_) => return Ok(false),
        LoanRoot::Parameter(i, path) => (&f.locals[f.parameters[*i as usize].0 as usize].ty, path),
        LoanRoot::Capture(i, path) => (
            &module.closures[f
                .environment
                .ok_or_else(|| bad("capture separation outside environment"))?
                .0 as usize]
                .captures[*i as usize],
            path,
        ),
    };
    for index in path {
        fee(b, 1)?;
        if let Type::Address { inner, .. } = ty {
            ty = inner;
        }
        let Type::Closure(id) = ty else {
            return Err(bad("symbolic capture path crosses a non-environment"));
        };
        ty = module
            .closures
            .get(id.0 as usize)
            .and_then(|d| d.captures.get(*index as usize))
            .ok_or_else(|| bad("symbolic capture path outside actual layout"))?;
    }
    Ok(matches!(ty, Type::Desc(_) | Type::Address { .. })
        && !matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_))))
}
fn require_apart(
    left: &[LoanRoot],
    right: &[LoanRoot],
    apart: &mut Apart,
    context: (&TypedModule, &TypedFunction),
    b: &mut Budget,
) -> CompileResult<()> {
    for a in left {
        for c in right {
            let paths = a.path().len().saturating_add(c.path().len());
            fee(b, paths.saturating_add(1))?;
            if a != c && external(a, context, b)? && external(c, context, b)? {
                fee(
                    b,
                    apart
                        .len()
                        .saturating_add(1)
                        .saturating_mul(paths.saturating_add(1)),
                )?;
                apart.insert(if a < c {
                    (a.clone(), c.clone())
                } else {
                    (c.clone(), a.clone())
                });
            }
        }
    }
    Ok(())
}
fn actual_root(
    root: &LoanRoot,
    args: &[ValueId],
    env: Option<&Carrier>,
    values: &[Carrier],
    nodes: &[Node],
    b: &mut Budget,
) -> CompileResult<Vec<LoanRoot>> {
    let (carrier, path) = match root {
        LoanRoot::Parameter(i, path) => {
            let arg = args
                .get(*i as usize)
                .ok_or_else(|| bad("head separation actual input absent"))?;
            fee(b, path.len())?;
            (&values[arg.0 as usize], path.clone())
        }
        LoanRoot::Capture(i, path) => {
            fee(b, path.len().saturating_add(1))?;
            let mut full = vec![*i];
            full.extend(path);
            (
                env.ok_or_else(|| bad("head separation actual capture absent"))?,
                full,
            )
        }
        LoanRoot::Local(_) => {
            return Err(bad("local root cannot enter a head separation contract"));
        }
    };
    fee(b, carrier.len().saturating_add(path.len()))?;
    let mut result = vec![];
    if let Some(loans) = carrier.get(&path) {
        fee(b, loans.len())?;
        for id in loans {
            fee(b, nodes[*id].roots.len())?;
            for root in &nodes[*id].roots {
                fee(b, root.path().len())?;
            }
            result.extend(nodes[*id].roots.iter().cloned());
        }
    }
    Ok(result)
}
fn seed_paths(
    module: &TypedModule,
    ty: &Type,
    owner: Option<u32>,
    capture: Option<u32>,
    reuse: Option<usize>,
    nodes: &mut Vec<Node>,
    b: &mut Budget,
) -> CompileResult<Carrier> {
    fee(b, 1)?;
    let mut out = Carrier::new();
    let mut stack = vec![(ty, Vec::<u32>::new())];
    while let Some((ty, path)) = stack.pop() {
        fee(b, path.len().saturating_add(1))?;
        if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            let id = if path.is_empty() && reuse.is_some() {
                reuse.expect("present")
            } else {
                fee(b, path.len().saturating_mul(2).saturating_add(4))?;
                let id = nodes.len();
                let root = if let Some(i) = owner {
                    LoanRoot::Parameter(i, path.clone())
                } else {
                    LoanRoot::Capture(capture.expect("capture seed"), path.clone())
                };
                nodes.push(Node {
                    roots: vec![root],
                    exclusive: matches!(ty, Type::Address { mutable: true, .. }),
                    weak: matches!(ty, Type::Desc(_)),
                    parents: Loans::new(),
                });
                id
            };
            out.insert(path.clone(), Loans::from([id]));
        }
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Type::Closure(id) = ty {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| bad("seed environment outside actual layout"))?;
            fee(b, d.captures.len())?;
            for (i, t) in d.captures.iter().enumerate() {
                fee(b, path.len().saturating_add(1))?;
                let mut child = path.clone();
                child.push(u32::try_from(i).map_err(|_| bad("capture path index overflow"))?);
                stack.push((t, child));
            }
        }
    }
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
fn access(
    own: &Loans,
    roots: &[LoanRoot],
    write: bool,
    active: &Loans,
    ignore: Option<usize>,
    nodes: &[Node],
    apart: &mut Apart,
    context: (&TypedModule, &TypedFunction),
    b: &mut Budget,
) -> CompileResult<()> {
    for other in active {
        fee(b, 1)?;
        if Some(*other) == ignore
            || nodes[*other].weak
            || own.contains(other)
            || !(write || nodes[*other].exclusive)
        {
            continue;
        }
        if !overlap(roots, &nodes[*other].roots, b)? {
            require_apart(roots, &nodes[*other].roots, apart, context, b)?;
            continue;
        }
        let mut relevant = false;
        let mut delegated = true;
        for id in own {
            if overlap(&nodes[*id].roots, &nodes[*other].roots, b)? {
                relevant = true;
                delegated &= nodes[*id].parents.contains(other);
            }
        }
        if relevant && delegated {
            continue;
        }
        return Err(bad(
            "actual borrowed resource access conflicts with a live child or unrelated address window",
        ));
    }
    Ok(())
}
fn pairs(
    carriers: &[Loans],
    nodes: &[Node],
    apart: &mut Apart,
    context: (&TypedModule, &TypedFunction),
    b: &mut Budget,
) -> CompileResult<()> {
    for (i, a) in carriers.iter().enumerate() {
        for c in &carriers[i + 1..] {
            for l in a {
                for r in c {
                    fee(b, 1)?;
                    if !nodes[*l].weak
                        && !nodes[*r].weak
                        && (nodes[*l].exclusive || nodes[*r].exclusive)
                    {
                        require_apart(&nodes[*l].roots, &nodes[*r].roots, apart, context, b)?;
                    }
                    if !nodes[*l].weak
                        && !nodes[*r].weak
                        && (nodes[*l].exclusive || nodes[*r].exclusive)
                        && overlap(&nodes[*l].roots, &nodes[*r].roots, b)?
                    {
                        return Err(bad(
                            "simultaneous actual head arguments or capture positions overlap an exclusive window",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn check_function(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    facts: &[ValueHold],
    summaries: &[Summary],
    previous_apart: &[Apart],
    b: &mut Budget,
) -> CompileResult<Apart> {
    let mut apart = Apart::new();
    let v = f.values.len();
    let count = v.saturating_add(f.parameters.len());
    fee(
        b,
        count
            .saturating_mul(4)
            .saturating_add(f.blocks.len().saturating_mul(2)),
    )?;
    let mut nodes = Vec::with_capacity(count);
    for (index, ty) in f.values.iter().enumerate() {
        fee(b, facts[index].roots.len())?;
        nodes.push(Node {
            roots: facts[index]
                .roots
                .iter()
                .copied()
                .map(LoanRoot::from_hold)
                .collect(),
            exclusive: matches!(ty, Type::Address { mutable: true, .. }),
            weak: matches!(ty, Type::Desc(_)),
            parents: Loans::new(),
        });
    }
    let mut seed = Locals::new();
    for (index, l) in f.parameters.iter().enumerate() {
        let ty = &f.locals[l.0 as usize].ty;
        nodes.push(Node {
            roots: vec![LoanRoot::Parameter(index as u32, vec![])],
            exclusive: matches!(ty, Type::Address { mutable: true, .. }),
            weak: matches!(ty, Type::Desc(_)),
            parents: Loans::new(),
        });
        if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            seed.insert(*l, Carrier::from([(vec![], Loans::from([v + index]))]));
        }
    }
    for (index, local) in f.parameters.iter().enumerate() {
        let ty = &f.locals[local.0 as usize].ty;
        if matches!(ty, Type::Closure(_))
            || matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_)))
        {
            let carrier = seed_paths(
                module,
                ty,
                Some(index as u32),
                None,
                Some(v + index),
                &mut nodes,
                b,
            )?;
            if !carrier.is_empty() {
                seed.insert(*local, carrier);
            }
        }
    }
    let mut capture_seed = Vec::new();
    if let Some(id) = f.environment {
        let definition = &module.closures[id.0 as usize];
        fee(b, definition.captures.len())?;
        for (index, ty) in definition.captures.iter().enumerate() {
            capture_seed.push(seed_paths(
                module,
                ty,
                None,
                Some(index as u32),
                None,
                &mut nodes,
                b,
            )?);
        }
    }
    let mut values = vec![Carrier::new(); v];
    let mut outputs = vec![Locals::new(); f.blocks.len()];
    let mut inputs = vec![Locals::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in f.blocks.iter().enumerate() {
            fee(b, 1)?;
            if !graph.1[index] {
                continue;
            }
            let mut state = if index == f.entry.0 as usize {
                state_copy(&seed, b)?
            } else {
                Locals::new()
            };
            for p in &graph.0[index] {
                if graph.1[*p] {
                    for (l, c) in &outputs[*p] {
                        merge(state.entry(*l).or_default(), c, b)?;
                    }
                }
            }
            inputs[index] = state_copy(&state, b)?;
            for i in &block.instructions {
                fee(b, 1)?;
                if let Some(result) = i.result {
                    let id = result.0 as usize;
                    let mut carrier = Carrier::new();
                    let mut parents = Loans::new();
                    let mut fresh = false;
                    match &i.kind {
                        InstructionKind::Load(l) | InstructionKind::TakeLocal { local: l } => {
                            if let Some(c) = state.get(l) {
                                carrier = copy(c, b)?;
                            }
                        }
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            fresh = true;
                            if matches!(f.locals[local.0 as usize].ty, Type::Closure(_)) {
                                nodes[id].roots = vec![LoanRoot::Local(*local)];
                                if let Some(c) = state.get(local) {
                                    carrier = copy(c, b)?;
                                }
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            fresh = true;
                            parents = direct(&values[address.0 as usize], b)?;
                            for (p, row) in &values[address.0 as usize] {
                                if !p.is_empty() {
                                    fee(b, p.len().saturating_add(row.len()))?;
                                    carrier.insert(p.clone(), row.clone());
                                }
                            }
                        }
                        InstructionKind::DescribeAddress { address }
                        | InstructionKind::AddressField { address, .. }
                        | InstructionKind::AddressSlice { address, .. }
                        | InstructionKind::AddressIndex { address, .. } => {
                            carrier = copy(&values[address.0 as usize], b)?;
                        }
                        InstructionKind::ReadCapture { index }
                        | InstructionKind::BorrowCapture { index, .. } => {
                            carrier = copy(
                                capture_seed
                                    .get(*index as usize)
                                    .ok_or_else(|| bad("actual capture seed absent"))?,
                                b,
                            )?;
                            if matches!(f.values[id], Type::Address { .. } | Type::Desc(_)) {
                                parents = direct(&carrier, b)?;
                                carrier.remove(&Vec::new());
                                fresh = true;
                            }
                        }
                        InstructionKind::DescribeLocal { .. }
                        | InstructionKind::DescribePackageField { .. }
                        | InstructionKind::DescribeCapture { .. }
                        | InstructionKind::BorrowPackageField { .. }
                        | InstructionKind::BorrowOwnedArrayElement { .. } => {
                            fresh = matches!(f.values[id], Type::Address { .. } | Type::Desc(_));
                        }
                        InstructionKind::ByteBufferBorrow { buffer, .. }
                        | InstructionKind::OwnedStoreBorrow { store: buffer, .. }
                        | InstructionKind::OwnedStoreBorrowMut { store: buffer, .. } => {
                            fresh = true;
                            parents = direct(&values[buffer.0 as usize], b)?;
                        }
                        InstructionKind::CallIndirect { arguments, .. }
                            if matches!(f.values[id], Type::Address { .. }) =>
                        {
                            fresh = true;
                            for argument in arguments {
                                if overlap(&nodes[id].roots, &nodes[argument.0 as usize].roots, b)?
                                {
                                    union(
                                        &mut parents,
                                        &direct(&values[argument.0 as usize], b)?,
                                        b,
                                    )?;
                                }
                            }
                        }
                        InstructionKind::MakeClosure { captures, .. } => {
                            for (index, a) in captures.iter().enumerate() {
                                for (path, row) in &values[a.0 as usize] {
                                    fee(b, path.len().saturating_add(row.len()).saturating_add(1))?;
                                    let mut p = vec![index as u32];
                                    p.extend(path);
                                    carrier.insert(p, row.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some((target, args, env)) = invocation(module, f, i, &state, &values) {
                        for (path, row) in &summaries[target].result {
                            let mut loans = Loans::new();
                            for q in row {
                                union(&mut loans, &select(q, args, env, &values, b)?, b)?;
                            }
                            if !loans.is_empty() {
                                fee(b, path.len())?;
                                carrier.insert(path.clone(), loans);
                            }
                        }
                        if matches!(f.values[id], Type::Address { .. }) {
                            parents = direct(&carrier, b)?;
                            carrier.remove(&Vec::new());
                            fresh = true;
                        }
                    }
                    if fresh {
                        carrier.entry(vec![]).or_default().insert(id);
                        changed |= union(&mut nodes[id].parents, &parents, b)?;
                    }
                    changed |= merge(&mut values[id], &carrier, b)?;
                }
                transfer(&mut state, i, &values, b)?;
            }
            if outputs[index] != state {
                outputs[index] = state;
                changed = true;
            }
        }
        for id in 0..nodes.len() {
            fee(b, nodes[id].parents.len())?;
            let mut ancestors = nodes[id].parents.clone();
            for p in &nodes[id].parents {
                fee(b, nodes[*p].parents.len())?;
                ancestors.extend(&nodes[*p].parents);
            }
            ancestors.remove(&id);
            changed |= union(&mut nodes[id].parents, &ancestors, b)?;
        }
        if !changed {
            break;
        }
    }
    let afters = resources::address_afters(f, &graph.1, b)?;
    for (index, block) in f.blocks.iter().enumerate() {
        if !graph.1[index] {
            continue;
        }
        let mut state = state_copy(&inputs[index], b)?;
        for (position, i) in block.instructions.iter().enumerate() {
            let mut active = Loans::new();
            if !afters.is_empty() {
                for place in &afters[index][position] {
                    let c = match place {
                        resources::Place::Value(id) => Some(&values[id.0 as usize]),
                        resources::Place::Local(l) => state.get(l),
                        _ => None,
                    };
                    if let Some(c) = c {
                        union(&mut active, &all(c, b)?, b)?;
                    }
                }
            }
            fee(b, resources::uses(&i.kind).count())?;
            let operands: Vec<_> = resources::uses(&i.kind).collect();
            for operand in &operands {
                union(&mut active, &all(&values[operand.0 as usize], b)?, b)?;
            }
            let ignore = i.result.map(|id| id.0 as usize);
            let empty = Loans::new();
            if let Some((target, args, env)) = invocation(module, f, i, &state, &values) {
                fee(b, previous_apart[target].len())?;
                for (left, right) in &previous_apart[target] {
                    let left = actual_root(left, args, env, &values, &nodes, b)?;
                    let right = actual_root(right, args, env, &values, &nodes, b)?;
                    if left.is_empty() || right.is_empty() {
                        return Err(bad("head separation has no actual grounded source"));
                    }
                    if overlap(&left, &right, b)? {
                        return Err(bad(
                            "actual caller aliases storage required separate by the callee's independently reconstructed live windows",
                        ));
                    }
                    require_apart(&left, &right, &mut apart, (module, f), b)?;
                }
                for q in &summaries[target].writes {
                    let own = select(q, args, env, &values, b)?;
                    let mut roots = vec![];
                    for id in &own {
                        fee(b, nodes[*id].roots.len())?;
                        for root in &nodes[*id].roots {
                            fee(b, root.path().len())?;
                        }
                        roots.extend(nodes[*id].roots.iter().cloned());
                    }
                    access(
                        &own,
                        &roots,
                        true,
                        &active,
                        ignore,
                        &nodes,
                        &mut apart,
                        (module, f),
                        b,
                    )?;
                }
            }
            let primitive = match i.kind {
                InstructionKind::ReadDesc { desc, .. } => Some((desc, false)),
                InstructionKind::WriteDesc { desc, .. } => Some((desc, true)),
                InstructionKind::ByteBufferLen { buffer }
                | InstructionKind::ByteBufferCapacity { buffer }
                | InstructionKind::ByteBufferBorrow {
                    buffer,
                    mutable: false,
                } => Some((buffer, false)),
                InstructionKind::ByteBufferReserve { buffer, .. }
                | InstructionKind::ByteBufferPush { buffer, .. }
                | InstructionKind::ByteBufferTruncate { buffer, .. }
                | InstructionKind::ByteBufferBorrow {
                    buffer,
                    mutable: true,
                } => Some((buffer, true)),
                _ => dynamic_store::access(&i.kind),
            };
            if let Some((id, write)) = primitive {
                access(
                    &direct(&values[id.0 as usize], b)?,
                    &nodes[id.0 as usize].roots,
                    write,
                    &active,
                    ignore,
                    &nodes,
                    &mut apart,
                    (module, f),
                    b,
                )?;
            }
            match &i.kind {
                InstructionKind::ReadAddress { address }
                | InstructionKind::WriteAddress { address, .. }
                | InstructionKind::ReborrowAddress { address, .. } => {
                    let write = matches!(
                        i.kind,
                        InstructionKind::WriteAddress { .. }
                            | InstructionKind::ReborrowAddress { mutable: true, .. }
                    );
                    access(
                        &direct(&values[address.0 as usize], b)?,
                        &nodes[address.0 as usize].roots,
                        write,
                        &active,
                        ignore,
                        &nodes,
                        &mut apart,
                        (module, f),
                        b,
                    )?;
                }
                InstructionKind::Store { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local)
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::TakeLocal { local }
                    if matches!(
                        f.locals[local.0 as usize].ty,
                        Type::Managed(_) | Type::Package { .. }
                    ) =>
                {
                    access(
                        &empty,
                        &[LoanRoot::Local(*local)],
                        true,
                        &active,
                        ignore,
                        &nodes,
                        &mut apart,
                        (module, f),
                        b,
                    )?;
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    access(
                        &empty,
                        &[LoanRoot::Local(*local)],
                        matches!(i.kind, InstructionKind::BorrowLocalMut { .. }),
                        &active,
                        ignore,
                        &nodes,
                        &mut apart,
                        (module, f),
                        b,
                    )?;
                }
                _ => {}
            }
            let args = match &i.kind {
                InstructionKind::Call { arguments, .. }
                | InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::MakeClosure {
                    captures: arguments,
                    ..
                } => arguments.as_slice(),
                _ => &[],
            };
            fee(b, args.len())?;
            let mut prepared = Vec::with_capacity(args.len());
            for id in args {
                prepared.push(all(&values[id.0 as usize], b)?);
            }
            pairs(&prepared, &nodes, &mut apart, (module, f), b)?;
            transfer(&mut state, i, &values, b)?;
        }
    }
    Ok(apart)
}
pub(super) fn check(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    facts: &[Vec<ValueHold>],
    summaries: &[Summary],
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.head_descriptions && !b.invocation_callbacks {
        return Ok(());
    }
    fee(b, module.functions.len())?;
    let mut apart = vec![Apart::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            let found = check_function(
                module,
                f,
                &graphs[index],
                &facts[index],
                summaries,
                &apart,
                b,
            )?;
            fee(b, found.len().saturating_add(apart[index].len()))?;
            for (left, right) in &found {
                fee(
                    b,
                    apart[index]
                        .len()
                        .saturating_add(1)
                        .saturating_mul(left.path().len().saturating_add(right.path().len())),
                )?;
            }
            let old = apart[index].len();
            apart[index].extend(found);
            changed |= apart[index].len() != old;
        }
        if !changed {
            break;
        }
    }
    Ok(())
}
