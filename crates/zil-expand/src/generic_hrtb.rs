//! Lexically bound callback invocation relations. No concrete environment
//! identity or capability is inferred here.
use super::*;

pub(crate) fn inner(mut bound: &TypeRef) -> &TypeRef {
    loop {
        bound = grouped(bound);
        match &bound.kind {
            TypeKind::ForLifetime { inner, .. } => bound = inner,
            _ => return bound,
        }
    }
}
pub(crate) fn inner_mut(bound: &mut TypeRef) -> &mut TypeRef {
    if matches!(bound.kind, TypeKind::ForLifetime { .. }) {
        let TypeKind::ForLifetime { inner, .. } = &mut bound.kind else { unreachable!() };
        inner_mut(inner)
    } else { bound }
}
pub(super) fn pay(remaining: &mut usize, n: usize, span: Span) -> R<()> {
    *remaining = remaining.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "higher-rank callable relation budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}
pub(super) fn relation(
    bound: &TypeRef,
    remaining: &mut usize,
) -> R<Option<zil_ir::FunctionLifeContract>> {
    let mut nested = grouped(bound);
    if !matches!(nested.kind, TypeKind::ForLifetime { .. }) {
        return Ok(None);
    }
    let mut names = Vec::new();
    while let TypeKind::ForLifetime { lifetime, inner } = &nested.kind {
        pay(remaining, lifetime.len().saturating_add(2), nested.span)?;
        names.push(lifetime.as_str());
        nested = grouped(inner);
    }
    let TypeKind::Named { path, args } = &nested.kind else {
        return Err(error(
            bound.span,
            "higher-rank callback bound requires Fn, FnMut or FnOnce",
        ));
    };
    if !matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn"|"FnMut"|"FnOnce")) {
        return Err(error(
            bound.span,
            "higher-rank callback bound requires a callable protocol",
        ));
    }
    let [GenericArg::Type(parameters), GenericArg::Type(result)] = args.as_slice() else {
        return Err(error(
            bound.span,
            "higher-rank callable signature is incomplete",
        ));
    };
    let TypeKind::Tuple(parameters) = &grouped(parameters).kind else {
        return Err(error(
            bound.span,
            "higher-rank callable parameters require a tuple",
        ));
    };
    pay(
        remaining,
        parameters
            .len()
            .saturating_mul(6)
            .saturating_add(names.len())
            .saturating_add(2),
        bound.span,
    )?;
    let mut remap = vec![None; names.len()];
    let mut next = 0u32;
    let mut tag = |ty: &TypeRef, output: bool| -> R<Option<u32>> {
        let TypeKind::Reference { lifetime, .. } = &grouped(ty).kind else {
            return Ok(None);
        };
        let name = lifetime.as_ref().ok_or_else(|| {
            error(
                ty.span,
                "a higher-rank address needs its declared life label",
            )
        })?;
        pay(
            remaining,
            names.len().saturating_mul(name.len().saturating_add(1)),
            ty.span,
        )?;
        let old = names
            .iter()
            .rposition(|n| *n == name)
            .ok_or_else(|| error(ty.span, "undeclared higher-rank life name"))?;
        if output && remap[old].is_none() {
            return Err(error(
                ty.span,
                "higher-rank result life has no actual input",
            ));
        }
        Ok(Some(*remap[old].get_or_insert_with(|| {
            let n = next;
            next += 1;
            n
        })))
    };
    let parameter_lives = parameters
        .iter()
        .map(|p| tag(p, false))
        .collect::<R<Vec<_>>>()?;
    let result_life = tag(result, true)?;
    if matches!(
        grouped(result).kind,
        TypeKind::Reference { mutable: true, .. }
    ) && !parameters.iter().zip(&parameter_lives).any(|(p, life)| {
        *life == result_life && matches!(grouped(p).kind, TypeKind::Reference { mutable: true, .. })
    }) {
        return Err(error(
            result.span,
            "higher-rank mutable result needs an exclusive actual input",
        ));
    }
    Ok((next != 0).then_some(zil_ir::FunctionLifeContract {
        binder_count: next,
        parameter_lives,
        result_life,
    }))
}
