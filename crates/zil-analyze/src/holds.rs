//! Candidate shared-address provenance. The verifier has its own implementation.
//! Only actual Hold relations are stored; unrelated locals are absent from CFG
//! states and roots are introduced only by executable borrow/capture operations.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Local(LocalId),
    Field(LocalId, u32),
    Capture(u32),
    Parameter(u32),
}

type Roots = BTreeSet<Root>;
type State = BTreeMap<LocalId, Roots>;

fn buffer_desc(ty: &Type) -> bool {
    matches!(ty, Type::Desc(inner) if **inner == Type::ByteBuffer)
}

fn state_cost(state: &State, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(state.len(), "sparse Hold state")?;
    for roots in state.values() {
        budget.spend(roots.len(), "sparse Hold relations")?;
    }
    Ok(())
}

fn extend(target: &mut Roots, source: &Roots, budget: &mut Budget) -> CompileResult<bool> {
    let before = target.len();
    for root in source {
        budget.lookup(target.len(), "Hold root union")?;
        target.insert(*root);
    }
    Ok(before != target.len())
}

fn infer(
    module: &TypedModule,
    summaries: &[Roots],
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    budget: &mut Budget,
) -> CompileResult<Vec<ValueHold>> {
    let v = f.values.len();
    budget.spend(v, "dense Hold candidate")?;
    let captured_address = |ty: &Type| match ty {
        Type::Closure(id) => module
            .closures
            .get(id.0 as usize)
            .is_some_and(|c| c.captures.iter().any(|t| matches!(t, Type::Address { .. }))),
        _ => false,
    };
    if !f
        .values
        .iter()
        .any(|ty| matches!(ty, Type::Address { .. }) || buffer_desc(ty))
        && !f
            .parameters
            .iter()
            .any(|p| captured_address(&f.locals[p.0 as usize].ty))
    {
        return Ok((0..v)
            .map(|i| ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect());
    }
    budget.spend(f.blocks.len(), "sparse Hold CFG states")?;
    let mut values = vec![Roots::new(); v];
    let mut outputs = vec![State::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1, "Hold fixed point")?;
            if !reachable[b] {
                continue;
            }
            let mut state = State::new();
            if b == f.entry.0 as usize {
                budget.spend(f.parameters.len(), "incoming Hold parameters")?;
                for (index, local) in f.parameters.iter().enumerate() {
                    if matches!(f.locals[local.0 as usize].ty, Type::Address { .. })
                        || buffer_desc(&f.locals[local.0 as usize].ty)
                        || captured_address(&f.locals[local.0 as usize].ty)
                    {
                        state.insert(*local, Roots::from([Root::Parameter(index as u32)]));
                    }
                }
            } else {
                for &predecessor in &predecessors[b] {
                    budget.spend(1, "Hold predecessor")?;
                    if !reachable[predecessor] {
                        continue;
                    }
                    for (local, incoming) in &outputs[predecessor] {
                        budget.lookup(state.len(), "Hold local join")?;
                        extend(state.entry(*local).or_default(), incoming, budget)?;
                    }
                }
            }
            for instruction in &block.instructions {
                budget.spend(1, "Hold instruction")?;
                let mut found = Roots::new();
                match &instruction.kind {
                    InstructionKind::DescribeLocal { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(inner) if **inner == Type::ByteBuffer) =>
                    {
                        found.insert(Root::Local(*local));
                    }
                    InstructionKind::DescribePackageField { local, index }
                        if instruction
                            .result
                            .is_some_and(|value| buffer_desc(&f.values[value.0 as usize])) =>
                    {
                        let root = match &f.locals[local.0 as usize].ty {
                            Type::Package { fields, .. } if fields.get(*index as usize)
                                .is_some_and(|(_, ty)| matches!(ty, Type::Managed(inner) if **inner == Type::ByteBuffer)) => Root::Field(*local, *index),
                            _ => Root::Local(*local),
                        };
                        found.insert(root);
                    }
                    InstructionKind::ProjectOwnedArrayField { local, .. }
                        if instruction
                            .result
                            .is_some_and(|value| buffer_desc(&f.values[value.0 as usize])) =>
                    {
                        // Array element slots are independently owned, but the
                        // initial window profile keeps a conservative whole
                        // array storage origin across element replacement.
                        found.insert(Root::Local(*local));
                    }
                    InstructionKind::DescribeCapture { index }
                        if instruction
                            .result
                            .is_some_and(|value| buffer_desc(&f.values[value.0 as usize])) =>
                    {
                        found.insert(Root::Capture(*index));
                    }
                    InstructionKind::ByteBufferBorrow { buffer, .. } => {
                        let source = values
                            .get(buffer.0 as usize)
                            .ok_or_else(|| bad("ByteBuffer source outside table"))?;
                        extend(&mut found, source, budget)?;
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        found.insert(Root::Local(*local));
                        if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                            && let Some(held) = state.get(local)
                        {
                            extend(&mut found, held, budget)?;
                        }
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. } => {
                        let source = values
                            .get(address.0 as usize)
                            .ok_or_else(|| bad("projection address outside table"))?;
                        extend(&mut found, source, budget)?;
                    }
                    InstructionKind::BorrowCapture { index, .. }
                    | InstructionKind::ReadCapture { index } => {
                        let ty = instruction
                            .result
                            .and_then(|id| f.values.get(id.0 as usize));
                        if matches!(ty, Some(Type::Address { .. })) {
                            found.insert(Root::Capture(*index));
                        }
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        budget.lookup(state.len(), "Hold load lookup")?;
                        if let Some(source) = state.get(local) {
                            budget.spend(source.len(), "Hold load")?;
                            found = source.clone();
                        }
                        if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                            state.remove(local);
                        }
                    }
                    InstructionKind::Store { local, value } => {
                        let source = values
                            .get(value.0 as usize)
                            .ok_or_else(|| bad("Hold value outside table"))?;
                        budget.lookup(state.len(), "Hold store lookup")?;
                        budget.spend(source.len(), "Hold store")?;
                        if source.is_empty() {
                            state.remove(local);
                        } else {
                            state.insert(*local, source.clone());
                        }
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::BindPackageCapture { local, .. }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropClosure { local } => {
                        budget.lookup(state.len(), "Hold storage lookup")?;
                        state.remove(local);
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } if instruction.result.is_some_and(|value| {
                        matches!(f.values.get(value.0 as usize), Some(Type::Address { .. }))
                    }) =>
                    {
                        let incoming = summaries
                            .get(function.0 as usize)
                            .ok_or_else(|| bad("call refers to an absent function"))?;
                        for root in incoming {
                            budget.spend(1, "call result Hold substitution")?;
                            let Root::Parameter(index) = root else {
                                return Err(bad("call summary has a non-parameter root"));
                            };
                            let argument = arguments
                                .get(*index as usize)
                                .ok_or_else(|| bad("call summary parameter is absent"))?;
                            let actual = values
                                .get(argument.0 as usize)
                                .ok_or_else(|| bad("call argument outside value table"))?;
                            extend(&mut found, actual, budget)?;
                        }
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        for value in captures {
                            budget.spend(1, "Hold capture operand")?;
                            let source = values
                                .get(value.0 as usize)
                                .ok_or_else(|| bad("Hold capture outside table"))?;
                            extend(&mut found, source, budget)?;
                        }
                    }
                    _ => {}
                }
                if let Some(result) = instruction.result {
                    let value = values
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("Hold result outside table"))?;
                    changed |= extend(value, &found, budget)?;
                }
            }
            state_cost(&state, budget)?;
            state_cost(&outputs[b], budget)?;
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut exported = Vec::with_capacity(values.len());
    for (value, roots) in values.into_iter().enumerate() {
        let mut coarse = BTreeSet::new();
        for root in roots {
            budget.lookup(coarse.len(), "public Hold projection")?;
            coarse.insert(match root {
                Root::Local(local) | Root::Field(local, _) => HoldRoot::Local(local),
                Root::Capture(index) => HoldRoot::Capture(index),
                Root::Parameter(index) => HoldRoot::Parameter(index),
            });
        }
        exported.push(ValueHold {
            value: ValueId(value as u32),
            roots: coarse.into_iter().collect(),
        });
    }
    Ok(exported)
}

/// Return contracts form a finite monotone lattice of parameter indices. An
/// empty approximation is never authority: the verifier rejects ungrounded
/// address returns after the independent fixed point.
pub(super) fn infer_module(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<ValueHold>>> {
    if !module
        .functions
        .iter()
        .any(|f| matches!(f.return_type, Type::Address { .. }))
    {
        return module
            .functions
            .iter()
            .zip(graphs)
            .map(|(f, (preds, reachable))| infer(module, &[], f, preds, reachable, budget))
            .collect();
    }
    let count = module.functions.len();
    // Only executable consumers of a return-origin summary create an edge.
    // Local CFG solving stays independent of this module-level scheduling.
    budget.product(count, 5, 1, "return frontier tables")?;
    let mut callers = vec![Vec::<usize>::new(); count];
    for (caller, (function, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in function.blocks.iter().zip(reachable) {
            budget.spend(1, "return frontier block")?;
            if !live {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1, "return frontier instruction")?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                    && instruction.result.is_some_and(|value| {
                        matches!(
                            function.values.get(value.0 as usize),
                            Some(Type::Address { .. })
                        )
                    })
                {
                    let users = callers
                        .get_mut(callee.0 as usize)
                        .ok_or_else(|| bad("call refers to an absent function"))?;
                    // Callers are visited in increasing order, so repeated calls
                    // in one body can be deduplicated without sorting or a set.
                    if users.last() != Some(&caller) {
                        budget.spend(1, "return frontier edge")?;
                        users.push(caller);
                    }
                }
            }
        }
    }
    let mut summaries = vec![Roots::new(); count];
    let mut results: Vec<Option<Vec<ValueHold>>> = (0..count).map(|_| None).collect();
    let mut pending: VecDeque<usize> = (0..count).collect();
    let mut queued = vec![true; count];
    while let Some(index) = pending.pop_front() {
        budget.spend(1, "return frontier pop")?;
        queued[index] = false;
        #[cfg(test)]
        TEST_SOLVES.with(|n| n.set(n.get() + 1));
        let function = &module.functions[index];
        let (predecessors, reachable) = &graphs[index];
        let values = infer(
            module,
            &summaries,
            function,
            predecessors,
            reachable,
            budget,
        )?;
        let mut changed = false;
        if matches!(function.return_type, Type::Address { .. }) {
            let mut result = Roots::new();
            for (block, live) in function.blocks.iter().zip(reachable) {
                budget.spend(1, "return contract block")?;
                if *live && let Terminator::Return(Some(value)) = block.terminator {
                    let roots = values
                        .get(value.0 as usize)
                        .ok_or_else(|| bad("return outside Hold table"))?;
                    for root in &roots.roots {
                        budget.spend(1, "return parameter origin")?;
                        if let HoldRoot::Parameter(index) = root {
                            result.insert(Root::Parameter(*index));
                        }
                    }
                }
            }
            changed = extend(&mut summaries[index], &result, budget)?;
        }
        results[index] = Some(values);
        if changed {
            budget.spend(callers[index].len(), "return frontier dependents")?;
            for &caller in &callers[index] {
                if !queued[caller] {
                    budget.spend(1, "return frontier push")?;
                    queued[caller] = true;
                    pending.push_back(caller);
                }
            }
        }
    }
    results
        .into_iter()
        .map(|result| result.ok_or_else(|| bad("return frontier omitted a function")))
        .collect()
}

#[cfg(test)]
thread_local! {
    static TEST_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod frontier_tests {
    use super::*;

    fn fixture(edges: &[Option<usize>], grounded: &[bool], swap: bool) -> TypedModule {
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let span = zil_base::Span::new(zil_base::FileId(0), 0, 1);
        let instruction = |result, kind| Instruction {
            result: Some(ValueId(result)),
            kind,
            span,
        };
        let functions = edges
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let mut values = vec![address.clone(), address.clone(), Type::Bool];
                let instructions = vec![
                    instruction(0, InstructionKind::Load(LocalId(0))),
                    instruction(1, InstructionKind::Load(LocalId(1))),
                    instruction(2, InstructionKind::Load(LocalId(2))),
                ];
                let mut blocks = vec![BasicBlock {
                    id: BlockId(0),
                    instructions,
                    terminator: Terminator::Return(Some(ValueId(0))),
                }];
                if let Some(target) = target {
                    values.push(address.clone());
                    let arguments = if swap {
                        vec![ValueId(1), ValueId(0), ValueId(2)]
                    } else {
                        vec![ValueId(0), ValueId(1), ValueId(2)]
                    };
                    let call = instruction(
                        3,
                        InstructionKind::Call {
                            function: FunctionId(*target as u32),
                            arguments,
                        },
                    );
                    if grounded[i] {
                        blocks[0].terminator = Terminator::Branch {
                            condition: ValueId(2),
                            then_block: BlockId(1),
                            else_block: BlockId(2),
                        };
                        blocks.push(BasicBlock {
                            id: BlockId(1),
                            instructions: vec![],
                            terminator: Terminator::Return(Some(ValueId(0))),
                        });
                        blocks.push(BasicBlock {
                            id: BlockId(2),
                            instructions: vec![call],
                            terminator: Terminator::Return(Some(ValueId(3))),
                        });
                    } else {
                        blocks[0].instructions.push(call);
                        blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
                    }
                }
                TypedFunction {
                    callable_return_contract: None,
                    callable_contract: None,
                    result_mode: zil_ir::PassingMode::Infer,
                    id: FunctionId(i as u32),
                    name: format!("f{i}"),
                    foreign: None,
                    environment: None,
                    parameters: vec![LocalId(0), LocalId(1), LocalId(2)],
                    return_type: address.clone(),
                    locals: vec![address.clone(), address.clone(), Type::Bool]
                        .into_iter()
                        .enumerate()
                        .map(|(n, ty)| LocalDecl {
                            id: LocalId(n as u32),
                            name: format!("p{n}"),
                            ty,
                            mutable: false,
                            span,
                        })
                        .collect(),
                    values,
                    blocks,
                    entry: BlockId(0),
                    span,
                }
            })
            .collect();
        TypedModule {
            resources: Vec::new(),
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            source_fingerprint: zil_base::Fingerprint::ZERO,
            target: zil_base::TargetProfile::default(),
            functions,
            closures: vec![],
            unsupported: vec![],
        }
    }
    fn slow(
        module: &TypedModule,
        graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
        budget: &mut Budget,
    ) -> CompileResult<Vec<Vec<ValueHold>>> {
        if !module
            .functions
            .iter()
            .any(|f| matches!(f.return_type, Type::Address { .. }))
        {
            return module
                .functions
                .iter()
                .zip(graphs)
                .map(|(f, (preds, reachable))| infer(module, &[], f, preds, reachable, budget))
                .collect();
        }
        budget.spend(module.functions.len(), "return contract table")?;
        let mut summaries = vec![Roots::new(); module.functions.len()];
        loop {
            let mut changed = false;
            let mut results = Vec::with_capacity(module.functions.len());
            for (index, function) in module.functions.iter().enumerate() {
                let (predecessors, reachable) = &graphs[index];
                TEST_SOLVES.with(|n| n.set(n.get() + 1));
                let values = infer(
                    module,
                    &summaries,
                    function,
                    predecessors,
                    reachable,
                    budget,
                )?;
                if matches!(function.return_type, Type::Address { .. }) {
                    let mut result = Roots::new();
                    for (block, live) in function.blocks.iter().zip(reachable) {
                        budget.spend(1, "return contract block")?;
                        if *live && let Terminator::Return(Some(value)) = block.terminator {
                            let roots = values
                                .get(value.0 as usize)
                                .ok_or_else(|| bad("return outside Hold table"))?;
                            for root in &roots.roots {
                                budget.spend(1, "return parameter origin")?;
                                if let HoldRoot::Parameter(index) = root {
                                    result.insert(Root::Parameter(*index));
                                }
                            }
                        }
                    }
                    changed |= extend(&mut summaries[index], &result, budget)?;
                }
                results.push(values);
            }
            if !changed {
                return Ok(results);
            }
        }
    }

    fn graphs(module: &TypedModule) -> Vec<(Vec<Vec<usize>>, Vec<bool>)> {
        module
            .functions
            .iter()
            .map(|f| {
                if f.blocks.len() == 1 {
                    (vec![vec![]], vec![true])
                } else {
                    (vec![vec![], vec![0], vec![0]], vec![true; 3])
                }
            })
            .collect()
    }
    fn compare(module: &TypedModule) -> (usize, usize) {
        let graphs = graphs(module);
        TEST_SOLVES.with(|n| n.set(0));
        let fast = infer_module(module, &graphs, &mut Budget::new(&Limits::default())).unwrap();
        let fast_visits = TEST_SOLVES.with(|n| n.get());
        TEST_SOLVES.with(|n| n.set(0));
        let reference = slow(module, &graphs, &mut Budget::new(&Limits::default())).unwrap();
        let slow_visits = TEST_SOLVES.with(|n| n.get());
        assert_eq!(fast, reference);
        (fast_visits, slow_visits)
    }
    #[test]
    fn frontier_matches_full_scan_on_cycles_components_and_parameter_permutations() {
        for seed in 0..24 {
            let count = 13;
            let edges: Vec<_> = (0..count)
                .map(|i| {
                    if (i + seed) % 7 == 0 {
                        None
                    } else {
                        Some((i * 7 + seed) % count)
                    }
                })
                .collect();
            let grounded: Vec<_> = (0..count).map(|i| (i + seed) % 3 == 0).collect();
            compare(&fixture(&edges, &grounded, seed % 2 == 0));
        }
        compare(&fixture(&[Some(1), Some(0)], &[false, false], true));
    }
    #[test]
    fn frontier_rechecks_only_relevant_functions_on_reverse_chain() {
        let n = 96;
        let edges: Vec<_> = (0..n).map(|i| (i + 1 < n).then_some(i + 1)).collect();
        let (fast, slow) = compare(&fixture(&edges, &vec![false; n], false));
        assert_eq!(fast, 2 * n - 1);
        assert_eq!(slow, n * (n + 1));
    }
    #[test]
    fn frontier_rejects_absent_target_and_zero_budget_without_panicking() {
        let module = fixture(&[Some(99)], &[false], false);
        let error = infer_module(
            &module,
            &graphs(&module),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
        assert_eq!(error[0].code, "E-ANALYZE-IR");
        let module = fixture(&[None], &[false], false);
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error = infer_module(&module, &graphs(&module), &mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error[0].category, zil_base::ResultCategory::ResourceLimit);
    }
}
