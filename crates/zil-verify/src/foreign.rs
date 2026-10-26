//! Independent structural FFI checking. Foreign implementation contracts remain
//! an explicitly trusted boundary; neither imports nor markers prove C behavior.
use super::*;
use std::collections::BTreeMap;

pub(super) fn check_declarations(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    let mut symbols = BTreeSet::new();
    for function in &module.functions {
        let Some(declaration) = &function.foreign else {
            continue;
        };
        let span = Some(function.span);
        if declaration.static_callbacks {
            native_values::target(&module.target, span)?;
        }
        budget.spend(
            declaration
                .abi
                .len()
                .saturating_add(declaration.symbol.len())
                .saturating_add(1),
        )?;
        let bad = |message| reject("E-VERIFY-FOREIGN", span, message);
        let bytes = declaration.symbol.as_bytes();
        if declaration.abi != "C"
            || !bytes
                .first()
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
            || !bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
            || declaration.symbol == "main"
        {
            return Err(bad(
                "foreign declarations require C ABI and an unreserved C identifier",
            ));
        }
        budget.spend(symbols.len().saturating_add(1))?;
        if !symbols.insert(&declaration.symbol) {
            return Err(bad(
                "a foreign symbol must have exactly one declaration in the module",
            ));
        }
        if function.environment.is_some()
            || !function.blocks.is_empty()
            || !function.values.is_empty()
            || function.entry != BlockId(0)
            || function.parameters.len() != function.locals.len()
            || !matches!(
                function.return_type,
                Type::Unit | Type::U8 | Type::U16 | Type::I32 | Type::I64 | Type::ForeignHandle
            )
        {
            return Err(bad(
                "foreign declarations have canonical parameter locals, no body/environment, and a C integer/handle/unit result",
            ));
        }
        if declaration.noescape_parameters.len() > function.parameters.len() {
            return Err(bad("noescape indices exceed the parameter count"));
        }
        budget.spend(declaration.noescape_parameters.len())?;
        let mut noescape = declaration.noescape_parameters.iter();
        budget.spend(function.parameters.len())?;
        for (index, (parameter, local)) in
            function.parameters.iter().zip(&function.locals).enumerate()
        {
            if parameter.0 as usize != index || local.id != *parameter || local.mutable {
                return Err(bad(
                    "foreign parameters must be canonical immutable parameter locals",
                ));
            }
            match &local.ty {
                Type::U8
                | Type::U16
                | Type::I32
                | Type::I64
                | Type::ForeignHandle
                | Type::CWideString => {}
                Type::ForeignCallback { parameters, result } => {
                    if !declaration.static_callbacks {
                        return Err(bad("foreign callback parameters require static_callbacks"));
                    }
                    budget.spend(parameters.len().saturating_add(1))?;
                    native_values::callback_signature(parameters, result, span)?;
                }
                Type::Address { inner, mutable } => {
                    if noescape.next().copied() != Some(index as u32) {
                        return Err(bad(
                            "noescape indices must exactly cover address parameters in ascending order",
                        ));
                    }
                    if c_layout::payload(
                        inner,
                        *mutable,
                        declaration.static_callbacks,
                        &module.target,
                        span,
                        budget,
                    )?
                    .is_none()
                    {
                        return Err(bad(
                            "borrowed C targets require the nonempty C payload profile; mutable storage excludes static strings/callbacks and callbacks require static_callbacks",
                        ));
                    }
                }
                _ => {
                    return Err(bad(
                        "foreign value parameters require C integers/handles/static strings/callbacks; aggregates require a supported borrowed C target",
                    ));
                }
            }
        }
        if noescape.next().is_some() {
            return Err(bad(
                "noescape indices contain a duplicate, scalar, or absent parameter",
            ));
        }
    }
    Ok(())
}

pub(super) fn check_call(
    module: &TypedModule,
    caller: &TypedFunction,
    callee: FunctionId,
    arguments: &[ValueId],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let bad = |message| reject("E-VERIFY-FOREIGN", span, message);
    let target = module
        .functions
        .get(callee.0 as usize)
        .ok_or_else(|| bad("foreign call target is absent"))?;
    if target.foreign.is_none() || target.environment.is_some() {
        return Err(bad("ForeignCall requires a declared foreign function"));
    }
    if arguments.len() != target.parameters.len() {
        return Err(bad(
            "foreign call arity differs from its declared C signature",
        ));
    }
    for (argument, parameter) in arguments.iter().zip(&target.parameters) {
        let actual = value_type(caller, *argument, span)?;
        let expected = local_type(target, *parameter, span)?;
        // Charge complete record and callback signatures on every call, not
        // only once during module preflight. No raw aggregate Eq shortcut.
        let equal = same_type(actual, expected, budget)?;
        if !equal {
            return Err(bad(
                "foreign argument differs from its declared C value or borrowed target type",
            ));
        }
    }
    Ok(if target.return_type == Type::Unit {
        None
    } else {
        Some(target.return_type.clone())
    })
}

fn same_type(actual: &Type, expected: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![(actual, expected)];
    while let Some((actual, expected)) = pending.pop() {
        budget.spend(1)?;
        match (actual, expected) {
            (
                Type::Address {
                    mutable: a,
                    inner: x,
                },
                Type::Address {
                    mutable: b,
                    inner: y,
                },
            ) if a == b => {
                pending.push((x, y));
            }
            (
                Type::Array {
                    element: x,
                    length: a,
                },
                Type::Array {
                    element: y,
                    length: b,
                },
            ) if a == b => {
                pending.push((x, y));
            }
            (Type::Record { name: a, fields: x }, Type::Record { name: b, fields: y }) => {
                budget.spend(a.len().saturating_add(b.len()).saturating_add(x.len()))?;
                if a != b || x.len() != y.len() {
                    return Ok(false);
                }
                for ((a, x), (b, y)) in x.iter().zip(y) {
                    budget.spend(a.len().saturating_add(b.len()))?;
                    if a != b {
                        return Ok(false);
                    }
                    pending.push((x, y));
                }
            }
            (
                Type::ForeignCallback {
                    parameters: x,
                    result: a,
                },
                Type::ForeignCallback {
                    parameters: y,
                    result: b,
                },
            ) if x.len() == y.len() => {
                budget.spend(x.len())?;
                pending.extend(x.iter().zip(y));
                pending.push((a, b));
            }
            (Type::Unit, Type::Unit)
            | (Type::U8, Type::U8)
            | (Type::U16, Type::U16)
            | (Type::I32, Type::I32)
            | (Type::I64, Type::I64)
            | (Type::ForeignHandle, Type::ForeignHandle)
            | (Type::CWideString, Type::CWideString) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn check_regions(
    function: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    let bad = |span, message| reject("E-VERIFY-UNSAFE", Some(span), message);
    // One syntactic enter per region; exits may occur on several control edges.
    let mut declarations = BTreeMap::<u32, Span>::new();
    for (index, block) in function.blocks.iter().enumerate() {
        for instruction in &block.instructions {
            budget.spend(1)?;
            if !graph.reachable[index]
                && matches!(
                    instruction.kind,
                    InstructionKind::UnsafeEnter { .. }
                        | InstructionKind::UnsafeExit { .. }
                        | InstructionKind::ForeignCall { .. }
                )
            {
                return Err(bad(
                    instruction.span,
                    "unreachable unsafe/foreign operations cannot bypass lexical verification",
                ));
            }
            if let InstructionKind::UnsafeEnter { region } = instruction.kind {
                budget.spend(declarations.len().saturating_add(1))?;
                if instruction.span.start > instruction.span.end
                    || declarations.insert(region, instruction.span).is_some()
                {
                    return Err(bad(
                        instruction.span,
                        "unsafe region requires one unique, valid lexical enter",
                    ));
                }
            }
        }
    }
    let entry = function.entry.0 as usize;
    budget.spend(function.blocks.len())?;
    let mut incoming = vec![None::<Vec<u32>>; function.blocks.len()];
    incoming[entry] = Some(Vec::new());
    let mut pending = vec![entry];
    while let Some(index) = pending.pop() {
        budget.spend(1)?;
        let input = incoming[index].as_ref().expect("queued region state");
        budget.spend(input.len())?;
        let mut stack = input.clone();
        let block = &function.blocks[index];
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::UnsafeEnter { region } => {
                    budget.spend(stack.len().saturating_add(1))?;
                    if stack.contains(&region) {
                        return Err(bad(
                            instruction.span,
                            "unsafe region is already active on this path",
                        ));
                    }
                    if let Some(parent) = stack.last() {
                        let outer = declarations[parent];
                        if outer.file != instruction.span.file
                            || outer.start > instruction.span.start
                            || outer.end < instruction.span.end
                        {
                            return Err(bad(
                                instruction.span,
                                "nested unsafe region is outside its lexical parent",
                            ));
                        }
                    }
                    stack.push(region);
                }
                InstructionKind::UnsafeExit { region } => {
                    if stack.pop() != Some(region) {
                        return Err(bad(
                            instruction.span,
                            "unsafe exit does not match the active lexical region",
                        ));
                    }
                }
                InstructionKind::ForeignCall { region, .. } => {
                    if stack.last() != Some(&region) {
                        return Err(bad(
                            instruction.span,
                            "foreign call lacks its active explicit unsafe region",
                        ));
                    }
                    let scope = declarations[&region];
                    if scope.file != instruction.span.file
                        || scope.start > instruction.span.start
                        || scope.end < instruction.span.end
                    {
                        return Err(bad(
                            instruction.span,
                            "foreign call is outside its lexical unsafe span",
                        ));
                    }
                }
                _ => {}
            }
        }
        if matches!(block.terminator, Terminator::Return(_)) && !stack.is_empty() {
            return Err(bad(
                function.span,
                "return exits must explicitly close every unsafe region",
            ));
        }
        for &successor in &graph.successors[index] {
            budget.spend(stack.len().saturating_add(1))?;
            if let Some(previous) = &incoming[successor] {
                if previous != &stack {
                    return Err(bad(
                        function.span,
                        "CFG join has incompatible unsafe region stacks",
                    ));
                }
            } else {
                incoming[successor] = Some(stack.clone());
                pending.push(successor);
            }
        }
    }
    Ok(())
}
