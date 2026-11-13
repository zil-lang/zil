//! Instantiate an original nominal field recipe at a real use. The replacement
//! expressions stay in the actual use's world; no ordinal is reinterpreted as
//! a same-numbered field of another definition.
use crate::*;
fn fail() -> Diagnostic {
    Diagnostic::unsupported(
        Phase::Types,
        "nominal field recipe requires its complete lexical instantiation",
        None,
    )
}
fn life(
    value: &mut SemanticLifeRecipe,
    arguments: &[SemanticActualRecipe],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if let SemanticLifeRecipe::Formal(SemanticFormalRef {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    }) = value
    {
        let Some(SemanticActualRecipe::Life(actual)) = arguments.get(*ordinal as usize) else {
            return Err(fail());
        };
        crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        *value = actual.clone();
    } else if matches!(value, SemanticLifeRecipe::Formal(_)) {
        return Err(fail());
    }
    Ok(())
}
fn constant(
    value: &mut SemanticConstRecipe,
    arguments: &[SemanticActualRecipe],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if let SemanticConstRecipe::Formal(SemanticFormalRef {
        scope: SemanticScopeRef::Ancestor(0),
        ordinal,
    }) = value
    {
        let Some(SemanticActualRecipe::Const { value: actual, .. }) =
            arguments.get(*ordinal as usize)
        else {
            return Err(fail());
        };
        crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        *value = actual.clone();
    } else if !matches!(
        value,
        SemanticConstRecipe::Value(_)
            | SemanticConstRecipe::IntegerLiteral(_)
            | SemanticConstRecipe::FloatLiteral(_)
    ) {
        return Err(fail());
    }
    Ok(())
}
fn actual(
    value: &mut SemanticActualRecipe,
    arguments: &[SemanticActualRecipe],
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<()> {
    match value {
        SemanticActualRecipe::Life(value) => life(value, arguments, limits, left),
        SemanticActualRecipe::Type(value) => rewrite(value, arguments, limits, left, depth + 1),
        SemanticActualRecipe::Const { carrier, value } => {
            rewrite(carrier, arguments, limits, left, depth + 1)?;
            constant(value, arguments, limits, left)
        }
    }
}
fn rewrite(
    value: &mut SemanticTypeRecipe,
    arguments: &[SemanticActualRecipe],
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<()> {
    *left = left.checked_sub(1).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "nominal original instantiation work exceeded",
            None,
        )
    })?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "nominal original instantiation depth exceeded",
            None,
        ));
    }
    match value {
        SemanticTypeRecipe::Atom(_) => {}
        SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        }) => {
            let Some(SemanticActualRecipe::Type(actual)) = arguments.get(*ordinal as usize) else {
                return Err(fail());
            };
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
            *value = actual.clone();
        }
        SemanticTypeRecipe::Nominal {
            arguments: values, ..
        } => {
            for value in values {
                actual(value, arguments, limits, left, depth + 1)?;
            }
        }
        SemanticTypeRecipe::Tuple(values) | SemanticTypeRecipe::AnonymousSum(values) => {
            for value in values {
                rewrite(value, arguments, limits, left, depth + 1)?;
            }
        }
        SemanticTypeRecipe::Array { element, length } => {
            rewrite(element, arguments, limits, left, depth + 1)?;
            constant(length, arguments, limits, left)?;
        }
        SemanticTypeRecipe::Address {
            lifetime, inner, ..
        }
        | SemanticTypeRecipe::Desc { lifetime, inner } => {
            if let SemanticOuterLifeRecipe::Class(value) = lifetime {
                life(value, arguments, limits, left)?;
            }
            rewrite(inner, arguments, limits, left, depth + 1)?;
        }
        SemanticTypeRecipe::Slice(inner)
        | SemanticTypeRecipe::Managed(inner)
        | SemanticTypeRecipe::OwnedStore(inner)
        | SemanticTypeRecipe::Mutex(inner)
        | SemanticTypeRecipe::Registration(inner)
        | SemanticTypeRecipe::AsyncRegistration(inner) => {
            rewrite(inner, arguments, limits, left, depth + 1)?
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => {
            rewrite(error, arguments, limits, left, depth + 1)?;
            rewrite(result, arguments, limits, left, depth + 1)?;
        }
        _ => return Err(fail()),
    }
    Ok(())
}
pub(super) fn ty(
    value: &SemanticTypeRecipe,
    arguments: &[SemanticActualRecipe],
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut result = value.clone();
    rewrite(&mut result, arguments, limits, left, 0)?;
    Ok(result)
}
