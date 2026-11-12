//! Check the bound's source bridge against the actual Core owner's separately
//! retained original Definition scope, then against its full concrete header.
//! Identity and source ordinals never create targets or lifetime equalities.
use super::*;

pub(super) fn check(
    scope: &CoreFunctionSourceScope,
    context: &CoreCallableReturnLifeContext,
    f: &TypedFunction,
    left: &mut usize,
) -> R<()> {
    let invalid = |text: &str| error("E-CALLABLE-RETURN-SCOPE", f.span, text);
    spend(
        left,
        scope
            .parameters
            .len()
            .saturating_add(context.parameters.len())
            .saturating_add(scope.identity.package.len())
            .saturating_add(context.identity.package.len()),
        f.span,
    )?;
    for part in scope
        .identity
        .module_path
        .iter()
        .chain(&scope.identity.declaration_path)
        .chain(&context.identity.module_path)
        .chain(&context.identity.declaration_path)
    {
        spend(left, part.len().saturating_add(1), f.span)?;
    }
    if scope.declaration != context.declaration
        || scope.identity != context.identity
        || scope.parameters != context.parameters
    {
        return Err(invalid(
            "returned callable bridge differs from its actual original Definition scope",
        ));
    }
    if scope.declaration.compiler || !scope.identity.generation_path.is_empty() {
        return Err(unsupported(
            f.span,
            "returned callable source scope must retain its actual original Definition",
        ));
    }
    let header = f
        .storage_life
        .as_ref()
        .ok_or_else(|| invalid("actual source scope has no completed concrete input header"))?;
    let count = header
        .bindings
        .iter()
        .filter(|row| matches!(row.position.base, StorageBase::Parameter(_)))
        .count()
        .checked_add(
            header
                .classes
                .iter()
                .filter(|row| matches!(row.position.base, StorageBase::Parameter(_)))
                .count(),
        )
        .ok_or_else(|| invalid("actual source scope input directory overflows"))?;
    spend(
        left,
        header
            .bindings
            .len()
            .saturating_add(header.classes.len())
            .saturating_add(scope.inputs.len()),
        f.span,
    )?;
    if count != scope.inputs.len() {
        return Err(invalid(
            "original source scope omits or adds a concrete input occurrence",
        ));
    }
    for (index, input) in scope.inputs.iter().enumerate() {
        if input.parameter as usize >= f.parameters.len()
            || input.source_parameter.is_some_and(|source| {
                scope.parameters.get(source as usize) != Some(&LifeParameterKind::Lifetime)
            })
        {
            return Err(invalid(
                "original source input does not identify its real full formal ordinal",
            ));
        }
        spend(left, index.saturating_add(1), f.span)?;
        for previous in &scope.inputs[..index] {
            let duplicate = match (&previous.position, &input.position) {
                (CoreClosureHeaderPosition::Borrow(a), CoreClosureHeaderPosition::Borrow(b)) => {
                    spend(left, a.path.len().saturating_add(b.path.len()), f.span)?;
                    a == b
                }
                (CoreClosureHeaderPosition::Class(a), CoreClosureHeaderPosition::Class(b)) => {
                    spend(left, a.path.len().saturating_add(b.path.len()), f.span)?;
                    a == b
                }
                _ => false,
            };
            if duplicate {
                return Err(invalid(
                    "original source scope repeats an input header occurrence",
                ));
            }
        }
        let binder = match &input.position {
            CoreClosureHeaderPosition::Borrow(position) => {
                if position.base != StorageBase::Parameter(input.parameter) {
                    return Err(invalid(
                        "source input borrow belongs to a different parameter",
                    ));
                }
                spend(
                    left,
                    header
                        .bindings
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                    f.span,
                )?;
                header
                    .bindings
                    .iter()
                    .find(|row| row.position == *position)
                    .map(|row| row.binder)
            }
            CoreClosureHeaderPosition::Class(position) => {
                if position.base != StorageBase::Parameter(input.parameter) {
                    return Err(invalid(
                        "source input class belongs to a different parameter",
                    ));
                }
                spend(
                    left,
                    header
                        .classes
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                    f.span,
                )?;
                header
                    .classes
                    .iter()
                    .find(|row| row.position == *position)
                    .map(|row| row.binder)
            }
        };
        if binder != Some(input.header_binder) {
            return Err(invalid(
                "original source input differs from the completed actual header",
            ));
        }
    }
    for row in &context.fixed {
        spend(left, scope.inputs.len(), f.span)?;
        let depth = match &row.input.position {
            CoreClosureHeaderPosition::Borrow(p) => p.path.len(),
            CoreClosureHeaderPosition::Class(p) => p.path.len(),
        };
        spend(left, scope.inputs.len().saturating_mul(depth), f.span)?;
        if !scope.inputs.contains(&row.input) {
            return Err(invalid(
                "return bound changes the original formal-to-input occurrence mapping",
            ));
        }
    }
    Ok(())
}
