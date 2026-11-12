//! Executable head ancestry and simultaneous access windows. This uses only the
//! producer's newly solved actual CFG roots; no public candidate is consumed.
use super::head_access::{Requirement, Summary};
use super::*;
use std::collections::BTreeMap;
type Loans = BTreeSet<usize>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LoanRoot {
    Static,
    Local(LocalId),
    LocalStoreField(LocalId, u32),
    Parameter(u32, Vec<u32>),
    Capture(u32, Vec<u32>),
    CaptureStoreField(u32, u32),
}
impl LoanRoot {
    fn from_hold(root: HoldRoot) -> Self {
        match root {
            HoldRoot::Static => Self::Static,
            HoldRoot::Local(l) => Self::Local(l),
            HoldRoot::Parameter(p) => Self::Parameter(p, vec![]),
            HoldRoot::Capture(c) => Self::Capture(c, vec![]),
        }
    }
    fn path(&self) -> &[u32] {
        match self {
            Self::Static
            | Self::Local(_)
            | Self::LocalStoreField(..)
            | Self::CaptureStoreField(..) => &[],
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
fn checked_add(a: usize, c: usize) -> CompileResult<usize> {
    a.checked_add(c).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Analyze,
            "capture-path work overflow",
            None,
        )]
    })
}
fn checked_mul(a: usize, c: usize) -> CompileResult<usize> {
    a.checked_mul(c).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Analyze,
            "capture-path comparison work overflow",
            None,
        )]
    })
}
fn path_lookup(entries: usize, path: usize, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        checked_add(checked_mul(entries, checked_add(path, 1)?)?, path)?,
    )
}
fn carrier_cost(v: &Carrier, b: &mut Budget) -> CompileResult<()> {
    fee(b, v.len())?;
    for (path, row) in v {
        fee(b, checked_add(path.len(), row.len())?)?;
    }
    Ok(())
}
fn state_cost(v: &Locals, b: &mut Budget) -> CompileResult<()> {
    fee(b, v.len())?;
    for row in v.values() {
        carrier_cost(row, b)?;
    }
    Ok(())
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
        path_lookup(to.len(), p.len(), b)?;
        changed |= union(to.entry(p.clone()).or_default(), loans, b)?;
    }
    Ok(changed)
}
fn copy(v: &Carrier, b: &mut Budget) -> CompileResult<Carrier> {
    carrier_cost(v, b)?;
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
            fee(b, checked_add(p.len(), 1)?)?;
            let mut path = vec![*i];
            path.extend(p);
            (
                env.ok_or_else(|| bad("head loan actual environment absent"))?,
                path,
            )
        }
    };
    path_lookup(value.len(), path.len(), b)?;
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
        InstructionKind::WithSyncCell { body, .. }
        | InstructionKind::WithScopedThread { body, .. }
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
        | InstructionKind::DropMutexGuard { local: l }
        | InstructionKind::DropClosure { local: l } => {
            locals.remove(&l);
        }
        _ => {}
    }
    Ok(())
}
fn overlap(a: &[LoanRoot], c: &[LoanRoot], b: &mut Budget) -> CompileResult<bool> {
    fee(b, checked_mul(a.len(), c.len())?)?;
    if a.is_empty() || c.is_empty() {
        return Ok(false);
    }
    let pa = a
        .iter()
        .try_fold(0usize, |n, r| checked_add(n, r.path().len()))?;
    let pc = c
        .iter()
        .try_fold(0usize, |n, r| checked_add(n, r.path().len()))?;
    fee(
        b,
        checked_add(checked_mul(pa, c.len())?, checked_mul(pc, a.len())?)?,
    )?;
    Ok(a.iter().any(|r| {
        c.iter().any(|s| match (r, s) {
            (LoanRoot::Local(x), LoanRoot::LocalStoreField(y, _))
            | (LoanRoot::LocalStoreField(y, _), LoanRoot::Local(x)) => x == y,
            (LoanRoot::LocalStoreField(x, i), LoanRoot::LocalStoreField(y, j)) => x == y && i == j,
            (LoanRoot::Capture(x, _), LoanRoot::CaptureStoreField(y, _))
            | (LoanRoot::CaptureStoreField(y, _), LoanRoot::Capture(x, _)) => x == y,
            (LoanRoot::CaptureStoreField(x, i), LoanRoot::CaptureStoreField(y, j)) => {
                x == y && i == j
            }
            _ => r == s,
        })
    }))
}
fn external(
    root: &LoanRoot,
    context: (&TypedModule, &TypedFunction),
    b: &mut Budget,
) -> CompileResult<bool> {
    let (module, f) = context;
    let (mut ty, path) = match root {
        LoanRoot::Static
        | LoanRoot::Local(_)
        | LoanRoot::LocalStoreField(..)
        | LoanRoot::CaptureStoreField(..) => return Ok(false),
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
            let paths = checked_add(a.path().len(), c.path().len())?;
            // Equality, canonical ordering and cloning precede the ordered-set
            // insertion; each tree comparison can inspect a common path prefix.
            fee(b, checked_mul(4, checked_add(paths, 1)?)?)?;
            if a != c && external(a, context, b)? && external(c, context, b)? {
                fee(b, checked_mul(apart.len(), checked_add(paths, 1)?)?)?;
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
            fee(b, checked_add(path.len(), 1)?)?;
            let mut full = vec![*i];
            full.extend(path);
            (
                env.ok_or_else(|| bad("head separation actual capture absent"))?,
                full,
            )
        }
        LoanRoot::Static
        | LoanRoot::Local(_)
        | LoanRoot::LocalStoreField(..)
        | LoanRoot::CaptureStoreField(..) => {
            return Err(bad(
                "local or static root cannot enter a head separation contract",
            ));
        }
    };
    path_lookup(carrier.len(), path.len(), b)?;
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
        fee(b, checked_add(path.len(), 1)?)?;
        if matches!(ty, Type::Address { .. } | Type::Desc(_))
            || (b.store_option_results && stored_flow::option_address::shape(ty, b)?)
        {
            let id = if let Some(id) = reuse.filter(|_| path.is_empty()) {
                id
            } else {
                fee(b, checked_add(checked_mul(path.len(), 2)?, 4)?)?;
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
            path_lookup(out.len(), path.len(), b)?;
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
                fee(b, checked_add(path.len(), 1)?)?;
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
fn shell(local: LocalId, bound: &BTreeMap<LocalId, u32>) -> LoanRoot {
    bound.get(&local).map_or(LoanRoot::Local(local), |index| {
        LoanRoot::Capture(*index, vec![])
    })
}
fn is_pod_store_field(f: &TypedFunction, local: LocalId, index: u32) -> bool {
    matches!(&f.locals[local.0 as usize].ty,
        Type::Package { fields, .. } if fields.get(index as usize)
            .is_some_and(|(_, ty)| super::ownership_store::pod_owned(ty)))
}
fn store_field(
    f: &TypedFunction,
    local: LocalId,
    index: u32,
    bound: &BTreeMap<LocalId, u32>,
) -> LoanRoot {
    if is_pod_store_field(f, local, index) {
        bound
            .get(&local)
            .map_or(LoanRoot::LocalStoreField(local, index), |capture| {
                LoanRoot::CaptureStoreField(*capture, index)
            })
    } else {
        shell(local, bound)
    }
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
    // Only the new ordinary Store profile pays for an environment/local alias
    // map. BindPackageCapture names the existing captured owner; it never
    // creates an independent local allocation.
    let store_fields = b
        .ordinary_store_functions
        .get(f.id.0 as usize)
        .copied()
        .unwrap_or(false);
    let mut bound = BTreeMap::new();
    if store_fields {
        fee(b, f.blocks.len())?;
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for instruction in &block.instructions {
                if let InstructionKind::BindPackageCapture { local, index } = instruction.kind {
                    let captured = f
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                        .and_then(|definition| definition.captures.get(index as usize))
                        .ok_or_else(|| bad("Store field capture binding has no actual parent"))?;
                    let local_ty = &f.locals[local.0 as usize].ty;
                    b.type_work(local_ty, false, "Store field capture binding type")?;
                    b.type_work(captured, false, "Store field actual capture type")?;
                    if captured != local_ty || bound.contains_key(&local) {
                        return Err(bad("Store field capture binding differs from actual owner"));
                    }
                    path_lookup(bound.len(), 0, b)?;
                    fee(b, 1)?;
                    bound.insert(local, index);
                }
            }
        }
    }
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
        let mut physical = None;
        if b.task_borrowed_results && b.store_option_results && matches!(ty, Type::Address { .. }) {
            fee(b, 1)?;
            let covered = b
                .task_stored_coverage
                .get(f.id.0 as usize)
                .copied()
                .ok_or_else(|| bad("head loan access lacks Task proof coverage"))?;
            if covered {
                let roots = b
                    .task_pointer_roots
                    .get(f.id.0 as usize)
                    .and_then(|rows| rows.get(index))
                    .ok_or_else(|| bad("head loan access lacks actual physical SSA roots"))?;
                let count = roots.len();
                fee(b, count)?;
                if count != 0 {
                    physical = Some(b.task_pointer_roots[f.id.0 as usize][index].clone());
                }
            }
        }
        let roots = physical.as_deref().unwrap_or(&facts[index].roots);
        fee(b, roots.len())?;
        nodes.push(Node {
            // Public Task Hold roots also keep the worker's external input
            // alive. Accessing the frame addresses its physical shell instead.
            roots: roots.iter().copied().map(LoanRoot::from_hold).collect(),
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
            // A same-block Store establishes one definite local address
            // source. Incoming CFG state stays coarse: a missing predecessor
            // must never turn a known branch into an alias certificate.
            let mut exact_locals = BTreeSet::new();
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
                        InstructionKind::MakeVariant { fields, .. }
                            if b.store_option_results
                                && stored_flow::option_address::shape(&f.values[id], b)? =>
                        {
                            if !facts[id].roots.is_empty() {
                                fresh = true;
                            }
                            fee(b, fields.len())?;
                            for field in fields {
                                union(&mut parents, &direct(&values[field.0 as usize], b)?, b)?;
                            }
                        }
                        InstructionKind::ReadVariant {
                            value,
                            variant: 1,
                            index: 0,
                        } if b.store_option_results && !facts[id].roots.is_empty() => {
                            fresh = true;
                            parents = direct(&values[value.0 as usize], b)?;
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } if b.store_option_results
                            && stored_flow::option_address::shape(&f.values[id], b)? =>
                        {
                            if !facts[id].roots.is_empty() {
                                fresh = true;
                            }
                            let callee = module
                                .functions
                                .get(function.0 as usize)
                                .ok_or_else(|| bad("Option callee is absent"))?;
                            fee(b, callee.blocks.len())?;
                            let mut selected = BTreeSet::new();
                            for block in &callee.blocks {
                                if let Terminator::Return(Some(value)) = block.terminator {
                                    let sources =
                                        stored_flow::option_sources_snapshot(*function, value, b)?;
                                    fee(b, sources.len())?;
                                    for source in sources {
                                        fee(
                                            b,
                                            source
                                                .stored
                                                .len()
                                                .saturating_add(source.place.len())
                                                .saturating_add(
                                                    source
                                                        .heap_element
                                                        .as_ref()
                                                        .map_or(0, Vec::len),
                                                ),
                                        )?;
                                        let HoldRoot::Parameter(position) = source.base else {
                                            return Err(bad(
                                                "Option return source is not an actual input",
                                            ));
                                        };
                                        if position as usize >= callee.parameters.len() {
                                            return Err(bad(
                                                "Option return source input is absent",
                                            ));
                                        }
                                        fee(b, selected.len().saturating_add(1))?;
                                        selected.insert(position);
                                    }
                                }
                            }
                            for position in selected {
                                let argument =
                                    arguments.get(position as usize).ok_or_else(|| {
                                        bad("Option actual manager argument is absent")
                                    })?;
                                union(&mut parents, &direct(&values[argument.0 as usize], b)?, b)?;
                            }
                        }
                        InstructionKind::BorrowDynOwned { local, .. }
                            if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
                        {
                            fresh = true;
                            if let Some(actual) = state.get(local) {
                                parents = direct(actual, b)?;
                            }
                        }
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local }
                        | InstructionKind::BorrowDynOwned { local, .. } => {
                            fresh = true;
                            if matches!(i.kind, InstructionKind::BorrowDynOwned { .. }) {
                                nodes[id].roots = vec![shell(*local, &bound)];
                            }
                            if matches!(f.locals[local.0 as usize].ty, Type::Closure(_)) {
                                nodes[id].roots = vec![shell(*local, &bound)];
                                if let Some(c) = state.get(local) {
                                    carrier = copy(c, b)?;
                                }
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            fresh = true;
                            parents = direct(&values[address.0 as usize], b)?;
                            if matches!(&f.values[address.0 as usize], Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
                            {
                                // A closure receiver addresses the environment shell.
                                // Its captured addresses remain separate leaf loans;
                                // coarse Hold roots also include those pointees and
                                // cannot name this receiver's actual storage.
                                let mut roots = Vec::new();
                                for parent in &parents {
                                    fee(b, nodes[*parent].roots.len())?;
                                    for root in &nodes[*parent].roots {
                                        fee(b, root.path().len().saturating_add(1))?;
                                        roots.push(root.clone());
                                    }
                                }
                                nodes[id].roots = roots;
                            }
                            for (p, row) in &values[address.0 as usize] {
                                if !p.is_empty() {
                                    path_lookup(carrier.len(), p.len(), b)?;
                                    fee(b, row.len())?;
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
                                fee(b, carrier.len())?;
                                carrier.remove(&Vec::new());
                                fresh = true;
                            }
                        }
                        InstructionKind::DescribePackageField { local, index } => {
                            fresh = matches!(f.values[id], Type::Address { .. } | Type::Desc(_));
                            if store_fields && fresh {
                                nodes[id].roots = vec![store_field(f, *local, *index, &bound)];
                            }
                        }
                        InstructionKind::DescribeLocal { .. }
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
                        InstructionKind::MutexLock { mutex } => {
                            fresh = true;
                            parents = direct(&values[mutex.0 as usize], b)?;
                        }
                        InstructionKind::MutexValue { guard } => {
                            fresh = true;
                            parents = direct(&values[guard.0 as usize], b)?;
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
                                    let extent = checked_add(path.len(), 1)?;
                                    path_lookup(carrier.len(), extent, b)?;
                                    fee(b, checked_add(extent, row.len())?)?;
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
                                path_lookup(carrier.len(), path.len(), b)?;
                                carrier.insert(path.clone(), loans);
                            }
                        }
                        if matches!(f.values[id], Type::Address { .. }) {
                            parents = direct(&carrier, b)?;
                            fee(b, carrier.len())?;
                            carrier.remove(&Vec::new());
                            fresh = true;
                        }
                    }
                    // Direct projections and a same-block, definitely stored
                    // local address inherit a certified Store field owner.
                    // Calls and CFG joins keep the coarse Hold root.
                    if store_fields
                        && matches!(f.values[id], Type::Address { .. } | Type::Desc(_))
                        && (matches!(
                            i.kind,
                            InstructionKind::OwnedStoreBorrow { .. }
                                | InstructionKind::OwnedStoreBorrowMut { .. }
                                | InstructionKind::ReborrowAddress { .. }
                                | InstructionKind::DescribeAddress { .. }
                                | InstructionKind::AddressField { .. }
                                | InstructionKind::AddressSlice { .. }
                                | InstructionKind::AddressIndex { .. }
                        ) || matches!(i.kind, InstructionKind::Load(local) if exact_locals.contains(&local)))
                    {
                        fee(b, parents.len())?;
                        let source = if fresh {
                            parents.clone()
                        } else {
                            direct(&carrier, b)?
                        };
                        if source.len() == 1 {
                            let parent = *source.iter().next().expect("one actual source");
                            fee(b, nodes[parent].roots.len())?;
                            let precise_root = match nodes[parent].roots.as_slice() {
                                [
                                    root @ (LoanRoot::LocalStoreField(..)
                                    | LoanRoot::CaptureStoreField(..)),
                                ] => {
                                    fee(b, 1)?;
                                    Some(root.clone())
                                }
                                _ => None,
                            };
                            if let Some(root) = precise_root {
                                fee(b, 1)?;
                                if nodes[id].roots.as_slice() != std::slice::from_ref(&root) {
                                    nodes[id].roots = vec![root];
                                    changed = true;
                                }
                            }
                        }
                    }
                    if fresh
                        && b.task_borrowed_results
                        && b.store_option_results
                        && matches!(f.values[id], Type::Address { .. })
                    {
                        fee(b, 1)?;
                        if b.task_stored_coverage
                            .get(f.id.0 as usize)
                            .copied()
                            .ok_or_else(|| bad("head loan parent lacks Task proof coverage"))?
                        {
                            let anchors = b
                                .task_pointer_parents
                                .get(f.id.0 as usize)
                                .and_then(|rows| rows.get(id))
                                .ok_or_else(|| {
                                    bad("head loan parent lacks actual stored anchors")
                                })?;
                            let work = anchors.iter().fold(anchors.len(), |n, a| {
                                n.saturating_add(match a {
                                    stored_flow::TaskLoanAnchor::Value(_) => 1,
                                    stored_flow::TaskLoanAnchor::Input(t) => {
                                        stored_flow::target_width(t)
                                    }
                                })
                            });
                            fee(b, work)?;
                            let anchors = b.task_pointer_parents[f.id.0 as usize][id].clone();
                            for anchor in anchors {
                                match anchor {
                                    stored_flow::TaskLoanAnchor::Value(source)
                                        if source.0 as usize != id =>
                                    {
                                        union(
                                            &mut parents,
                                            &direct(&values[source.0 as usize], b)?,
                                            b,
                                        )?;
                                    }
                                    stored_flow::TaskLoanAnchor::Input(t)
                                        if t.stored.is_empty()
                                            && t.place.is_empty()
                                            && t.retained_port.is_none()
                                            && t.heap_element.is_none() =>
                                    {
                                        if let HoldRoot::Parameter(position) = t.base {
                                            if position as usize >= f.parameters.len() {
                                                return Err(bad(
                                                    "head loan actual input anchor is absent",
                                                ));
                                            }
                                            fee(b, 1)?;
                                            let parameter = f.parameters[position as usize];
                                            if matches!(
                                                f.locals[parameter.0 as usize].ty,
                                                Type::Address { .. }
                                            ) {
                                                parents.insert(v + position as usize);
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    if fresh {
                        path_lookup(carrier.len(), 0, b)?;
                        carrier.entry(vec![]).or_default().insert(id);
                        changed |= union(&mut nodes[id].parents, &parents, b)?;
                    }
                    changed |= merge(&mut values[id], &carrier, b)?;
                }
                if store_fields {
                    match i.kind {
                        InstructionKind::Store { local, value } => {
                            b.lookup(exact_locals.len(), "Store field exact local source")?;
                            let precise = matches!(
                                nodes[value.0 as usize].roots.as_slice(),
                                [LoanRoot::LocalStoreField(..) | LoanRoot::CaptureStoreField(..)]
                            ) && direct(&values[value.0 as usize], b)?.len() == 1;
                            if precise {
                                fee(b, 1)?;
                                exact_locals.insert(local);
                            } else {
                                exact_locals.remove(&local);
                            }
                        }
                        InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local)
                        | InstructionKind::TakeLocal { local }
                        | InstructionKind::DropClosure { local } => {
                            exact_locals.remove(&local);
                        }
                        _ => {}
                    }
                }
                transfer(&mut state, i, &values, b)?;
            }
            state_cost(&outputs[index], b)?;
            state_cost(&state, b)?;
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
                InstructionKind::AsyncRegistrationValid { registration } => {
                    Some((registration, false))
                }
                InstructionKind::AsyncRegistrationRequestClose { registration }
                | InstructionKind::AsyncRegistrationPollClose { registration } => {
                    Some((registration, true))
                }
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
                | InstructionKind::DropDynOwned { local }
                | InstructionKind::DropMutexGuard { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::TakeLocal { local }
                    if matches!(
                        f.locals[local.0 as usize].ty,
                        Type::Managed(_)
                            | Type::Package { .. }
                            | Type::MutexGuard { .. }
                            | Type::OwnedDyn { .. }
                    ) =>
                {
                    access(
                        &empty,
                        &[shell(*local, &bound)],
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
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::BorrowDynOwned { local, .. } => {
                    access(
                        &empty,
                        &[shell(*local, &bound)],
                        matches!(
                            i.kind,
                            InstructionKind::BorrowLocalMut { .. }
                                | InstructionKind::BorrowDynOwned { mutable: true, .. }
                        ) && !(matches!(i.kind, InstructionKind::BorrowDynOwned { .. })
                            && matches!(f.locals[local.0 as usize].ty, Type::Address { .. })),
                        &active,
                        ignore,
                        &nodes,
                        &mut apart,
                        (module, f),
                        b,
                    )?;
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. }
                    if store_fields && is_pod_store_field(f, *local, *index) =>
                {
                    access(
                        &empty,
                        &[store_field(f, *local, *index, &bound)],
                        true,
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
            let extent = checked_add(checked_add(apart[index].len(), found.len())?, 1)?;
            for (left, right) in &found {
                let path = checked_add(left.path().len(), right.path().len())?;
                fee(b, checked_mul(extent, checked_add(path, 1)?)?)?;
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

#[cfg(test)]
mod store_field_tests {
    use super::*;
    use zil_base::Limits;

    #[test]
    fn actual_store_field_windows_keep_parent_overlap() {
        let mut budget = Budget::new(&Limits::default());
        let parent = LoanRoot::Local(LocalId(3));
        let first = LoanRoot::LocalStoreField(LocalId(3), 0);
        let second = LoanRoot::LocalStoreField(LocalId(3), 1);
        assert!(!overlap(std::slice::from_ref(&first), &[second], &mut budget).unwrap());
        assert!(
            overlap(
                std::slice::from_ref(&first),
                std::slice::from_ref(&first),
                &mut budget
            )
            .unwrap()
        );
        assert!(
            overlap(
                &[parent],
                &[LoanRoot::LocalStoreField(LocalId(3), 0)],
                &mut budget
            )
            .unwrap()
        );
    }
}
