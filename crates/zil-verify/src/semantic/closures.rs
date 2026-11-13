//! Construct an opaque closure type only from its exact actual entry instance
//! and complete sealed ghost environment. No Capture or borrow root is invented.
use super::*;
impl CheckedContext {
    pub(super) fn closure_type(
        &self,
        module: &TypedModule,
        definition: SemanticDefinitionId,
        argument_recipes: &[SemanticActualRecipe],
        environment_recipes: &[SemanticActualRecipe],
        caller: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let original = self.definition(module, definition, b)?;
        if original.kind != SemanticDefinitionKind::Closure {
            return Err(bad(
                "Closure type recipe selects another original definition kind",
            ));
        }
        if argument_recipes.len() != original.parameters.len()
            || environment_recipes.len() != original.environment.len()
        {
            return Err(bad(
                "Closure type omits original formals or its complete ghost environment",
            ));
        }
        b.spend(
            argument_recipes
                .len()
                .saturating_add(environment_recipes.len()),
        )?;
        let mut actuals = Vec::with_capacity(argument_recipes.len());
        let mut sealed = Vec::with_capacity(environment_recipes.len());
        for recipe in argument_recipes {
            actuals.push(self.evaluate_actual(module, recipe, caller, external, active, b)?);
        }
        for recipe in environment_recipes {
            sealed.push(self.evaluate_actual(module, recipe, caller, external, active, b)?);
        }
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("Closure type has no original semantic program"))?;
        b.spend(program.instances.len())?;
        let mut selected = None;
        for instance in &program.instances {
            if instance.definition != definition {
                continue;
            }
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
                instance.target
            else {
                continue;
            };
            let entry = module
                .functions
                .get(function.0 as usize)
                .filter(|entry| entry.id == function)
                .ok_or_else(|| bad("Closure type instance has no exact Typed function"))?;
            let Some(id) = entry.environment else {
                return Err(bad("Closure type instance is not an environment entry"));
            };
            let environment = self.environment_for_instance(module, instance.id, active, b)?;
            let frame = environment
                .lexical
                .first()
                .ok_or_else(|| bad("Closure type instance has no checked lexical frame"))?;
            if !nominal::same_actuals(self, caller, &actuals, &frame.actuals, b)?
                || !nominal::same_actuals(self, caller, &sealed, &frame.environment, b)?
            {
                continue;
            }
            let ty = Type::Closure(id);
            let (count, _pending_sources) = dynamic_witness::intrinsic_domain(module, &ty, b)?;
            b.spend(count as usize + instance.header_embedding.len())?;
            let mut classes = Vec::with_capacity(count as usize);
            for class in 0..count {
                let mut rows = instance
                    .header_embedding
                    .iter()
                    .filter(|row| row.header == StorageBinder::Fixed(class));
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Closure type loses a free Fixed/ghost class"))?;
                if rows.next().is_some() {
                    return Err(bad("Closure type repeats a free Fixed class"));
                }
                classes.push(self.class(module, &row.actual, &environment, b)?);
            }
            b.spend(module.closures.len())?;
            let mut definitions = module
                .closures
                .iter()
                .filter(|closure| closure.id == id && closure.function == function);
            let closure = definitions
                .next()
                .ok_or_else(|| bad("Closure type has no exact actual definition"))?;
            if definitions.next().is_some() {
                return Err(bad("Closure type has duplicate actual definitions"));
            }
            let header = closure
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("Closure type's complete Fixed/Static relation is Unknown"))?;
            let mut statics = Vec::with_capacity(header.static_bindings.len());
            for position in &header.static_bindings {
                let root = match position.base {
                    StorageBase::Parameter(i) => closure.parameters.get(i as usize),
                    StorageBase::Capture(i) => closure.captures.get(i as usize),
                    StorageBase::Result => Some(&closure.result),
                    StorageBase::Receiver => None,
                }
                .ok_or_else(|| bad("Closure static occurrence has no actual typed root"))?;
                let leaf = static_leaf(module, root, &position.path, b)?;
                let mode = match leaf {
                    Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                    Type::Desc(_) => StoredBorrowMode::ReadOnlyDescription,
                    _ => {
                        return Err(bad(
                            "Closure static occurrence is not a shared actual borrow",
                        ));
                    }
                };
                statics.push(StaticOccurrence::Logical {
                    closure: id,
                    position: position.clone(),
                    leaf: leaf.clone(),
                    mode,
                });
            }
            let value = CheckedType {
                ty,
                life: StorageLifeShape {
                    binder_count: count,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                // Receiver here selects this exact opaque environment's type
                // domain, not a physical Capture or an activation borrow.
                origins: (0..count)
                    .map(|class| IntrinsicOrigin::Existing {
                        scope: frame.key,
                        operand: OperandKey::Receiver,
                        path: vec![],
                        class,
                    })
                    .collect(),
                classes,
                statics,
            };
            if let Some(previous) = &selected {
                if previous != &value {
                    return Err(bad(
                        "Closure type selects conflicting exact actual specializations",
                    ));
                }
            } else {
                selected = Some(value);
            }
        }
        selected.ok_or_else(|| bad("Closure type has no exact actual specialization with its complete sealed environment"))
    }
}

fn static_leaf<'a>(
    module: &'a TypedModule,
    mut ty: &'a Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    b.spend(path.len().saturating_add(1))?;
    for projection in path {
        while let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        ty = match (projection, ty) {
            (
                ContractProjection::Place(PlaceProjection::Field(i)),
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) => {
                &fields
                    .get(*i as usize)
                    .ok_or_else(|| bad("static field occurrence is absent"))?
                    .1
            }
            (ContractProjection::Place(PlaceProjection::Field(i)), Type::Tuple(fields)) => fields
                .get(*i as usize)
                .ok_or_else(|| bad("static tuple occurrence is absent"))?,
            (
                ContractProjection::Place(PlaceProjection::Variant(VariantProjection {
                    variant,
                    index,
                })),
                Type::Enum { variants, .. },
            ) => variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| bad("static variant occurrence is absent"))?,
            (
                ContractProjection::Place(PlaceProjection::ErrorArm(error)),
                Type::ErrorUnion { error: a, result },
            ) => {
                if *error {
                    a
                } else {
                    result
                }
            }
            (ContractProjection::Capture(i), Type::Closure(id)) => {
                b.spend(module.closures.len())?;
                let mut rows = module.closures.iter().filter(|d| d.id == *id);
                let d = rows
                    .next()
                    .ok_or_else(|| bad("static logical capture has no actual definition"))?;
                if rows.next().is_some() {
                    return Err(bad("static logical capture definition is ambiguous"));
                }
                d.captures
                    .get(*i as usize)
                    .ok_or_else(|| bad("static logical capture is absent"))?
            }
            _ => {
                return Err(unsupported(
                    "static occurrence requires its complete actual logical leaf directory",
                    None,
                ));
            }
        };
    }
    Ok(ty)
}
