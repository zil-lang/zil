//! Producer diagnostics for explicit lexical storage. This reconstruction is
//! independent of Verify and exports no trusted scope certificate.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    stack: Vec<ScopeId>,
    live: BTreeMap<LocalId, ScopeId>,
}

pub(super) fn check(
    module: &TypedModule,
    function: &TypedFunction,
    mut package: bool,
    budget: &mut Budget,
) -> CompileResult<()> {
    if function.foreign.is_some() {
        return Ok(());
    }
    let mut entries = BTreeSet::new();
    let mut scoped = false;
    let mut has_end = false;
    let mut views = BTreeMap::new();
    let mut bound = BTreeSet::new();
    let mut expected = BTreeSet::new();
    let captures = function
        .environment
        .map(|id| &module.closures[id.0 as usize].captures);
    if let Some(captures) = captures {
        budget.spend(captures.len(), "package capture view inventory")?;
        for (index, ty) in captures.iter().enumerate() {
            if matches!(ty, Type::Package { .. }) {
                budget.lookup(expected.len(), "package capture inventory insertion")?;
                expected.insert(index as u32);
            }
        }
    }
    let entry = function.entry.0 as usize;
    let mut prefix = 1usize;
    if let Some(block) = function.blocks.get(entry) {
        while block
            .instructions
            .get(prefix)
            .is_some_and(|i| matches!(i.kind, InstructionKind::BindPackageCapture { .. }))
        {
            budget.spend(1, "package view entry prefix")?;
            prefix += 1;
        }
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        if !expected.is_empty() {
            budget.spend(1, "package view entry edge")?;
            if matches!(block.terminator, Terminator::Jump(target) if target == function.entry)
                || matches!(block.terminator, Terminator::Branch { then_block, else_block, .. }
                    if then_block == function.entry || else_block == function.entry)
            {
                return Err(bad("Package capture entry cannot have an incoming edge"));
            }
        }
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1, "scope entry scan")?;
            has_end |= matches!(instruction.kind, InstructionKind::EndPackageCapture { .. });
            package |= matches!(
                instruction.kind,
                InstructionKind::MakePackage { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::DescribePackageField { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::DropPackageField { .. }
                    | InstructionKind::StorePackageField { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::BindPackageCapture { .. }
                    | InstructionKind::EndPackageCapture { .. }
            );
            if let InstructionKind::BindPackageCapture { local, index } = instruction.kind {
                let ty = captures
                    .and_then(|captures| captures.get(index as usize))
                    .ok_or_else(|| bad("Package view has no actual capture"))?;
                budget.type_work(ty, false, "Package view exact type")?;
                budget.spend(
                    function.parameters.len(),
                    "Package view parameter exclusion",
                )?;
                budget.lookup(views.len(), "Package view local identity")?;
                budget.lookup(bound.len(), "Package view capture identity")?;
                if instruction.result.is_some()
                    || block_index != entry
                    || position == 0
                    || position >= prefix
                    || !matches!(ty, Type::Package { .. })
                    || function.locals.get(local.0 as usize).map(|decl| &decl.ty) != Some(ty)
                    || function.parameters.contains(&local)
                    || views.insert(local, index).is_some()
                    || !bound.insert(index)
                {
                    return Err(bad(
                        "Package capture requires one exact non-parameter view in the entry prefix",
                    ));
                }
            }
            if let InstructionKind::ScopeEnter { scope } = instruction.kind {
                budget.lookup(entries.len(), "scope identity")?;
                if !entries.insert(scope) {
                    return Err(bad("duplicate static scope entry"));
                }
                scoped = true;
            }
        }
    }
    budget.spend(
        expected.len().saturating_add(bound.len()),
        "Package view coverage",
    )?;
    if expected != bound {
        return Err(bad("every Package capture requires exactly one view"));
    }
    if !views.is_empty() {
        for block in &function.blocks {
            for instruction in &block.instructions {
                budget.spend(1, "Package view boundary scan")?;
                match instruction.kind {
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        budget.lookup(views.len(), "Package view storage boundary lookup")?;
                        if views.contains_key(&local) {
                            return Err(bad(
                                "Package view cannot use ordinary storage or address operations",
                            ));
                        }
                    }
                    InstructionKind::EndPackageCapture { local, index } => {
                        budget.lookup(views.len(), "Package view end boundary lookup")?;
                        if instruction.result.is_some() || views.get(&local) != Some(&index) {
                            return Err(bad("Package view End differs from its binding"));
                        }
                    }
                    _ => {}
                }
            }
        }
    } else if has_end {
        return Err(bad("Package view End has no binding"));
    }
    // Preserve the existing non-error raw-IR test/API subset. New source
    // functions always opt into the explicit storage/scope contract.
    if !scoped {
        if package {
            return Err(bad("package operations require explicit storage scopes"));
        }
        return Ok(());
    }
    let Some(Instruction {
        kind: InstructionKind::ScopeEnter { scope: root },
        ..
    }) = function.blocks[entry].instructions.first()
    else {
        return Err(bad("scope function must begin with its root entry"));
    };
    let mut inputs = vec![None; function.blocks.len()];
    inputs[entry] = Some(State::default());
    let mut pending = VecDeque::from([entry]);
    while let Some(index) = pending.pop_front() {
        let input = inputs[index].as_ref().expect("queued scope state");
        budget.spend(
            input.stack.len().saturating_add(input.live.len()),
            "scope state copy",
        )?;
        let mut state = input.clone();
        let block = &function.blocks[index];
        for instruction in &block.instructions {
            budget.spend(1, "scope transfer")?;
            match instruction.kind {
                InstructionKind::ScopeEnter { scope } => {
                    budget.spend(state.stack.len(), "scope nesting")?;
                    if state.stack.contains(&scope) {
                        return Err(bad("scope reentered while active"));
                    }
                    if state.stack.is_empty() && scope != *root {
                        return Err(bad("non-root scope entered outside root"));
                    }
                    state.stack.push(scope);
                    if scope == *root {
                        for local in &function.parameters {
                            budget.lookup(state.live.len(), "parameter scope registration")?;
                            if state.live.insert(*local, scope).is_some() {
                                return Err(bad("duplicate scoped parameter"));
                            }
                        }
                    }
                }
                InstructionKind::ScopeExit { scope, .. } => {
                    budget.spend(state.live.len(), "scope exit storage")?;
                    if state.stack.pop() != Some(scope) {
                        return Err(bad("scope exit is not innermost"));
                    }
                    if state.live.values().any(|owner| *owner == scope) {
                        return Err(bad("scope exit retains live storage"));
                    }
                }
                InstructionKind::StorageLive(local) => {
                    let scope = state
                        .stack
                        .last()
                        .copied()
                        .ok_or_else(|| bad("storage starts outside a scope"))?;
                    budget.lookup(state.live.len(), "storage start")?;
                    if state.live.insert(local, scope).is_some() {
                        return Err(bad("storage starts twice"));
                    }
                }
                InstructionKind::BindPackageCapture { local, .. } => {
                    budget.lookup(state.live.len(), "Package view activation")?;
                    if state.stack.as_slice() != [*root]
                        || state.live.insert(local, *root).is_some()
                    {
                        return Err(bad(
                            "Package view starts outside the root activation or starts twice",
                        ));
                    }
                }
                InstructionKind::EndPackageCapture { local, .. } => {
                    budget.lookup(state.live.len(), "Package view end")?;
                    if state.live.remove(&local) != Some(*root) {
                        return Err(bad("Package view ends before binding or more than once"));
                    }
                }
                InstructionKind::StorageDead(local) => {
                    budget.lookup(state.live.len(), "storage end")?;
                    if state.live.remove(&local).is_none() {
                        return Err(bad("storage ends before it starts"));
                    }
                }
                InstructionKind::Load(local)
                | InstructionKind::Store { local, .. }
                | InstructionKind::TakeLocal { local }
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::DescribeLocal { local }
                | InstructionKind::ReadPackageField { local, .. }
                | InstructionKind::DescribePackageField { local, .. }
                | InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::CallClosureMut { callee: local, .. }
                | InstructionKind::CallClosureShared { callee: local, .. } => {
                    budget.lookup(state.live.len(), "storage use")?;
                    if !state.live.contains_key(&local) {
                        return Err(bad("local used outside its storage activation"));
                    }
                }
                _ => {}
            }
        }
        let targets: Vec<BlockId> = match block.terminator {
            Terminator::Jump(target) => vec![target],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            Terminator::Return(_) => {
                if !state.stack.is_empty() || !state.live.is_empty() {
                    return Err(bad("return retains scope or storage"));
                }
                vec![]
            }
            _ => vec![],
        };
        for target in targets {
            budget.spend(
                state
                    .stack
                    .len()
                    .saturating_add(state.live.len())
                    .saturating_add(1),
                "scope edge",
            )?;
            let slot = &mut inputs[target.0 as usize];
            if let Some(prior) = slot {
                if prior != &state {
                    return Err(bad(
                        "control-flow join has inconsistent scope/storage activation",
                    ));
                }
            } else {
                *slot = Some(state.clone());
                pending.push_back(target.0 as usize);
            }
        }
    }
    Ok(())
}
