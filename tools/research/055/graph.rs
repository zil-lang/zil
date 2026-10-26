use crate::budget::Work;
use std::collections::{BTreeSet, VecDeque};
use zil_ir::*;

pub const MAX_NODES: usize = 200_000;
pub const MAX_SLOTS: usize = 8192;
pub const MAX_EDGES: usize = 2_000_000;
pub const MAX_WORK: usize = crate::budget::LIMIT;
pub const FILL_LIMIT: usize = 64;
pub type Row = Vec<u64>;
pub type R<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Local(u32),
    Value(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub to: usize,
    pub guard: Option<(u32, bool)>,
}
#[derive(Clone, Debug)]
pub struct Node {
    pub block: usize,
    pub ordinal: usize,
    pub uses: Vec<usize>,
    pub kill: Vec<usize>,
    pub observable: bool,
    pub label: String,
    pub edges: Vec<Edge>,
}
#[derive(Clone, Debug)]
pub struct Graph {
    pub function: u32,
    pub slots: Vec<Slot>,
    pub nodes: Vec<Node>,
    pub starts: Vec<usize>,
    pub entry: usize,
    pub reachable: Vec<bool>,
    pub implicit_exits: usize,
    pub interaction: Vec<BTreeSet<usize>>,
}

pub fn description(module: &TypedModule, ty: &Type) -> bool {
    matches!(ty, Type::Desc(_))
        || matches!(ty, Type::Closure(id)
        if module.closures[id.0 as usize].captures.iter().any(|t|matches!(t,Type::Desc(_))))
}
fn capability(module: &TypedModule, ty: &Type) -> bool {
    matches!(ty, Type::Managed(_) | Type::Desc(_) | Type::Address { .. })
        || matches!(ty, Type::Closure(id) if !module.closures[id.0 as usize].captures.is_empty())
}
pub fn operands(k: &InstructionKind) -> Vec<ValueId> {
    use InstructionKind::*;
    match k {
        MakeManaged { fields } | MakeAggregate { fields } | MakeVariant { fields, .. } => {
            fields.clone()
        }
        MakeClosure { captures, .. } => captures.clone(),
        Call { arguments, .. }
        | ForeignCall { arguments, .. }
        | CallClosureShared { arguments, .. }
        | CallClosureMut { arguments, .. } => arguments.clone(),
        CallIndirect { callee, arguments }
        | CallClosure { callee, arguments }
        | CallClosureOnce { callee, arguments } => std::iter::once(*callee)
            .chain(arguments.iter().copied())
            .collect(),
        Store { value, .. }
        | WriteCapture { value, .. }
        | VariantTag { value }
        | MakeErrorUnion { value, .. }
        | ErrorTag { value }
        | ReadErrorUnion { value, .. }
        | ReadVariant { value, .. } => vec![*value],
        Unary { operand, .. } => vec![*operand],
        Binary { left, right, .. } => vec![*left, *right],
        ReadDesc { desc, .. } => vec![*desc],
        WriteDesc { desc, value, .. } => vec![*desc, *value],
        ReborrowAddress { address, .. }
        | ReadAddress { address }
        | AddressField { address, .. } => vec![*address],
        WriteAddress { address, value } => vec![*address, *value],
        AddressIndex { address, index } => vec![*address, *index],
        ExtractIndex { aggregate, index } => vec![*aggregate, *index],
        UpdateIndex {
            aggregate,
            index,
            value,
        } => vec![*aggregate, *index, *value],
        ExtractField { aggregate, .. } => vec![*aggregate],
        UpdateField {
            aggregate, value, ..
        } => vec![*aggregate, *value],
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
        | Unsupported(_) => vec![],
    }
}
fn local_operand(k: &InstructionKind) -> Option<LocalId> {
    use InstructionKind::*;
    match k {
        Load(l) | StorageLive(l) | StorageDead(l) => Some(*l),
        TakeLocal { local }
        | Store { local, .. }
        | BorrowLocal { local }
        | BorrowLocalMut { local }
        | DescribeLocal { local }
        | DropManaged { local }
        | DropClosure { local } => Some(*local),
        CallClosureMut { callee, .. } | CallClosureShared { callee, .. } => Some(*callee),
        _ => None,
    }
}
// Conservative graph exits. They do not assert that a numeric failure is feasible.
// Calls remain opaque; an exceptional/non-returning callee is never silently lost.
pub fn potential_failure(k: &InstructionKind) -> bool {
    matches!(
        k,
        InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ReadVariant { .. }
            | InstructionKind::ReadErrorUnion { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::UpdateIndex { .. }
            | InstructionKind::AddressIndex { .. }
            | InstructionKind::Call { .. }
            | InstructionKind::ForeignCall { .. }
            | InstructionKind::CallIndirect { .. }
            | InstructionKind::CallClosure { .. }
            | InstructionKind::CallClosureShared { .. }
            | InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureOnce { .. }
    )
}
fn event(module: &TypedModule, f: &TypedFunction, k: &InstructionKind) -> bool {
    use InstructionKind::*;
    match k {
        Constant(_) | FunctionRef { .. } => false,
        Load(l) | StorageLive(l) | StorageDead(l) => capability(module, &f.locals[l.0 as usize].ty),
        Store { local, .. } | TakeLocal { local } => {
            capability(module, &f.locals[local.0 as usize].ty)
        }
        MakeAggregate { fields } => fields
            .iter()
            .any(|v| capability(module, &f.values[v.0 as usize])),
        ExtractField { aggregate, .. } | UpdateField { aggregate, .. } => {
            capability(module, &f.values[aggregate.0 as usize])
        }
        _ => true,
    }
}

pub fn extract(module: &TypedModule, f: &TypedFunction) -> R<Graph> {
    let mut work = Work::new(MAX_WORK, "extraction");
    work.charge(module.closures.len())?;
    let capture_bound = module
        .closures
        .iter()
        .map(|c| c.captures.len())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    work.charge(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_mul(capture_bound)
            .saturating_mul(4),
    )?;
    if f.foreign.is_some() {
        return Err("foreign declaration has no CFG".into());
    }
    if description(module, &f.return_type) {
        return Err(
            "Unsupported: resource-bearing returns are outside the frozen schema7 query".into(),
        );
    }
    let mut slots = Vec::new();
    let mut locals = vec![None; f.locals.len()];
    let mut values = vec![None; f.values.len()];
    let mut il = vec![None; f.locals.len()];
    let mut iv = vec![None; f.values.len()];
    let mut interaction = Vec::new();
    for (i, l) in f.locals.iter().enumerate() {
        if description(module, &l.ty) {
            locals[i] = Some(slots.len());
            slots.push(Slot::Local(i as u32));
        }
        if capability(module, &l.ty) {
            il[i] = Some(interaction.len());
            interaction.push(BTreeSet::new());
        }
    }
    for (i, t) in f.values.iter().enumerate() {
        if description(module, t) {
            values[i] = Some(slots.len());
            slots.push(Slot::Value(i as u32));
        }
        if capability(module, t) {
            iv[i] = Some(interaction.len());
            interaction.push(BTreeSet::new());
        }
    }
    if slots.len() > MAX_SLOTS {
        return Err("ResourceLimit: slots".into());
    }
    let mut starts = Vec::new();
    let mut total = 0usize;
    work.charge(f.blocks.len().saturating_mul(2))?;
    for b in &f.blocks {
        starts.push(total);
        total += b.instructions.len() + 1;
    }
    if total > MAX_NODES {
        return Err("ResourceLimit: nodes".into());
    }
    work.charge(total.saturating_mul(3))?;
    let mut nodes = Vec::with_capacity(total);
    let mut traps = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, ins) in block.instructions.iter().enumerate() {
            work.charge(
                crate::budget::instruction_bound(&ins.kind).saturating_add(
                    crate::budget::instruction_items(&ins.kind)
                        .saturating_mul(capture_bound)
                        .saturating_mul(3),
                ),
            )?;
            let mut uses = BTreeSet::new();
            let mut kill = BTreeSet::new();
            if let Some(v) = ins.result.and_then(|v| values[v.0 as usize]) {
                kill.insert(v);
            }
            use InstructionKind::*;
            match &ins.kind {
                Store { local, .. } | StorageLive(local) | StorageDead(local) => {
                    if let Some(s) = locals[local.0 as usize] {
                        kill.insert(s);
                    }
                }
                Load(local)
                | TakeLocal { local }
                | CallClosureShared { callee: local, .. }
                | CallClosureMut { callee: local, .. } => {
                    if let Some(s) = locals[local.0 as usize] {
                        uses.insert(s);
                    }
                }
                _ => {}
            }
            let ops = operands(&ins.kind);
            for op in &ops {
                if let Some(s) = values[op.0 as usize] {
                    uses.insert(s);
                }
            }
            let mut footprint = BTreeSet::new();
            if let Some(s) = ins.result.and_then(|v| iv[v.0 as usize]) {
                footprint.insert(s);
            }
            if let Some(s) = local_operand(&ins.kind).and_then(|l| il[l.0 as usize]) {
                footprint.insert(s);
            }
            for op in &ops {
                if let Some(s) = iv[op.0 as usize] {
                    footprint.insert(s);
                }
            }
            work.charge(footprint.len().saturating_mul(footprint.len()))?;
            for &x in &footprint {
                for &y in &footprint {
                    if x != y {
                        interaction[x].insert(y);
                    }
                }
            }
            let id = nodes.len();
            let mut edges = vec![Edge {
                to: id + 1,
                guard: None,
            }];
            if potential_failure(&ins.kind) {
                edges.push(Edge {
                    to: total + traps.len(),
                    guard: None,
                });
                traps.push(Node {
                    block: b,
                    ordinal: i,
                    uses: vec![],
                    kill: vec![],
                    observable: true,
                    label: format!("conservative-failure@{b}:{i}"),
                    edges: vec![],
                });
            }
            nodes.push(Node {
                block: b,
                ordinal: i,
                observable: event(module, f, &ins.kind) || !uses.is_empty() || !kill.is_empty(),
                uses: uses.into_iter().collect(),
                kill: kill.into_iter().collect(),
                label: format!("{:?}", ins.kind),
                edges,
            });
        }
        let mut uses = vec![];
        let (edges, observable) = match &block.terminator {
            Terminator::Jump(t) => (
                vec![Edge {
                    to: starts[t.0 as usize],
                    guard: None,
                }],
                t.0 as usize <= b,
            ),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => (
                vec![
                    Edge {
                        to: starts[then_block.0 as usize],
                        guard: Some((condition.0, true)),
                    },
                    Edge {
                        to: starts[else_block.0 as usize],
                        guard: Some((condition.0, false)),
                    },
                ],
                true,
            ),
            Terminator::Return(v) => {
                if let Some(s) = v.and_then(|v| values[v.0 as usize]) {
                    uses.push(s);
                }
                (vec![], true)
            }
            Terminator::Trap | Terminator::Unreachable => (vec![], true),
        };
        nodes.push(Node {
            block: b,
            ordinal: block.instructions.len(),
            uses,
            kill: vec![],
            observable,
            label: format!("{:?}", block.terminator),
            edges,
        });
    }
    let implicit_exits = traps.len();
    nodes.extend(traps);
    if nodes.len() > MAX_NODES || nodes.iter().map(|n| n.edges.len()).sum::<usize>() > MAX_EDGES {
        return Err("ResourceLimit: graph".into());
    }
    let entry = starts[f.entry.0 as usize];
    nodes[entry].observable = true;
    let mut reachable = vec![false; nodes.len()];
    let mut todo = vec![entry];
    while let Some(p) = todo.pop() {
        work.charge(1)?;
        if !reachable[p] {
            work.charge(nodes[p].edges.len())?;
            reachable[p] = true;
            todo.extend(nodes[p].edges.iter().map(|e| e.to));
        }
    }
    Ok(Graph {
        function: f.id.0,
        slots,
        nodes,
        starts,
        entry,
        reachable,
        implicit_exits,
        interaction,
    })
}

#[derive(Default, Clone, Debug)]
pub struct Metrics {
    pub kept: usize,
    pub eliminated: usize,
    pub edges: usize,
    pub remaining_edges: usize,
    pub max_separator: usize,
    pub max_fill: usize,
    pub retained_for_fill: usize,
    pub work: usize,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub active: Vec<bool>,
    pub edges: Vec<Vec<Edge>>,
    pub removed: Vec<(usize, Vec<usize>)>,
    pub metrics: Metrics,
}

pub fn plan(g: &Graph, cut: bool) -> R<Plan> {
    plan_with_limit(g, cut, MAX_WORK)
}
pub fn plan_with_limit(g: &Graph, cut: bool, limit: usize) -> R<Plan> {
    let n = g.nodes.len();
    let mut work = Work::new(limit, "plan");
    work.charge(n.saturating_mul(5))?;
    for node in &g.nodes {
        work.charge(node.edges.len().saturating_mul(2))?;
    }
    let mut edges = g.nodes.iter().map(|p| p.edges.clone()).collect::<Vec<_>>();
    let mut pred = vec![BTreeSet::new(); n];
    let mut active = vec![true; n];
    for (p, out) in edges.iter().enumerate() {
        for e in out {
            pred[e.to].insert(p);
        }
    }
    let mut removed = vec![];
    let mut metrics = Metrics {
        edges: edges.iter().map(Vec::len).sum(),
        ..Metrics::default()
    };
    let mut edge_count = metrics.edges;
    if cut {
        for p in (0..n).rev() {
            work.charge(1)?;
            if g.nodes[p].observable {
                continue;
            }
            if !g.nodes[p].uses.is_empty() || !g.nodes[p].kill.is_empty() {
                return Err("non-identity cut".into());
            }
            work.charge(pred[p].len().saturating_add(edges[p].len()))?;
            let prev = pred[p]
                .iter()
                .copied()
                .filter(|&x| x != p)
                .collect::<Vec<_>>();
            let next = edges[p]
                .iter()
                .map(|e| e.to)
                .filter(|&x| x != p)
                .collect::<BTreeSet<_>>();
            metrics.max_separator = metrics.max_separator.max(prev.len() + next.len());
            let mut incoming_count = 0usize;
            for &before in &prev {
                work.charge(edges[before].len().saturating_add(1))?;
                incoming_count += edges[before].iter().filter(|e| e.to == p).count();
            }
            let fill = incoming_count.saturating_mul(next.len());
            metrics.max_fill = metrics.max_fill.max(fill);
            if fill > FILL_LIMIT {
                metrics.retained_for_fill += 1;
                continue;
            }
            if edge_count.saturating_add(fill) > MAX_EDGES {
                return Err("ResourceLimit: planned edges".into());
            }
            work.charge(
                fill.saturating_add(prev.len())
                    .saturating_add(next.len())
                    .saturating_add(1),
            )?;
            for &before in &prev {
                work.charge(edges[before].len().saturating_mul(2))?;
                let incoming = edges[before]
                    .iter()
                    .filter(|e| e.to == p)
                    .copied()
                    .collect::<Vec<_>>();
                edge_count -= incoming.len();
                edges[before].retain(|e| e.to != p);
                for edge in incoming {
                    for &after in &next {
                        work.charge(edges[before].len().saturating_add(1))?;
                        let e = Edge {
                            to: after,
                            guard: edge.guard,
                        };
                        if !edges[before].contains(&e) {
                            if edge_count >= MAX_EDGES {
                                return Err("ResourceLimit: planned edges".into());
                            }
                            edges[before].push(e);
                            pred[after].insert(before);
                            edge_count += 1;
                        }
                    }
                }
                work.charge(edges[before].len().saturating_mul(edges[before].len()))?;
                edges[before].sort_unstable();
            }
            for &after in &next {
                pred[after].remove(&p);
            }
            pred[p].clear();
            edge_count -= edges[p].len();
            edges[p].clear();
            active[p] = false;
            removed.push((p, next.into_iter().collect()));
        }
    }
    metrics.eliminated = removed.len();
    metrics.kept = n - removed.len();
    metrics.remaining_edges = edges.iter().map(Vec::len).sum();
    if metrics.remaining_edges > MAX_EDGES {
        return Err("ResourceLimit: planned edges".into());
    }
    metrics.work = work.used();
    Ok(Plan {
        active,
        edges,
        removed,
        metrics,
    })
}

pub fn solve(g: &Graph, p: &Plan) -> R<(Vec<Row>, usize)> {
    let words = g.slots.len().div_ceil(64);
    let n = g.nodes.len();
    let mut work = Work::new(MAX_WORK, "solve and restore");
    work.charge(n.saturating_mul(words.saturating_add(4)))?;
    if words == 0 {
        return Ok((vec![vec![]; n], work.used()));
    }
    let mut rows = vec![vec![0; words]; n];
    let mut pred = vec![vec![]; n];
    for (a, es) in p.edges.iter().enumerate() {
        work.charge(es.len())?;
        for e in es {
            pred[e.to].push(a);
        }
    }
    let mut queued = p
        .active
        .iter()
        .zip(&g.reachable)
        .map(|(&a, &r)| a && r)
        .collect::<Vec<_>>();
    let mut todo = (0..n).rev().filter(|&x| queued[x]).collect::<VecDeque<_>>();
    while let Some(a) = todo.pop_front() {
        work.charge(
            words
                .saturating_mul(p.edges[a].len().saturating_add(2))
                .saturating_add(g.nodes[a].uses.len())
                .saturating_add(g.nodes[a].kill.len())
                .saturating_add(1),
        )?;
        queued[a] = false;
        let mut row = vec![0; words];
        for edge in &p.edges[a] {
            for (x, y) in row.iter_mut().zip(&rows[edge.to]) {
                *x |= *y;
            }
        }
        for &s in &g.nodes[a].kill {
            row[s / 64] &= !(1u64 << (s % 64));
        }
        for &s in &g.nodes[a].uses {
            row[s / 64] |= 1u64 << (s % 64);
        }
        if row != rows[a] {
            rows[a] = row;
            work.charge(pred[a].len())?;
            for &before in &pred[a] {
                if g.reachable[before] && !queued[before] {
                    queued[before] = true;
                    todo.push_back(before);
                }
            }
        }
    }
    for (id, next) in p.removed.iter().rev() {
        work.charge(1)?;
        if !g.reachable[*id] {
            continue;
        }
        work.charge(words.saturating_mul(next.len().saturating_add(1)))?;
        let mut row = vec![0; words];
        for &b in next {
            for (x, y) in row.iter_mut().zip(&rows[b]) {
                *x |= *y;
            }
        }
        rows[*id] = row;
    }
    Ok((rows, work.used()))
}

pub type Boundary = BTreeSet<(usize, Option<(u32, bool)>, usize)>;
pub fn boundaries(g: &Graph, p: &Plan) -> R<Boundary> {
    let mut result = BTreeSet::new();
    let mut work = Work::new(MAX_WORK, "boundary audit");
    work.charge(g.nodes.len())?;
    for (id, node) in g.nodes.iter().enumerate() {
        if !node.observable {
            continue;
        }
        if !p.active[id] {
            return Err("observed node was cut".into());
        }
        for edge in &p.edges[id] {
            work.charge(1)?;
            let mut seen = BTreeSet::new();
            let mut todo = vec![edge.to];
            while let Some(v) = todo.pop() {
                work.charge(1)?;
                if g.nodes[v].observable {
                    result.insert((id, edge.guard, v));
                    continue;
                }
                if seen.insert(v) {
                    work.charge(p.edges[v].len())?;
                    for e in &p.edges[v] {
                        if e.guard.is_some() {
                            return Err("branch inside transparent segment".into());
                        }
                        todo.push(e.to);
                    }
                }
            }
        }
    }
    Ok(result)
}

pub type Shape = (usize, usize, usize, f64, Option<usize>);
pub fn shape(g: &Graph) -> R<Shape> {
    Ok(shape_with_limit(g, MAX_WORK)?.0)
}
pub fn shape_with_limit(g: &Graph, limit: usize) -> R<(Shape, usize)> {
    let mut work = Work::new(limit, "shape census");
    let n = g.interaction.len();
    work.charge(n.saturating_mul(3))?;
    let edges = g.interaction.iter().map(BTreeSet::len).sum::<usize>() / 2;
    let density = if n > 1 {
        2.0 * edges as f64 / (n * (n - 1)) as f64
    } else {
        0.0
    };
    let mut largest = 0;
    let mut seen = BTreeSet::new();
    for i in 0..n {
        if seen.contains(&i) {
            continue;
        }
        let mut todo = vec![i];
        let mut count = 0;
        while let Some(v) = todo.pop() {
            work.charge(1)?;
            if seen.insert(v) {
                work.charge(g.interaction[v].len())?;
                count += 1;
                todo.extend(g.interaction[v].iter().copied());
            }
        }
        largest = largest.max(count);
    }
    // This is a topology census, not the candidate's solving order or a treewidth proof.
    let width = if n > 256 {
        None
    } else {
        work.charge(n.saturating_add(edges.saturating_mul(2)))?;
        let mut graph = g.interaction.clone();
        let mut active = (0..n).collect::<BTreeSet<_>>();
        let mut width = 0;
        while !active.is_empty() {
            let mut best = None;
            for &v in &active {
                let degree = graph[v].len();
                work.charge(
                    degree
                        .saturating_mul(degree)
                        .saturating_add(degree)
                        .saturating_add(1),
                )?;
                let ns = graph[v].iter().copied().collect::<Vec<_>>();
                let mut fill = 0;
                for (i, &a) in ns.iter().enumerate() {
                    for &b in &ns[i + 1..] {
                        fill += usize::from(!graph[a].contains(&b));
                    }
                }
                let key = (fill, degree, v);
                if best.is_none_or(|old| key < old) {
                    best = Some(key);
                }
            }
            let v = best.expect("nonempty active graph").2;
            work.charge(
                graph[v]
                    .len()
                    .saturating_mul(graph[v].len())
                    .saturating_add(graph[v].len().saturating_mul(2))
                    .saturating_add(1),
            )?;
            let ns = graph[v].iter().copied().collect::<Vec<_>>();
            width = width.max(ns.len());
            for &a in &ns {
                graph[a].remove(&v);
                for &b in &ns {
                    if a != b {
                        graph[a].insert(b);
                    }
                }
            }
            graph[v].clear();
            active.remove(&v);
        }
        Some(width)
    };
    Ok(((n, edges, largest, density, width), work.used()))
}
