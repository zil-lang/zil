#![forbid(unsafe_code)]
//! Produce candidate initialization and sparse shared-address Hold facts.
//! Verification uses a separate implementation of both analyses.
//!
//! `max_analysis_steps` is a module-wide work bound. Input preflight reserves
//! one structural/metadata traversal for checking and one for fingerprinting;
//! target metadata has a third traversal for the separate target fingerprint.
//! CFG visits, instruction visits, matrix cells, and set elements traversed or
//! materialized are charged before the corresponding work or allocation. These
//! are deterministic logical units, not elapsed time or an allocator byte count.
mod budget;
mod callable_fields;
#[cfg(test)]
mod defer_tests;
mod defers;
mod dynamic_store;
mod helpers;
mod holds;
mod incremental;
mod owned_callbacks;
#[cfg(test)]
mod resource_tests;
mod resources;
mod retained;
mod scopes;
#[cfg(test)]
mod sum_tests;

use std::collections::{BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, Phase};
use zil_ir::*;

use budget::{Budget, preflight};
pub use incremental::{AnalysisSession, AnalysisStats};

fn bad(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-ANALYZE-IR",
        None,
        message,
    )]
}

pub fn analyze(module: &TypedModule, limits: &Limits) -> CompileResult<CandidatePlan> {
    let mut budget = Budget::new(limits);
    analyze_budgeted(module, limits, &mut budget, None)
}

fn analyze_budgeted(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
    mut reuse: Option<&mut incremental::Reuse<'_>>,
) -> CompileResult<CandidatePlan> {
    // Raw callers need not have passed through types. In particular, reject
    // oversized metadata and unsupported recursive types before encoding a hash.
    let (package_functions, store_profiles) = preflight(module, limits, budget)?;
    let mut plan = CandidatePlan::empty_for(module);
    let mut hold_graphs = Vec::with_capacity(module.functions.len());
    for f in &module.functions {
        budget.spend(1, "function analysis")?;
        if f.foreign.is_some() {
            retained::check_function(module, f, &[], budget)?;
            if !f.blocks.is_empty() || !f.values.is_empty() {
                return Err(bad("foreign declaration carries an executable body"));
            }
            hold_graphs.push((Vec::new(), Vec::new()));
            plan.functions.push(FunctionPlan {
                function: f.id,
                blocks: Vec::new(),
                holds: Vec::new(),
            });
            continue;
        }
        let initialization = match reuse.as_deref_mut() {
            Some(reuse) => reuse.initialize(f, budget)?,
            None => initialize(f, budget)?,
        };
        let Initialization {
            blocks,
            predecessors,
            reachable,
        } = initialization;
        // Share the established CFG walk with returned-environment checks;
        // modules with no callable factory need no extra whole-module pass.
        retained::check_function(module, f, &reachable, budget)?;
        hold_graphs.push((predecessors, reachable));
        plan.functions.push(FunctionPlan {
            function: f.id,
            blocks,
            holds: Vec::new(),
        });
    }
    for (function, package) in module.functions.iter().zip(package_functions) {
        scopes::check(module, function, package, budget)?;
    }
    callable_fields::check(module, budget)?;
    helpers::check(module, budget)?;
    owned_callbacks::check(module, budget)?;
    resources::check_module(module, &hold_graphs, budget)?;
    for (function, holds) in plan.functions.iter_mut().zip(holds::infer_module(
        module,
        &hold_graphs,
        &store_profiles,
        budget,
    )?) {
        function.holds = holds;
    }
    Ok(plan)
}

#[derive(Clone)]
struct Initialization {
    blocks: Vec<BlockPlan>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
}

fn initialize(f: &TypedFunction, budget: &mut Budget) -> CompileResult<Initialization> {
    let n = f.blocks.len();
    if n == 0 || f.entry.0 as usize >= n {
        return Err(bad("missing entry block"));
    }
    // Reserve the maximum resident initialization set cells before building
    // any of the tables. This reservation accumulates across functions too.
    budget.product(n, f.locals.len().max(1), 3, "initialization matrices")?;
    budget.product(n, 2, 1, "CFG tables")?;
    let mut successors = vec![vec![]; n];
    let mut predecessors = vec![vec![]; n];
    for (bi, b) in f.blocks.iter().enumerate() {
        budget.spend(1, "CFG block scan")?;
        if b.id.0 as usize != bi {
            return Err(bad("noncanonical block ID"));
        }
        let targets: &[BlockId] = match &b.terminator {
            Terminator::Jump(t) => std::slice::from_ref(t),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                // Each edge is visited once in each adjacency direction.
                budget.spend(4, "CFG edge scan")?;
                for target in [then_block, else_block] {
                    let s = target.0 as usize;
                    if s >= n {
                        return Err(bad("CFG edge outside function"));
                    }
                    successors[bi].push(s);
                    predecessors[s].push(bi);
                }
                continue;
            }
            _ => &[],
        };
        budget.product(targets.len(), 2, 1, "CFG edge scan")?;
        for target in targets {
            let s = target.0 as usize;
            if s >= n {
                return Err(bad("CFG edge outside function"));
            }
            successors[bi].push(s);
            predecessors[s].push(bi);
        }
    }
    budget.spend(n, "reachability table")?;
    let mut reachable = vec![false; n];
    budget.spend(1, "reachability queue")?;
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    while let Some(b) = queue.pop_front() {
        budget.spend(1, "reachability scan")?;
        if reachable[b] {
            continue;
        }
        reachable[b] = true;
        budget.spend(successors[b].len(), "reachability edges")?;
        queue.extend(&successors[b]);
    }
    budget.spend(f.locals.len(), "local universe")?;
    let universe: BTreeSet<_> = (0..f.locals.len()).map(|x| LocalId(x as u32)).collect();
    budget.spend(f.parameters.len(), "parameter set")?;
    let entry: BTreeSet<_> = f.parameters.iter().copied().collect();
    budget.sets(&entry, &universe, "parameter membership")?;
    if !entry.is_subset(&universe) {
        return Err(bad("parameter local outside table"));
    }
    let mut ins = Vec::with_capacity(n);
    for (i, &is_reachable) in reachable.iter().enumerate() {
        budget.spend(1, "initial input table")?;
        let initial = if i == f.entry.0 as usize {
            clone_set(&entry, budget)?
        } else if is_reachable {
            clone_set(&universe, budget)?
        } else {
            BTreeSet::new()
        };
        ins.push(initial);
    }
    let mut outs = Vec::with_capacity(n);
    for (input, block) in ins.iter().zip(&f.blocks) {
        outs.push(transfer(input, block, budget)?);
    }
    loop {
        let mut changed = false;
        for b in 0..n {
            budget.spend(1, "fixed-point block scan")?;
            if !reachable[b] {
                continue;
            }
            let new_in = if b == f.entry.0 as usize {
                clone_set(&entry, budget)?
            } else {
                budget.spend(predecessors[b].len(), "fixed-point predecessor scan")?;
                let mut preds = predecessors[b].iter().filter(|p| reachable[**p]);
                if let Some(&first) = preds.next() {
                    let mut v = clone_set(&outs[first], budget)?;
                    for &p in preds {
                        budget.sets(&v, &outs[p], "initialization intersection")?;
                        v = v.intersection(&outs[p]).copied().collect();
                    }
                    v
                } else {
                    BTreeSet::new()
                }
            };
            let new_out = transfer(&new_in, &f.blocks[b], budget)?;
            budget.sets(&new_in, &ins[b], "initialization input comparison")?;
            budget.sets(&new_out, &outs[b], "initialization output comparison")?;
            if new_in != ins[b] || new_out != outs[b] {
                ins[b] = new_in;
                outs[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (bi, b) in f.blocks.iter().enumerate() {
        budget.spend(1, "initialization block scan")?;
        if !reachable[bi] {
            continue;
        }
        let mut state = clone_set(&ins[bi], budget)?;
        for i in &b.instructions {
            budget.spend(1, "initialization instruction scan")?;
            if let InstructionKind::UnpackVariant { local, fields, .. } = &i.kind {
                budget.spend(fields.len().saturating_add(1), "Unpack initialization atom")?;
                budget.lookup(state.len(), "Unpack initialized source")?;
                if !state.contains(local) || fields.iter().any(|l| state.contains(l)) {
                    return Err(bad("Unpack source must be initialized and targets vacant"));
                }
                state.remove(local);
                state.extend(fields.iter().copied());
                continue;
            }
            match incremental::Operation::of(&i.kind) {
                incremental::Operation::Read(local) | incremental::Operation::Take(local) => {
                    budget.lookup(state.len(), "initialized local lookup")?;
                    if !state.contains(&local) {
                        return Err(vec![Diagnostic::error(
                            Phase::Analyze,
                            "E-UNINITIALIZED",
                            Some(i.span),
                            format!(
                                "local {} is not initialized on every incoming path",
                                local.0
                            ),
                        )]);
                    }
                    if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                        state.remove(&local);
                    }
                }
                incremental::Operation::Store(local) => {
                    budget.lookup(state.len(), "initialized local insertion")?;
                    state.insert(local);
                }
                incremental::Operation::Kill(local) => {
                    budget.lookup(state.len(), "storage initialization removal")?;
                    state.remove(&local);
                }
                incremental::Operation::Unpack { source, fields } => {
                    budget.lookup(state.len(), "Unpack initialized source")?;
                    if !state.contains(&source) {
                        return Err(bad("Unpack source is not initialized on all paths"));
                    }
                    budget.spend(fields.len(), "Unpack initialization publication")?;
                    if fields.iter().any(|l| state.contains(l)) {
                        return Err(bad("Unpack target is already initialized"));
                    }
                    state.remove(&source);
                    state.extend(fields);
                }
                incremental::Operation::Other => {}
            }
        }
    }
    let mut blocks = Vec::with_capacity(n);
    for i in 0..n {
        budget.spend(1, "candidate block export")?;
        budget.sets(&ins[i], &outs[i], "candidate initialization export")?;
        blocks.push(BlockPlan {
            block: BlockId(i as u32),
            initialized_in: ins[i].iter().copied().collect(),
            initialized_out: outs[i].iter().copied().collect(),
        });
    }
    Ok(Initialization {
        blocks,
        predecessors,
        reachable,
    })
}

fn clone_set(set: &BTreeSet<LocalId>, budget: &mut Budget) -> CompileResult<BTreeSet<LocalId>> {
    budget.spend(set.len(), "initialization set clone")?;
    Ok(set.clone())
}

fn transfer(
    input: &BTreeSet<LocalId>,
    block: &BasicBlock,
    budget: &mut Budget,
) -> CompileResult<BTreeSet<LocalId>> {
    let mut state = clone_set(input, budget)?;
    for instruction in &block.instructions {
        budget.spend(1, "initialization transfer")?;
        if let InstructionKind::UnpackVariant { local, fields, .. } = &instruction.kind {
            budget.spend(
                fields.len().saturating_add(1),
                "Unpack initialization transfer atom",
            )?;
            state.remove(local);
            state.extend(fields.iter().copied());
            continue;
        }
        match incremental::Operation::of(&instruction.kind) {
            incremental::Operation::Store(local) => {
                budget.lookup(state.len(), "initialization insert")?;
                state.insert(local);
            }
            incremental::Operation::Take(local) | incremental::Operation::Kill(local) => {
                budget.lookup(state.len(), "initialization remove")?;
                state.remove(&local);
            }
            incremental::Operation::Unpack { source, fields } => {
                budget.spend(
                    fields.len().saturating_add(1),
                    "Unpack initialization transfer",
                )?;
                state.remove(&source);
                state.extend(fields);
            }
            _ => {}
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, Span, TargetProfile};
    fn s() -> Span {
        Span::new(FileId(0), 0, 1)
    }
    fn module(initialized: bool) -> TypedModule {
        let mut instructions = vec![];
        let mut values = vec![];
        if initialized {
            values.push(Type::I32);
            instructions.push(Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Constant(Constant::I32(7)),
                span: s(),
            });
            instructions.push(Instruction {
                result: None,
                kind: InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
                span: s(),
            });
        }
        let v = ValueId(values.len() as u32);
        values.push(Type::I32);
        instructions.push(Instruction {
            result: Some(v),
            kind: InstructionKind::Load(LocalId(0)),
            span: s(),
        });
        TypedModule {
            resources: Vec::new(),
            closures: vec![],
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions: vec![TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                environment: None,
                id: FunctionId(0),
                name: "main".into(),
                parameters: vec![],
                return_type: Type::I32,
                locals: vec![LocalDecl {
                    id: LocalId(0),
                    name: "x".into(),
                    ty: Type::I32,
                    mutable: false,
                    span: s(),
                }],
                values,
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions,
                    terminator: Terminator::Return(Some(v)),
                }],
                entry: BlockId(0),
                span: s(),
            }],
            unsupported: vec![],
        }
    }
    #[test]
    fn emits_initialized_boundary_facts() {
        let m = module(true);
        let p = analyze(&m, &Limits::default()).unwrap();
        assert!(p.functions[0].blocks[0].initialized_in.is_empty());
        assert_eq!(p.functions[0].blocks[0].initialized_out, vec![LocalId(0)]);
    }
    #[test]
    fn uninitialized_load_is_rejected() {
        assert_eq!(
            analyze(&module(false), &Limits::default()).unwrap_err()[0].code,
            "E-UNINITIALIZED"
        )
    }
    #[test]
    fn invalid_edge_rejected_without_panic() {
        let mut m = module(true);
        m.functions[0].blocks[0].terminator = Terminator::Jump(BlockId(200));
        assert!(analyze(&m, &Limits::default()).is_err());
    }

    #[test]
    fn initialization_queries_stop_on_equal_semantic_projection() {
        let limits = Limits::default();
        let mut session = AnalysisSession::default();
        let mut m = module(true);
        assert_eq!(session.analyze(&m, &limits), analyze(&m, &limits));
        assert_eq!(session.last_stats().executed_functions, 1);
        m.functions[0].blocks[0].instructions[0].kind =
            InstructionKind::Constant(Constant::I32(99));
        m.functions[0].span = Span::new(FileId(7), 100, 200);
        for i in &mut m.functions[0].blocks[0].instructions {
            i.span = Span::new(FileId(7), 105, 110);
        }
        m.source_fingerprint = Fingerprint::of(b"current source");
        let changed = session.analyze(&m, &limits).unwrap();
        assert_eq!(changed, analyze(&m, &limits).unwrap());
        assert_eq!(session.last_stats().reused_functions, 1);
        assert_eq!(session.last_stats().executed_functions, 0);
        assert_eq!(changed.module_fingerprint, m.fingerprint());
    }

    #[test]
    fn initialization_cache_preserves_every_budget_boundary_and_diagnostic() {
        let m = module(true);
        let mut session = AnalysisSession::default();
        session.analyze(&m, &Limits::default()).unwrap();
        let first_success = (0..20_000)
            .find(|steps| {
                let limits = Limits {
                    max_analysis_steps: *steps,
                    ..Limits::default()
                };
                analyze(&m, &limits).is_ok()
            })
            .unwrap();
        for steps in 0..=first_success + 1 {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            assert_eq!(
                session.analyze(&m, &limits),
                analyze(&m, &limits),
                "budget {steps}"
            );
        }
        assert_eq!(session.last_stats().reused_functions, 1);
    }

    #[test]
    fn initialization_cache_misses_changed_reads_edges_and_keeps_failed_epoch_private() {
        let limits = Limits::default();
        let original = module(true);
        let mut session = AnalysisSession::default();
        session.analyze(&original, &limits).unwrap();
        let mut changed = original.clone();
        changed.functions[0].blocks[0].instructions[1].kind =
            InstructionKind::StorageDead(LocalId(0));
        changed.functions[0].blocks[0].instructions[2].span = Span::new(FileId(9), 40, 45);
        assert_eq!(
            session.analyze(&changed, &limits),
            analyze(&changed, &limits)
        );
        assert_eq!(session.last_stats().executed_functions, 1);
        assert_eq!(session.last_stats().reused_functions, 0);
        assert_eq!(session.last_stats().retained_queries, 1);
        changed = original.clone();
        changed.functions[0].blocks[0].terminator = Terminator::Jump(BlockId(200));
        assert_eq!(
            session.analyze(&changed, &limits),
            analyze(&changed, &limits)
        );
        assert_eq!(
            session.analyze(&original, &limits),
            analyze(&original, &limits)
        );
        assert_eq!(session.last_stats().reused_functions, 1);
        session.clear();
        assert_eq!(session.last_stats().retained_queries, 0);
    }

    #[test]
    fn initialization_cache_content_queries_survive_unrelated_declaration_insertion() {
        let limits = Limits::default();
        let mut m = module(true);
        let mut session = AnalysisSession::default();
        session.analyze(&m, &limits).unwrap();
        let mut inserted = m.functions[0].clone();
        inserted.name = "inserted".into();
        m.functions[0].id = FunctionId(1);
        m.functions.insert(0, inserted);
        assert_eq!(session.analyze(&m, &limits), analyze(&m, &limits));
        assert_eq!(session.last_stats().reused_functions, 2);
        assert_eq!(session.last_stats().executed_functions, 0);
        assert_eq!(session.last_stats().retained_queries, 1);
    }
}
