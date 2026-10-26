//! Reconstruct lexical cleanup boundaries from executable CFG operations.
//!
//! Scope IDs, exit kinds and producer plans are untrusted. The resource checker
//! separately proves actual debt conservation and reverse construction order;
//! this checker prevents a producer from moving storage cleanup past its scope.
//! Old non-error raw IR without scope operations retains its former whole-
//! function resource contract. Any scope, error-union or package use enables this entire
//! structural contract, including parameter storage and every local access.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    stack: Vec<ScopeId>,
    live: BTreeMap<LocalId, ScopeId>,
    root_entered: bool,
}

fn state_cost(state: &State, budget: &mut Budget) -> Check<()> {
    budget.spend(
        state
            .stack
            .len()
            .saturating_add(state.live.len())
            .saturating_add(1),
    )
}

fn has_scoped_type(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::ErrorUnion { .. } | Type::Package { .. } => return Ok(true),
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Array { element, .. }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => pending.push(element),
            Type::Record { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(false)
}

fn accessed_local(instruction: &InstructionKind) -> Option<LocalId> {
    match instruction {
        InstructionKind::Load(local)
        | InstructionKind::Store { local, .. }
        | InstructionKind::DescribeLocal { local }
        | InstructionKind::ReadPackageField { local, .. }
        | InstructionKind::DescribePackageField { local, .. }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => Some(*local),
        _ => None,
    }
}

pub(super) fn check_function(
    function: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    let bad = |span, message| reject("E-VERIFY-CLEANUP", Some(span), message);
    let mut required = false;
    for ty in std::iter::once(&function.return_type)
        .chain(function.locals.iter().map(|local| &local.ty))
        .chain(&function.values)
    {
        required |= has_scoped_type(ty, budget)?;
    }
    let mut declarations = BTreeSet::new();
    let mut error_values = BTreeSet::new();
    for (index, block) in function.blocks.iter().enumerate() {
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::ScopeEnter { scope } => {
                    required = true;
                    budget.spend(declarations.len().saturating_add(1))?;
                    if !declarations.insert(scope) {
                        return Err(bad(
                            instruction.span,
                            "a cleanup scope requires exactly one syntactic enter",
                        ));
                    }
                }
                InstructionKind::ScopeExit { .. }
                | InstructionKind::BindPackageCapture { .. }
                | InstructionKind::EndPackageCapture { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::ReadPackageField { .. }
                | InstructionKind::DescribePackageField { .. }
                | InstructionKind::TakePackageField { .. }
                | InstructionKind::DropPackageField { .. }
                | InstructionKind::StorePackageField { .. }
                | InstructionKind::DropPackage { .. }
                | InstructionKind::MakeErrorUnion { .. }
                | InstructionKind::ErrorTag { .. }
                | InstructionKind::ReadErrorUnion { .. } => required = true,
                _ => {}
            }
            if let InstructionKind::MakeErrorUnion { is_error: true, .. } = instruction.kind {
                budget.spend(error_values.len().saturating_add(1))?;
                error_values.insert(instruction.result.expect("checked error union result"));
            }
            if !graph.reachable[index]
                && matches!(
                    instruction.kind,
                    InstructionKind::ScopeEnter { .. }
                        | InstructionKind::ScopeExit { .. }
                        | InstructionKind::BindPackageCapture { .. }
                        | InstructionKind::EndPackageCapture { .. }
                )
            {
                return Err(bad(
                    instruction.span,
                    "unreachable scope operations cannot bypass cleanup verification",
                ));
            }
        }
    }
    if !required {
        return Ok(());
    }
    let entry = function.entry.0 as usize;
    if !function.blocks[entry]
        .instructions
        .first()
        .is_some_and(|i| matches!(i.kind, InstructionKind::ScopeEnter { .. }))
    {
        return Err(bad(
            function.span,
            "scoped or error-union functions must begin with a root ScopeEnter",
        ));
    }
    budget.spend(function.blocks.len())?;
    let mut inputs = vec![None::<State>; function.blocks.len()];
    inputs[entry] = Some(State::default());
    let mut pending = vec![entry];
    while let Some(index) = pending.pop() {
        budget.spend(1)?;
        let input = inputs[index].as_ref().expect("queued cleanup state");
        state_cost(input, budget)?;
        let mut state = input.clone();
        let block = &function.blocks[index];
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::ScopeEnter { scope } => {
                    budget.spend(state.stack.len().saturating_add(1))?;
                    if state.stack.contains(&scope) {
                        return Err(bad(instruction.span, "cleanup scope is already active"));
                    }
                    if state.stack.is_empty() {
                        if state.root_entered {
                            return Err(bad(
                                instruction.span,
                                "a closed function root cannot be reopened or replaced",
                            ));
                        }
                        state.root_entered = true;
                        // Parameter storage exists on entry. Registering it in
                        // the root does not reset its independently tracked debt.
                        budget.spend(function.parameters.len())?;
                        for &parameter in &function.parameters {
                            budget.spend(state.live.len().saturating_add(1))?;
                            state.live.insert(parameter, scope);
                        }
                    }
                    state.stack.push(scope);
                }
                InstructionKind::ScopeExit { scope, kind } => {
                    if state.stack.last() != Some(&scope) {
                        return Err(bad(
                            instruction.span,
                            "cleanup exit does not match the innermost active scope",
                        ));
                    }
                    budget.spend(state.live.len())?;
                    if state.live.values().any(|owner| *owner == scope) {
                        return Err(bad(
                            instruction.span,
                            "scope exit leaves live storage; ordinary locals need StorageDead and capture views need EndPackageCapture",
                        ));
                    }
                    match kind {
                        CleanupExitKind::Return => {
                            if !matches!(block.terminator, Terminator::Return(_)) {
                                return Err(bad(
                                    instruction.span,
                                    "Return cleanup must lead to an ordinary return on this edge",
                                ));
                            }
                        }
                        CleanupExitKind::Error => {
                            budget.spend(error_values.len().saturating_add(1))?;
                            if !matches!(block.terminator, Terminator::Return(Some(value)) if error_values.contains(&value))
                            {
                                return Err(bad(
                                    instruction.span,
                                    "Error cleanup must return an independently constructed error value",
                                ));
                            }
                        }
                        CleanupExitKind::Break | CleanupExitKind::Continue => {
                            if state.stack.len() == 1
                                || !matches!(block.terminator, Terminator::Jump(_))
                            {
                                return Err(bad(
                                    instruction.span,
                                    "loop cleanup requires a jump with an enclosing scope retained",
                                ));
                            }
                        }
                        CleanupExitKind::Fallthrough => {}
                    }
                    state.stack.pop();
                }
                _ => {
                    let Some(&scope) = state.stack.last() else {
                        return Err(bad(
                            instruction.span,
                            "executable instruction is outside the active function root",
                        ));
                    };
                    match instruction.kind {
                        InstructionKind::BindPackageCapture { local, .. } => {
                            budget.spend(state.live.len().saturating_add(1))?;
                            if state.live.insert(local, scope).is_some() {
                                return Err(bad(
                                    instruction.span,
                                    "Package capture view is already active",
                                ));
                            }
                        }
                        InstructionKind::EndPackageCapture { local, .. } => {
                            budget.spend(state.live.len().saturating_add(1))?;
                            if state.live.remove(&local).is_none() {
                                return Err(bad(
                                    instruction.span,
                                    "Package capture End has no active view",
                                ));
                            }
                        }
                        InstructionKind::StorageLive(local) => {
                            budget.spend(state.live.len().saturating_add(1))?;
                            if state.live.insert(local, scope).is_some() {
                                return Err(bad(
                                    instruction.span,
                                    "StorageLive reactivates already live local storage",
                                ));
                            }
                        }
                        InstructionKind::StorageDead(local) => {
                            budget.spend(state.live.len().saturating_add(1))?;
                            if state.live.remove(&local).is_none() {
                                return Err(bad(
                                    instruction.span,
                                    "StorageDead has no active local storage",
                                ));
                            }
                        }
                        _ => {
                            if let Some(local) = accessed_local(&instruction.kind) {
                                budget.spend(state.live.len().saturating_add(1))?;
                                if !state.live.contains_key(&local) {
                                    return Err(bad(
                                        instruction.span,
                                        "local operation occurs outside its active storage scope",
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        if matches!(block.terminator, Terminator::Return(_))
            && (!state.stack.is_empty() || !state.live.is_empty())
        {
            return Err(bad(
                function.span,
                "return must explicitly close all cleanup scopes and local storage",
            ));
        }
        // A defined trap is an abort, not a returnable error. It intentionally
        // has no normal cleanup promise; Error markers above cannot use it.
        for &successor in &graph.successors[index] {
            state_cost(&state, budget)?;
            if let Some(previous) = &inputs[successor] {
                state_cost(previous, budget)?;
                if previous != &state {
                    return Err(bad(
                        function.span,
                        "CFG join has incompatible cleanup scopes or storage ownership",
                    ));
                }
            } else {
                inputs[successor] = Some(state.clone());
                pending.push(successor);
            }
        }
    }
    Ok(())
}
