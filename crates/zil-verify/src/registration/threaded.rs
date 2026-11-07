//! Verify reconstructs thread entry authority from the actual concrete graph.
//! No Analyze result, declared Transfer bit or provider role certifies that graph.
use super::*;
fn bad(span: Option<Span>) -> Diagnostic {
    error(
        span,
        "thread callback has nontransferable data, unknown cleanup or an unproved thread entry dependency",
    )
}
fn ty(t: &Type, internal: bool, budget: &mut Budget, span: Option<Span>) -> Check<bool> {
    budget.spend(1)?;
    let mut work = vec![(t, true)];
    let mut owns_buffer = false;
    while let Some((t, owning)) = work.pop() {
        budget.spend(1)?;
        match t {
            Type::Managed(p) if owning && p.as_ref() == &Type::ByteBuffer => owns_buffer = true,
            Type::Desc(p) if internal && p.as_ref() == &Type::ByteBuffer => owns_buffer = true,
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
            Type::Tuple(ts) => {
                budget.spend(ts.len())?;
                work.extend(ts.iter().map(|t| (t, owning)));
            }
            Type::Array { element, .. } => {
                budget.spend(1)?;
                work.push((element.as_ref(), false));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                if !owning && matches!(t, Type::Package { .. }) {
                    return Err(bad(span));
                }
                let child_owning = owning && matches!(t, Type::Package { .. });
                budget.spend(fields.len())?;
                for (_, t) in fields {
                    work.push((t, child_owning));
                }
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    work.extend(fields.iter().map(|t| (t, owning)));
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                work.push((result.as_ref(), owning));
                work.push((error.as_ref(), owning));
            }
            _ => return Err(bad(span)),
        }
    }
    Ok(owns_buffer)
}
pub(super) fn actual(
    module: &TypedModule,
    root: ClosureId,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(module.closures.len().saturating_add(1))?;
    let closure = closures::definition(module, root, span)?;
    if closure.id != root
        || !closure.lending_captures.is_empty()
        || !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
    {
        return Err(bad(span));
    }
    budget.spend(closure.captures.len())?;
    let mut buffer = false;
    for capture in &closure.captures {
        buffer |= ty(capture, false, budget, span)?;
    }
    if buffer {
        budget.spend(module.resources.len())?;
        let mut definitions = module
            .resources
            .iter()
            .filter(|r| r.payload == Type::ByteBuffer);
        let r = definitions.next().ok_or_else(|| bad(span))?;
        if r.name != zil_ir::BYTE_BUFFER_RESOURCE_NAME
            || r.teardown.is_some()
            || definitions.next().is_some()
        {
            return Err(bad(span));
        }
    }
    let mut required = vec![closure.function];
    let mut verified = BTreeSet::new();
    while let Some(next) = required.pop() {
        budget.spend(
            verified
                .len()
                .saturating_add(module.functions.len())
                .saturating_add(1),
        )?;
        if !verified.insert(next) {
            continue;
        }
        let f = module
            .functions
            .get(next.0 as usize)
            .ok_or_else(|| bad(span))?;
        let site = Some(f.span);
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(module.resources.len())
                .saturating_add(1),
        )?;
        if f.id != next
            || f.foreign.is_some()
            || f.call_kind != CallKind::Direct
            || f.result_mode != PassingMode::Infer
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || if next == closure.function {
                f.environment != Some(root)
            } else {
                f.environment.is_some()
            }
            || module.resources.iter().any(|r| r.teardown == Some(next))
        {
            return Err(bad(site));
        }
        ty(&f.return_type, true, budget, site)?;
        for local in &f.locals {
            ty(&local.ty, true, budget, site)?;
        }
        for value in &f.values {
            ty(value, true, budget, site)?;
        }
        budget.spend(f.blocks.len())?;
        for block in &f.blocks {
            budget.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                match &instruction.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        budget.spend(arguments.len().saturating_add(1))?;
                        required.push(*function);
                    }
                    InstructionKind::ReadCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::DescribeCapture { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::DescribePackageField { .. }
                    | InstructionKind::BindPackageCapture { .. }
                    | InstructionKind::EndPackageCapture { .. }
                    | InstructionKind::ByteBufferLen { .. }
                    | InstructionKind::ByteBufferCapacity { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::Constant(_)
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::TakeLocal { .. } => {}
                    _ => return Err(bad(Some(instruction.span))),
                }
            }
        }
    }
    Ok(())
}
