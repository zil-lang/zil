//! Exact bridge from each raw recipe occurrence to its original lexical
//! declaration. This is data consistency, not a provider or effect proof.
use super::*;

fn ty(
    raw: &DynTypePattern,
    scope: SemanticDefinitionId,
    original: &SemanticTypeRecipe,
    b: &mut Budget,
) -> CompileResult<bool> {
    crate::dyn_metadata::recipe(raw, b)?;
    crate::dyn_metadata::recipe(original, b)?;
    Ok(
        matches!(raw, DynTypePattern::Semantic { scope: actual, ty } if *actual == scope && ty.as_ref() == original),
    )
}
fn life(raw: &DynLifePattern, original: &SemanticLifeRecipe, method: bool) -> bool {
    match (raw, original) {
        (DynLifePattern::DeclarationParameter(index), SemanticLifeRecipe::Formal(reference)) => {
            *index == reference.ordinal
                && reference.scope == SemanticScopeRef::Ancestor(u32::from(method))
        }
        (DynLifePattern::MethodParameter(index), SemanticLifeRecipe::Formal(reference)) => {
            method
                && *index == reference.ordinal
                && reference.scope == SemanticScopeRef::Ancestor(0)
        }
        (DynLifePattern::Invocation(index), SemanticLifeRecipe::Invocation(expected)) => {
            index == expected
        }
        (
            DynLifePattern::Bound { depth, index },
            SemanticLifeRecipe::Bound {
                depth: expected,
                index: ordinal,
            },
        ) => depth == expected && index == ordinal,
        _ => false,
    }
}
fn constant(
    raw: &DynConstPattern,
    scope: SemanticDefinitionId,
    original: &SemanticConstRecipe,
    method: bool,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<bool> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unknown(
            "raw original constant bridge exceeds its complete depth",
        ));
    }
    Ok(match (raw, original) {
        (DynConstPattern::Value(a), SemanticConstRecipe::Value(z)) => a == z,
        (DynConstPattern::IntegerLiteral(a), SemanticConstRecipe::IntegerLiteral(z))
        | (DynConstPattern::FloatLiteral(a), SemanticConstRecipe::FloatLiteral(z)) => {
            b.spend(a.len().saturating_add(z.len()), WORK)?;
            a == z
        }
        (DynConstPattern::Parameter(index), SemanticConstRecipe::Formal(reference)) => {
            *index == reference.ordinal
                && reference.scope == SemanticScopeRef::Ancestor(u32::from(method))
        }
        (DynConstPattern::MethodParameter(index), SemanticConstRecipe::Formal(reference)) => {
            method
                && *index == reference.ordinal
                && reference.scope == SemanticScopeRef::Ancestor(0)
        }
        (
            DynConstPattern::Unary {
                op,
                value,
                ty: carrier,
            },
            SemanticConstRecipe::Unary {
                op: expected,
                operand_type,
                value: source,
            },
        ) => {
            op == expected
                && ty(carrier, scope, operand_type, b)?
                && constant(value, scope, source, method, b, depth + 1)?
        }
        (
            DynConstPattern::Binary {
                op,
                left,
                right,
                ty: carrier,
            },
            SemanticConstRecipe::Binary {
                op: expected,
                operand_type,
                left: a,
                right: z,
            },
        ) => {
            op == expected
                && ty(carrier, scope, operand_type, b)?
                && constant(left, scope, a, method, b, depth + 1)?
                && constant(right, scope, z, method, b, depth + 1)?
        }
        (
            DynConstPattern::Convert { value, ty: target },
            SemanticConstRecipe::Convert {
                target_type,
                value: source,
                ..
            },
        ) => {
            ty(target, scope, target_type, b)?
                && constant(value, scope, source, method, b, depth + 1)?
        }
        (
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                member,
            },
            SemanticConstRecipe::AssociatedConst {
                receiver: expected,
                application: source,
                member: ordinal,
            },
        ) => {
            member == ordinal
                && ty(receiver, scope, expected, b)?
                && application_matches(application, scope, source, method, b, depth + 1)?
        }
        _ => false,
    })
}
fn application_matches(
    raw: &DynSelfPattern,
    scope: SemanticDefinitionId,
    original: &SemanticTraitRecipe,
    method: bool,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<bool> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(unknown("raw original trait bridge exceeds its full depth"));
    }
    if raw.declaration != original.declaration || raw.arguments.len() != original.arguments.len() {
        return Ok(false);
    }
    for (raw, original) in raw.arguments.iter().zip(&original.arguments) {
        let valid = match (raw, original) {
            (DynPatternActual::Type(raw), SemanticActualRecipe::Type(original)) => {
                ty(raw, scope, original, b)?
            }
            (DynPatternActual::Life(raw), SemanticActualRecipe::Life(original)) => {
                life(raw, original, method)
            }
            (DynPatternActual::Const(raw), SemanticActualRecipe::Const { value, .. }) => {
                constant(raw, scope, value, method, b, depth + 1)?
            }
            _ => false,
        };
        if !valid {
            return Ok(false);
        }
    }
    Ok(true)
}
fn signature(
    raw: &DynSignatureRecipe,
    scope: SemanticDefinitionId,
    original: &SemanticSignatureRecipe,
    b: &mut Budget,
) -> CompileResult<bool> {
    if raw.parameters.len() != original.parameters.len()
        || raw.result_mode != original.result_mode
        || raw.call_kind != original.call_kind
        || raw.invocation_binder_count != original.invocation_lifetimes
    {
        return Ok(false);
    }
    for (raw, original) in raw.parameters.iter().zip(&original.parameters) {
        if raw.mode != original.mode || !ty(&raw.ty, scope, &original.ty, b)? {
            return Ok(false);
        }
    }
    ty(&raw.result, scope, &original.result, b)
}
fn requirement(
    raw: &DynObligation,
    scope: SemanticDefinitionId,
    original: &SemanticRequirementRecipe,
    method: bool,
    b: &mut Budget,
) -> CompileResult<bool> {
    crate::dyn_metadata::recipe(raw, b)?;
    crate::dyn_metadata::recipe(original, b)?;
    Ok(match (raw, original) {
        (
            DynObligation::Structural { receiver, bound },
            SemanticRequirementRecipe::Structural {
                ty: original,
                bound: expected,
            },
        ) => bound == expected && ty(receiver, scope, original, b)?,
        (
            DynObligation::Structural { receiver, bound },
            SemanticRequirementRecipe::Builtin {
                ty: original,
                trait_kind,
            },
        ) => {
            let expected = match trait_kind {
                SemanticBuiltinTrait::Copy => Some(DynStructuralBound::Copy),
                SemanticBuiltinTrait::Value => Some(DynStructuralBound::Value),
                SemanticBuiltinTrait::SyncDrop => Some(DynStructuralBound::SyncDrop),
                SemanticBuiltinTrait::StoreElement => Some(DynStructuralBound::StoreElement),
                _ => None,
            };
            expected == Some(*bound) && ty(receiver, scope, original, b)?
        }
        (
            DynObligation::Trait {
                receiver,
                application,
            },
            SemanticRequirementRecipe::Trait {
                ty: original,
                application: expected,
            },
        ) => {
            ty(receiver, scope, original, b)?
                && application_matches(application, scope, expected, method, b, 0)?
        }
        (
            DynObligation::Callable {
                receiver,
                kind,
                quantified_lifetimes,
                signature: raw,
            },
            SemanticRequirementRecipe::Callable {
                ty: original,
                kind: expected,
                signature: source,
            },
        ) => {
            kind == expected
                && *quantified_lifetimes == source.bound_lifetimes
                && ty(receiver, scope, original, b)?
                && signature(raw, scope, source, b)?
        }
        (
            DynObligation::TypeEqual { left, right },
            SemanticRequirementRecipe::TypeEqual { left: a, right: z },
        ) => ty(left, scope, a, b)? && ty(right, scope, z, b)?,
        (
            DynObligation::ConstEqual {
                ty: carrier,
                left,
                right,
            },
            SemanticRequirementRecipe::ConstEqual {
                carrier: expected,
                left: a,
                right: z,
            },
        ) => {
            ty(carrier, scope, expected, b)?
                && constant(left, scope, a, method, b, 0)?
                && constant(right, scope, z, method, b, 0)?
        }
        _ => false,
    })
}
fn requirements(
    raw: &[DynObligation],
    original: &SemanticDefinition,
    method: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut index = 0usize;
    for source in original.requirements.iter().chain(
        original
            .parameters
            .iter()
            .filter_map(|formal| {
                if let SemanticFormal::Type { requirements } = formal {
                    Some(requirements.as_slice())
                } else {
                    None
                }
            })
            .flatten(),
    ) {
        let row = raw.get(index).ok_or_else(|| {
            fail("raw trait declaration drops an original where or formal premise")
        })?;
        if !requirement(row, original.id, source, method, b)? {
            return Err(fail(
                "raw trait declaration changes the exact scope, subject or full original premise",
            ));
        }
        index = index
            .checked_add(1)
            .ok_or_else(|| resource("original requirement ordinal overflow"))?;
    }
    if index != raw.len() {
        return Err(fail(
            "raw trait declaration adds a premise absent from the original declaration",
        ));
    }
    Ok(())
}

impl Solver<'_, '_> {
    pub(super) fn raw_trait_consistency(&self, b: &mut Budget) -> CompileResult<()> {
        for (declaration, scope) in &self.trait_state.scopes {
            let raw = self.inventory.original_dyn(*declaration, b)?;
            let source = self.inventory.definition(*scope, b)?;
            requirements(
                raw.requirements
                    .as_ref()
                    .ok_or_else(|| unknown("raw trait requirement inventory is unknown"))?,
                source,
                false,
                b,
            )?;
            let SemanticDefinitionBody::Scope(recipe) = &source.body else {
                return Err(fail("raw trait source is not its actual original scope"));
            };
            if let SemanticScopeRecipe::TraitImpl {
                trait_application, ..
            } = recipe
            {
                if !application_matches(
                    raw.implemented_trait
                        .as_ref()
                        .ok_or_else(|| unknown("raw implemented trait application is unknown"))?,
                    *scope,
                    trait_application,
                    false,
                    b,
                    0,
                )? {
                    return Err(fail(
                        "raw implemented trait application changes its complete original ordered actuals",
                    ));
                }
            }
            let mut index = 0usize;
            for requirement in &source.requirements {
                if let SemanticRequirementRecipe::Trait {
                    ty:
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(0),
                        },
                    application,
                } = requirement
                {
                    if raw.kind != DynDeclarationKind::Trait {
                        continue;
                    }
                    let row = raw
                        .direct_supertraits
                        .as_ref()
                        .and_then(|rows| rows.get(index))
                        .ok_or_else(|| {
                            fail("raw trait drops an original direct supertrait occurrence")
                        })?;
                    if !application_matches(row, *scope, application, false, b, 0)? {
                        return Err(fail(
                            "raw supertrait changes its original application or actual occurrence",
                        ));
                    }
                    index += 1;
                }
            }
            if raw
                .direct_supertraits
                .as_ref()
                .is_none_or(|rows| rows.len() != index)
            {
                return Err(fail("raw trait contains an extra original supertrait edge"));
            }
            for method in raw
                .method_signatures
                .as_ref()
                .ok_or_else(|| unknown("raw method signature inventory is unknown"))?
            {
                let scope = trait_members::member_scope(
                    self.program,
                    *declaration,
                    method.member,
                    self.inventory,
                    b,
                )?;
                let original = self.inventory.definition(scope, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature: source, ..
                } = &original.body
                else {
                    return Err(fail("raw method signature has no exact original function"));
                };
                if !signature(&method.signature, scope, source, b)? {
                    return Err(fail(
                        "raw method signature changes the complete original type or life expression",
                    ));
                }
                requirements(&method.requirements, original, true, b)?;
            }
            for method in raw
                .method_definitions
                .as_ref()
                .ok_or_else(|| unknown("raw method body inventory is unknown"))?
            {
                let scope =
                    trait_members::definition_scope(self.program, method.id, self.inventory, b)?;
                let original = self.inventory.definition(scope, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature: source, ..
                } = &original.body
                else {
                    return Err(fail("raw method definition has no exact original function"));
                };
                if !signature(&method.signature, scope, source, b)? {
                    return Err(fail(
                        "raw method body signature changes its original scoped types",
                    ));
                }
                requirements(&method.requirements, original, true, b)?;
            }
        }
        Ok(())
    }
}
