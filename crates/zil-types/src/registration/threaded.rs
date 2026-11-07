//! Reconstruct concrete environment Transfer separately from thread-call entry.
//! A declared Transfer premise queues only the data/cleanup check. A concrete
//! threaded constructor also queues its reachable entry graph; no declaration
//! bit or provider role is evidence about either actual graph.
use super::*;

fn spend(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "threaded registration capability exceeded budget",
            None,
        )
    })?;
    Ok(())
}
fn denied(span: Span) -> Diagnostic {
    error(
        "E-REGISTRATION-THREAD",
        span,
        "threaded callback requires actual closed Transfer captures, sealed synchronous cleanup and a finite thread-safe entry graph",
    )
}
fn shape(ty: &Type, descriptor: bool, left: &mut usize, span: Span) -> R<bool> {
    spend(left, 1)?;
    let mut pending = vec![(ty, true)];
    let mut buffer = false;
    while let Some((ty, owning)) = pending.pop() {
        spend(left, 1)?;
        match ty {
            Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64 => {}
            Type::Managed(p) if owning && p.as_ref() == &Type::ByteBuffer => buffer = true,
            Type::Desc(p) if descriptor && p.as_ref() == &Type::ByteBuffer => buffer = true,
            Type::Tuple(fields) => {
                spend(left, fields.len())?;
                pending.extend(fields.iter().map(|t| (t, owning)));
            }
            Type::Array { element, .. } => {
                spend(left, 1)?;
                pending.push((element.as_ref(), false));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                if !owning && matches!(ty, Type::Package { .. }) {
                    return Err(denied(span));
                }
                let child_owning = owning && matches!(ty, Type::Package { .. });
                spend(left, fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, child_owning)));
            }
            Type::Enum { variants, .. } => {
                spend(left, variants.len())?;
                for (_, fields) in variants {
                    spend(left, fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, owning)));
                }
            }
            Type::ErrorUnion { error, result } => {
                spend(left, 2)?;
                pending.extend([(error.as_ref(), owning), (result.as_ref(), owning)]);
            }
            _ => return Err(denied(span)),
        }
    }
    Ok(buffer)
}
fn environment(module: &TypedModule, id: ClosureId, left: &mut usize, span: Span) -> R<FunctionId> {
    spend(
        left,
        module
            .closures
            .len()
            .saturating_add(module.resources.len())
            .saturating_add(4),
    )?;
    let c = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| denied(span))?;
    if c.id != id || !c.lending_captures.is_empty() {
        return Err(denied(span));
    }
    spend(left, c.captures.len())?;
    let mut buffer = false;
    for ty in &c.captures {
        buffer |= shape(ty, false, left, span)?;
    }
    if buffer {
        let definitions: Vec<_> = module
            .resources
            .iter()
            .filter(|r| r.payload == Type::ByteBuffer)
            .collect();
        if definitions.len() != 1
            || definitions[0].name != BYTE_BUFFER_RESOURCE_NAME
            || definitions[0].teardown.is_some()
        {
            return Err(denied(span));
        }
    }
    Ok(c.function)
}
fn entry_graph(module: &TypedModule, id: ClosureId, left: &mut usize, span: Span) -> R<()> {
    spend(left, module.closures.len().saturating_add(1))?;
    let c = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| denied(span))?;
    if c.id != id || !c.lending_captures.is_empty() {
        return Err(denied(span));
    }
    let entry = c.function;
    let mut queue = vec![entry];
    let mut seen = BTreeSet::new();
    while let Some(fid) = queue.pop() {
        spend(left, seen.len().saturating_add(1))?;
        if !seen.insert(fid) {
            continue;
        }
        let f = module
            .functions
            .get(fid.0 as usize)
            .ok_or_else(|| denied(span))?;
        spend(
            left,
            f.values
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(module.resources.len())
                .saturating_add(1),
        )?;
        if f.id != fid
            || f.call_kind != CallKind::Direct
            || f.foreign.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || f.result_mode != PassingMode::Infer
            || if fid == entry {
                f.environment != Some(id)
            } else {
                f.environment.is_some()
            }
            || module.resources.iter().any(|r| r.teardown == Some(fid))
        {
            return Err(denied(f.span));
        }
        shape(&f.return_type, true, left, f.span)?;
        for ty in f.values.iter().chain(f.locals.iter().map(|l| &l.ty)) {
            shape(ty, true, left, f.span)?;
        }
        spend(left, f.blocks.len())?;
        for block in &f.blocks {
            spend(left, block.instructions.len())?;
            for i in &block.instructions {
                match &i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        spend(left, arguments.len().saturating_add(1))?;
                        queue.push(*function);
                    }
                    InstructionKind::ReadCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::DescribeCapture { .. }
                    | InstructionKind::BindPackageCapture { .. }
                    | InstructionKind::EndPackageCapture { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::DescribePackageField { .. }
                    | InstructionKind::ByteBufferLen { .. }
                    | InstructionKind::ByteBufferCapacity { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Constant(_)
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. } => {}
                    _ => return Err(denied(i.span)),
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn check(
    module: &TypedModule,
    transfers: &BTreeSet<ClosureId>,
    entries: &BTreeSet<ClosureId>,
    left: &mut usize,
) -> R<()> {
    if transfers.is_empty() && entries.is_empty() {
        return Ok(());
    }
    spend(left, transfers.len().saturating_add(entries.len()))?;
    for id in transfers {
        environment(module, *id, left, Span::default())?;
    }
    for id in entries {
        entry_graph(module, *id, left, Span::default())?;
    }
    Ok(())
}

/// Definition-time obligations are checked even without an instantiation. This
/// only recognizes actual static provider declarations; it cannot certify F.
pub(crate) fn definition(
    f: &CoreFunction,
    context: &helpers::Context,
    state: &mut closures::State,
) -> R<()> {
    if !state.threaded_providers {
        return Ok(());
    }
    let tables = state.helper_tables.clone();
    let Some(table) = tables.get(&context.module) else {
        return Ok(());
    };
    enum Node<'a> {
        Block(&'a CoreBlock),
        Expr(&'a CoreExpr),
    }
    state.snapshot(1, f.span)?;
    let mut pending = vec![Node::Block(&f.body)];
    while let Some(node) = pending.pop() {
        state.snapshot(1, f.span)?;
        match node {
            Node::Block(block) => {
                state.snapshot(block.statements.len().saturating_mul(4), block.span)?;
                for statement in &block.statements {
                    use CoreStatementKind as S;
                    match &statement.kind {
                        S::Binding { initializer, .. } | S::Return(initializer) => {
                            if let Some(e) = initializer {
                                pending.push(Node::Expr(e));
                            }
                        }
                        S::TupleBinding { initializer, .. } | S::Expression(initializer) => {
                            pending.push(Node::Expr(initializer))
                        }
                        S::Assign { target, value } => {
                            pending.push(Node::Expr(target));
                            pending.push(Node::Expr(value));
                        }
                        S::If {
                            condition,
                            then_body,
                            else_body,
                        } => {
                            pending.push(Node::Expr(condition));
                            pending.push(Node::Block(then_body));
                            if let Some(b) = else_body {
                                pending.push(Node::Block(b));
                            }
                        }
                        S::IfLet {
                            value,
                            then_body,
                            else_body,
                            ..
                        } => {
                            pending.push(Node::Expr(value));
                            pending.push(Node::Block(then_body));
                            if let Some(b) = else_body {
                                pending.push(Node::Block(b));
                            }
                        }
                        S::While { condition, body } => {
                            pending.push(Node::Expr(condition));
                            pending.push(Node::Block(body));
                        }
                        S::WhileLet { value, body, .. } => {
                            pending.push(Node::Expr(value));
                            pending.push(Node::Block(body));
                        }
                        S::ForRange {
                            start, end, body, ..
                        } => {
                            pending.push(Node::Expr(start));
                            pending.push(Node::Expr(end));
                            pending.push(Node::Block(body));
                        }
                        S::Match { value, arms } => {
                            state.snapshot(arms.len(), statement.span)?;
                            pending.push(Node::Expr(value));
                            for arm in arms {
                                pending.push(Node::Block(&arm.body));
                            }
                        }
                        S::Block(b) | S::Unsafe(b) | S::Defer(b) => pending.push(Node::Block(b)),
                        S::Break | S::Continue | S::Unsupported(_) => {}
                    }
                }
            }
            Node::Expr(e) => {
                use CoreExprKind as E;
                match &e.kind {
                    E::Call { callee, arguments } => {
                        state.snapshot(arguments.len().saturating_add(2), e.span)?;
                        if matches!(&callee.kind, E::Name(n) if n == "c_register_async")
                            && let Some(provider) = arguments.first()
                        {
                            let text =
                                match &provider.kind {
                                    E::Name(n) => n.len(),
                                    E::Member { member, object, .. } => member
                                        .len()
                                        .saturating_add(if let E::Name(n) = &object.kind {
                                            n.len()
                                        } else {
                                            0
                                        }),
                                    _ => 0,
                                };
                            state.snapshot(
                                table
                                    .len()
                                    .saturating_add(tables.len())
                                    .saturating_add(context.imports.len())
                                    .saturating_add(text)
                                    .saturating_add(1),
                                provider.span,
                            )?;
                            let signature = match &provider.kind {
                                E::Name(n) => table.get(n),
                                E::Member { object, member, .. } => {
                                    if let E::Name(alias) = &object.kind {
                                        context
                                            .imports
                                            .get(alias)
                                            .and_then(|m| tables.get(m))
                                            .and_then(|t| t.get(member))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };
                            if signature.is_some_and(|s| {
                                s.scoped_callback == Some(ScopedCallbackRole::ThreadAsyncRegister)
                            }) && !matches!(f.callable_parameters.as_slice(), [b] if b.closed_environment && b.transfer && b.thread_callable && b.kind == CallableKind::FnMut)
                            {
                                return Err(error(
                                    "E-REGISTRATION-THREAD-BOUND",
                                    e.span,
                                    "threaded registration definition requires explicit FnMut + ClosedEnvironment + Transfer + ThreadCallable",
                                ));
                            }
                        }
                        pending.push(Node::Expr(callee));
                        for a in arguments {
                            pending.push(Node::Expr(a));
                        }
                    }
                    E::CallDynSlot {
                        callee, arguments, ..
                    } => {
                        state.snapshot(arguments.len().saturating_add(1), e.span)?;
                        pending.push(Node::Expr(callee));
                        for a in arguments {
                            pending.push(Node::Expr(a));
                        }
                    }
                    E::Unary { operand, .. }
                    | E::NumericConvert { value: operand, .. }
                    | E::IntegerConvert { value: operand, .. }
                    | E::Ascribed { value: operand, .. }
                    | E::CompileTime(operand)
                    | E::Try(operand)
                    | E::Member {
                        object: operand, ..
                    }
                    | E::TypeApply {
                        callee: operand, ..
                    }
                    | E::MakeDynBorrow {
                        source: operand, ..
                    }
                    | E::MakeDynOwned {
                        source: operand, ..
                    } => {
                        state.snapshot(1, e.span)?;
                        pending.push(Node::Expr(operand));
                    }
                    E::Binary { left, right, .. }
                    | E::Index {
                        object: left,
                        index: right,
                    } => {
                        state.snapshot(2, e.span)?;
                        pending.push(Node::Expr(left));
                        pending.push(Node::Expr(right));
                    }
                    E::Tuple(es) | E::Array(es) => {
                        state.snapshot(es.len(), e.span)?;
                        for e in es {
                            pending.push(Node::Expr(e));
                        }
                    }
                    E::Record { fields, .. } => {
                        state.snapshot(fields.len(), e.span)?;
                        for (_, e) in fields {
                            pending.push(Node::Expr(e));
                        }
                    }
                    E::RecordUpdate { base, fields, .. } => {
                        state.snapshot(fields.len().saturating_add(1), e.span)?;
                        pending.push(Node::Expr(base));
                        for (_, e) in fields {
                            pending.push(Node::Expr(e));
                        }
                    }
                    E::Sequence { bindings, value } => {
                        state.snapshot(bindings.len().saturating_add(1), e.span)?;
                        pending.push(Node::Expr(value));
                        for (_, e) in bindings {
                            pending.push(Node::Expr(e));
                        }
                    }
                    E::Slice { object, start, end } => {
                        state.snapshot(3, e.span)?;
                        pending.push(Node::Expr(object));
                        for e in start.iter().chain(end) {
                            pending.push(Node::Expr(e));
                        }
                    }
                    E::Closure { body, .. } => {
                        state.snapshot(1, e.span)?;
                        pending.push(Node::Block(body));
                    }
                    E::If {
                        condition,
                        then_value,
                        else_value,
                    } => {
                        state.snapshot(3, e.span)?;
                        pending.push(Node::Expr(condition));
                        pending.push(Node::Expr(then_value));
                        pending.push(Node::Expr(else_value));
                    }
                    E::Literal(_)
                    | E::Integer(_)
                    | E::Float(_)
                    | E::Bool(_)
                    | E::String(_)
                    | E::Unit
                    | E::Name(_)
                    | E::ReceiverMember(_)
                    | E::Unsupported(_) => {}
                }
            }
        }
    }
    Ok(())
}
