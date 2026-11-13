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
mod address_sums;
mod borrowed_fp;
mod budget;
mod callable_fields;
mod class_flow;
mod composition;
#[cfg(test)]
mod defer_tests;
mod defers;
mod dyn_cleanup;
mod dyn_context;
mod dyn_dispatch;
mod dyn_metadata;
mod dyn_provider;
mod dynamic_store;
mod foreign_scoped;
mod forward;
mod generic_mutex;
mod head_access;
mod head_loans;
mod helpers;
mod holds;
mod incremental;
mod lifetimes;
mod mixed_address_sums;
mod nested_store;
mod owned_callbacks;
mod ownership_store;
mod place_replacements;
mod pod_store;
mod registration;
#[cfg(test)]
mod resource_tests;
mod resources;
mod retained;
mod scoped_thread;
mod scopes;
mod signature_life;
#[cfg(test)]
mod static_utf8_tests;
mod stored_borrows;
mod stored_callables;
mod stored_flow;
#[cfg(test)]
mod sum_tests;
mod tasks;

use std::collections::{BTreeSet, VecDeque};
use zil_base::{CompileResult, Diagnostic, Limits, Phase};
use zil_ir::*;

pub(crate) fn exact_storage_source(source: &StorageSource) -> CompileResult<&StoragePosition> {
    match source {
        StorageSource::Exact(position) => Ok(position),
        _ => Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "source atom requires an independent region consumer",
            None,
        )]),
    }
}
pub(crate) fn exact_storage_sources_contain(
    sources: &[StorageSource],
    position: &StoragePosition,
) -> CompileResult<bool> {
    let mut present = false;
    for source in sources {
        present |= exact_storage_source(source)? == position;
    }
    Ok(present)
}

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

pub fn analyze_program(program: &TypedProgram, limits: &Limits) -> CompileResult<CandidatePlan> {
    let mut budget = Budget::new(limits);
    let plan = analyze_budgeted(&program.module, limits, &mut budget, None)?;
    tasks::interfaces(program, &mut budget)?;
    Ok(plan)
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
    if budget.task_sources() && budget.task_input_scopes.is_none() && !module.closures.is_empty() {
        budget.task_input_scopes = Some(tasks::input_scope::build(module, limits, budget)?);
    }
    retained::check_static_occurrences(module, budget)?;
    dyn_dispatch::check(module, budget)?;
    lifetimes::headers(module, budget)?;
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
        resources::sums::dyn_tags::check(f, &predecessors, &reachable, budget)?;
        // Share the established CFG walk with returned-environment checks;
        // modules with no callable factory need no extra whole-module pass.
        retained::check_function(module, f, &reachable, budget)?;
        address_sums::check(module, f, &reachable, limits, budget)?;
        mixed_address_sums::check(module, f, &reachable, limits, budget)?;
        hold_graphs.push((predecessors, reachable));
        plan.functions.push(FunctionPlan {
            function: f.id,
            blocks,
            holds: Vec::new(),
        });
    }
    let mut stored_holds = None;
    let ordinary_input_heads =
        budget.stored_borrows && signature_life::ordinary_heads::module(module, limits, budget)?;
    if !ordinary_input_heads
        && (budget.stored_borrows || budget.task_sources() || budget.masked_stored_tasks())
    {
        // These actual opaque carriers cannot enter the legacy stored lane.
        // A stored module with only scalar helpers retains its old path/fees.
        let ordinary_required = budget.tasks || budget.registrations;
        if budget.task_sources() || budget.masked_stored_tasks() {
            if budget.masked_stored_tasks() && !budget.static_occurrences {
                budget.spend(module.functions.len(), "qualified Option function mask")?;
                budget.composition_stored = module
                    .functions
                    .iter()
                    .map(|function| stored_flow::qualified_function(function, budget))
                    .collect::<CompileResult<Vec<_>>>()?;
            }
            for (index, function) in module.functions.iter().enumerate() {
                if budget.composition_stored.is_empty() || budget.composition_stored[index] {
                    stored_flow::header(module, function, budget)?;
                }
            }
            stored_holds = Some(stored_flow::infer(module, &hold_graphs, budget)?);
        } else if ordinary_required {
            let members = composition::partition(module, budget)?;
            if !members.iter().any(|member| *member) || !members.iter().any(|member| !*member) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "stored/ordinary composition has no separate actual function domains",
                    None,
                )]);
            }
            budget.composition_stored = members;
            // Both solvers see the original module and CFG. The membership
            // mask only selects which function each solver is authorized to
            // prove; it never changes an ID, declaration, or contract.
            for (index, function) in module.functions.iter().enumerate() {
                if budget.composition_stored[index] {
                    stored_flow::header(module, function, budget)?;
                }
            }
            stored_holds = Some(stored_flow::infer(module, &hold_graphs, budget)?);
        } else {
            if !module.closures.is_empty() {
                stored_callables::definitions(module, budget)?;
                helpers::check(module, budget)?;
            }
            for f in &module.functions {
                stored_flow::header(module, f, budget)?;
            }
            for (f, package) in module.functions.iter().zip(&package_functions) {
                scopes::check(module, f, *package, budget)?;
            }
            let actual = stored_flow::infer(module, &hold_graphs, budget)?;
            for (function, holds) in plan.functions.iter_mut().zip(actual) {
                function.holds = holds;
            }
            dyn_dispatch::check_roots(module, &plan.functions, budget)?;
            retained::check_pending_sources(budget)?;
            return Ok(plan);
        }
    }
    // The Option mask schedules only the stored-address solver. Every
    // function, including the ones with a qualified Option, still needs its
    // independent ordinary Hold/resource solution over the original CFG.
    let store_option_mask = if budget.masked_stored_tasks() {
        std::mem::take(&mut budget.composition_stored)
    } else {
        Vec::new()
    };
    foreign_scoped::check(module, &hold_graphs, budget)?;
    scoped_thread::check(module, budget)?;
    generic_mutex::check(module, budget)?;
    tasks::check(module, budget)?;
    registration::check(module, budget)?;
    for (function, package) in module.functions.iter().zip(package_functions) {
        scopes::check(module, function, package, budget)?;
    }
    callable_fields::check(module, budget)?;
    helpers::check(module, budget)?;
    owned_callbacks::check(module, budget)?;
    let head_effects = head_access::check(module, &hold_graphs, budget)?;
    let early_holds = if budget.generic_mutex {
        Some(holds::infer_module(
            module,
            &hold_graphs,
            &store_profiles,
            budget,
        )?)
    } else {
        None
    };
    if let Some(rows) = early_holds.as_ref() {
        budget.spend(module.functions.len(), "guard return source functions")?;
        budget.guard_return_sources = vec![BTreeSet::new(); module.functions.len()];
        for (index, function) in module.functions.iter().enumerate() {
            if !matches!(function.return_type, Type::MutexGuard { .. }) {
                continue;
            }
            budget.spend(function.blocks.len(), "guard return source blocks")?;
            for (block, live) in function.blocks.iter().zip(&hold_graphs[index].1) {
                if !live {
                    continue;
                }
                if let Terminator::Return(Some(value)) = block.terminator {
                    let roots = &rows[index][value.0 as usize].roots;
                    budget.spend(roots.len(), "guard return source roots")?;
                    for root in roots {
                        if let HoldRoot::Parameter(parameter) = root {
                            budget.guard_return_sources[index].insert(*parameter);
                        }
                    }
                }
            }
        }
    }
    resources::check_module(module, &hold_graphs, budget)?;
    let mut actual_holds = if let Some(rows) = early_holds {
        rows
    } else {
        holds::infer_module(module, &hold_graphs, &store_profiles, budget)?
    };
    if let Some(stored) = stored_holds {
        for (index, rows) in stored.into_iter().enumerate() {
            if budget.masked_stored_tasks() && budget.task_sources() {
                budget.spend(1, "mixed stored proof coverage")?;
                if !budget
                    .task_stored_coverage
                    .get(index)
                    .copied()
                    .ok_or_else(|| bad("mixed Task proof omitted function coverage"))?
                {
                    continue;
                }
                if rows.len() != module.functions[index].values.len() {
                    return Err(bad("mixed Task proof omitted an actual SSA value"));
                }
                for (value, (ty, stored)) in
                    module.functions[index].values.iter().zip(&rows).enumerate()
                {
                    budget.spend(1, "mixed stored value coverage")?;
                    budget.type_work(ty, true, "mixed strong Hold type classification")?;
                    // Only the complete selected stored proof supplies these
                    // strong rows. Ordinary descriptors and other values keep
                    // the independent ordinary resource/borrow solution.
                    if matches!(ty, Type::BorrowedStorage { .. })
                        || stored_flow::enum_address::signature(ty)
                        || holds::retained_task_value(ty) && !matches!(ty, Type::Desc(_))
                        || budget.task_retained_environments
                            && !matches!(ty, Type::Desc(_))
                            && stored_flow::task_carrier(module, ty, budget)?
                        || matches!(ty, Type::Address { .. }) && !stored.roots.is_empty()
                    {
                        budget.spend(stored.roots.len(), "mixed strong Hold copy")?;
                        actual_holds[index][value] = stored.clone();
                    }
                }
                continue;
            }
            if budget.masked_stored_tasks() && !budget.task_sources() {
                if !store_option_mask.is_empty() && !store_option_mask[index] {
                    continue;
                }
                if rows.len() != module.functions[index].values.len() {
                    return Err(bad("stored Option proof omitted an actual SSA value"));
                }
                for (value, (ty, stored)) in
                    module.functions[index].values.iter().zip(&rows).enumerate()
                {
                    budget.spend(1, "stored Option and ordinary Hold comparison")?;
                    if matches!(ty, Type::BorrowedStorage { .. }) {
                        budget.spend(stored.roots.len(), "qualified actual Hold copy")?;
                        actual_holds[index][value] = stored.clone();
                    } else if (matches!(ty, Type::Address { mutable: false, .. })
                        || (budget.shared_byte_borrow
                            && matches!(ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)))
                        && stored.roots != actual_holds[index][value].roots
                    {
                        let targets = stored_flow::option_sources_snapshot(
                            module.functions[index].id,
                            ValueId(value as u32),
                            budget,
                        )?;
                        let count = targets.len();
                        let mut incomplete = targets.is_empty();
                        for target in &targets {
                            let existing = target.heap_element.is_some()
                                || matches!((target.base,target.stored.as_slice()),
                                    (zil_ir::HoldRoot::Parameter(_),[zil_ir::PlaceProjection::Variant(selector)])
                                    if selector.variant==1 && selector.index==0
                                        && target.place.iter().all(|step|matches!(step,zil_ir::PlaceProjection::Field(_))));
                            if !existing
                                && !stored_flow::inline_hold_source(
                                    &module.functions[index],
                                    target,
                                    ty,
                                    budget,
                                )?
                            {
                                incomplete = true;
                                break;
                            }
                        }
                        budget.spend(count, "stored Option element proof")?;
                        if incomplete {
                            return Err(bad("stored Option and ordinary actual roots disagree"));
                        }
                        budget.spend(stored.roots.len(), "Store element actual Hold copy")?;
                        actual_holds[index][value] = stored.clone();
                    }
                }
            } else if budget.task_sources() || budget.composition_stored[index] {
                if rows.len() != module.functions[index].values.len() {
                    return Err(bad("stored composition solver omitted an owned function"));
                }
                actual_holds[index] = rows;
            }
        }
    }
    head_loans::check(module, &hold_graphs, &actual_holds, &head_effects, budget)?;
    for (function, holds) in plan.functions.iter_mut().zip(actual_holds) {
        function.holds = holds;
    }
    dyn_dispatch::check_roots(module, &plan.functions, budget)?;
    tasks::loans(module, &plan, &hold_graphs, budget)?;
    retained::check_pending_sources(budget)?;
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
            if let InstructionKind::UnpackVariant { local, fields, .. }
            | InstructionKind::UnpackTuple { local, fields } = &i.kind
            {
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
        if let InstructionKind::UnpackVariant { local, fields, .. }
        | InstructionKind::UnpackTuple { local, fields } = &instruction.kind
        {
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
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            resources: Vec::new(),
            closures: vec![],
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions: vec![TypedFunction {
                dyn_instance: None,
                retained_return_sources: None,
                storage_life: None,
                call_kind: zil_ir::CallKind::Direct,
                resume_bound: zil_ir::TaskResumeBound::Local,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contracts: Vec::new(),
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
