//! Independent call-language reconstruction. This checker explores virtual
//! instruction continuations directly in BFS order; it never consumes a
//! producer graph as the proof of a caller, or calls Analyze/Types.
use super::*;
use std::collections::BTreeMap;
use zil_ir::{CallableFlow, CallableFlowNext, CallableFlowNode, CallableTraces};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Site {
    Body {
        block: usize,
        from: usize,
    },
    Child {
        block: usize,
        call: usize,
        node: usize,
    },
}

#[derive(Default)]
struct Allowance {
    nodes: usize,
    events: usize,
    edges: usize,
}

impl Allowance {
    fn add(
        &mut self,
        nodes: usize,
        events: usize,
        edges: usize,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Check<()> {
        let n = self
            .nodes
            .checked_add(nodes)
            .filter(|n| *n <= limits.max_nodes && u32::try_from(*n).is_ok())
            .ok_or_else(|| limit("independent helper flow node allowance exhausted"))?;
        let c = self
            .events
            .checked_add(events)
            .filter(|n| *n <= limits.max_instructions)
            .ok_or_else(|| limit("independent helper flow call allowance exhausted"))?;
        let e = self
            .edges
            .checked_add(edges)
            .filter(|n| *n <= limits.max_analysis_steps)
            .ok_or_else(|| limit("independent helper flow edge allowance exhausted"))?;
        let copying = nodes
            .checked_add(events)
            .and_then(|n| n.checked_add(edges))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| limit("independent helper flow copy work overflow"))?;
        budget.spend(copying)?;
        (self.nodes, self.events, self.edges) = (n, c, e);
        Ok(())
    }

    fn child(
        &mut self,
        child: &CallableTraces,
        expected: CallableKind,
        limits: &Limits,
        budget: &mut Budget,
        f: &TypedFunction,
    ) -> Check<()> {
        let (mut nodes, mut edges, mut calls) = (1usize, 1usize, 0usize);
        // One continuation and one prefix-to-child edge belong to every site.
        match child {
            CallableTraces::Finite(paths) => {
                budget.spend(paths.len().saturating_add(1))?;
                if paths.is_empty() {
                    return Err(bad(f, "nested finite helper has no path"));
                }
                nodes = nodes
                    .checked_add(paths.len())
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| limit("helper child node count overflow"))?;
                edges = edges
                    .checked_add(paths.len())
                    .ok_or_else(|| limit("helper child edge count overflow"))?;
                for path in paths {
                    budget.spend(path.calls.len())?;
                    if path.calls.iter().any(|kind| *kind != expected) {
                        return Err(bad(f, "nested callback kind differs"));
                    }
                    calls = calls
                        .checked_add(path.calls.len())
                        .ok_or_else(|| limit("helper child event count overflow"))?;
                    if path.exit == CallableExit::Return {
                        edges = edges
                            .checked_add(1)
                            .ok_or_else(|| limit("helper continuation count overflow"))?;
                    }
                }
            }
            CallableTraces::Flow(flow) => {
                budget.spend(flow.nodes.len().saturating_add(1))?;
                if flow.entry != 0 || flow.nodes.is_empty() {
                    return Err(bad(f, "nested helper has no canonical entry"));
                }
                nodes = nodes
                    .checked_add(flow.nodes.len())
                    .ok_or_else(|| limit("helper child node count overflow"))?;
                for node in &flow.nodes {
                    budget.spend(node.calls.len())?;
                    if node.calls.iter().any(|kind| *kind != expected) {
                        return Err(bad(f, "nested callback kind differs"));
                    }
                    calls = calls
                        .checked_add(node.calls.len())
                        .ok_or_else(|| limit("helper child event count overflow"))?;
                    let outgoing = match &node.next {
                        CallableFlowNext::Edges(next) => {
                            budget.spend(next.len())?;
                            next.len()
                        }
                        CallableFlowNext::Exit(CallableExit::Return) => 1,
                        CallableFlowNext::Exit(CallableExit::Trap) => 0,
                    };
                    edges = edges
                        .checked_add(outgoing)
                        .ok_or_else(|| limit("helper child edge count overflow"))?;
                }
            }
        }
        // All child calls, ordered edges, returns and mapping slots are
        // checked and charged before any corresponding owned vector exists.
        self.add(nodes, calls, edges, limits, budget)
    }
}

fn intern(
    site: Site,
    ids: &mut BTreeMap<Site, u32>,
    queue: &mut Vec<Site>,
    limit_nodes: usize,
    budget: &mut Budget,
) -> Check<u32> {
    budget.spend(count_tree_work(ids.len()))?;
    if let Some(id) = ids.get(&site) {
        return Ok(*id);
    }
    if queue.len() >= limit_nodes {
        return Err(limit("independent helper flow node allowance exhausted"));
    }
    let id =
        u32::try_from(queue.len()).map_err(|_| limit("independent helper flow index overflow"))?;
    budget.spend(count_tree_work(ids.len()).saturating_add(1))?;
    ids.insert(site, id);
    queue.push(site);
    Ok(id)
}

enum Destination {
    Sites(Vec<Site>),
    Exit(CallableExit),
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: LocalId,
    kind: CallableKind,
    checked: &[bool],
    limits: &Limits,
    budget: &mut Budget,
) -> Check<CallableTraces> {
    let origins = environment_origins(module, f, parameter, budget)?;
    let mut allowance = Allowance::default();
    allowance.add(f.blocks.len(), 0, 0, limits, budget)?;
    let mut ids = BTreeMap::new();
    let mut queue = Vec::new();
    intern(
        Site::Body {
            block: f.entry.0 as usize,
            from: 0,
        },
        &mut ids,
        &mut queue,
        limits.max_nodes,
        budget,
    )?;
    let mut nodes = Vec::new();
    let mut cursor = 0;
    while cursor < queue.len() {
        budget.spend(1)?;
        let (calls, destination) = match queue[cursor] {
            Site::Body { block, from } => {
                let body = &f.blocks[block];
                let mut calls = Vec::new();
                let mut child_destination = None;
                for (position, instruction) in body.instructions.iter().enumerate().skip(from) {
                    budget.spend(1)?;
                    if matches!(instruction.kind, InstructionKind::MakeClosure { .. }) {
                        return Err(unsupported(
                            "helper-local nested environments require an additional contract",
                            Some(instruction.span),
                        ));
                    }
                    if let Some(child) =
                        forwarded(module, f, instruction, &origins, kind, checked, budget)?
                    {
                        allowance.child(child, kind, limits, budget, f)?;
                        child_destination = Some(Destination::Sites(vec![Site::Child {
                            block,
                            call: position,
                            node: 0,
                        }]));
                        break;
                    }
                    if let Some(actual) = invocation(f, instruction, &origins)? {
                        if actual != kind {
                            return Err(bad(
                                f,
                                "helper callback use differs from its requested bound",
                            ));
                        }
                        allowance.add(0, 1, 0, limits, budget)?;
                        calls.push(actual);
                    }
                }
                let destination = if let Some(destination) = child_destination {
                    destination
                } else {
                    match body.terminator {
                        Terminator::Return(_) => Destination::Exit(CallableExit::Return),
                        Terminator::Trap => Destination::Exit(CallableExit::Trap),
                        Terminator::Jump(target) => {
                            allowance.add(0, 0, 1, limits, budget)?;
                            Destination::Sites(vec![Site::Body {
                                block: target.0 as usize,
                                from: 0,
                            }])
                        }
                        Terminator::Branch {
                            then_block,
                            else_block,
                            ..
                        } => {
                            let count = if then_block == else_block { 1 } else { 2 };
                            allowance.add(0, 0, count, limits, budget)?;
                            let mut next = vec![Site::Body {
                                block: then_block.0 as usize,
                                from: 0,
                            }];
                            if then_block != else_block {
                                next.push(Site::Body {
                                    block: else_block.0 as usize,
                                    from: 0,
                                });
                            }
                            Destination::Sites(next)
                        }
                        Terminator::Unreachable => {
                            return Err(bad(f, "helper has a reachable undefined exit"));
                        }
                    }
                };
                (calls, destination)
            }
            Site::Child { block, call, node } => {
                let InstructionKind::Call {
                    function: callee, ..
                } = f.blocks[block].instructions[call].kind
                else {
                    unreachable!("child site comes only from a checked direct call");
                };
                let child = &module.functions[callee.0 as usize]
                    .callable_contract
                    .as_ref()
                    .expect("independently checked child")
                    .traces;
                let continuation = Site::Body {
                    block,
                    from: call + 1,
                };
                match child {
                    CallableTraces::Finite(paths) if node == 0 => {
                        let next = (1..=paths.len())
                            .map(|node| Site::Child { block, call, node })
                            .collect();
                        (Vec::new(), Destination::Sites(next))
                    }
                    CallableTraces::Finite(paths) => {
                        let path = &paths[node - 1];
                        let next = if path.exit == CallableExit::Return {
                            Destination::Sites(vec![continuation])
                        } else {
                            Destination::Exit(CallableExit::Trap)
                        };
                        (path.calls.clone(), next)
                    }
                    CallableTraces::Flow(flow) => {
                        let source = &flow.nodes[node];
                        let next = match &source.next {
                            CallableFlowNext::Edges(edges) => Destination::Sites(
                                edges
                                    .iter()
                                    .map(|node| Site::Child {
                                        block,
                                        call,
                                        node: *node as usize,
                                    })
                                    .collect(),
                            ),
                            CallableFlowNext::Exit(CallableExit::Return) => {
                                Destination::Sites(vec![continuation])
                            }
                            CallableFlowNext::Exit(CallableExit::Trap) => {
                                Destination::Exit(CallableExit::Trap)
                            }
                        };
                        (source.calls.clone(), next)
                    }
                }
            }
        };
        let next = match destination {
            Destination::Exit(exit) => CallableFlowNext::Exit(exit),
            Destination::Sites(sites) => {
                budget.spend(sites.len())?;
                let mut targets = Vec::with_capacity(sites.len());
                for site in sites {
                    targets.push(intern(
                        site,
                        &mut ids,
                        &mut queue,
                        limits.max_nodes,
                        budget,
                    )?);
                }
                CallableFlowNext::Edges(targets)
            }
        };
        nodes.push(CallableFlowNode { calls, next });
        cursor += 1;
    }
    classify(CallableFlow { entry: 0, nodes }, kind, limits, budget, f)
}

fn classify(
    flow: CallableFlow,
    kind: CallableKind,
    limits: &Limits,
    budget: &mut Budget,
    f: &TypedFunction,
) -> Check<CallableTraces> {
    let n = flow.nodes.len();
    budget.spend(n.saturating_mul(3))?;
    let mut color = vec![0u8; n];
    let mut pending = vec![(0usize, 0usize)];
    let mut postorder = Vec::with_capacity(n);
    color[0] = 1;
    let mut cyclic = false;
    while let Some((node, edge)) = pending.last_mut() {
        budget.spend(1)?;
        let next = match &flow.nodes[*node].next {
            CallableFlowNext::Edges(targets) => targets.get(*edge).copied(),
            CallableFlowNext::Exit(_) => None,
        };
        if let Some(next) = next {
            *edge += 1;
            let next = next as usize;
            match color[next] {
                0 => {
                    color[next] = 1;
                    pending.push((next, 0));
                }
                1 => cyclic = true,
                _ => {}
            }
        } else {
            color[*node] = 2;
            postorder.push(*node);
            pending.pop();
        }
    }
    if cyclic {
        if kind == CallableKind::FnOnce {
            return Err(unsupported(
                "cyclic FnOnce helper requires an additional consumption contract",
                Some(f.span),
            ));
        }
        return Ok(CallableTraces::Flow(flow));
    }
    budget.spend(n)?;
    let mut at = vec![BTreeSet::<usize>::new(); n];
    at[0].insert(0);
    let mut finished = BTreeSet::new();
    for index in postorder.into_iter().rev() {
        budget.spend(1)?;
        let node = &flow.nodes[index];
        let prefixes = std::mem::take(&mut at[index]);
        for before in prefixes {
            budget.spend(1)?;
            let count = before
                .checked_add(node.calls.len())
                .ok_or_else(|| limit("helper call count overflow"))?;
            if kind == CallableKind::FnOnce && count > 1 {
                return Err(bad(f, "helper consumes its callback more than once"));
            }
            match &node.next {
                CallableFlowNext::Exit(exit) => {
                    budget.spend(count_tree_work(finished.len()))?;
                    finished.insert((count, *exit));
                }
                CallableFlowNext::Edges(targets) => {
                    for target in targets {
                        budget.spend(count_tree_work(at[*target as usize].len()))?;
                        at[*target as usize].insert(count);
                    }
                }
            }
        }
    }
    if finished.len() > limits.max_nodes {
        return Err(limit("finite helper path allowance exhausted"));
    }
    budget.spend(finished.len())?;
    let mut paths = Vec::with_capacity(finished.len());
    for (count, exit) in finished {
        if count > limits.max_nodes {
            return Err(limit("finite helper call allowance exhausted"));
        }
        budget.spend(count.saturating_add(1))?;
        paths.push(CallableTrace {
            calls: vec![kind; count],
            exit,
        });
    }
    Ok(CallableTraces::Finite(paths))
}

pub(super) fn comparison_work(traces: &CallableTraces, budget: &mut Budget) -> Check<()> {
    match traces {
        CallableTraces::Finite(paths) => {
            budget.spend(paths.len())?;
            for path in paths {
                budget.spend(path.calls.len())?;
            }
        }
        CallableTraces::Flow(flow) => {
            budget.spend(flow.nodes.len().saturating_add(1))?;
            for node in &flow.nodes {
                budget.spend(node.calls.len().saturating_add(1))?;
                if let CallableFlowNext::Edges(edges) = &node.next {
                    budget.spend(edges.len())?;
                }
            }
        }
    }
    Ok(())
}
