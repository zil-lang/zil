//! Analyze reconstructs the actual thread callback closure and its reachable
//! code. The external role and Core Transfer requirement are not evidence.
use super::*;
fn bad(span: Span) -> Vec<Diagnostic> {
    fail(
        span,
        "threaded F lacks actual closed Transfer data, synchronous sealed cleanup or a thread-safe reachable entry",
    )
}
fn fields(ty: &Type, access: bool, budget: &mut Budget, span: Span) -> CompileResult<bool> {
    budget.spend(1, "thread F shape queue")?;
    let mut pending = vec![(ty, true)];
    let mut buffer = false;
    while let Some((t, owning)) = pending.pop() {
        budget.spend(1, "thread F actual type")?;
        match t {
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
            Type::Desc(p) if access && p.as_ref() == &Type::ByteBuffer => buffer = true,
            Type::Tuple(ts) => {
                budget.spend(ts.len(), "thread F tuple")?;
                pending.extend(ts.iter().map(|t| (t, owning)));
            }
            Type::Array { element, .. } => {
                budget.spend(1, "thread F array")?;
                pending.push((element.as_ref(), false));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                if !owning && matches!(t, Type::Package { .. }) {
                    return Err(bad(span));
                }
                let child_owning = owning && matches!(t, Type::Package { .. });
                budget.spend(fields.len(), "thread F product")?;
                pending.extend(fields.iter().map(|(_, t)| (t, child_owning)));
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "thread F tags")?;
                for (_, ts) in variants {
                    budget.spend(ts.len(), "thread F variant")?;
                    pending.extend(ts.iter().map(|t| (t, owning)));
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2, "thread F arms")?;
                pending.extend([(error.as_ref(), owning), (result.as_ref(), owning)]);
            }
            _ => return Err(bad(span)),
        }
    }
    Ok(buffer)
}
pub(super) fn actual(
    module: &TypedModule,
    root: ClosureId,
    budget: &mut Budget,
    span: Span,
) -> CompileResult<()> {
    budget.spend(
        module.closures.len().saturating_add(1),
        "thread F closure lookup",
    )?;
    let c = module
        .closures
        .get(root.0 as usize)
        .ok_or_else(|| bad(span))?;
    if c.id != root
        || !c.lending_captures.is_empty()
        || !matches!(c.kind, CallableKind::Fn | CallableKind::FnMut)
    {
        return Err(bad(span));
    }
    budget.spend(c.captures.len(), "thread F complete captures")?;
    let mut buffer = false;
    for t in &c.captures {
        buffer |= fields(t, false, budget, span)?;
    }
    if buffer {
        budget.spend(module.resources.len(), "thread F sealed buffer cleanup")?;
        let mut n = 0;
        for r in &module.resources {
            if r.payload == Type::ByteBuffer {
                n += 1;
                if r.name != zil_ir::BYTE_BUFFER_RESOURCE_NAME || r.teardown.is_some() {
                    return Err(bad(span));
                }
            }
        }
        if n != 1 {
            return Err(bad(span));
        }
    }
    let mut pending = vec![c.function];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.lookup(seen.len(), "thread F reachable visit")?;
        if !seen.insert(id) {
            continue;
        }
        budget.spend(
            module.functions.len().saturating_add(1),
            "thread F actual function lookup",
        )?;
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| bad(span))?;
        budget.spend(
            f.values
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(module.resources.len())
                .saturating_add(1),
            "thread F function inventory",
        )?;
        if f.id != id
            || f.foreign.is_some()
            || f.call_kind != CallKind::Direct
            || f.result_mode != PassingMode::Infer
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || if id == c.function {
                f.environment != Some(root)
            } else {
                f.environment.is_some()
            }
            || module.resources.iter().any(|r| r.teardown == Some(id))
        {
            return Err(bad(f.span));
        }
        fields(&f.return_type, true, budget, f.span)?;
        for t in f.values.iter().chain(f.locals.iter().map(|l| &l.ty)) {
            fields(t, true, budget, f.span)?;
        }
        budget.spend(f.blocks.len(), "thread F all CFG blocks")?;
        for b in &f.blocks {
            budget.spend(b.instructions.len(), "thread F actual instructions")?;
            for i in &b.instructions {
                match &i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        budget.spend(arguments.len().saturating_add(1), "thread F call edge")?;
                        pending.push(*function);
                    }
                    InstructionKind::Constant(_)
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::ReadCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::DescribeCapture { .. }
                    | InstructionKind::BindPackageCapture { .. }
                    | InstructionKind::EndPackageCapture { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::DescribePackageField { .. }
                    | InstructionKind::ByteBufferLen { .. }
                    | InstructionKind::ByteBufferCapacity { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. } => {}
                    _ => return Err(bad(i.span)),
                }
            }
        }
    }
    Ok(())
}
