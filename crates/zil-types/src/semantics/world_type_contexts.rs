//! The world interpreter may share only an immutable type result whose recipe
//! observes the same original declaration and complete rigid substitution.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Context {
    pub definition: SemanticDefinitionId,
    pub representative: SemanticInstanceId,
}

pub(super) fn group(
    program: &SemanticModule,
    limits: &Limits,
    left: &mut usize,
) -> R<BTreeMap<SemanticInstanceId, Context>> {
    let mut groups = BTreeMap::<FunctionId, Vec<&SemanticInstance>>::new();
    let mut contexts = BTreeMap::new();
    for row in &program.instances {
        index::pay(left, 1)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) = row.target
        else {
            continue;
        };
        if row.parent.is_some()
            || row.self_type.is_some()
            || row.environment.is_some()
            || row.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
        {
            continue;
        }
        index::pay(left, row.actuals.len())?;
        if row.actuals.iter().any(|actual| {
            !matches!(
                actual,
                SemanticActual::Type(SemanticTypeValue::Resolved { .. })
                    | SemanticActual::Const {
                        ty: SemanticTypeValue::Resolved { .. },
                        ..
                    }
                    | SemanticActual::Life(_)
            )
        }) {
            continue;
        }
        index::pay(left, index::search(groups.len()))?;
        if let Some(candidates) = groups.get(&target) {
            let mut matched = None;
            for candidate in candidates {
                index::pay(left, 1)?;
                if row.definition != candidate.definition
                    || row.actuals.len() != candidate.actuals.len()
                    || row.header_embedding.len() != candidate.header_embedding.len()
                {
                    continue;
                }
                for actual in row.actuals.iter().chain(&candidate.actuals) {
                    crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                }
                if row.actuals != candidate.actuals {
                    continue;
                }
                for image in row
                    .header_embedding
                    .iter()
                    .chain(&candidate.header_embedding)
                {
                    crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
                }
                if row.header_embedding == candidate.header_embedding {
                    matched = Some(*candidate);
                    break;
                }
            }
            if let Some(representative) = matched {
                let context = Context {
                    definition: row.definition,
                    representative: representative.id,
                };
                index::pay(
                    left,
                    (index::search(contexts.len())
                        .saturating_add(std::mem::size_of::<(SemanticInstanceId, Context)>()))
                    .saturating_mul(2),
                )?;
                contexts.insert(representative.id, context);
                contexts.insert(row.id, context);
                continue;
            }
        }
        index::pay(left, index::search(groups.len()).saturating_add(1))?;
        groups.entry(target).or_default().push(row);
    }
    Ok(contexts)
}

pub(super) fn portable(
    proof: &super::super::original::Proof,
    definition: SemanticDefinitionId,
    recipe: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<bool> {
    index::pay(left, 1)?;
    if depth > limits.max_call_depth {
        return Ok(false);
    }
    let mut check = |child| portable(proof, definition, child, limits, left, depth + 1);
    Ok(match recipe {
        SemanticTypeRecipe::Atom(_) => true,
        SemanticTypeRecipe::Exact(value) => {
            index::pay(left, value.classes.len())?;
            value.classes.iter().all(local_life)
        }
        SemanticTypeRecipe::Formal(formal) => local_formal(formal),
        SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source,
        } => {
            let Some(recipe) = proof.closed_world_recipe(definition, source, limits, left)? else {
                return Ok(false);
            };
            portable(proof, definition, recipe, limits, left, depth + 1)?
        }
        SemanticTypeRecipe::Address {
            lifetime, inner, ..
        }
        | SemanticTypeRecipe::Desc { lifetime, inner } => outer_life(lifetime) && check(inner)?,
        SemanticTypeRecipe::Slice(child)
        | SemanticTypeRecipe::Managed(child)
        | SemanticTypeRecipe::OwnedStore(child)
        | SemanticTypeRecipe::Mutex(child) => check(child)?,
        SemanticTypeRecipe::Array { element, length } => local_const(length) && check(element)?,
        SemanticTypeRecipe::Tuple(fields) => {
            for field in fields {
                if !check(field)? {
                    return Ok(false);
                }
            }
            true
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => check(error)? && check(result)?,
        SemanticTypeRecipe::Nominal { arguments, .. } => {
            for actual in arguments {
                index::pay(left, 1)?;
                let pure = match actual {
                    SemanticActualRecipe::Type(recipe) => {
                        portable(proof, definition, recipe, limits, left, depth + 1)?
                    }
                    SemanticActualRecipe::Const { carrier, value } => {
                        local_const(value)
                            && portable(proof, definition, carrier, limits, left, depth + 1)?
                    }
                    SemanticActualRecipe::Life(life) => local_life(life),
                    SemanticActualRecipe::Infer => false,
                };
                if !pure {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}
fn local_formal(formal: &SemanticFormalRef) -> bool {
    formal.scope == SemanticScopeRef::Ancestor(0)
}
fn local_const(value: &SemanticConstRecipe) -> bool {
    match value {
        SemanticConstRecipe::Value(_)
        | SemanticConstRecipe::IntegerLiteral(_)
        | SemanticConstRecipe::FloatLiteral(_) => true,
        SemanticConstRecipe::Formal(formal) => local_formal(formal),
        _ => false,
    }
}
fn outer_life(life: &SemanticOuterLifeRecipe) -> bool {
    match life {
        SemanticOuterLifeRecipe::Static => true,
        SemanticOuterLifeRecipe::Class(life) => local_life(life),
    }
}
fn local_life(life: &SemanticLifeRecipe) -> bool {
    match life {
        SemanticLifeRecipe::Formal(formal) => local_formal(formal),
        SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ..
        }
        | SemanticLifeRecipe::Bound { .. } => true,
        SemanticLifeRecipe::OuterOccurrence { operand, .. }
        | SemanticLifeRecipe::IntrinsicClass { operand, .. } => match operand {
            SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => true,
            SemanticRecipeOperand::TypeFormal(formal) => local_formal(formal),
            SemanticRecipeOperand::Environment { .. } => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(index: u32) -> SemanticInstanceId {
        SemanticInstanceId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    fn owner(index: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    fn instance(index: u32, ty: Type) -> SemanticInstance {
        SemanticInstance {
            id: scope(index),
            definition: owner(0),
            parent: None,
            target: SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(FunctionId(0))),
            actuals: vec![SemanticActual::Type(SemanticTypeValue::Resolved {
                ty,
                life: empty(),
                classes: vec![],
            })],
            environment: None,
            self_type: None,
            lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
            header_embedding: vec![],
        }
    }

    #[test]
    fn shared_type_context_requires_the_complete_actuals_and_original_owner() {
        let mut wrong_owner = instance(3, Type::I32);
        wrong_owner.definition = owner(1);
        let mut captured = instance(4, Type::I32);
        captured.parent = Some(scope(0));
        captured.environment = Some(SemanticEnvironmentInstance {
            closure: SemanticClosureTarget::Typed(ClosureId(0)),
            actuals: vec![],
        });
        let mut unformed = instance(5, Type::I32);
        unformed.actuals = vec![SemanticActual::CoreInferred {
            scope: scope(0),
            call: SemanticSiteId(0),
            formal: 0,
        }];
        let program = SemanticModule {
            definitions: vec![],
            instances: vec![
                instance(0, Type::I32),
                instance(1, Type::I32),
                instance(2, Type::I64),
                wrong_owner,
                captured,
                unformed,
            ],
            sites: vec![],
            dyn_scopes: vec![],
        };
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let contexts = group(&program, &limits, &mut left).unwrap();
        assert_eq!(contexts.len(), 2);
        for id in [scope(0), scope(1)] {
            assert_eq!(contexts[&id].representative, scope(0));
            assert_eq!(contexts[&id].definition, owner(0));
        }
    }

    #[test]
    fn shared_type_queries_keep_lexical_environment_and_invocation_separate() {
        let proof = super::super::super::original::Proof::default();
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let direct = SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        let address = |life| SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: SemanticOuterLifeRecipe::Class(life),
            inner: Box::new(direct.clone()),
        };
        let local = address(SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        assert!(portable(&proof, owner(0), &local, &limits, &mut left, 0).unwrap());
        for contextual in [
            address(SemanticLifeRecipe::Invocation(0)),
            address(SemanticLifeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: 0,
            }),
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(1),
                ordinal: 0,
            }),
        ] {
            assert!(!portable(&proof, owner(0), &contextual, &limits, &mut left, 0).unwrap());
        }
    }
}
