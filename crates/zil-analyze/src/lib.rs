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
mod dynamic_store;
mod helpers;
mod holds;
#[cfg(test)]
mod resource_tests;
mod resources;
mod retained;
mod scopes;

use std::collections::{BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, Phase};
use zil_ir::*;

use budget::{Budget, preflight};

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
    analyze_budgeted(module, limits, &mut budget)
}

fn analyze_budgeted(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> CompileResult<CandidatePlan> {
    // Raw callers need not have passed through types. In particular, reject
    // oversized metadata and unsupported recursive types before encoding a hash.
    let package_functions = preflight(module, limits, budget)?;
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
                match i.kind {
                    InstructionKind::Load(local)
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DescribeLocal { local }
                    | InstructionKind::ReadPackageField { local, .. }
                    | InstructionKind::DescribePackageField { local, .. }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::ProjectOwnedArrayField { local, .. }
                    | InstructionKind::TakeOwnedArrayElement { local, .. }
                    | InstructionKind::DropOwnedArrayElement { local, .. }
                    | InstructionKind::StoreOwnedArrayElement { local, .. }
                    | InstructionKind::BorrowOwnedArrayElement { local, .. }
                    | InstructionKind::BorrowPackageField { local, .. }
                    | InstructionKind::CallClosureMut { callee: local, .. }
                    | InstructionKind::CallClosureShared { callee: local, .. } => {
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
                    InstructionKind::Store { local, .. }
                    | InstructionKind::BindPackageCapture { local, .. } => {
                        budget.lookup(state.len(), "initialized local insertion")?;
                        state.insert(local);
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropErrorUnion { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::DropClosure { local } => {
                        budget.lookup(state.len(), "storage initialization removal")?;
                        state.remove(&local);
                    }
                    _ => {}
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
    helpers::check(module, budget)?;
    resources::check_module(module, &hold_graphs, budget)?;
    for (function, holds) in
        plan.functions
            .iter_mut()
            .zip(holds::infer_module(module, &hold_graphs, budget)?)
    {
        function.holds = holds;
    }
    Ok(plan)
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
        match instruction.kind {
            InstructionKind::Store { local, .. }
            | InstructionKind::BindPackageCapture { local, .. } => {
                budget.lookup(state.len(), "initialization insert")?;
                state.insert(local);
            }
            InstructionKind::TakeLocal { local }
            | InstructionKind::DropManaged { local }
            | InstructionKind::DropErrorUnion { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropOwnedArray { local }
            | InstructionKind::EndPackageCapture { local, .. }
            | InstructionKind::DropClosure { local }
            | InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local) => {
                budget.lookup(state.len(), "initialization remove")?;
                state.remove(&local);
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
}
