//! Instantiate original fields in the nominal declaration's mixed domain.
//! No synthetic function instance or actual field layout supplies a recipe.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Check<'_, '_> {
    pub(super) fn actual_nominal_field(&self, parent: &Value, field: u32,
        b: &mut Budget, depth: usize) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth { return Err(unknown("actual stored field class path exceeds its finite depth")); }
        let (definition, mut arguments) = self.nominal_inventory_source(&parent.ty, b)?;
        // The inventory uses local directory atoms. Rebind every full mixed
        // occurrence to this real whole value without merging its images.
        let image = |class: &Class| -> CompileResult<Class> {
            let Class::Nominal(_, binder) = class else {
                return Err(fail("nominal field inventory has another original domain coordinate"));
            };
            parent.classes.get(*binder as usize).cloned()
                .ok_or_else(|| fail("nominal field source omits its complete parent class"))
        };
        for argument in &mut arguments {
            match argument {
                Argument::Life(class) => { b.spend(1, WORK)?; *class = image(class)?; }
                Argument::Type(value) => for class in &mut value.classes { b.spend(1, WORK)?; *class = image(class)?; },
                Argument::Const { .. } => {}
            }
        }
        let SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Product { fields }, .. } =
            &self.inventory.definition(definition, b)?.body else {
                return Err(fail("actual stored field has another original product identity"));
            };
        let recipe = &fields.get(field as usize)
            .ok_or_else(|| fail("actual stored field leaves its complete original product"))?.ty;
        let value = self.nominal_field_value(definition, &arguments, recipe, b, depth + 1)?;
        if !geometry::same_type(&value.ty, self.borrow_field_type(&parent.ty, &[field], b)?, b)? {
            return Err(fail("actual stored field differs from its complete original substitution"));
        }
        Ok(value)
    }

    fn nominal_life(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        recipe: &SemanticLifeRecipe,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        b.spend(1, WORK)?;
        let SemanticLifeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        }) = recipe
        else {
            return Err(unknown(
                "nominal field life needs its exact lexical or derived occurrence",
            ));
        };
        if !matches!(
            self.inventory
                .definition(owner, b)?
                .parameters
                .get(*ordinal as usize),
            Some(SemanticFormal::Lifetime)
        ) {
            return Err(fail(
                "nominal field life changes its full mixed formal kind",
            ));
        }
        let Some(Argument::Life(class)) = args.get(*ordinal as usize) else {
            return Err(fail("nominal field life has no actual at its full ordinal"));
        };
        Ok(class.clone())
    }

    fn nominal_constant(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        recipe: &SemanticConstRecipe,
        carrier: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Constant> {
        b.spend(1, WORK)?;
        let value = match recipe {
            SemanticConstRecipe::Value(value) => {
                crate::dyn_metadata::recipe(recipe, b)?;
                value.clone()
            }
            SemanticConstRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                let definition = self.inventory.definition(owner, b)?;
                let Some(SemanticFormal::Const { carrier: source }) =
                    definition.parameters.get(*ordinal as usize)
                else {
                    return Err(fail("nominal constant changes its original formal kind"));
                };
                let source = self.nominal_field_value(owner, args, source, b, depth + 1)?;
                let Some(Argument::Const {
                    carrier: actual,
                    value,
                }) = args.get(*ordinal as usize)
                else {
                    return Err(fail("nominal constant omits its original mixed actual"));
                };
                if !empty(&source.life)
                    || !source.classes.is_empty()
                    || source.ty != *actual
                    || actual != carrier
                {
                    return Err(fail(
                        "nominal constant changes its complete declared carrier",
                    ));
                }
                value.clone()
            }
            SemanticConstRecipe::IntegerLiteral(text) if carrier == &Type::Usize => {
                b.spend(text.len(), WORK)?;
                let digits = text.strip_suffix("usize").unwrap_or(text);
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(unknown(
                        "nominal extent literal requires its complete numeric spelling interpretation",
                    ));
                }
                Constant::Usize(
                    digits
                        .parse()
                        .map_err(|_| fail("nominal extent exceeds its declared usize carrier"))?,
                )
            }
            _ => {
                return Err(unknown(
                    "nominal constant expression requires its original carrier-aware evaluation",
                ));
            }
        };
        if value.ty() != *carrier {
            return Err(fail("nominal constant has another exact carrier"));
        }
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    fn nominal_mixed_actuals(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        definition: SemanticDefinitionId,
        arguments: &[SemanticActualRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Argument>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("nominal mixed actuals exceed their finite depth"));
        }
        let declaration = self.inventory.definition(definition, b)?;
        if declaration.parameters.len() != arguments.len() {
            return Err(fail("nested nominal omits original mixed arguments"));
        }
        let mut actuals = Vec::new();
        for (formal, argument) in declaration.parameters.iter().zip(arguments) {
            let actual = match (formal, argument) {
                (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(ty)) => {
                    Argument::Type(self.nominal_field_value(owner, args, ty, b, depth + 1)?)
                }
                (SemanticFormal::Lifetime, SemanticActualRecipe::Life(life)) => {
                    Argument::Life(self.nominal_life(owner, args, life, b)?)
                }
                (
                    SemanticFormal::Const { carrier: expected },
                    SemanticActualRecipe::Const { carrier, value },
                ) => {
                    let carrier = self.nominal_field_value(owner, args, carrier, b, depth + 1)?;
                    if !empty(&carrier.life) || !carrier.classes.is_empty() {
                        return Err(fail(
                            "nested nominal constant carrier has persistent classes",
                        ));
                    }
                    let expected =
                        self.nominal_field_value(definition, &actuals, expected, b, depth + 1)?;
                    if expected != carrier {
                        return Err(fail(
                            "nested nominal constant differs from its original dependent carrier",
                        ));
                    }
                    let constant =
                        self.nominal_constant(owner, args, value, &carrier.ty, b, depth + 1)?;
                    Argument::Const {
                        carrier: carrier.into_type(b)?,
                        value: constant,
                    }
                }
                _ => {
                    return Err(fail("nested nominal changes original mixed argument kind"));
                }
            };
            self.push_value(&mut actuals, actual, b)?;
        }
        Ok(actuals)
    }

    pub(super) fn nominal_trait_actuals(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        application: &SemanticTraitRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Argument>> {
        let scope =
            original::declaration_scope(self.program, application.declaration, self.inventory, b)?;
        let declaration = self.inventory.definition(scope, b)?;
        if !matches!(
            declaration.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
        ) {
            return Err(fail(
                "nominal trait premise changes its original declaration kind",
            ));
        }
        self.nominal_mixed_actuals(owner, args, scope, &application.arguments, b, depth + 1)
    }

    pub(super) fn nominal_field_value(
        &self,
        owner: SemanticDefinitionId,
        args: &[Argument],
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "nominal field substitution exceeds its complete depth",
            ));
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty)
                if ty.is_b0_scalar() || matches!(ty, Type::ByteBuffer | Type::ForeignHandle) =>
            {
                crate::dyn_metadata::ty(ty, b)?;
                Ok(value::scalar(ty.clone()))
            }
            SemanticTypeRecipe::Exact(exact) if empty(&exact.life) && exact.classes.is_empty() => {
                self.zero_data(&exact.ty, b, depth + 1)?;
                crate::dyn_metadata::ty(&exact.ty, b)?;
                Ok(value::scalar(exact.ty.clone()))
            }
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                if !matches!(
                    self.inventory
                        .definition(owner, b)?
                        .parameters
                        .get(*ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("nominal field changes its original Type formal kind"));
                }
                let Some(Argument::Type(value)) = args.get(*ordinal as usize) else {
                    return Err(fail("nominal field omits its full original Type actual"));
                };
                value.copy(b)
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } => {
                let receiver = self.nominal_field_value(owner, args, receiver, b, depth + 1)?;
                let arguments =
                    self.nominal_trait_actuals(owner, args, application, b, depth + 1)?;
                self.associated_actual(
                    &receiver,
                    application.declaration,
                    &arguments,
                    *member,
                    b,
                    depth + 1,
                )
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let actuals =
                    self.nominal_mixed_actuals(owner, args, *definition, arguments, b, depth + 1)?;
                self.nominal_value(*definition, &actuals, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
                ..
            }
            | SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let class = self.nominal_life(owner, args, life, b)?;
                let child = self.nominal_field_value(owner, args, inner, b, depth + 1)?;
                let mutable = match recipe {
                    SemanticTypeRecipe::Address { mutable, .. } => *mutable,
                    _ => false,
                };
                let mut result = value::address(mutable, class, child, b)?;
                if matches!(recipe, SemanticTypeRecipe::Desc { .. }) {
                    let geometry = result.geometry_mut(b)?;
                let Type::Address { inner, .. } = std::mem::replace(&mut geometry.ty, Type::Unit) else {
                        return Err(fail("nominal description loses its child"));
                    };
                    geometry.ty = Type::Desc(inner);
                    geometry.life.slots[0].mode = StoredBorrowMode::WritableDescription;
                }
                Ok(result)
            }
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner)
            | SemanticTypeRecipe::Slice(inner) => {
                let child = self.nominal_field_value(owner, args, inner, b, depth + 1)?;
                if !empty(&child.life) || !child.classes.is_empty() {
                    return Err(unknown(
                        "nominal opaque field needs its complete child scope transport",
                    ));
                }
                let ty = match recipe {
                    SemanticTypeRecipe::Managed(_) => Type::Managed(Box::new(child.into_type(b)?)),
                    SemanticTypeRecipe::OwnedStore(_) => Type::OwnedStore {
                        element: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::Mutex(_) => Type::Mutex {
                        payload: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::Registration(_) => Type::Registration {
                        environment: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::AsyncRegistration(_) => Type::AsyncRegistration {
                        environment: Box::new(child.into_type(b)?),
                    },
                    SemanticTypeRecipe::Slice(_) => Type::Slice {
                        element: Box::new(child.into_type(b)?),
                    },
                    _ => return Err(fail("nominal field constructor changed family")),
                };
                Ok(value::scalar(ty))
            }
            SemanticTypeRecipe::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    let child = self.nominal_field_value(owner, args, field, b, depth + 1)?;
                    if !empty(&child.life) || !child.classes.is_empty() {
                        return Err(unknown(
                            "nominal tuple field needs complete nested child class embedding",
                        ));
                    }
                    self.push_value(&mut types, child.into_type(b)?, b)?;
                }
                Ok(value::scalar(if types.is_empty() {
                    Type::Unit
                } else {
                    Type::Tuple(types)
                }))
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.nominal_field_value(owner, args, error, b, depth + 1)?;
                let result = self.nominal_field_value(owner, args, result, b, depth + 1)?;
                if !empty(&error.life)
                    || !error.classes.is_empty()
                    || !empty(&result.life)
                    || !result.classes.is_empty()
                {
                    return Err(unknown(
                        "nominal error arm field needs complete active child class transport",
                    ));
                }
                Ok(value::scalar(Type::ErrorUnion {
                    error: Box::new(error.into_type(b)?),
                    result: Box::new(result.into_type(b)?),
                }))
            }
            SemanticTypeRecipe::Array { element, length } => {
                let element = self.nominal_field_value(owner, args, element, b, depth + 1)?;
                if !empty(&element.life) || !element.classes.is_empty() {
                    return Err(unknown(
                        "nominal array field needs all child occurrence mappings",
                    ));
                }
                let Constant::Usize(length) =
                    self.nominal_constant(owner, args, length, &Type::Usize, b, depth + 1)?
                else {
                    return Err(fail("nominal extent loses its usize carrier"));
                };
                let length = u32::try_from(length).map_err(|_| {
                    original::resource("nominal array extent exceeds its physical representation")
                })?;
                Ok(value::scalar(Type::Array {
                    element: Box::new(element.into_type(b)?),
                    length,
                }))
            }
            _ => Err(unknown(
                "original nominal field needs complete scoped constructor substitution",
            )),
        }
    }
}
