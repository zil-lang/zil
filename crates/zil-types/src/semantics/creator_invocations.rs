//! Coordinates for original invocation quantifiers, chosen only at the exact
//! physical occurrence of that quantifier in the checked original signature.
//! The caller and World still prove every incoming occurrence and full domain.
use super::*;

fn selected(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    instance: &SemanticInstance,
    recipe: &SemanticTypeRecipe,
    path: &[ContractProjection],
    limits: &Limits,
    left: &mut usize,
) -> R<Option<(u32, StoredBorrowMode)>> {
    if path.len() > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "invocation occurrence path exceeded",
            None,
        ));
    }
    pay(left, path.len().saturating_add(1))?;
    let mut current = super::super::materialize::symbolic_at(
        program,
        module,
        ledger,
        instance.id,
        recipe,
        limits,
        left,
    )?;
    for projection in path {
        pay(left, 1)?;
        current = match (&current, projection) {
            (
                SemanticTypeRecipe::Tuple(fields),
                ContractProjection::Place(PlaceProjection::Field(index)),
            ) => {
                let field = fields.get(*index as usize).ok_or_else(|| {
                    bad("invocation physical occurrence exceeds its original tuple")
                })?;
                crate::dyn_metadata::semantic_operand_snapshot(field, limits, left, 1)?;
                field.clone()
            }
            (
                SemanticTypeRecipe::Array { element, .. },
                ContractProjection::Place(PlaceProjection::Field(_)),
            ) => {
                crate::dyn_metadata::semantic_operand_snapshot(element.as_ref(), limits, left, 1)?;
                element.as_ref().clone()
            }
            (
                SemanticTypeRecipe::ErrorUnion { error, result },
                ContractProjection::Place(PlaceProjection::ErrorArm(is_error)),
            ) => {
                let arm = if *is_error { error } else { result };
                crate::dyn_metadata::semantic_operand_snapshot(arm.as_ref(), limits, left, 1)?;
                arm.as_ref().clone()
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                },
                ContractProjection::Place(part),
            ) => {
                pay(left, program.definitions.len())?;
                let declaration = program
                    .definitions
                    .iter()
                    .find(|row| row.id == *definition)
                    .ok_or_else(|| {
                        bad("invocation occurrence lacks its original nominal declaration")
                    })?;
                let field = match (&declaration.body, part) {
                    (
                        SemanticDefinitionBody::Nominal {
                            body: SemanticNominalBody::Product { fields },
                            ..
                        },
                        PlaceProjection::Field(index),
                    ) => fields.get(*index as usize).map(|field| &field.ty),
                    (
                        SemanticDefinitionBody::Nominal {
                            body: SemanticNominalBody::Enum { variants },
                            ..
                        },
                        PlaceProjection::Variant(index),
                    ) => variants
                        .get(index.variant as usize)
                        .and_then(|variant| variant.fields.get(index.index as usize)),
                    _ => None,
                }
                .ok_or_else(|| {
                    bad("invocation occurrence changes its original nominal field path")
                })?;
                let proof = ledger.original.borrow();
                let proof = proof
                    .as_ref()
                    .ok_or_else(|| bad("invocation occurrence lacks its original type proof"))?;
                super::super::instantiate::ty(
                    program,
                    proof,
                    field,
                    super::super::instantiate::DomainMapping {
                        source: declaration.id,
                        use_definition: instance.definition,
                        arguments,
                        parent_call: None,
                    },
                    limits,
                    left,
                )?
            }
            // Pointees, code quantifiers, opaque retained ports and capture
            // storage are independent domains, not physical inline fields.
            _ => return Ok(None),
        };
        current = super::super::materialize::symbolic_at(
            program,
            module,
            ledger,
            instance.id,
            &current,
            limits,
            left,
        )?;
    }
    Ok(match current {
        SemanticTypeRecipe::Address {
            mutable,
            lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(index)),
            ..
        } => Some((
            index,
            if mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            },
        )),
        SemanticTypeRecipe::Desc {
            lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(index)),
            ..
        } => Some((index, StoredBorrowMode::WritableDescription)),
        _ => None,
    })
}

pub(super) fn seed(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut changes = Vec::new();
    for instance in &program.instances {
        pay(
            left,
            module
                .functions
                .len()
                .saturating_add(program.definitions.len())
                .saturating_add(1),
        )?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) =
            instance.target
        else {
            continue;
        };
        let function = module
            .functions
            .iter()
            .find(|row| row.id == target)
            .ok_or_else(|| bad("invocation seed has no actual function"))?;
        let Some(header) = &function.storage_life else {
            continue;
        };
        let original = program
            .definitions
            .iter()
            .find(|row| row.id == instance.definition)
            .ok_or_else(|| bad("invocation seed has no original definition"))?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &original.body
        else {
            continue;
        };
        if signature.invocation_lifetimes == 0 {
            continue;
        }
        let mut seeds = std::collections::BTreeMap::new();
        pay(left, position_work(header))?;
        for row in &header.bindings {
            let StorageBinder::Invocation(binder) = row.binder else {
                continue;
            };
            let StorageBase::Parameter(parameter) = row.position.base else {
                continue;
            };
            let recipe = SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Parameter(parameter),
            };
            let Some((original_index, mode)) = selected(
                program,
                module,
                ledger,
                instance,
                &recipe,
                &row.position.path,
                limits,
                left,
            )?
            else {
                continue;
            };
            if original_index != binder || original_index >= signature.invocation_lifetimes {
                return Err(bad(
                    "actual invocation binder changes its original quantifier occurrence",
                ));
            }
            pay(
                left,
                seeds
                    .len()
                    .saturating_add(row.position.path.len())
                    .saturating_add(1),
            )?;
            let image = SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::Parameter(parameter),
                path: row
                    .position
                    .path
                    .iter()
                    .map(|part| match part {
                        ContractProjection::Place(part) => Ok(LifeTypeProjection::Place(*part)),
                        _ => Err(bad("invocation seed crosses a nonphysical contract port")),
                    })
                    .collect::<R<Vec<_>>>()?,
                mode,
            };
            seeds.entry(row.binder).or_insert(image);
        }
        let mut additions = Vec::new();
        for (header, actual) in seeds {
            pay(left, instance.header_embedding.len().saturating_add(1))?;
            let mut existing = instance
                .header_embedding
                .iter()
                .filter(|row| row.header == header);
            if let Some(old) = existing.next() {
                crate::dyn_metadata::semantic_operand_snapshot(&old.actual, limits, left, 1)?;
                if existing.next().is_some() || old.actual != actual {
                    return Err(bad(
                        "invocation coordinate changes its existing exact occurrence",
                    ));
                }
            } else {
                additions.push(SemanticHeaderBinding { header, actual });
            }
        }
        changes.push((instance.id, additions));
    }
    for (id, additions) in changes {
        pay(
            left,
            program.instances.len().saturating_add(additions.len()),
        )?;
        let instance = program
            .instances
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("invocation seed instance disappeared"))?;
        instance.header_embedding.extend(additions);
    }
    Ok(())
}
