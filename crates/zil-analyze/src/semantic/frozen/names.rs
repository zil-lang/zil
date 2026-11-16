//! Exact lexical transport into one real child query wrapper. Only Ancestor
//! selectors move; protected Bound, Invocation and intrinsic class ordinals do
//! not. These copies are comparison operands and never grant a type or loan.
use super::*;

fn step(b: &mut Budget, depth: usize) -> CompileResult<()> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unsupported(
            "CT lexical transport exceeds its bounded depth",
        ));
    }
    Ok(())
}
fn scope(scope: &mut SemanticScopeRef) -> CompileResult<()> {
    let SemanticScopeRef::Ancestor(level) = scope;
    *level = level
        .checked_sub(1)
        .ok_or_else(|| fail("CT inherited recipe selects the new wrapper frame"))?;
    Ok(())
}
fn operand(value: &mut SemanticRecipeOperand) -> CompileResult<()> {
    match value {
        SemanticRecipeOperand::TypeFormal(formal) => scope(&mut formal.scope),
        SemanticRecipeOperand::Environment { scope: at, .. } => scope(at),
        SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => Ok(()),
    }
}
fn life(value: &mut SemanticLifeRecipe) -> CompileResult<()> {
    match value {
        SemanticLifeRecipe::Formal(formal) => scope(&mut formal.scope),
        SemanticLifeRecipe::Implicit { scope: at, .. }
        | SemanticLifeRecipe::Environment { scope: at, .. } => scope(at),
        SemanticLifeRecipe::OuterOccurrence { operand: input, .. }
        | SemanticLifeRecipe::IntrinsicClass { operand: input, .. } => operand(input),
        SemanticLifeRecipe::Bound { .. } | SemanticLifeRecipe::Invocation(_) => Ok(()),
    }
}
fn outer(value: &mut SemanticOuterLifeRecipe) -> CompileResult<()> {
    match value {
        SemanticOuterLifeRecipe::Class(value) => life(value),
        SemanticOuterLifeRecipe::Static => Ok(()),
    }
}
fn actual(value: &mut SemanticActualRecipe, b: &mut Budget, depth: usize) -> CompileResult<()> {
    step(b, depth)?;
    match value {
        SemanticActualRecipe::Type(value) => ty(value, b, depth + 1),
        SemanticActualRecipe::Const { carrier, value } => {
            ty(carrier, b, depth + 1)?;
            constant(value, b, depth + 1)
        }
        SemanticActualRecipe::Life(value) => life(value),
        SemanticActualRecipe::Infer => Ok(()),
    }
}
fn actuals(values: &mut [SemanticActualRecipe], b: &mut Budget, depth: usize) -> CompileResult<()> {
    for value in values {
        actual(value, b, depth + 1)?;
    }
    Ok(())
}
fn constant(value: &mut SemanticConstRecipe, b: &mut Budget, depth: usize) -> CompileResult<()> {
    step(b, depth)?;
    match value {
        SemanticConstRecipe::Value(_)
        | SemanticConstRecipe::IntegerLiteral(_)
        | SemanticConstRecipe::FloatLiteral(_) => Ok(()),
        SemanticConstRecipe::Formal(formal) => scope(&mut formal.scope),
        SemanticConstRecipe::Environment { scope: at, .. } => scope(at),
        SemanticConstRecipe::Unary {
            operand_type,
            value,
            ..
        } => {
            ty(operand_type, b, depth + 1)?;
            constant(value, b, depth + 1)
        }
        SemanticConstRecipe::Binary {
            operand_type,
            left,
            right,
            ..
        } => {
            ty(operand_type, b, depth + 1)?;
            constant(left, b, depth + 1)?;
            constant(right, b, depth + 1)
        }
        SemanticConstRecipe::Convert {
            source_type,
            target_type,
            value,
        } => {
            ty(source_type, b, depth + 1)?;
            ty(target_type, b, depth + 1)?;
            constant(value, b, depth + 1)
        }
        SemanticConstRecipe::AssociatedConst {
            receiver,
            application,
            ..
        } => {
            ty(receiver, b, depth + 1)?;
            actuals(&mut application.arguments, b, depth + 1)
        }
    }
}
fn sequence(
    value: &mut SemanticClassSequenceRecipe,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    step(b, depth)?;
    for part in &mut value.parts {
        step(b, depth + 1)?;
        match part {
            SemanticClassSequencePart::One(value) => life(value)?,
            SemanticClassSequencePart::AllClasses { ty: value, .. } => ty(value, b, depth + 1)?,
        }
    }
    Ok(())
}
fn retained(
    values: &mut [SemanticRetainedPart],
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    for part in values {
        step(b, depth)?;
        match part {
            SemanticRetainedPart::One { parent, .. } => life(parent)?,
            SemanticRetainedPart::All { parents, .. } => sequence(parents, b, depth + 1)?,
        }
    }
    Ok(())
}
fn nominal(values: &mut [SemanticNominalPart], b: &mut Budget, depth: usize) -> CompileResult<()> {
    for part in values {
        step(b, depth)?;
        match part {
            SemanticNominalPart::One { class, .. } => life(class)?,
            SemanticNominalPart::FromType {
                ty: value, classes, ..
            } => {
                ty(value, b, depth + 1)?;
                sequence(classes, b, depth + 1)?;
            }
        }
    }
    Ok(())
}
fn shape(
    value: &mut SemanticRetainedShapeRecipe,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    step(b, depth)?;
    sequence(&mut value.domain, b, depth + 1)?;
    for port in &mut value.ports {
        step(b, depth + 1)?;
        match port {
            SemanticPortPart::One { leaf, class, .. } => {
                ty(leaf, b, depth + 1)?;
                life(class)?;
            }
            SemanticPortPart::FromType { ty: value, classes } => {
                ty(value, b, depth + 1)?;
                sequence(classes, b, depth + 1)?;
            }
        }
    }
    retained(&mut value.retained, b, depth + 1)?;
    nominal(&mut value.nominal, b, depth + 1)
}
fn signature(
    value: &mut SemanticSignatureRecipe,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    step(b, depth)?;
    for parameter in &mut value.parameters {
        ty(&mut parameter.ty, b, depth + 1)?;
    }
    ty(&mut value.result, b, depth + 1)
}
pub(super) fn ty(
    value: &mut SemanticTypeRecipe,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    step(b, depth)?;
    match value {
        SemanticTypeRecipe::Atom(_)
        | SemanticTypeRecipe::UnresolvedNamed(_)
        | SemanticTypeRecipe::Unsupported(_) => Ok(()),
        SemanticTypeRecipe::Exact(value) => {
            for class in &mut value.classes {
                step(b, depth + 1)?;
                life(class)?;
            }
            Ok(())
        }
        SemanticTypeRecipe::Formal(formal) => scope(&mut formal.scope),
        SemanticTypeRecipe::Environment { scope: at, .. }
        | SemanticTypeRecipe::SelfType { scope: at }
        | SemanticTypeRecipe::OriginalType { scope: at, .. } => scope(at),
        SemanticTypeRecipe::Nominal { arguments, .. } => actuals(arguments, b, depth + 1),
        SemanticTypeRecipe::AssociatedType {
            receiver,
            application,
            ..
        } => {
            ty(receiver, b, depth + 1)?;
            actuals(&mut application.arguments, b, depth + 1)
        }
        SemanticTypeRecipe::Tuple(values) | SemanticTypeRecipe::AnonymousSum(values) => {
            for value in values {
                ty(value, b, depth + 1)?;
            }
            Ok(())
        }
        SemanticTypeRecipe::Array { element, length } => {
            ty(element, b, depth + 1)?;
            constant(length, b, depth + 1)
        }
        SemanticTypeRecipe::Slice(value)
        | SemanticTypeRecipe::Managed(value)
        | SemanticTypeRecipe::OwnedStore(value)
        | SemanticTypeRecipe::Mutex(value)
        | SemanticTypeRecipe::Registration(value)
        | SemanticTypeRecipe::AsyncRegistration(value) => ty(value, b, depth + 1),
        SemanticTypeRecipe::Address {
            lifetime, inner, ..
        }
        | SemanticTypeRecipe::Desc { lifetime, inner } => {
            outer(lifetime)?;
            ty(inner, b, depth + 1)
        }
        SemanticTypeRecipe::MutexGuard { payload, retained }
        | SemanticTypeRecipe::Task {
            result: payload,
            retained,
        } => {
            ty(payload, b, depth + 1)?;
            shape(retained, b, depth + 1)
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => {
            ty(error, b, depth + 1)?;
            ty(result, b, depth + 1)
        }
        SemanticTypeRecipe::Qualified {
            storage,
            life: value,
        } => {
            ty(storage, b, depth + 1)?;
            sequence(&mut value.domain, b, depth + 1)?;
            for slot in &mut value.slots {
                step(b, depth + 1)?;
                life(&mut slot.class)?;
            }
            retained(&mut value.retained, b, depth + 1)?;
            nominal(&mut value.nominal, b, depth + 1)
        }
        SemanticTypeRecipe::Code {
            signature: value, ..
        } => signature(value, b, depth + 1),
        SemanticTypeRecipe::Closure {
            arguments,
            environment,
            ..
        } => {
            actuals(arguments, b, depth + 1)?;
            actuals(environment, b, depth + 1)
        }
        SemanticTypeRecipe::Opaque {
            arguments, hidden, ..
        } => {
            actuals(arguments, b, depth + 1)?;
            ty(hidden, b, depth + 1)
        }
        SemanticTypeRecipe::Dyn { interface, .. } => {
            actuals(&mut interface.application.arguments, b, depth + 1)?;
            match &mut interface.context {
                SemanticDynContextRecipe::Closed => {}
                SemanticDynContextRecipe::Type(value) => ty(value, b, depth + 1)?,
                SemanticDynContextRecipe::Shape(value) => shape(value, b, depth + 1)?,
            }
            for binding in &mut interface.associated {
                step(b, depth + 1)?;
                actuals(&mut binding.application.arguments, b, depth + 1)?;
                match &mut binding.value {
                    SemanticAssociatedRecipe::Type(value) => ty(value, b, depth + 1)?,
                    SemanticAssociatedRecipe::Const { carrier, value } => {
                        ty(carrier, b, depth + 1)?;
                        constant(value, b, depth + 1)?;
                    }
                }
            }
            Ok(())
        }
    }
}

pub(super) fn inherited(
    expected: &SemanticOriginalNameBinding,
    actual: &SemanticOriginalNameBinding,
    b: &mut Budget,
) -> CompileResult<()> {
    crate::dyn_metadata::recipe(expected, b)?;
    crate::dyn_metadata::recipe(actual, b)?;
    let mut lowered = actual.clone();
    if let SemanticOriginalNameValue::Recipe(value) = &mut lowered.value {
        self::actual(value, b, 0)?;
    }
    if expected != &lowered {
        return Err(fail(
            "CT wrapper changes an inherited complete lexical name recipe",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding(scope_level: u32, bound: u32) -> SemanticOriginalNameBinding {
        SemanticOriginalNameBinding {
            path: vec!["F".into()],
            namespace: SemanticOriginalNamespace::Type,
            value: SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(
                SemanticTypeRecipe::Code {
                    kind: SemanticCodeKind::BorrowedFunctionPointer,
                    signature: Box::new(SemanticSignatureRecipe {
                        bound_lifetimes: 1,
                        invocation_lifetimes: 1,
                        parameters: vec![SemanticParameterRecipe {
                            ty: SemanticTypeRecipe::Address {
                                mutable: false,
                                lifetime: SemanticOuterLifeRecipe::Class(
                                    SemanticLifeRecipe::Bound {
                                        depth: bound,
                                        index: 0,
                                    },
                                ),
                                inner: Box::new(SemanticTypeRecipe::Formal(SemanticFormalRef {
                                    scope: SemanticScopeRef::Ancestor(scope_level),
                                    ordinal: 2,
                                })),
                            },
                            mode: PassingMode::Infer,
                        }],
                        result: SemanticTypeRecipe::Address {
                            mutable: false,
                            lifetime: SemanticOuterLifeRecipe::Class(
                                SemanticLifeRecipe::Invocation(0),
                            ),
                            inner: Box::new(SemanticTypeRecipe::Atom(Type::I32)),
                        },
                        result_mode: PassingMode::Infer,
                        call_kind: CallKind::Direct,
                    }),
                },
            )),
        }
    }
    #[test]
    fn inherited_recipe_preserves_protected_and_invocation_frames() {
        let limits = Limits::default();
        let expected = binding(0, 0);
        let mut b = Budget::new(&limits);
        assert!(inherited(&expected, &binding(1, 0), &mut b).is_ok());
        assert!(inherited(&expected, &binding(1, 1), &mut b).is_err());
        assert!(inherited(&expected, &binding(0, 0), &mut b).is_err());
        assert!(inherited(&expected, &binding(2, 0), &mut b).is_err());
    }
}
