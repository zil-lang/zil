//! Construct the helper's call language from its concrete CFG. Temporary block
//! fragments are bounded before copying a child; only the reachable graph is
//! canonically numbered. Nothing here evaluates a runtime iteration.
use super::*;
use std::collections::VecDeque;
use zil_ir::{CallableFlow, CallableFlowNext, CallableFlowNode, CallableTraces};

struct Fragment {
    calls: Vec<CallableKind>,
    next: Option<CallableFlowNext>,
}

#[derive(Default)]
struct Size {
    nodes: usize,
    calls: usize,
    edges: usize,
}

fn limited(span: Span) -> Diagnostic {
    Diagnostic::limit(
        Phase::Types,
        "helper flow size allowance exhausted",
        Some(span),
    )
}

impl Size {
    fn reserve(
        &mut self,
        nodes: usize,
        calls: usize,
        edges: usize,
        limits: &Limits,
        state: &mut closures::State,
        span: Span,
    ) -> R<()> {
        let next_nodes = self
            .nodes
            .checked_add(nodes)
            .filter(|n| *n <= limits.max_nodes && u32::try_from(*n).is_ok())
            .ok_or_else(|| limited(span))?;
        let next_calls = self
            .calls
            .checked_add(calls)
            .filter(|n| *n <= limits.max_instructions)
            .ok_or_else(|| limited(span))?;
        let next_edges = self
            .edges
            .checked_add(edges)
            .filter(|n| *n <= limits.max_analysis_steps)
            .ok_or_else(|| limited(span))?;
        let work = nodes
            .checked_add(calls)
            .and_then(|n| n.checked_add(edges))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| limited(span))?;
        // Reserve owned copying and relocation independently of the preceding
        // metadata scan. No child vector has been cloned at this point.
        state.snapshot(work, span)?;
        (self.nodes, self.calls, self.edges) = (next_nodes, next_calls, next_edges);
        Ok(())
    }
}

fn child_size(child: &CallableTraces, kind: CallableKind, span: Span) -> R<Size> {
    let mut size = Size {
        nodes: 1,
        calls: 0,
        edges: 1,
    }; // continuation and incoming edge
    let mut add_calls = |calls: &[CallableKind]| -> R<()> {
        if calls.iter().any(|mode| *mode != kind) {
            return Err(unsupported(
                span,
                "nested helper contains a different invocation kind",
            ));
        }
        size.calls = size
            .calls
            .checked_add(calls.len())
            .ok_or_else(|| limited(span))?;
        Ok(())
    };
    match child {
        CallableTraces::Finite(paths) => {
            if paths.is_empty() {
                return Err(unsupported(span, "nested helper has no defined call path"));
            }
            size.nodes = size
                .nodes
                .checked_add(paths.len())
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| limited(span))?;
            size.edges = size
                .edges
                .checked_add(paths.len())
                .ok_or_else(|| limited(span))?;
            for path in paths {
                add_calls(&path.calls)?;
                if path.exit == CallableExit::Return {
                    size.edges = size.edges.checked_add(1).ok_or_else(|| limited(span))?;
                }
            }
        }
        CallableTraces::Flow(flow) => {
            if flow.nodes.is_empty() || flow.entry != 0 {
                return Err(unsupported(
                    span,
                    "nested helper flow has no canonical entry",
                ));
            }
            size.nodes = size
                .nodes
                .checked_add(flow.nodes.len())
                .ok_or_else(|| limited(span))?;
            for node in &flow.nodes {
                add_calls(&node.calls)?;
                let edges = match &node.next {
                    CallableFlowNext::Edges(next) => next.len(),
                    CallableFlowNext::Exit(CallableExit::Return) => 1,
                    CallableFlowNext::Exit(CallableExit::Trap) => 0,
                };
                size.edges = size.edges.checked_add(edges).ok_or_else(|| limited(span))?;
            }
        }
    }
    Ok(size)
}

fn child_work(child: &CallableTraces, span: Span) -> R<usize> {
    let mut count = 1usize;
    match child {
        CallableTraces::Finite(paths) => {
            for path in paths {
                count = count
                    .checked_add(path.calls.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| limited(span))?;
            }
        }
        CallableTraces::Flow(flow) => {
            for node in &flow.nodes {
                count = count
                    .checked_add(node.calls.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| limited(span))?;
                if let CallableFlowNext::Edges(next) = &node.next {
                    count = count.checked_add(next.len()).ok_or_else(|| limited(span))?;
                }
            }
        }
    }
    Ok(count)
}

fn callback(
    instruction: &Instruction,
    locals: &[u8],
    values: &[u8],
    requested: CallableKind,
) -> R<Option<CallableKind>> {
    let (origin, mode, borrowed) = match instruction.kind {
        InstructionKind::CallClosure { callee, .. } => {
            (values[callee.0 as usize], CallableKind::Fn, false)
        }
        InstructionKind::CallClosureShared { callee, .. } => {
            (locals[callee.0 as usize], CallableKind::Fn, false)
        }
        InstructionKind::CallClosureMut { callee, .. } => {
            (locals[callee.0 as usize], CallableKind::FnMut, false)
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            (values[callee.0 as usize], CallableKind::FnOnce, false)
        }
        InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
            (values[callee.0 as usize], kind, true)
        }
        _ => return Ok(None),
    };
    if origin == 2 && !borrowed {
        return Ok(None);
    }
    if origin != 1 {
        return Err(unsupported(
            instruction.span,
            "helper callable origin is unknown, mixed, or a borrowed local factory result",
        ));
    }
    if mode != requested {
        return Err(error(
            "E-CLOSURE-MODE",
            instruction.span,
            "helper callback mode differs from its bound",
        ));
    }
    Ok(Some(mode))
}

pub(super) fn contract(
    function: &TypedFunction,
    bound: &CoreCallableParameter,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<CallableTraces> {
    let parameter = function.parameters[bound.index as usize];
    let (locals, values) = environment_origins(function, parameter, state, span)?;
    let mut size = Size::default();
    size.reserve(function.blocks.len(), 0, 0, limits, state, span)?;
    let mut fragments: Vec<_> = (0..function.blocks.len())
        .map(|_| Fragment {
            calls: Vec::new(),
            next: None,
        })
        .collect();
    for (block_id, block) in function.blocks.iter().enumerate() {
        state.snapshot(1, span)?;
        let mut current = block_id;
        for instruction in &block.instructions {
            state.snapshot(1, instruction.span)?;
            if let Some(position) =
                forwarded_contract(function, instruction, &values, bound.kind, state)?
            {
                let child = &state.helper_functions[position]
                    .callable_contract
                    .as_ref()
                    .unwrap()
                    .traces;
                let rows = match child {
                    CallableTraces::Finite(paths) => paths.len(),
                    CallableTraces::Flow(flow) => flow.nodes.len(),
                };
                state.snapshot(rows, instruction.span)?;
                let child = &state.helper_functions[position]
                    .callable_contract
                    .as_ref()
                    .unwrap()
                    .traces;
                let scan = child_work(child, instruction.span)?;
                state.snapshot(scan, instruction.span)?;
                let child = &state.helper_functions[position]
                    .callable_contract
                    .as_ref()
                    .unwrap()
                    .traces;
                let addition = child_size(child, bound.kind, instruction.span)?;
                size.reserve(
                    addition.nodes,
                    addition.calls,
                    addition.edges,
                    limits,
                    state,
                    instruction.span,
                )?;
                let child = state.helper_functions[position]
                    .callable_contract
                    .as_ref()
                    .unwrap()
                    .traces
                    .clone();
                let start = fragments.len();
                let continuation = start + addition.nodes - 1;
                fragments[current].next = Some(CallableFlowNext::Edges(vec![start as u32]));
                match child {
                    CallableTraces::Finite(paths) => {
                        let leaves = (1..=paths.len()).map(|n| (start + n) as u32).collect();
                        fragments.push(Fragment {
                            calls: Vec::new(),
                            next: Some(CallableFlowNext::Edges(leaves)),
                        });
                        for path in paths {
                            fragments.push(Fragment {
                                calls: path.calls,
                                next: Some(match path.exit {
                                    CallableExit::Return => {
                                        CallableFlowNext::Edges(vec![continuation as u32])
                                    }
                                    CallableExit::Trap => {
                                        CallableFlowNext::Exit(CallableExit::Trap)
                                    }
                                }),
                            });
                        }
                    }
                    CallableTraces::Flow(flow) => {
                        for node in flow.nodes {
                            let next = match node.next {
                                CallableFlowNext::Edges(mut targets) => {
                                    for target in &mut targets {
                                        *target += start as u32;
                                    }
                                    CallableFlowNext::Edges(targets)
                                }
                                CallableFlowNext::Exit(CallableExit::Return) => {
                                    CallableFlowNext::Edges(vec![continuation as u32])
                                }
                                CallableFlowNext::Exit(CallableExit::Trap) => {
                                    CallableFlowNext::Exit(CallableExit::Trap)
                                }
                            };
                            fragments.push(Fragment {
                                calls: node.calls,
                                next: Some(next),
                            });
                        }
                    }
                }
                fragments.push(Fragment {
                    calls: Vec::new(),
                    next: None,
                });
                current = continuation;
            } else if let Some(kind) = callback(instruction, &locals, &values, bound.kind)? {
                size.reserve(0, 1, 0, limits, state, instruction.span)?;
                fragments[current].calls.push(kind);
            }
        }
        let edge_count = match block.terminator {
            Terminator::Jump(_) => 1,
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                if then_block == else_block {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        };
        size.reserve(0, 0, edge_count, limits, state, span)?;
        let next = match block.terminator {
            Terminator::Return(_) => Some(CallableFlowNext::Exit(CallableExit::Return)),
            Terminator::Trap => Some(CallableFlowNext::Exit(CallableExit::Trap)),
            Terminator::Jump(target) => Some(CallableFlowNext::Edges(vec![target.0])),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => Some(CallableFlowNext::Edges(if then_block == else_block {
                vec![then_block.0]
            } else {
                vec![then_block.0, else_block.0]
            })),
            Terminator::Unreachable => None,
        };
        fragments[current].next = next;
    }
    let graph = canonical(fragments, function.entry.0 as usize, state, span)?;
    classify(graph, bound.kind, state, limits, span)
}

fn canonical(
    mut fragments: Vec<Fragment>,
    entry: usize,
    state: &mut closures::State,
    span: Span,
) -> R<CallableFlow> {
    state.snapshot(fragments.len().saturating_mul(3), span)?;
    if entry >= fragments.len() {
        return Err(error("E-HELPER-TRACE", span, "helper flow entry is absent"));
    }
    let mut numbering = vec![u32::MAX; fragments.len()];
    let mut order = vec![entry];
    numbering[entry] = 0;
    let mut nodes = Vec::new();
    let mut cursor = 0;
    while cursor < order.len() {
        state.snapshot(1, span)?;
        let index = order[cursor];
        let calls = std::mem::take(&mut fragments[index].calls);
        let next = fragments[index].next.take().ok_or_else(|| {
            error(
                "E-HELPER-TRACE",
                span,
                "reachable helper path has no defined exit",
            )
        })?;
        let next = match next {
            CallableFlowNext::Edges(mut targets) => {
                for target in &mut targets {
                    state.snapshot(2, span)?;
                    let old = *target as usize;
                    if old >= numbering.len() {
                        return Err(error(
                            "E-HELPER-TRACE",
                            span,
                            "helper flow target is absent",
                        ));
                    }
                    if numbering[old] == u32::MAX {
                        numbering[old] = order.len() as u32;
                        order.push(old);
                    }
                    *target = numbering[old];
                }
                CallableFlowNext::Edges(targets)
            }
            exit => exit,
        };
        nodes.push(CallableFlowNode { calls, next });
        cursor += 1;
    }
    Ok(CallableFlow { entry: 0, nodes })
}

fn classify(
    flow: CallableFlow,
    kind: CallableKind,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<CallableTraces> {
    let n = flow.nodes.len();
    state.snapshot(n.saturating_mul(3), span)?;
    let mut incoming = vec![0usize; n];
    for node in &flow.nodes {
        state.snapshot(1, span)?;
        if let CallableFlowNext::Edges(targets) = &node.next {
            for &target in targets {
                state.snapshot(1, span)?;
                incoming[target as usize] += 1;
            }
        }
    }
    let mut ready: VecDeque<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(n);
    while let Some(index) = ready.pop_front() {
        state.snapshot(1, span)?;
        order.push(index);
        if let CallableFlowNext::Edges(targets) = &flow.nodes[index].next {
            for &target in targets {
                state.snapshot(1, span)?;
                incoming[target as usize] -= 1;
                if incoming[target as usize] == 0 {
                    ready.push_back(target as usize);
                }
            }
        }
    }
    if order.len() != n {
        if kind == CallableKind::FnOnce {
            return Err(unsupported(
                span,
                "cyclic FnOnce helper requires an additional consumption contract",
            ));
        }
        return Ok(CallableTraces::Flow(flow));
    }
    state.snapshot(n, span)?;
    let mut prefixes = vec![BTreeSet::<usize>::new(); n];
    prefixes[0].insert(0);
    let mut exits = BTreeSet::new();
    for index in order {
        let node = &flow.nodes[index];
        state.snapshot(1, span)?;
        for prefix in std::mem::take(&mut prefixes[index]) {
            let count = prefix
                .checked_add(node.calls.len())
                .ok_or_else(|| limited(span))?;
            if kind == CallableKind::FnOnce && count > 1 {
                return Err(unsupported(
                    span,
                    "finite helper supports at most one consumed call",
                ));
            }
            match &node.next {
                CallableFlowNext::Exit(exit) => {
                    state.snapshot(plan::tree_work(exits.len()), span)?;
                    exits.insert((count, *exit));
                }
                CallableFlowNext::Edges(targets) => {
                    for &target in targets {
                        state.snapshot(plan::tree_work(prefixes[target as usize].len()), span)?;
                        prefixes[target as usize].insert(count);
                    }
                }
            }
        }
    }
    if exits.len() > limits.max_nodes {
        return Err(limited(span));
    }
    state.snapshot(exits.len(), span)?;
    let mut paths = Vec::with_capacity(exits.len());
    for (count, exit) in exits {
        if count > limits.max_nodes {
            return Err(limited(span));
        }
        state.snapshot(count.saturating_add(1), span)?;
        paths.push(CallableTrace {
            calls: vec![kind; count],
            exit,
        });
    }
    Ok(CallableTraces::Finite(paths))
}
