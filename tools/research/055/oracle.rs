//! Deliberately separate TypedIR interpreter for the finite may-liveness relation.
//! The solver uses no Graph, candidate operand visitor, bitset, cut plan or queue.
//! A separate raw-IR audit checks graph extraction before any input is frozen.
use crate::budget::{LIMIT, Work, instruction_bound, instruction_items};
use crate::graph::{R, Slot};
use std::collections::{BTreeMap, BTreeSet};
use zil_ir::*;
type Set = BTreeSet<Slot>;

fn tracked(module: &TypedModule, t: &Type) -> bool {
    match t {
        Type::Desc(_) => true,
        Type::Closure(c) => module.closures[c.0 as usize]
            .captures
            .iter()
            .any(|t| matches!(t, Type::Desc(_))),
        _ => false,
    }
}
fn before(module: &TypedModule, f: &TypedFunction, ins: &Instruction, live: &mut Set) {
    if let Some(v) = ins.result {
        live.remove(&Slot::Value(v.0));
    }
    use InstructionKind::*;
    let local = match &ins.kind {
        StorageLive(l) | StorageDead(l) | Store { local: l, .. } => {
            live.remove(&Slot::Local(l.0));
            None
        }
        Load(l)
        | TakeLocal { local: l }
        | CallClosureMut { callee: l, .. }
        | CallClosureShared { callee: l, .. } => Some(l),
        _ => None,
    };
    if let Some(l) = local
        && tracked(module, &f.locals[l.0 as usize].ty)
    {
        live.insert(Slot::Local(l.0));
    }
    let mut use_value = |v: ValueId| {
        if tracked(module, &f.values[v.0 as usize]) {
            live.insert(Slot::Value(v.0));
        }
    };
    match &ins.kind {
        Binary { left, right, .. } => {
            use_value(*left);
            use_value(*right)
        }
        Unary { operand, .. } => use_value(*operand),
        Store { value, .. }
        | WriteCapture { value, .. }
        | VariantTag { value }
        | MakeErrorUnion { value, .. }
        | ErrorTag { value }
        | ReadErrorUnion { value, .. }
        | ReadVariant { value, .. } => use_value(*value),
        MakeManaged { fields } | MakeAggregate { fields } | MakeVariant { fields, .. } => {
            for &v in fields {
                use_value(v)
            }
        }
        MakeClosure { captures, .. } => {
            for &v in captures {
                use_value(v)
            }
        }
        Call { arguments, .. }
        | ForeignCall { arguments, .. }
        | CallClosureMut { arguments, .. }
        | CallClosureShared { arguments, .. } => {
            for &v in arguments {
                use_value(v)
            }
        }
        CallClosure { callee, arguments }
        | CallClosureOnce { callee, arguments }
        | CallIndirect { callee, arguments } => {
            use_value(*callee);
            for &v in arguments {
                use_value(v)
            }
        }
        ReadDesc { desc, .. } => use_value(*desc),
        WriteDesc { desc, value, .. } => {
            use_value(*desc);
            use_value(*value)
        }
        ReadAddress { address }
        | ReborrowAddress { address, .. }
        | AddressField { address, .. } => use_value(*address),
        WriteAddress { address, value } => {
            use_value(*address);
            use_value(*value)
        }
        AddressIndex { address, index } => {
            use_value(*address);
            use_value(*index)
        }
        ExtractIndex { aggregate, index } => {
            use_value(*aggregate);
            use_value(*index)
        }
        UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            use_value(*aggregate);
            use_value(*index);
            use_value(*value)
        }
        ExtractField { aggregate, .. } => use_value(*aggregate),
        UpdateField {
            aggregate, value, ..
        } => {
            use_value(*aggregate);
            use_value(*value)
        }
        Constant(_)
        | Load(_)
        | TakeLocal { .. }
        | StorageLive(_)
        | StorageDead(_)
        | BorrowLocal { .. }
        | BorrowLocalMut { .. }
        | DescribeLocal { .. }
        | DropManaged { .. }
        | UnsafeEnter { .. }
        | UnsafeExit { .. }
        | ScopeEnter { .. }
        | ScopeExit { .. }
        | FunctionRef { .. }
        | ReadCapture { .. }
        | DescribeCapture { .. }
        | TakeCapture { .. }
        | DropClosure { .. }
        | Unsupported(_) => {}
    }
}
fn successors(t: &Terminator) -> Vec<usize> {
    match t {
        Terminator::Jump(b) => vec![b.0 as usize],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0 as usize, else_block.0 as usize],
        Terminator::Return(_) | Terminator::Trap | Terminator::Unreachable => vec![],
    }
}
fn output(
    module: &TypedModule,
    f: &TypedFunction,
    b: usize,
    entry: &[Set],
    work: &mut Work,
) -> R<Set> {
    let mut row = Set::new();
    for to in successors(&f.blocks[b].terminator) {
        work.charge(entry[to].len().saturating_add(1))?;
        row.extend(entry[to].iter().copied());
    }
    if let Terminator::Return(Some(v)) = &f.blocks[b].terminator
        && tracked(module, &f.values[v.0 as usize])
    {
        row.insert(Slot::Value(v.0));
    }
    Ok(row)
}
pub fn solve(module: &TypedModule, f: &TypedFunction) -> R<BTreeMap<(usize, usize), Set>> {
    let mut work = Work::new(LIMIT, "independent oracle");
    work.charge(
        module
            .closures
            .len()
            .saturating_add(f.blocks.len().saturating_mul(3)),
    )?;
    let capture_bound = module
        .closures
        .iter()
        .map(|c| c.captures.len())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut reachable = BTreeSet::new();
    let mut todo = vec![f.entry.0 as usize];
    while let Some(b) = todo.pop() {
        work.charge(3)?;
        if reachable.insert(b) {
            todo.extend(successors(&f.blocks[b].terminator));
        }
    }
    let mut entry = vec![Set::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for b in (0..f.blocks.len()).rev() {
            work.charge(capture_bound.saturating_add(1))?;
            if !reachable.contains(&b) {
                continue;
            }
            let mut live = output(module, f, b, &entry, &mut work)?;
            for ins in f.blocks[b].instructions.iter().rev() {
                work.charge(
                    instruction_items(&ins.kind)
                        .saturating_mul(capture_bound)
                        .saturating_mul(2),
                )?;
                before(module, f, ins, &mut live);
            }
            work.charge(live.len().saturating_add(entry[b].len()))?;
            if live != entry[b] {
                entry[b] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut result = BTreeMap::new();
    for (b, block) in f.blocks.iter().enumerate() {
        work.charge(capture_bound.saturating_add(1))?;
        let mut live = if reachable.contains(&b) {
            output(module, f, b, &entry, &mut work)?
        } else {
            Set::new()
        };
        work.charge(live.len().saturating_add(1))?;
        result.insert((b, block.instructions.len()), live.clone());
        for (i, ins) in block.instructions.iter().enumerate().rev() {
            work.charge(
                instruction_items(&ins.kind)
                    .saturating_mul(capture_bound)
                    .saturating_mul(2),
            )?;
            if reachable.contains(&b) {
                before(module, f, ins, &mut live);
            }
            work.charge(live.len().saturating_add(1))?;
            result.insert((b, i), live.clone());
        }
    }
    Ok(result)
}

/// Reconstruct every raw edge and check transfer functions at the bottom and
/// top of the finite powerset lattice. For a gen/kill function those two images
/// determine the whole function. This audit does not reuse the graph extractor,
/// its operand visitor, its event classifier, or its reachability walk.
pub fn audit_extraction(module: &TypedModule, f: &TypedFunction, g: &crate::graph::Graph) -> R<()> {
    audit_with_limit(module, f, g, LIMIT).map(|_| ())
}
pub fn audit_with_limit(
    module: &TypedModule,
    f: &TypedFunction,
    g: &crate::graph::Graph,
    limit: usize,
) -> R<usize> {
    use InstructionKind::*;
    let mut work = Work::new(limit, "raw extraction audit");
    work.charge(module.closures.len())?;
    let capture_bound = module
        .closures
        .iter()
        .map(|c| c.captures.len())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    work.charge(
        f.blocks.len().saturating_mul(3).saturating_add(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_mul(capture_bound)
                .saturating_mul(3),
        ),
    )?;
    let mut starts = vec![];
    let mut raw_count = 0;
    for b in &f.blocks {
        starts.push(raw_count);
        raw_count += b.instructions.len() + 1;
    }
    let slots = f
        .locals
        .iter()
        .enumerate()
        .filter(|(_, l)| tracked(module, &l.ty))
        .map(|(i, _)| Slot::Local(i as u32))
        .chain(
            f.values
                .iter()
                .enumerate()
                .filter(|(_, t)| tracked(module, t))
                .map(|(i, _)| Slot::Value(i as u32)),
        )
        .collect::<Vec<_>>();
    if g.function != f.id.0
        || g.starts != starts
        || g.entry != starts[f.entry.0 as usize]
        || g.slots != slots
    {
        return Err("raw audit: identity/slot map".into());
    }
    work.charge(slots.len().saturating_add(raw_count))?;
    let top = slots.iter().copied().collect::<Set>();
    let mut edges = vec![vec![]; raw_count];
    let mut failures = vec![];
    let important_type = |t: &Type| {
        matches!(t, Type::Managed(_) | Type::Desc(_) | Type::Address { .. })
            || matches!(t,Type::Closure(c) if !module.closures[c.0 as usize].captures.is_empty())
    };
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, ins) in block.instructions.iter().enumerate() {
            let id = starts[b] + i;
            let node = g.nodes.get(id).ok_or("raw audit: missing instruction")?;
            work.charge(
                instruction_bound(&ins.kind)
                    .saturating_mul(3)
                    .saturating_add(
                        instruction_items(&ins.kind)
                            .saturating_mul(capture_bound)
                            .saturating_mul(3),
                    )
                    .saturating_add(top.len().saturating_mul(6))
                    .saturating_add(node.label.len())
                    .saturating_add(
                        node.kill
                            .len()
                            .saturating_add(node.uses.len())
                            .saturating_mul(2),
                    ),
            )?;
            if node.block != b || node.ordinal != i || node.label != format!("{:?}", ins.kind) {
                return Err("raw audit: instruction identity".into());
            }
            let mut low = Set::new();
            let mut high = top.clone();
            before(module, f, ins, &mut low);
            before(module, f, ins, &mut high);
            for (input, want) in [(Set::new(), &low), (top.clone(), &high)] {
                let mut got = input;
                for &k in &node.kill {
                    got.remove(slots.get(k).ok_or("raw audit: kill slot")?);
                }
                for &u in &node.uses {
                    got.insert(*slots.get(u).ok_or("raw audit: use slot")?);
                }
                if &got != want {
                    return Err("raw audit: gen/kill transfer".into());
                }
            }
            let scalar_only = match &ins.kind {
                Constant(_) | FunctionRef { .. } => true,
                Load(l)
                | StorageLive(l)
                | StorageDead(l)
                | Store { local: l, .. }
                | TakeLocal { local: l } => !important_type(&f.locals[l.0 as usize].ty),
                MakeAggregate { fields } => fields
                    .iter()
                    .all(|v| !important_type(&f.values[v.0 as usize])),
                ExtractField { aggregate, .. } | UpdateField { aggregate, .. } => {
                    !important_type(&f.values[aggregate.0 as usize])
                }
                _ => false,
            };
            if (!scalar_only || !low.is_empty() || high != top || id == g.entry) && !node.observable
            {
                return Err("raw audit: lost resource/control event".into());
            }
            edges[id].push((id + 1, None));
            let can_fail = matches!(
                ins.kind,
                Unary { .. }
                    | Binary { .. }
                    | ReadVariant { .. }
                    | ReadErrorUnion { .. }
                    | ExtractIndex { .. }
                    | UpdateIndex { .. }
                    | AddressIndex { .. }
                    | Call { .. }
                    | ForeignCall { .. }
                    | CallIndirect { .. }
                    | CallClosure { .. }
                    | CallClosureShared { .. }
                    | CallClosureMut { .. }
                    | CallClosureOnce { .. }
            );
            if can_fail {
                edges[id].push((raw_count + failures.len(), None));
                failures.push((b, i));
            }
        }
        let id = starts[b] + block.instructions.len();
        let node = g.nodes.get(id).ok_or("raw audit: missing terminator")?;
        work.charge(
            capture_bound
                .saturating_add(node.label.len())
                .saturating_add(node.uses.len())
                .saturating_add(256),
        )?;
        if node.block != b
            || node.ordinal != block.instructions.len()
            || node.label != format!("{:?}", block.terminator)
        {
            return Err("raw audit: terminator identity".into());
        }
        let mandatory = match &block.terminator {
            Terminator::Jump(to) => {
                edges[id].push((starts[to.0 as usize], None));
                to.0 as usize <= b
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                edges[id].push((starts[then_block.0 as usize], Some((condition.0, true))));
                edges[id].push((starts[else_block.0 as usize], Some((condition.0, false))));
                true
            }
            Terminator::Return(_) | Terminator::Trap | Terminator::Unreachable => true,
        };
        if (mandatory || id == g.entry) && !node.observable {
            return Err("raw audit: lost terminal/control event".into());
        }
        let expected_use = if let Terminator::Return(Some(v)) = &block.terminator {
            if tracked(module, &f.values[v.0 as usize]) {
                Set::from([Slot::Value(v.0)])
            } else {
                Set::new()
            }
        } else {
            Set::new()
        };
        let actual_use = node
            .uses
            .iter()
            .map(|&i| {
                slots
                    .get(i)
                    .copied()
                    .ok_or("raw audit: terminal use slot".to_string())
            })
            .collect::<R<Set>>()?;
        if actual_use != expected_use || !node.kill.is_empty() {
            return Err("raw audit: terminal transfer".into());
        }
    }
    if g.nodes.len() != raw_count + failures.len() || g.implicit_exits != failures.len() {
        return Err("raw audit: failure exit count".into());
    }
    for (i, &(b, ordinal)) in failures.iter().enumerate() {
        let node = &g.nodes[raw_count + i];
        work.charge(node.label.len().saturating_add(128))?;
        if node.block != b
            || node.ordinal != ordinal
            || node.label != format!("conservative-failure@{b}:{ordinal}")
            || !node.observable
            || !node.edges.is_empty()
            || !node.uses.is_empty()
            || !node.kill.is_empty()
        {
            return Err("raw audit: failure exit".into());
        }
        edges.push(vec![]);
    }
    for (node, want) in g.nodes.iter().zip(&edges) {
        work.charge(node.edges.len().saturating_mul(2).saturating_add(1))?;
        let got = node
            .edges
            .iter()
            .map(|e| (e.to, e.guard))
            .collect::<Vec<_>>();
        if &got != want {
            return Err("raw audit: CFG edge/guard".into());
        }
    }
    let mut reached = BTreeSet::new();
    let mut frontier = BTreeSet::from([starts[f.entry.0 as usize]]);
    while let Some(p) = frontier.pop_first() {
        work.charge(edges[p].len().saturating_add(1))?;
        if reached.insert(p) {
            frontier.extend(
                edges[p]
                    .iter()
                    .map(|e| e.0)
                    .filter(|q| !reached.contains(q)),
            );
        }
    }
    work.charge(g.reachable.len())?;
    if g.reachable.len() != g.nodes.len()
        || g.reachable
            .iter()
            .enumerate()
            .any(|(p, &r)| r != reached.contains(&p))
    {
        return Err("raw audit: reachability".into());
    }
    Ok(work.used())
}
