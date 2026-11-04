//! Analyze's independent structured external-call envelope; no producer claims.
use super::*;
pub(super) fn entry(
    module: &TypedModule,
    closure: &ClosureDefinition,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(
        closure.parameters.len().saturating_add(1),
        "scoped actual closure entry signature",
    )?;
    let f = module
        .functions
        .get(closure.function.0 as usize)
        .ok_or_else(|| bad("scoped closure entry is absent"))?;
    if f.environment != Some(closure.id)
        || f.foreign.is_some()
        || f.return_type != closure.result
        || f.parameters.len() != closure.parameters.len()
        || f.result_mode != PassingMode::Infer
    {
        return Err(bad("scoped closure differs from its actual entry"));
    }
    for (local, ty) in f.parameters.iter().zip(&closure.parameters) {
        if f.locals.get(local.0 as usize).map(|l| &l.ty) != Some(ty) {
            return Err(bad("scoped closure entry parameter differs"));
        }
    }
    Ok(())
}
pub(super) fn declaration(
    function: &TypedFunction,
    role: ScopedCallbackRole,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(
        function.parameters.len().saturating_add(1),
        "scoped C declaration signature",
    )?;
    let foreign = function
        .foreign
        .as_ref()
        .ok_or_else(|| bad("scoped callback target is not foreign"))?;
    let callback = Type::ForeignCallback {
        parameters: vec![Type::ForeignHandle, Type::I32],
        result: Box::new(Type::I64),
    };
    let expected = match role {
        ScopedCallbackRole::AsyncRegister | ScopedCallbackRole::AsyncRequestClose | ScopedCallbackRole::AsyncPollDrain => {
            return Err(vec![Diagnostic::unsupported(Phase::Analyze, "pollable registration foreign role consumer is not implemented", Some(function.span))]);
        }
        ScopedCallbackRole::Register => vec![callback, Type::ForeignHandle],
        ScopedCallbackRole::Close => vec![Type::ForeignHandle],
    };
    let result = if role == ScopedCallbackRole::Register {
        Type::ForeignHandle
    } else {
        Type::Unit
    };
    if foreign.abi != "C"
        || !foreign.static_callbacks
        || foreign.scoped_callback != Some(role)
        || !foreign.noescape_parameters.is_empty()
        || function.environment.is_some()
        || function.parameters.len() != expected.len()
        || function.return_type != result
    {
        return Err(bad(
            "scoped callback declaration role or C signature differs",
        ));
    }
    for (parameter, expected) in function.parameters.iter().zip(expected) {
        if function.locals.get(parameter.0 as usize).map(|l| &l.ty) != Some(&expected) {
            return Err(bad("scoped callback declaration parameter differs"));
        }
    }
    Ok(())
}
pub(super) fn check(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<()> {
    if !budget.scoped_foreign {
        return Ok(());
    }
    let mut calls = Vec::new();
    for function in &module.functions {
        budget.spend(1, "scoped callback declaration inventory")?;
        if let Some(role) = function.foreign.as_ref().and_then(|f| f.scoped_callback) {
            declaration(function, role, budget)?;
        }
        for block in &function.blocks {
            budget.spend(1, "scoped callback envelope block")?;
            for instruction in &block.instructions {
                budget.spend(1, "scoped callback envelope instruction")?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    budget.spend(1, "scoped actual call dependency")?;
                    calls.push((function.id, callee));
                }
                let closure = match &instruction.kind {
                    InstructionKind::MakeClosure { closure, .. } => Some(*closure),
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        function.locals.get(callee.0 as usize).and_then(|l| {
                            if let Type::Closure(id) = l.ty {
                                Some(id)
                            } else {
                                None
                            }
                        })
                    }
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        function.values.get(callee.0 as usize).and_then(|ty| {
                            let ty = if let Type::Address { inner, .. } = ty {
                                inner.as_ref()
                            } else {
                                ty
                            };
                            if let Type::Closure(id) = ty {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                    }
                    _ => None,
                };
                if let Some(id) = closure {
                    let entry = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad("scoped actual call environment is absent"))?
                        .function;
                    budget.spend(1, "scoped actual environment dependency")?;
                    calls.push((function.id, entry));
                }
                match instruction.kind {
                    InstructionKind::ForeignCall {
                        function: callee, ..
                    }
                    | InstructionKind::Call {
                        function: callee, ..
                    }
                    | InstructionKind::FunctionRef { function: callee }
                    | InstructionKind::ForeignFunctionRef { function: callee }
                        if module
                            .functions
                            .get(callee.0 as usize)
                            .and_then(|f| f.foreign.as_ref())
                            .is_some_and(|f| f.scoped_callback.is_some()) =>
                    {
                        return Err(bad(
                            "scoped callback role cannot be an ordinary ForeignCall",
                        ));
                    }
                    InstructionKind::WithForeignCallback {
                        register,
                        close,
                        callback,
                        body,
                        ..
                    } => {
                        declaration(
                            &module.functions[register.0 as usize],
                            ScopedCallbackRole::Register,
                            budget,
                        )?;
                        declaration(
                            &module.functions[close.0 as usize],
                            ScopedCallbackRole::Close,
                            budget,
                        )?;
                        let Type::Address {
                            mutable: true,
                            inner,
                        } = &function.values[callback.0 as usize]
                        else {
                            return Err(bad("scoped callback needs an exclusive closure address"));
                        };
                        let Type::Closure(id) = inner.as_ref() else {
                            return Err(bad("scoped callback address has no actual closure"));
                        };
                        let callback = module
                            .closures
                            .get(id.0 as usize)
                            .ok_or_else(|| bad("scoped callback definition is absent"))?;
                        if !matches!(callback.kind, CallableKind::Fn | CallableKind::FnMut)
                            || callback.parameters != [Type::I32]
                            || callback.result != Type::I32
                            || !callback.lending_captures.is_empty()
                        {
                            return Err(bad(
                                "scoped callback requires self-contained Fn/FnMut(i32)->i32",
                            ));
                        }
                        callable_fields::self_contained(module, *id, budget)?;
                        entry(module, callback, budget)?;
                        let Type::Closure(body) = function.values[body.0 as usize] else {
                            return Err(bad("scoped body must be a concrete closure value"));
                        };
                        let body = module
                            .closures
                            .get(body.0 as usize)
                            .ok_or_else(|| bad("scoped body definition is absent"))?;
                        if !body.parameters.is_empty()
                            || body.result != Type::I32
                            || body.kind == CallableKind::FunctionPointer
                        {
                            return Err(bad(
                                "scoped body must support once-consumption with ()->i32",
                            ));
                        }
                        entry(module, body, budget)?;
                        if instruction
                            .result
                            .and_then(|v| function.values.get(v.0 as usize))
                            != Some(&Type::Tuple(vec![Type::Bool, Type::I32]))
                        {
                            return Err(bad("scoped callback result must be (bool,i32)"));
                        }
                    }
                    _ => {}
                }
            }
        }
        budget.lookup(
            budget.scoped_functions.len(),
            "scoped callback CFG inventory",
        )?;
        if !budget.scoped_functions.contains(&function.id) {
            continue;
        }
        let reachable = &graphs[function.id.0 as usize].1;
        budget.spend(
            function.blocks.len().saturating_mul(2),
            "scoped callback unsafe CFG states",
        )?;
        let mut inputs = vec![None; function.blocks.len()];
        let mut queue = VecDeque::new();
        inputs[function.entry.0 as usize] = Some(Vec::<u32>::new());
        queue.push_back(function.entry.0 as usize);
        while let Some(index) = queue.pop_front() {
            budget.spend(1, "scoped callback unsafe CFG visit")?;
            if !reachable[index] {
                continue;
            }
            let incoming = inputs[index].as_ref().unwrap();
            budget.spend(incoming.len(), "scoped callback unsafe stack copy")?;
            let mut stack = incoming.clone();
            let block = &function.blocks[index];
            for instruction in &block.instructions {
                budget.spend(1, "scoped callback unsafe instruction")?;
                match instruction.kind {
                    InstructionKind::UnsafeEnter { region } => {
                        budget.spend(1, "scoped callback unsafe enter")?;
                        stack.push(region);
                    }
                    InstructionKind::UnsafeExit { region } => {
                        if stack.pop() != Some(region) {
                            return Err(bad(
                                "scoped callback unsafe exit mismatches its active region",
                            ));
                        }
                    }
                    InstructionKind::WithForeignCallback { region, .. }
                    | InstructionKind::RegistrationStart { region, .. }
                        if stack.last() != Some(&region) =>
                    {
                        return Err(bad("scoped callback has no actual active unsafe region"));
                    }
                    _ => {}
                }
            }
            let successors = match block.terminator {
                Terminator::Jump(a) => vec![a],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block, else_block],
                _ => vec![],
            };
            if matches!(block.terminator, Terminator::Return(_)) && !stack.is_empty() {
                return Err(bad("scoped callback return leaves an unsafe region active"));
            }
            budget.spend(successors.len(), "scoped callback unsafe CFG edges")?;
            for successor in successors {
                let target = &mut inputs[successor.0 as usize];
                if let Some(previous) = target {
                    budget.spend(
                        previous.len().saturating_add(stack.len()),
                        "scoped unsafe join",
                    )?;
                    if previous != &stack {
                        return Err(bad(
                            "scoped callback unsafe region differs across predecessors",
                        ));
                    }
                } else {
                    budget.spend(
                        stack.len().saturating_add(1),
                        "scoped unsafe state publication",
                    )?;
                    *target = Some(stack.clone());
                    queue.push_back(successor.0 as usize);
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (caller, callee) in &calls {
            budget.lookup(
                budget.scoped_functions.len(),
                "scoped actual caller dependency",
            )?;
            if budget.scoped_functions.contains(callee) {
                budget.lookup(
                    budget.scoped_functions.len(),
                    "scoped caller window publication",
                )?;
                changed |= budget.scoped_functions.insert(*caller);
            }
        }
        if !changed {
            break;
        }
    }
    Ok(())
}
