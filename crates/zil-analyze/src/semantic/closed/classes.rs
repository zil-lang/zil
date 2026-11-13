//! Original class relations and actual header embeddings. These equations
//! never create a memory root, loan, borrow permission or cleanup capability.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Check<'_, '_> {
    pub(super) fn life(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticLifeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Class> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original lifetime relation exceeds its finite depth",
            ));
        }
        let expression = match recipe {
            SemanticLifeRecipe::Formal(reference) => SemanticClassExpr::Formal {
                scope: reference.scope,
                ordinal: reference.ordinal,
            },
            SemanticLifeRecipe::Implicit { scope, ordinal } => SemanticClassExpr::Implicit {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Environment { scope, ordinal } => SemanticClassExpr::Environment {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Invocation(ordinal) => {
                let declaration = self.inventory.definition(instance.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &declaration.body
                else {
                    return Err(fail("invocation lifetime has no original signature"));
                };
                if *ordinal >= signature.invocation_lifetimes {
                    return Err(fail("invocation lifetime exceeds its original frame"));
                }
                return Ok(Class::Invocation(instance.id, *ordinal));
            }
            SemanticLifeRecipe::IntrinsicClass {
                operand: SemanticRecipeOperand::Environment { scope, ordinal },
                path,
                class,
            } => {
                let context = self.ancestor(instance, *scope, b)?;
                return value::class(
                    context,
                    &SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Environment(*ordinal),
                        path: path.clone(),
                        class: *class,
                    },
                    self.inventory,
                    b,
                    depth + 1,
                );
            }
            _ => {
                return Err(unknown(
                    "original derived or quantified lifetime needs its complete actual occurrence relation",
                ));
            }
        };
        value::class(instance, &expression, self.inventory, b, depth + 1)
    }
    pub(super) fn recipe_value(
        &self,
        instance: &SemanticInstance,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "complete original type value exceeds its finite depth",
            ));
        }
        match recipe {
            SemanticTypeRecipe::OriginalType { scope, source } => self.original.source_value(
                instance,
                *scope,
                source,
                self.program,
                self.inventory,
                b,
            ),
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(value::scalar(ty.clone())),
            SemanticTypeRecipe::Formal(reference) => {
                let context = self.ancestor(instance, reference.scope, b)?;
                if !matches!(
                    self.inventory
                        .definition(context.definition, b)?
                        .parameters
                        .get(reference.ordinal as usize),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("original type value changes its full formal kind"));
                }
                let Some(SemanticActual::Type(actual)) =
                    context.actuals.get(reference.ordinal as usize)
                else {
                    return Err(fail("original type value omits a complete Type actual"));
                };
                value::resolved(context, actual, self.inventory, b, depth + 1)
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                let context = self.ancestor(instance, *scope, b)?;
                let declaration = self.inventory.definition(context.definition, b)?;
                if !matches!(
                    declaration
                        .environment
                        .get(*ordinal as usize)
                        .map(|formal| &formal.kind),
                    Some(SemanticFormal::Type { .. })
                ) {
                    return Err(fail("original environment type changes its declared kind"));
                }
                let parent = self.inventory.instance(
                    context
                        .parent
                        .ok_or_else(|| fail("original environment has no real creator context"))?,
                    b,
                )?;
                let Some(SemanticActual::Type(actual)) = context
                    .environment
                    .as_ref()
                    .and_then(|environment| environment.actuals.get(*ordinal as usize))
                else {
                    return Err(fail("original environment type omits a sealed actual"));
                };
                value::resolved(parent, actual, self.inventory, b, depth + 1)
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(life),
                inner,
            } => {
                let class = self.life(instance, life, b, depth + 1)?;
                let inner = self.recipe_value(instance, inner, b, depth + 1)?;
                value::address(*mutable, class, inner, b)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let declaration = self.inventory.definition(*definition, b)?;
                if declaration.parameters.len() != arguments.len() {
                    return Err(fail("nominal original type omits full mixed arguments"));
                }
                let mut actuals = Vec::new();
                for (formal, recipe) in declaration.parameters.iter().zip(arguments) {
                    let actual = self.argument_recipe(instance, formal, recipe, b, depth + 1)?;
                    self.push_value(&mut actuals, actual, b)?;
                }
                // The full original declaration was checked with rigid mixed
                // formals. Fields are instantiated through the same evaluator.
                self.nominal_value(*definition, &actuals, b, depth + 1)
            }
            SemanticTypeRecipe::Tuple(fields) => {
                if fields.is_empty() {
                    return Ok(value::scalar(Type::Unit));
                }
                let mut types = Vec::new();
                for field in fields {
                    let field = self.recipe_value(instance, field, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "original tuple requires its complete retained class embedding",
                        ));
                    }
                    self.push_value(&mut types, field.ty, b)?;
                }
                Ok(value::scalar(Type::Tuple(types)))
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                if *definition != instance.definition {
                    return Err(unknown(
                        "opaque value needs its exact original definition specialization",
                    ));
                }
                let declaration = self.inventory.definition(*definition, b)?;
                if arguments.len() != declaration.parameters.len()
                    || arguments.len() != instance.actuals.len()
                {
                    return Err(fail(
                        "opaque original type omits mixed specialization arguments",
                    ));
                }
                for ((formal, recipe), actual) in declaration
                    .parameters
                    .iter()
                    .zip(arguments)
                    .zip(&instance.actuals)
                {
                    if self.argument_recipe(instance, formal, recipe, b, depth + 1)?
                        != self.actual_value(instance, actual, b, depth + 1)?
                    {
                        return Err(fail("opaque result changes its original specialization"));
                    }
                }
                self.recipe_value(instance, hidden, b, depth + 1)
            }
            _ => {
                // Existing closed scalar/closure recipes keep their complete
                // independent consumer. This does not turn None into zero.
                let ty = self.ty_closed(instance, recipe, b, depth + 1)?;
                if let Type::Closure(id) = ty {
                    let count = self
                        .module
                        .closures
                        .get(id.0 as usize)
                        .and_then(|definition| definition.storage_life.as_ref())
                        .ok_or_else(|| unknown("closure has no complete class inventory"))?
                        .fixed_binder_count;
                    if count != 0 {
                        return Err(unknown(
                            "classful opaque recipe needs its complete original environment value",
                        ));
                    }
                }
                Ok(value::scalar(ty))
            }
        }
    }
    fn push_value<T>(&self, out: &mut Vec<T>, value: T, b: &mut Budget) -> CompileResult<()> {
        b.spend(out.len().saturating_add(1), WORK)?;
        out.try_reserve(1)
            .map_err(|_| fail("complete original value allocation failed"))?;
        out.push(value);
        Ok(())
    }
    pub(super) fn actual_value(
        &self,
        instance: &SemanticInstance,
        actual: &SemanticActual,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        match actual {
            SemanticActual::Type(actual) => Ok(Argument::Type(value::resolved(
                instance,
                actual,
                self.inventory,
                b,
                depth + 1,
            )?)),
            SemanticActual::Life(actual) => Ok(Argument::Life(value::class(
                instance,
                actual,
                self.inventory,
                b,
                depth + 1,
            )?)),
            SemanticActual::Const {
                ty: carrier,
                value: constant,
            } => {
                let carrier = value::resolved(instance, carrier, self.inventory, b, depth + 1)?;
                if !carrier.classes.is_empty() || constant.ty() != carrier.ty {
                    return Err(fail("semantic constant has another complete carrier"));
                }
                Ok(Argument::Const {
                    carrier: carrier.ty,
                    value: constant.clone(),
                })
            }
            _ => Err(fail(
                "Core actual remains in an executable original context",
            )),
        }
    }
    fn argument_recipe(
        &self,
        instance: &SemanticInstance,
        formal: &SemanticFormal,
        recipe: &SemanticActualRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        match (formal, recipe) {
            (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(recipe)) => Ok(
                Argument::Type(self.recipe_value(instance, recipe, b, depth + 1)?),
            ),
            (SemanticFormal::Lifetime, SemanticActualRecipe::Life(recipe)) => {
                Ok(Argument::Life(self.life(instance, recipe, b, depth + 1)?))
            }
            (
                SemanticFormal::Const { .. },
                SemanticActualRecipe::Const {
                    carrier,
                    value: SemanticConstRecipe::Value(constant),
                },
            ) => {
                let carrier = self.recipe_value(instance, carrier, b, depth + 1)?;
                if !carrier.classes.is_empty() || carrier.ty != constant.ty() {
                    return Err(fail("original constant recipe has another actual carrier"));
                }
                Ok(Argument::Const {
                    carrier: carrier.ty,
                    value: constant.clone(),
                })
            }
            _ => Err(unknown(
                "original mixed actual needs its complete carrier-aware expression substitution",
            )),
        }
    }
    fn nominal_value(
        &self,
        definition: SemanticDefinitionId,
        arguments: &[Argument],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let declaration = self.inventory.definition(definition, b)?;
        let SemanticDefinitionBody::Nominal { body, .. } = &declaration.body else {
            return Err(fail(
                "original nominal value selects another declaration kind",
            ));
        };
        let mut fields = Vec::new();
        match body {
            SemanticNominalBody::Product { fields: recipes } => {
                for field in recipes {
                    let value =
                        self.nominal_field(definition, arguments, &field.ty, b, depth + 1)?;
                    self.push_value(&mut fields, value, b)?;
                }
            }
            SemanticNominalBody::Enum { variants } => {
                for variant in variants {
                    for field in &variant.fields {
                        let value =
                            self.nominal_field(definition, arguments, field, b, depth + 1)?;
                        self.push_value(&mut fields, value, b)?;
                    }
                }
            }
        }
        value::nominal(definition, arguments, &fields, self.inventory, b, depth + 1)
    }
    fn nominal_field(
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
                "original nominal field value exceeds its complete depth",
            ));
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => Ok(value::scalar(ty.clone())),
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) => {
                let Some(Argument::Type(value)) = args.get(*ordinal as usize) else {
                    return Err(fail("original field changes its full Type formal kind"));
                };
                crate::dyn_metadata::ty(&value.ty, b)?;
                crate::dyn_metadata::shape(&value.life, b)?;
                Ok(value.clone())
            }
            SemanticTypeRecipe::Address {
                mutable,
                lifetime:
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    })),
                inner,
            } => {
                let Some(Argument::Life(class)) = args.get(*ordinal as usize) else {
                    return Err(fail("original field changes its full Life formal kind"));
                };
                let inner = self.nominal_field(owner, args, inner, b, depth + 1)?;
                value::address(*mutable, class.clone(), inner, b)
            }
            _ => Err(unknown(
                "original nominal field needs its complete mixed type constructor substitution",
            )),
        }
    }
}
