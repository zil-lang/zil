//! Analyze's own call-control reconstruction. Because every admitted actual
//! parameter call has one requested mode, private rows retain exact event
//! counts; full public call vectors are checked when the rows are compared.
use super::*;
use zil_ir::{CallableFlowNext, CallableParameterContract, CallableTraces};

enum End {
    Links(Vec<usize>),
    Exit(CallableExit),
    Undefined,
}
struct Row {
    calls: usize,
    end: End,
}

#[derive(Default)]
struct Counts {
    rows: usize,
    events: usize,
    edges: usize,
}

fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Analyze,
        "helper flow reconstruction allowance exhausted",
        None,
    )]
}

impl Counts {
    fn add(
        &mut self,
        rows: usize,
        events: usize,
        edges: usize,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let r = self
            .rows
            .checked_add(rows)
            .filter(|n| *n <= budget.helper_nodes && u32::try_from(*n).is_ok())
            .ok_or_else(exhausted)?;
        let c = self
            .events
            .checked_add(events)
            .filter(|n| *n <= budget.helper_calls)
            .ok_or_else(exhausted)?;
        let e = self
            .edges
            .checked_add(edges)
            .filter(|n| *n <= budget.helper_edges)
            .ok_or_else(exhausted)?;
        let work = rows
            .checked_add(events)
            .and_then(|n| n.checked_add(edges))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(exhausted)?;
        budget.spend(work, "helper rows and relocation before copying")?;
        (self.rows, self.events, self.edges) = (r, c, e);
        Ok(())
    }
}

fn append_child(
    rows: &mut Vec<Row>,
    child: &CallableTraces,
    kind: CallableKind,
    counts: &mut Counts,
    budget: &mut Budget,
) -> CompileResult<(usize, usize)> {
    let (mut added_rows, mut calls, mut edges) = (1usize, 0usize, 1usize);
    match child {
        CallableTraces::Finite(paths) => {
            budget.spend(paths.len().saturating_add(1), "nested finite row shape")?;
            if paths.is_empty() {
                return Err(bad("nested finite helper has no call path"));
            }
            added_rows = added_rows
                .checked_add(paths.len())
                .and_then(|n| n.checked_add(1))
                .ok_or_else(exhausted)?;
            edges = edges.checked_add(paths.len()).ok_or_else(exhausted)?;
            for path in paths {
                budget.spend(path.calls.len(), "nested finite invocation modes")?;
                if path.calls.iter().any(|mode| *mode != kind) {
                    return Err(bad("nested helper invocation mode differs"));
                }
                calls = calls.checked_add(path.calls.len()).ok_or_else(exhausted)?;
                if path.exit == CallableExit::Return {
                    edges = edges.checked_add(1).ok_or_else(exhausted)?;
                }
            }
        }
        CallableTraces::Flow(flow) => {
            budget.spend(flow.nodes.len().saturating_add(1), "nested flow row shape")?;
            if flow.entry != 0 || flow.nodes.is_empty() {
                return Err(bad("nested helper has no canonical flow entry"));
            }
            added_rows = added_rows
                .checked_add(flow.nodes.len())
                .ok_or_else(exhausted)?;
            for node in &flow.nodes {
                budget.spend(node.calls.len(), "nested flow invocation modes")?;
                if node.calls.iter().any(|mode| *mode != kind) {
                    return Err(bad("nested helper invocation mode differs"));
                }
                calls = calls.checked_add(node.calls.len()).ok_or_else(exhausted)?;
                let links = match &node.next {
                    CallableFlowNext::Edges(targets) => {
                        budget.spend(targets.len(), "nested flow edge metadata")?;
                        targets.len()
                    }
                    CallableFlowNext::Exit(CallableExit::Return) => 1,
                    CallableFlowNext::Exit(CallableExit::Trap) => 0,
                };
                edges = edges.checked_add(links).ok_or_else(exhausted)?;
            }
        }
    }
    counts.add(added_rows, calls, edges, budget)?;
    let entry = rows.len();
    let continuation = entry + added_rows - 1;
    match child {
        CallableTraces::Finite(paths) => {
            rows.push(Row {
                calls: 0,
                end: End::Links((1..=paths.len()).map(|n| entry + n).collect()),
            });
            for path in paths {
                rows.push(Row {
                    calls: path.calls.len(),
                    end: if path.exit == CallableExit::Return {
                        End::Links(vec![continuation])
                    } else {
                        End::Exit(CallableExit::Trap)
                    },
                });
            }
        }
        CallableTraces::Flow(flow) => {
            for node in &flow.nodes {
                let end = match &node.next {
                    CallableFlowNext::Edges(targets) => {
                        End::Links(targets.iter().map(|t| entry + *t as usize).collect())
                    }
                    CallableFlowNext::Exit(CallableExit::Return) => End::Links(vec![continuation]),
                    CallableFlowNext::Exit(CallableExit::Trap) => End::Exit(CallableExit::Trap),
                };
                rows.push(Row {
                    calls: node.calls.len(),
                    end,
                });
            }
        }
    }
    rows.push(Row {
        calls: 0,
        end: End::Undefined,
    });
    Ok((entry, continuation))
}

fn actual_call(
    instruction: &Instruction,
    locals: &[u8],
    values: &[u8],
    borrowed: bool,
) -> CompileResult<Option<CallableKind>> {
    let (origin, kind, address) = match instruction.kind {
        InstructionKind::CallClosure { callee, .. } => {
            (values[callee.0 as usize], CallableKind::Fn, false)
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            (values[callee.0 as usize], CallableKind::FnOnce, false)
        }
        InstructionKind::CallClosureShared { callee, .. } => {
            (locals[callee.0 as usize], CallableKind::Fn, false)
        }
        InstructionKind::CallClosureMut { callee, .. } => {
            (locals[callee.0 as usize], CallableKind::FnMut, false)
        }
        InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
            (values[callee.0 as usize], kind, true)
        }
        InstructionKind::DropClosure { local } if borrowed && locals[local.0 as usize] & 1 != 0 => {
            return Err(bad("borrowed helper cannot destroy its environment"));
        }
        InstructionKind::MakeClosure { .. } => {
            return Err(bad(
                "helper-local nested environments require an additional contract",
            ));
        }
        _ => return Ok(None),
    };
    Ok(parameter_invocation(origin, address)?.then_some(kind))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    module: &TypedModule,
    function: &TypedFunction,
    claim: &CallableParameterContract,
    local: LocalId,
    borrowed: bool,
    checked: &[bool],
    budget: &mut Budget,
) -> CompileResult<()> {
    let (locals, values) = origins(module, function, local, budget)?;
    let mut counts = Counts::default();
    counts.add(function.blocks.len(), 0, 0, budget)?;
    let mut rows: Vec<_> = (0..function.blocks.len())
        .map(|_| Row {
            calls: 0,
            end: End::Undefined,
        })
        .collect();
    for (index, block) in function.blocks.iter().enumerate() {
        budget.spend(1, "helper actual block")?;
        let mut output = index;
        for instruction in &block.instructions {
            budget.spend(1, "helper actual instruction")?;
            if let Some(child) = forwarded(
                module,
                function,
                instruction,
                &values,
                claim.kind,
                checked,
                budget,
            )? {
                let (entry, continuation) =
                    append_child(&mut rows, child, claim.kind, &mut counts, budget)?;
                rows[output].end = End::Links(vec![entry]);
                output = continuation;
            } else if let Some(kind) = actual_call(instruction, &locals, &values, borrowed)? {
                if kind != claim.kind {
                    return Err(bad("helper callback call mode differs from its bound"));
                }
                counts.add(0, 1, 0, budget)?;
                rows[output].calls += 1;
            }
        }
        rows[output].end = match block.terminator {
            Terminator::Return(_) => End::Exit(CallableExit::Return),
            Terminator::Trap => End::Exit(CallableExit::Trap),
            Terminator::Jump(target) => {
                counts.add(0, 0, 1, budget)?;
                End::Links(vec![target.0 as usize])
            }
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                let count = if then_block == else_block { 1 } else { 2 };
                counts.add(0, 0, count, budget)?;
                let mut targets = vec![then_block.0 as usize];
                if then_block != else_block {
                    targets.push(else_block.0 as usize);
                }
                End::Links(targets)
            }
            Terminator::Unreachable => End::Undefined,
        };
    }
    let rows = numbered(rows, function.entry.0 as usize, budget)?;
    let n = rows.len();
    budget.product(n, 3, 1, "helper cycle frontier")?;
    let mut predecessors = vec![0usize; n];
    for row in &rows {
        budget.spend(1, "helper row successors")?;
        if let End::Links(targets) = &row.end {
            for target in targets {
                budget.spend(1, "helper row predecessor")?;
                predecessors[*target] += 1;
            }
        }
    }
    let mut ready: VecDeque<_> = predecessors
        .iter()
        .enumerate()
        .filter_map(|(i, degree)| (*degree == 0).then_some(i))
        .collect();
    let mut ordered = Vec::with_capacity(n);
    while let Some(row) = ready.pop_front() {
        budget.spend(1, "helper cycle frontier visit")?;
        ordered.push(row);
        if let End::Links(targets) = &rows[row].end {
            for target in targets {
                budget.spend(1, "helper cycle frontier edge")?;
                predecessors[*target] -= 1;
                if predecessors[*target] == 0 {
                    ready.push_back(*target);
                }
            }
        }
    }
    if ordered.len() != n {
        if claim.kind == CallableKind::FnOnce {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "cyclic FnOnce helper requires an additional consumption contract",
                Some(function.span),
            )]);
        }
        let CallableTraces::Flow(flow) = &claim.traces else {
            return Err(bad("cyclic helper requires its exact Flow contract"));
        };
        budget.spend(
            flow.nodes.len().saturating_add(1),
            "helper public flow comparison",
        )?;
        if flow.entry != 0 || flow.nodes.len() != n {
            return Err(bad("public helper flow differs from actual CFG"));
        }
        for (row, node) in rows.iter().zip(&flow.nodes) {
            budget.spend(
                node.calls.len().saturating_add(1),
                "helper flow calls comparison",
            )?;
            if row.calls != node.calls.len() || node.calls.iter().any(|kind| *kind != claim.kind) {
                return Err(bad("public helper flow omits or changes calls"));
            }
            match (&row.end, &node.next) {
                (End::Exit(actual), CallableFlowNext::Exit(claimed)) if actual == claimed => {}
                (End::Links(actual), CallableFlowNext::Edges(claimed)) => {
                    budget.spend(claimed.len(), "helper ordered flow edge comparison")?;
                    if actual.len() != claimed.len()
                        || actual.iter().zip(claimed).any(|(a, b)| *a != *b as usize)
                    {
                        return Err(bad("public helper flow changes actual ordered edges"));
                    }
                }
                _ => return Err(bad("public helper flow changes its exit")),
            }
        }
        return Ok(());
    }
    let CallableTraces::Finite(paths) = &claim.traces else {
        return Err(bad("acyclic helper requires its exact Finite contract"));
    };
    budget.spend(n, "finite helper prefix tables")?;
    let mut prefixes = vec![BTreeSet::<usize>::new(); n];
    prefixes[0].insert(0);
    let mut traces = BTreeSet::new();
    for index in ordered {
        budget.spend(1, "finite helper row")?;
        let row = &rows[index];
        for prefix in std::mem::take(&mut prefixes[index]) {
            budget.spend(1, "finite helper count addition")?;
            let calls = prefix.checked_add(row.calls).ok_or_else(exhausted)?;
            if claim.kind == CallableKind::FnOnce && calls > 1 {
                return Err(bad("helper exceeds its finite call allowance"));
            }
            match &row.end {
                End::Exit(exit) => {
                    budget.lookup(traces.len(), "finite helper exact exit")?;
                    traces.insert((calls, *exit));
                }
                End::Links(targets) => {
                    for target in targets {
                        budget.lookup(prefixes[*target].len(), "finite helper exact prefix")?;
                        prefixes[*target].insert(calls);
                    }
                }
                End::Undefined => unreachable!("numbered reachable rows have defined successors"),
            }
        }
    }
    budget.spend(paths.len(), "helper public finite comparison")?;
    if traces.len() != paths.len() {
        return Err(bad("public helper trace claim differs from its CFG"));
    }
    for ((calls, exit), path) in traces.into_iter().zip(paths) {
        budget.spend(path.calls.len(), "helper public finite calls")?;
        if calls != path.calls.len()
            || exit != path.exit
            || path.calls.iter().any(|kind| *kind != claim.kind)
        {
            return Err(bad("public helper trace claim differs from its CFG"));
        }
    }
    Ok(())
}

fn numbered(mut rows: Vec<Row>, entry: usize, budget: &mut Budget) -> CompileResult<Vec<Row>> {
    budget.product(rows.len(), 3, 1, "helper BFS mapping tables")?;
    if entry >= rows.len() {
        return Err(bad("helper entry outside CFG"));
    }
    let mut mapped = vec![None; rows.len()];
    let mut pending = VecDeque::from([entry]);
    mapped[entry] = Some(0usize);
    let mut allocated = 1usize;
    let mut result = Vec::new();
    while let Some(index) = pending.pop_front() {
        budget.spend(1, "helper BFS row")?;
        let end = std::mem::replace(&mut rows[index].end, End::Undefined);
        let end = match end {
            End::Links(mut targets) => {
                for target in &mut targets {
                    budget.spend(2, "helper BFS edge and enqueue")?;
                    if *target >= mapped.len() {
                        return Err(bad("helper flow target outside CFG"));
                    }
                    let id = if let Some(id) = mapped[*target] {
                        id
                    } else {
                        let id = allocated;
                        allocated += 1;
                        mapped[*target] = Some(id);
                        pending.push_back(*target);
                        id
                    };
                    *target = id;
                }
                End::Links(targets)
            }
            End::Exit(exit) => End::Exit(exit),
            End::Undefined => return Err(bad("helper has a reachable undefined exit")),
        };
        result.push(Row {
            calls: rows[index].calls,
            end,
        });
    }
    Ok(result)
}
