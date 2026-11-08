//! Complete private fees for shared-result context admission and selection.
//! Ordinary payload walkers intentionally do not traverse opaque Task ports.
use super::*;

pub(crate) fn context(ty: &Type, b: &mut Budget) -> CompileResult<()> {
    let Type::Task {
        result,
        retained_life,
    } = ty
    else {
        return Ok(());
    };
    if !matches!(result.as_ref(), Type::Address { mutable: false, .. }) {
        return Ok(());
    }
    b.spend(
        retained_life
            .ports
            .len()
            .saturating_add(retained_life.retained.len())
            .saturating_add(retained_life.nominal_arguments.len())
            .saturating_add(1)
            .saturating_mul(4),
        "Task result context comparison",
    )?;
    for row in &retained_life.retained {
        b.spend(
            row.path.len().saturating_mul(4),
            "Task result context comparison",
        )?;
    }
    for row in &retained_life.nominal_arguments {
        b.spend(
            row.path.len().saturating_mul(4),
            "Task result context comparison",
        )?;
    }
    // Reserve container growth before building the worklist. Every supported
    // leaf node/name pays its walk, admission and complete equality visits.
    let mut pending = Vec::new();
    pending.push((result.as_ref(), 0usize));
    pending.extend(retained_life.ports.iter().map(|p| (&p.leaf, 0usize)));
    while let Some((leaf, depth)) = pending.pop() {
        b.spend(4, "Task result context comparison")?;
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "Task result context nesting exceeds analysis limit",
                None,
            )]);
        }
        match leaf {
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => {
                pending.push((inner.as_ref(), depth + 1));
            }
            Type::Record { name, fields } => {
                b.spend(
                    name.len().saturating_add(fields.len()).saturating_mul(4),
                    "Task result context comparison",
                )?;
                for (name, child) in fields {
                    b.spend(
                        name.len().saturating_mul(4),
                        "Task result context comparison",
                    )?;
                    pending.push((child, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                b.spend(
                    fields.len().saturating_mul(4),
                    "Task result context comparison",
                )?;
                pending.extend(fields.iter().map(|child| (child, depth + 1)));
            }
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
            // These cannot be a whole retained input/result leaf. Reject before
            // an unmetered structural classifier or equality can inspect them.
            _ => {
                return Err(fail(
                    zil_base::Span::default(),
                    "shared Task result requires its complete actual shared input context",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn selection(mut ty: &Type, b: &mut Budget) -> CompileResult<()> {
    loop {
        match ty {
            Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
                b.spend(1, "Task result context comparison")?;
                ty = inner;
            }
            Type::ErrorUnion { result, .. } => {
                b.spend(1, "Task result context comparison")?;
                ty = result;
            }
            _ => return context(ty, b),
        }
    }
}
