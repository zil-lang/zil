//! The original helper bound is a raw proposal, not a class-equality proof.
use super::*;

fn bad(span: Span, message: &str) -> Diagnostic {
    error("E-CALLABLE-CONTEXT", span, message)
}
fn pay(left: &mut usize, count: usize, span: Span) -> R<()> {
    *left = left.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "source helper context work exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

/// Flat context preflight before a role/signature/Core snapshot. None performs
/// no work. Const kinds are outside this profile and rejected before cloning.
pub(crate) fn reserve(
    bound: &CoreCallableParameter,
    limits: &Limits,
    left: &mut usize,
    copies: usize,
    span: Span,
) -> R<()> {
    let Some(context) = &bound.source_life else {
        return Ok(());
    };
    let factor = copies.saturating_add(1);
    for count in [
        context.parameters.len(),
        context.named_fixed.len(),
        context.inputs.len(),
        context.identity.module_path.len(),
        context.identity.declaration_path.len(),
        context.identity.generation_path.len(),
    ] {
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper context vector exceeded",
                Some(span),
            ));
        }
        pay(left, count.saturating_mul(factor), span)?;
    }
    if context.declaration.compiler
        || !context.identity.generation_path.is_empty()
        || bound.kind != CallableKind::Fn
        || bound.closed_environment
        || bound.generic_index != 0
        || context.bound_ordinal != 0
        || context.parameters.iter().enumerate().any(|(index, kind)| {
            if index == context.generic_parameter as usize {
                *kind != LifeParameterKind::Type
            } else {
                *kind != LifeParameterKind::Lifetime
            }
        })
        || context.parameters.get(context.generic_parameter as usize)
            != Some(&LifeParameterKind::Type)
    {
        return Err(unsupported(
            span,
            "source helper context requires one original shared Fn bound and life declarations",
        ));
    }
    for text in std::iter::once(&context.identity.package)
        .chain(&context.identity.module_path)
        .chain(&context.identity.declaration_path)
    {
        pay(
            left,
            text.len().saturating_add(1).saturating_mul(factor),
            span,
        )?;
    }
    let life = bound
        .life
        .as_ref()
        .ok_or_else(|| bad(span, "source helper context omits its bound P/R proposal"))?;
    for count in [life.bindings.len(), life.classes.len()] {
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper bound vector exceeded",
                Some(span),
            ));
        }
    }
    let occurrences = life.bindings.len().saturating_add(life.classes.len());
    if life.fixed_binder_count as usize > occurrences
        || life.invocation_binder_count as usize > occurrences
    {
        return Err(bad(
            span,
            "source helper bound domain exceeds actual P/R occurrences",
        ));
    }
    pay(
        left,
        borrowed_fp::metadata_work(life).saturating_mul(factor),
        span,
    )?;
    for path in life
        .bindings
        .iter()
        .map(|r| r.position.path.len())
        .chain(life.classes.iter().map(|r| r.position.path.len()))
        .chain(context.inputs.iter().map(|r| match &r.position {
            CoreClosureHeaderPosition::Borrow(p) => p.path.len(),
            CoreClosureHeaderPosition::Class(p) => p.path.len(),
        }))
    {
        if path > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "source helper context path depth exceeded",
                Some(span),
            ));
        }
        pay(left, path.saturating_add(1).saturating_mul(factor), span)?;
    }
    if life
        .bindings
        .iter()
        .any(|r| matches!(r.position.base, StorageBase::Capture(_)))
        || life
            .classes
            .iter()
            .any(|r| matches!(r.position.base, StorageBase::Capture(_)))
    {
        return Err(bad(
            span,
            "source helper bound is a P/R proposal, not a capture contract",
        ));
    }
    Ok(())
}

pub(super) fn pending_consumer(bound: &CoreCallableParameter, span: Span) -> R<()> {
    if bound.source_life.is_some() {
        return Err(unsupported(
            span,
            "source helper context actual bound/header formation is pending",
        ));
    }
    Ok(())
}
