//! Analyze's own ordered call-control reconstruction. Every role substitution
//! belongs to a particular static call site, including two calls of one child.
use super::*;
use zil_ir::{CallableFlowNext, CallableParameterContract, CallableTraces};

enum End {
    Links(Vec<usize>),
    Exit(CallableExit),
    Undefined,
}
struct Row {
    calls: Vec<CallEvent>,
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
    child: &Forwarded<'_>,
    counts: &mut Counts,
    budget: &mut Budget,
) -> CompileResult<(usize, usize)> {
    let (mut added_rows, mut calls, mut edges) = (1usize, 0usize, 1usize);
    match child.traces {
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
                budget.spend(path.calls.len(), "nested finite invocation shape")?;
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
                budget.spend(node.calls.len(), "nested flow invocation shape")?;
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
    match child.traces {
        CallableTraces::Finite(paths) => {
            rows.push(Row {
                calls: Vec::new(),
                end: End::Links((1..=paths.len()).map(|n| entry + n).collect()),
            });
            for path in paths {
                rows.push(Row {
                    calls: remap(&path.calls, &child.roles, budget)?,
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
                    calls: remap(&node.calls, &child.roles, budget)?,
                    end,
                });
            }
        }
    }
    rows.push(Row {
        calls: Vec::new(),
        end: End::Undefined,
    });
    Ok((entry, continuation))
}

fn remap(
    events: &[CallEvent],
    roles: &[(u32, Option<u32>)],
    budget: &mut Budget,
) -> CompileResult<Vec<CallEvent>> {
    budget.product(
        events.len(),
        roles.len().saturating_add(2),
        1,
        "child ordered role substitution",
    )?;
    events
        .iter()
        .filter_map(|event| {
            match roles.iter().find(|(child, _)| *child == event.parameter) {
                Some((_, Some(parameter))) => Some(Ok(CallEvent {
                    parameter: *parameter,
                    kind: event.kind,
                })),
                // This child role belongs to an independently checked actual
                // concrete value, so it contributes no wrapper F event.
                Some((_, None)) => None,
                None => Some(Err(bad("child event names no actual forwarded role"))),
            }
        })
        .collect()
}

fn actual_call(
    instruction: &Instruction,
    locals: &[u8],
    values: &[u8],
    borrowed_roles: u8,
    roles: &[CallableRole],
    concrete: bool,
) -> CompileResult<Option<CallEvent>> {
    let (origin, kind, address) = match instruction.kind {
        InstructionKind::CallClosure { callee, .. } => {
            (values[callee.0 as usize], CallableKind::Fn, false)
        }
        InstructionKind::CallClosureOnce { callee, .. }
        | InstructionKind::WithScopedThread { body: callee, .. } => {
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
        InstructionKind::DropClosure { local }
            if locals[local.0 as usize] & borrowed_roles != 0 =>
        {
            return Err(bad("borrowed helper cannot destroy its environment"));
        }
        InstructionKind::MakeClosure { .. } => {
            return Err(bad(
                "helper-local nested environments require an additional contract",
            ));
        }
        _ => return Ok(None),
    };
    if concrete && origin == FACTORY {
        return Ok(None);
    }
    let Some(origin) = parameter_invocation(origin, address)? else {
        return Ok(None);
    };
    let role = roles
        .get(origin)
        .ok_or_else(|| bad("helper call refers to an absent role"))?;
    if role.kind != kind {
        return Err(bad("helper callback call mode differs from its bound"));
    }
    Ok(Some(CallEvent {
        parameter: role.parameter,
        kind,
    }))
}

fn scoped_event(
    origin: u8,
    wanted: CallableKind,
    borrowed: bool,
    roles: &[CallableRole],
    concrete: bool,
) -> CompileResult<Option<CallEvent>> {
    if concrete && origin == FACTORY {
        return Ok(None);
    }
    let Some(index) = parameter_invocation(origin, borrowed)? else {
        return Ok(None);
    };
    let role = roles
        .get(index)
        .ok_or_else(|| bad("scoped call has no actual callable role"))?;
    if !crate::forward::compatible(role.kind, wanted) {
        return Err(bad("scoped call upgrades its original callable role"));
    }
    Ok(Some(CallEvent {
        parameter: role.parameter,
        kind: wanted,
    }))
}

fn once_paths(rows: &[Row], roles: &BTreeSet<u32>, budget: &mut Budget) -> CompileResult<()> {
    for role in roles {
        budget.product(rows.len(), 2, 1, "scoped once-role path states")?;
        let mut seen = vec![[false; 2]; rows.len()];
        let mut pending = VecDeque::from([(0usize, false)]);
        seen[0][0] = true;
        while let Some((index, invoked)) = pending.pop_front() {
            budget.spend(
                rows[index].calls.len().saturating_add(1),
                "scoped once-role events",
            )?;
            let count = rows[index]
                .calls
                .iter()
                .filter(|event| event.parameter == *role)
                .count();
            if count > usize::from(!invoked) {
                return Err(bad("scoped helper can invoke a consumed body role again"));
            }
            let invoked = invoked || count != 0;
            if let End::Links(next) = &rows[index].end {
                for target in next {
                    budget.spend(1, "scoped once-role edge")?;
                    if !seen[*target][usize::from(invoked)] {
                        seen[*target][usize::from(invoked)] = true;
                        pending.push_back((*target, invoked));
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    module: &TypedModule,
    function: &TypedFunction,
    claim: &CallableParameterContract,
    parameters: &[(u32, LocalId)],
    borrowed_roles: u8,
    checked: &[bool],
    budget: &mut Budget,
) -> CompileResult<()> {
    let precise = if budget
        .forward_functions
        .get(function.id.0 as usize)
        .copied()
        .unwrap_or(false)
    {
        Some(super::forward::prove(
            module, function, claim, parameters, checked, budget,
        )?)
    } else {
        None
    };
    let (locals, values) = if precise.is_none() {
        origins(module, function, parameters, budget)?
    } else {
        (Vec::new(), Vec::new())
    };
    let mut counts = Counts::default();
    let mut scoped_bodies = BTreeSet::new();
    let mut has_scoped = false;
    counts.add(function.blocks.len(), 0, 0, budget)?;
    let mut rows: Vec<_> = (0..function.blocks.len())
        .map(|_| Row {
            calls: Vec::new(),
            end: End::Undefined,
        })
        .collect();
    for (index, block) in function.blocks.iter().enumerate() {
        budget.spend(1, "helper actual block")?;
        let mut output = index;
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1, "helper actual instruction")?;
            let (locals, values) = if let Some(precise) = &precise {
                let Some(snapshot) = &precise[index][position] else {
                    continue;
                };
                if let InstructionKind::MakeClosure { captures, .. } = &instruction.kind {
                    budget.spend(captures.len(), "concrete helper capture sources")?;
                    for value in captures {
                        if env(&function.values[value.0 as usize]).is_some()
                            && snapshot.values[value.0 as usize] != FACTORY
                        {
                            return Err(bad(
                                "local environment capture has a symbolic, mixed or unknown callable source",
                            ));
                        }
                    }
                    continue;
                }
                (&snapshot.locals[..], &snapshot.values[..])
            } else {
                (&locals[..], &values[..])
            };
            if let InstructionKind::WithForeignCallback { callback, body, .. } = instruction.kind {
                has_scoped = true;
                let f = scoped_event(
                    values[callback.0 as usize],
                    CallableKind::FnMut,
                    true,
                    &claim.roles,
                    precise.is_some(),
                )?;
                let b = scoped_event(
                    values[body.0 as usize],
                    CallableKind::FnOnce,
                    false,
                    &claim.roles,
                    precise.is_some(),
                )?;
                if let Some(b) = &b {
                    budget.lookup(scoped_bodies.len(), "scoped actual body role")?;
                    scoped_bodies.insert(b.parameter);
                }
                counts.add(
                    4,
                    usize::from(f.is_some()) * 2 + usize::from(b.is_some()),
                    10,
                    budget,
                )?;
                let before = rows.len();
                let body = before + 1;
                let after = before + 2;
                let continuation = before + 3;
                rows[output].end = End::Links(vec![before, body, continuation]);
                rows.push(Row {
                    calls: f.into_iter().collect(),
                    end: End::Links(vec![before, body, continuation]),
                });
                rows.push(Row {
                    calls: b.into_iter().collect(),
                    end: End::Links(vec![after, continuation]),
                });
                rows.push(Row {
                    calls: f.into_iter().collect(),
                    end: End::Links(vec![after, continuation]),
                });
                rows.push(Row {
                    calls: vec![],
                    end: End::Undefined,
                });
                output = continuation;
            } else if let Some(child) = forwarded(
                module,
                function,
                instruction,
                values,
                &claim.roles,
                checked,
                budget,
            )? {
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                    && !budget.scoped_helper_bodies.is_empty()
                {
                    budget.lookup(
                        budget.scoped_helper_bodies.len(),
                        "scoped child consumed roles lookup",
                    )?;
                    if let Some(n) = budget.scoped_helper_bodies.get(&callee).map(BTreeSet::len) {
                        has_scoped = true;
                        budget.product(
                            n,
                            child.roles.len().saturating_add(1),
                            1,
                            "scoped child consumed role substitution",
                        )?;
                        let bodies: Vec<_> = budget.scoped_helper_bodies[&callee]
                            .iter()
                            .copied()
                            .collect();
                        for body in bodies {
                            let parent = child
                                .roles
                                .iter()
                                .find_map(|(from, to)| (*from == body).then_some(*to))
                                .ok_or_else(|| {
                                    bad("scoped child body has no actual parent role")
                                })?;
                            if let Some(parent) = parent {
                                budget
                                    .lookup(scoped_bodies.len(), "scoped parent consumed role")?;
                                scoped_bodies.insert(parent);
                            }
                        }
                    }
                }
                let (entry, continuation) = append_child(&mut rows, &child, &mut counts, budget)?;
                rows[output].end = End::Links(vec![entry]);
                output = continuation;
            } else if let Some(event) = actual_call(
                instruction,
                locals,
                values,
                borrowed_roles,
                &claim.roles,
                precise.is_some(),
            )? {
                counts.add(0, 1, 0, budget)?;
                rows[output].calls.push(event);
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
    if has_scoped {
        budget.spend(claim.roles.len(), "scoped original once-role inventory")?;
        for role in &claim.roles {
            if role.kind == CallableKind::FnOnce {
                scoped_bodies.insert(role.parameter);
            }
        }
        once_paths(&rows, &scoped_bodies, budget)?;
        budget.spend(scoped_bodies.len(), "scoped consumed role publication")?;
        budget.lookup(
            budget.scoped_helper_bodies.len(),
            "scoped helper publication",
        )?;
        budget
            .scoped_helper_bodies
            .insert(function.id, scoped_bodies);
    }
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
        if !has_scoped
            && precise.is_none()
            && claim
                .roles
                .iter()
                .any(|role| role.kind == CallableKind::FnOnce)
        {
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
            if row.calls != node.calls {
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
    let mut prefixes = vec![BTreeSet::<Vec<CallEvent>>::new(); n];
    prefixes[0].insert(Vec::new());
    let mut traces = BTreeSet::new();
    for index in ordered {
        budget.spend(1, "finite helper row")?;
        let row = &rows[index];
        for mut calls in std::mem::take(&mut prefixes[index]) {
            let length = calls
                .len()
                .checked_add(row.calls.len())
                .filter(|n| *n <= budget.helper_calls)
                .ok_or_else(exhausted)?;
            budget.product(length, 2, 1, "finite helper ordered prefix concatenation")?;
            calls.extend_from_slice(&row.calls);
            budget.product(
                calls.len(),
                claim.roles.len(),
                1,
                "finite helper per-role consumption",
            )?;
            for role in &claim.roles {
                if role.kind == CallableKind::FnOnce
                    && calls
                        .iter()
                        .filter(|event| event.parameter == role.parameter)
                        .count()
                        > 1
                {
                    return Err(bad("helper exceeds a role's finite call allowance"));
                }
            }
            match &row.end {
                End::Exit(exit) => {
                    sequence_lookup(traces.len(), calls.len(), budget)?;
                    traces.insert(CallableTrace { calls, exit: *exit });
                }
                End::Links(targets) => {
                    for target in targets {
                        sequence_lookup(prefixes[*target].len(), calls.len(), budget)?;
                        budget.product(calls.len(), 2, 1, "finite helper branch prefix copy")?;
                        prefixes[*target].insert(calls.clone());
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
    for (actual, path) in traces.into_iter().zip(paths) {
        budget.product(
            path.calls.len().saturating_add(1),
            2,
            1,
            "helper public ordered finite calls",
        )?;
        if actual != *path {
            return Err(bad("public helper trace claim differs from its CFG"));
        }
    }
    Ok(())
}

fn sequence_lookup(size: usize, length: usize, budget: &mut Budget) -> CompileResult<()> {
    let levels = size.checked_ilog2().map_or(1, |n| n as usize + 2);
    budget.product(
        length.saturating_add(1),
        levels,
        2,
        "finite helper ordered sequence comparison",
    )
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
            calls: std::mem::take(&mut rows[index].calls),
            end,
        });
    }
    Ok(result)
}
