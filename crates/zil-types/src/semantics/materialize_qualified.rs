//! Complete nominal qualification from the original declaration's fields.
//! The private Bound coordinates below exist only during substitution. They
//! are consumed into integer class maps and never published as semantic lives.
use super::*;

impl Eval<'_, '_, '_> {
    pub(super) fn qualified_nominal(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        arguments: &[SemanticActualRecipe],
        hint: &Type,
        depth: usize,
    ) -> R<LifeNominalInstance> {
        self.node(depth)?;
        let original = self.definition(definition)?;
        let SemanticDefinitionBody::Nominal {
            declaration, body, ..
        } = &original.body
        else {
            return Err(bad(
                "qualified application lost its original nominal declaration",
            ));
        };
        if arguments.len() != original.parameters.len() {
            return Err(bad("qualified application omits full mixed actuals"));
        }
        self.fee(arguments.len())?;
        let mut actuals = Vec::new();
        let mut coordinates = Vec::new();
        let mut shape = empty();
        let mut lifetime_images = Vec::<(SemanticClassExpr, u32)>::new();
        for (ordinal, (formal, recipe)) in original.parameters.iter().zip(arguments).enumerate() {
            let actual = self.recipe_actual(scope, recipe, depth + 1)?;
            match (formal, actual) {
                (
                    SemanticFormal::Type { .. },
                    SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes }),
                ) => {
                    if life.binder_count as usize != classes.len() {
                        return Err(bad(
                            "qualified Type actual omits its complete intrinsic domain",
                        ));
                    }
                    self.fee(classes.len())?;
                    let start = shape.binder_count;
                    shape.binder_count = start.checked_add(life.binder_count).ok_or_else(|| {
                        Diagnostic::limit(Phase::Types, "qualified class domain overflow", None)
                    })?;
                    if shape.binder_count as usize > self.limits.max_nodes {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "qualified class domain limit exceeded",
                            None,
                        ));
                    }
                    let map: Vec<_> = (start..shape.binder_count).collect();
                    inherit_child_shape(
                        &mut shape,
                        &life,
                        LifeTypeProjection::TypeArgument(ordinal as u32),
                        &map,
                        self.limits,
                        self.left,
                    )?;
                    inference::charge_type_snapshot(
                        &ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let copied = self.copy_shape(&life)?;
                    coordinates.push(SemanticActualRecipe::Type(SemanticTypeRecipe::Exact(
                        Box::new(SemanticExactTypeRecipe {
                            ty: ty.clone(),
                            life: copied,
                            classes: map
                                .iter()
                                .map(|index| SemanticLifeRecipe::Bound {
                                    depth: 0,
                                    index: *index,
                                })
                                .collect(),
                        }),
                    )));
                    actuals.push(
                        if life != empty() || matches!(ty, Type::BorrowedStorage { .. }) {
                            LifeNominalActual::QualifiedType { ty, life }
                        } else {
                            LifeNominalActual::Type(ty)
                        },
                    );
                }
                (SemanticFormal::Lifetime, SemanticActual::Life(image)) => {
                    let mut selected = None;
                    for (prior, index) in &lifetime_images {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            prior,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &image,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if prior == &image {
                            selected = Some(*index);
                            break;
                        }
                    }
                    let index = if let Some(index) = selected {
                        index
                    } else {
                        let index = shape.binder_count;
                        shape.binder_count = index.checked_add(1).ok_or_else(|| {
                            Diagnostic::limit(Phase::Types, "qualified class domain overflow", None)
                        })?;
                        if shape.binder_count as usize > self.limits.max_nodes {
                            return Err(Diagnostic::limit(
                                Phase::Types,
                                "qualified class domain limit exceeded",
                                None,
                            ));
                        }
                        self.fee(1)?;
                        lifetime_images.push((image, index));
                        index
                    };
                    self.fee(1)?;
                    shape.nominal_arguments.push(NominalLifeArgument {
                        path: Vec::new(),
                        declaration: *declaration,
                        argument: ordinal as u32,
                        binder: index,
                    });
                    actuals.push(LifeNominalActual::Lifetime(index));
                    coordinates.push(SemanticActualRecipe::Life(SemanticLifeRecipe::Bound {
                        depth: 0,
                        index,
                    }));
                }
                (
                    SemanticFormal::Const { .. },
                    SemanticActual::Const {
                        ty: SemanticTypeValue::Resolved { ty, life, classes },
                        value,
                    },
                ) => {
                    if life != empty() || !classes.is_empty() || value.ty() != ty {
                        return Err(bad("qualified Const actual changes its complete carrier"));
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        recipe,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    coordinates.push(recipe.clone());
                    actuals.push(LifeNominalActual::Const(value));
                }
                _ => {
                    return Err(bad(
                        "qualified application changes a full mixed actual kind",
                    ));
                }
            }
        }
        let storage = stored::physical(hint);
        match (body, storage) {
            (
                SemanticNominalBody::Product { fields },
                Type::Record {
                    fields: physical, ..
                }
                | Type::Package {
                    fields: physical, ..
                },
            ) => {
                self.fee(fields.len().saturating_add(physical.len()))?;
                if fields.len() != physical.len() {
                    return Err(bad("qualified nominal changes original field arity"));
                }
                for (index, (field, (_, ty))) in fields.iter().zip(physical).enumerate() {
                    self.qualified_field(
                        scope,
                        definition,
                        &coordinates,
                        &field.ty,
                        ty,
                        PlaceProjection::Field(index as u32),
                        &mut shape,
                        depth + 1,
                    )?;
                }
            }
            (
                SemanticNominalBody::Enum { variants },
                Type::Enum {
                    variants: physical, ..
                },
            ) => {
                self.fee(variants.len().saturating_add(physical.len()))?;
                if variants.len() != physical.len() {
                    return Err(bad("qualified nominal changes original variant arity"));
                }
                for (variant, (original, (_, fields))) in variants.iter().zip(physical).enumerate()
                {
                    self.fee(original.fields.len().saturating_add(fields.len()))?;
                    if original.fields.len() != fields.len() {
                        return Err(bad("qualified nominal changes original payload arity"));
                    }
                    for (index, (recipe, ty)) in original.fields.iter().zip(fields).enumerate() {
                        self.qualified_field(
                            scope,
                            definition,
                            &coordinates,
                            recipe,
                            ty,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            &mut shape,
                            depth + 1,
                        )?;
                    }
                }
            }
            _ => {
                return Err(bad(
                    "qualified nominal changes original physical constructor",
                ));
            }
        }
        // The complete rows use the canonical raw ordering, not traversal order.
        let rows = shape
            .slots
            .len()
            .saturating_add(shape.retained.len())
            .saturating_add(shape.nominal_arguments.len());
        let width = shape
            .slots
            .iter()
            .map(|x| x.path.len())
            .chain(shape.retained.iter().map(|x| x.path.len()))
            .chain(shape.nominal_arguments.iter().map(|x| x.path.len()))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.fee(
            rows.saturating_mul(rows.saturating_add(1).ilog2() as usize + 1)
                .saturating_mul(width),
        )?;
        shape.slots.sort();
        shape.retained.sort();
        shape.nominal_arguments.sort();
        // Canonical numbering follows the same nominal/physical/retained
        // occurrence order as the ordinary shape consumer. This is a pure
        // permutation of original coordinates, never an image-based quotient.
        self.fee((shape.binder_count as usize).saturating_add(rows))?;
        let mut permutation = vec![None; shape.binder_count as usize];
        let mut next = 0u32;
        for binder in shape
            .nominal_arguments
            .iter()
            .map(|r| r.binder)
            .chain(shape.slots.iter().map(|r| r.binder))
            .chain(shape.retained.iter().map(|r| r.binder))
        {
            let slot = permutation
                .get_mut(binder as usize)
                .ok_or_else(|| bad("qualified occurrence leaves its original domain"))?;
            if slot.is_none() {
                *slot = Some(next);
                next += 1;
            }
        }
        if next != shape.binder_count {
            return Err(bad(
                "qualified domain contains an unrepresented original class",
            ));
        }
        self.fee(rows.saturating_add(actuals.len()))?;
        let mapped = |binder: u32| {
            permutation
                .get(binder as usize)
                .copied()
                .flatten()
                .ok_or_else(|| bad("qualified canonical permutation omits an original class"))
        };
        for row in &mut shape.nominal_arguments {
            row.binder = mapped(row.binder)?;
        }
        for row in &mut shape.slots {
            row.binder = mapped(row.binder)?;
        }
        for row in &mut shape.retained {
            row.binder = mapped(row.binder)?;
        }
        for actual in &mut actuals {
            if let LifeNominalActual::Lifetime(binder) = actual {
                *binder = mapped(*binder)?;
            }
        }
        if let Type::BorrowedStorage { life, .. } = hint {
            self.copy_shape(life)?;
            self.copy_shape(&shape)?;
            if life != &shape {
                return Err(bad(
                    "existing nominal qualification differs from its complete original fields",
                ));
            }
        }
        let ty = if shape == empty() && !matches!(hint, Type::BorrowedStorage { .. }) {
            self.physical(storage)?
        } else {
            Type::BorrowedStorage {
                storage: Box::new(self.physical(storage)?),
                life: shape,
            }
        };
        Ok(LifeNominalInstance {
            declaration: *declaration,
            ty,
            arguments: actuals,
        })
    }

    fn qualified_field(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        coordinates: &[SemanticActualRecipe],
        recipe: &SemanticTypeRecipe,
        actual: &Type,
        field: PlaceProjection,
        parent: &mut StorageLifeShape,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        let use_definition = self.instance(scope)?.definition;
        let recipe = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .or_else(|| self.ledger.frozen.as_ref().map(|x| &x.original))
                .ok_or_else(|| bad("qualified nominal lacks original domain proof"))?;
            crate::semantics::instantiate::ty(
                self.input,
                proof,
                recipe,
                crate::semantics::instantiate::DomainMapping {
                    source: definition,
                    use_definition,
                    arguments: coordinates,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?
        };
        let value = self.ty(scope, &recipe, Some(actual), depth + 1)?;
        let SemanticTypeValue::Resolved { life, classes, .. } = value else {
            return Err(bad("qualified field remained unresolved"));
        };
        if life.binder_count as usize != classes.len() {
            return Err(bad("qualified field omits its complete domain"));
        }
        self.fee(classes.len())?;
        let mut map = Vec::new();
        for image in classes {
            let SemanticClassExpr::Bound { depth: 0, index } = image else {
                return Err(bad(
                    "qualified field class is not grounded in an original full actual",
                ));
            };
            if index >= parent.binder_count {
                return Err(bad("qualified field escapes its original class domain"));
            }
            map.push(index);
        }
        inherit_child_shape(
            parent,
            &life,
            LifeTypeProjection::Place(field),
            &map,
            self.limits,
            self.left,
        )?;
        for slot in &life.slots {
            self.fee(slot.path.len().saturating_add(2))?;
            if parent.slots.len() >= self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "qualified physical slot limit exceeded",
                    None,
                ));
            }
            let mut path = vec![field];
            path.extend_from_slice(&slot.path);
            let binder = *map
                .get(slot.binder as usize)
                .ok_or_else(|| bad("qualified physical slot leaves child domain"))?;
            parent.slots.push(StorageLifeSlot {
                path,
                binder,
                mode: slot.mode,
            });
        }
        Ok(())
    }
}
