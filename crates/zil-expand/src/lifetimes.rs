//! Explicit function life declarations survive address layout erasure.
use super::*;

pub(super) fn only_life(function: &s::Function) -> bool {
    !function.generics.is_empty()
        && function
            .generics
            .iter()
            .all(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
}

fn failure(span: Span, text: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-LIFE-DECLARATION", Some(span), text)
}

fn spend(remaining: &mut usize, work: usize, span: Span) -> Result<(), Diagnostic> {
    *remaining = remaining.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Expand,
            "life declaration budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

/// Called on the actual source header, after receiver selection but before any
/// Reference is lowered. None is the unchanged body-inferred legacy lane.
pub(super) fn contract(
    function: &s::Function,
    implicit_receiver: Option<&s::Param>,
    remaining: &mut usize,
) -> Result<Option<CoreLifeContract>, Diagnostic> {
    fn outer(ty: &s::TypeRef) -> Option<&str> {
        match &ty.kind {
            s::TypeKind::Group(inner) => outer(inner),
            s::TypeKind::Reference { lifetime, .. } => lifetime.as_deref(),
            _ => None,
        }
    }
    let parameters = implicit_receiver.into_iter().chain(function.params.iter());
    let present = function
        .generics
        .iter()
        .any(|p| matches!(p.kind, s::GenericParamKind::Lifetime))
        || parameters.clone().any(|p| outer(&p.ty).is_some())
        || function
            .return_type
            .as_ref()
            .is_some_and(|t| outer(t).is_some());
    if !present {
        return Ok(None);
    }
    if function
        .generics
        .iter()
        .any(|p| !matches!(p.kind, s::GenericParamKind::Lifetime))
        || !function.where_clause.is_empty()
    {
        return Err(Diagnostic::unsupported(
            Phase::Expand,
            "life with type/callable/const parameters or where clauses is not implemented",
            Some(function.span),
        ));
    }
    spend(
        remaining,
        function
            .generics
            .len()
            .saturating_add(function.params.len())
            .saturating_add(2),
        function.span,
    )?;
    let mut binders = Vec::new();
    for p in &function.generics {
        spend(
            remaining,
            p.name.len().saturating_mul(binders.len().saturating_add(2)),
            p.span,
        )?;
        if binders.contains(&p.name) {
            return Err(failure(p.span, "duplicate life binder"));
        }
        binders.push(p.name.clone());
    }
    fn tag(
        ty: &s::TypeRef,
        binders: &[String],
        remaining: &mut usize,
    ) -> Result<Option<u32>, Diagnostic> {
        match &ty.kind {
            s::TypeKind::Group(inner) => tag(inner, binders, remaining),
            s::TypeKind::Reference {
                lifetime: Some(name),
                mutable,
                ..
            } => {
                spend(
                    remaining,
                    name.len().saturating_mul(binders.len().saturating_add(1)),
                    ty.span,
                )?;
                let index = binders
                    .iter()
                    .position(|b| b == name)
                    .ok_or_else(|| failure(ty.span, "undeclared life name"))?;
                if *mutable {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "explicit life on mutable addresses is not implemented",
                        Some(ty.span),
                    ));
                }
                Ok(Some(index as u32))
            }
            _ => Ok(None),
        }
    }
    let parameter_lives = parameters
        .map(|p| tag(&p.ty, &binders, remaining))
        .collect::<Result<Vec<_>, _>>()?;
    let result_life = function
        .return_type
        .as_ref()
        .map(|t| tag(t, &binders, remaining))
        .transpose()?
        .flatten();
    if result_life.is_some_and(|result| !parameter_lives.contains(&Some(result))) {
        return Err(failure(
            function.span,
            "returned life must name a nonempty shared input parameter set",
        ));
    }
    Ok(Some(CoreLifeContract {
        binders,
        parameter_lives,
        result_life,
    }))
}

/// Explicit labels may be erased only in the enclosing checked signature.
/// Other type positions retain an Unsupported marker instead of losing life.
pub(super) fn signature_type(expander: &Expander<'_>, ty: &s::TypeRef) -> Type {
    match &ty.kind {
        s::TypeKind::Group(inner) => signature_type(expander, inner),
        s::TypeKind::Reference {
            lifetime: Some(_),
            mutable,
            inner,
        } => Type::Address {
            mutable: *mutable,
            inner: Box::new(expander.type_ref(inner)),
        },
        _ => expander.type_ref(ty),
    }
}
