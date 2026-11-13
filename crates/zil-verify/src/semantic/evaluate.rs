//! Interpretation of the original recipe in one exact lexical environment.
//! Unimplemented forms fail before their claimed type can enter class flow.
use super::*;
impl CheckedContext {
    pub(crate) fn evaluate_type(
        &self,
        module: &TypedModule,
        recipe: &SemanticTypeRecipe,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        dynamic_metadata::recipe(recipe, b)?;
        let key = DependencyKey {
            kind: QueryKind::Type,
            token: active.fresh(b)?,
        };
        active.with_query(key, b, |active, b| {
            self.type_inner(module, recipe, environment, external, active, b)
        })
    }
    fn type_inner(
        &self,
        module: &TypedModule,
        recipe: &SemanticTypeRecipe,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        match recipe {
            SemanticTypeRecipe::OriginalType{scope,source}=>{
                let frame=self.frame(environment,*scope,b)?;
                let definition=frame.definition;
                if let Some(value)=self.original_closure_value(module,definition,source,frame.key,environment,b)? {return Ok(value);}
                let recipe=self.original.borrow().as_ref().ok_or_else(||bad("original type body has not been independently checked"))?.type_recipe(module,definition,source,environment,b)?;
                self.evaluate_type(module,&recipe,environment,external,active,b)
            }
            SemanticTypeRecipe::Atom(ty) => {
                if !(ty.is_b0_scalar()
                    || matches!(
                        ty,
                        Type::ForeignHandle
                            | Type::CWideString
                            | Type::ByteBuffer
                            | Type::SyncCell(_)
                    ))
                {
                    return Err(bad("Atom is not a true atomic type"));
                }
                dynamic_metadata::ty(ty, b)?;
                Ok(CheckedType {
                    ty: ty.clone(),
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                })
            }
            SemanticTypeRecipe::Exact(exact) => {
                let mut classes = Vec::new();
                for class in &exact.classes {
                    classes.push(self.life(module, class, environment, b)?);
                }
                self.checked_value(module, &exact.ty, &exact.life, classes, b)
            }
            SemanticTypeRecipe::Formal(reference) => {
                match self.formal(environment, reference, b)? {
                    CheckedActual::Type(value) => {
                        dynamic_metadata::ty(&value.ty, b)?;
                        b.spend(
                            value
                                .classes
                                .len()
                                .saturating_add(value.origins.len())
                                .saturating_add(value.statics.len()),
                        )?;
                        Ok(value.clone())
                    }
                    _ => Err(bad("Type recipe selects a different formal kind")),
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                match self.environment_actual(environment, *scope, *ordinal, b)? {
                    CheckedActual::Type(value) => {
                        dynamic_metadata::ty(&value.ty, b)?;
                        b.spend(
                            value
                                .classes
                                .len()
                                .saturating_add(value.origins.len())
                                .saturating_add(value.statics.len()),
                        )?;
                        Ok(value.clone())
                    }
                    _ => Err(bad(
                        "Type recipe selects a different sealed environment kind",
                    )),
                }
            }
            SemanticTypeRecipe::SelfType { scope } => {
                let value = self
                    .frame(environment, *scope, b)?
                    .self_type
                    .as_ref()
                    .ok_or_else(|| bad("SelfType has no independently selected receiver"))?;
                dynamic_metadata::ty(&value.ty, b)?;
                b.spend(value.classes.len())?;
                Ok(value.clone())
            }
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                let child = self.evaluate_type(module, inner, environment, external, active, b)?;
                let mode = match recipe {
                    SemanticTypeRecipe::Address { mutable: true, .. } => {
                        StoredBorrowMode::ExclusiveAddress
                    }
                    SemanticTypeRecipe::Address { .. } => StoredBorrowMode::SharedAddress,
                    _ => StoredBorrowMode::ReadOnlyDescription,
                };
                let outer = match lifetime {
                    SemanticOuterLifeRecipe::Class(life) => {
                        Some(self.life(module, life, environment, b)?)
                    }
                    SemanticOuterLifeRecipe::Static => None,
                };
                dynamic_witness::semantic_external::construct(
                    self,
                    environment,
                    dynamic_witness::semantic_external::Constructor::Address { mode, outer },
                    &[child],
                    active,
                    b,
                )
            }
            SemanticTypeRecipe::Tuple(recipes) => {
                b.spend(recipes.len())?;
                let mut fields = Vec::with_capacity(recipes.len());
                for recipe in recipes {
                    fields.push(self.evaluate_type(
                        module,
                        recipe,
                        environment,
                        external,
                        active,
                        b,
                    )?);
                }
                dynamic_witness::semantic_external::construct(
                    self,
                    environment,
                    dynamic_witness::semantic_external::Constructor::Tuple,
                    &fields,
                    active,
                    b,
                )
            }
            SemanticTypeRecipe::Managed(inner) => {
                let child = self.evaluate_type(module, inner, environment, external, active, b)?;
                dynamic_witness::semantic_external::construct(
                    self,
                    environment,
                    dynamic_witness::semantic_external::Constructor::Managed,
                    &[child],
                    active,
                    b,
                )
            }
            SemanticTypeRecipe::Array { element, length } => {
                let child =
                    self.evaluate_type(module, element, environment, external, active, b)?;
                let carrier = CheckedType {
                    ty: Type::Usize,
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                };
                let length = self.evaluate_constant(
                    module,
                    length,
                    &carrier,
                    environment,
                    external,
                    active,
                    b,
                )?;
                let Constant::Usize(length) = length else {
                    return Err(bad("array length has a different actual carrier"));
                };
                let length = u32::try_from(length)
                    .map_err(|_| bad("array length exceeds the finite IR domain"))?;
                let mut value = CheckedType {
                    ty: Type::Array {
                        element: Box::new(child.ty.clone()),
                        length,
                    },
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                };
                self.attach_child(
                    environment,
                    &mut value,
                    &child,
                    vec![LifeTypeProjection::Element],
                    None,
                    b,
                )?;
                value.life.binder_count = value.classes.len() as u32;
                Ok(value)
            }
            SemanticTypeRecipe::Slice(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                let child = self.evaluate_type(module, inner, environment, external, active, b)?;
                let (ty, projection) = match recipe {
                    SemanticTypeRecipe::Slice(_) => (
                        Type::Slice {
                            element: Box::new(child.ty.clone()),
                        },
                        LifeTypeProjection::Element,
                    ),
                    SemanticTypeRecipe::OwnedStore(_) => (
                        Type::OwnedStore {
                            element: Box::new(child.ty.clone()),
                        },
                        LifeTypeProjection::Element,
                    ),
                    SemanticTypeRecipe::Mutex(_) => (
                        Type::Mutex {
                            payload: Box::new(child.ty.clone()),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                    SemanticTypeRecipe::Registration(_) => (
                        Type::Registration {
                            environment: Box::new(child.ty.clone()),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                    _ => (
                        Type::AsyncRegistration {
                            environment: Box::new(child.ty.clone()),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                };
                let mut value = CheckedType {
                    ty,
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                };
                self.attach_child(environment, &mut value, &child, vec![projection], None, b)?;
                value.life.binder_count = value.classes.len() as u32;
                Ok(value)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.evaluate_type(module, error, environment, external, active, b)?;
                let result =
                    self.evaluate_type(module, result, environment, external, active, b)?;
                let mut value = CheckedType {
                    ty: Type::ErrorUnion {
                        error: Box::new(error.ty.clone()),
                        result: Box::new(result.ty.clone()),
                    },
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                };
                for (arm, child) in [(true, error), (false, result)] {
                    let projection = PlaceProjection::ErrorArm(arm);
                    self.attach_child(
                        environment,
                        &mut value,
                        &child,
                        vec![LifeTypeProjection::Place(projection)],
                        Some(projection),
                        b,
                    )?;
                }
                value.life.binder_count = value.classes.len() as u32;
                Ok(value)
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                member,
            } => {
                let receiver =
                    self.evaluate_type(module, receiver, environment, external, active, b)?;
                let application =
                    self.application(module, application, environment, external, active, b)?;
                match external.query(
                    module,
                    self,
                    environment,
                    &ExternalRequest::AssociatedType {
                        receiver,
                        application,
                        member: *member,
                    },
                    RequirementMode::ActualObligation,
                    active,
                    b,
                )? {
                    ExternalValue::Type(value) => Ok(value),
                    _ => Err(bad("associated Type proof returned another kind")),
                }
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => self.nominal(
                module,
                *definition,
                arguments,
                environment,
                external,
                active,
                b,
            ),
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment: sealed,
            } => self.closure_type(
                module,
                *definition,
                arguments,
                sealed,
                environment,
                external,
                active,
                b,
            ),
            SemanticTypeRecipe::UnresolvedNamed(_) | SemanticTypeRecipe::Unsupported(_) => {
                Err(bad("executable semantic recipe retains unresolved syntax"))
            }
            _ => Err(unsupported(
                "semantic recipe requires its complete nominal/opaque/qualified interpreter",
                None,
            )),
        }
    }
    pub(super) fn checked_value(
        &self,
        module: &TypedModule,
        ty: &Type,
        life: &StorageLifeShape,
        classes: Vec<ClassKey>,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        dynamic_metadata::ty(ty, b)?;
        let count =
            u32::try_from(classes.len()).map_err(|_| limit("semantic class domain exceeds u32"))?;
        let scope = DynInstantiationScope {
            fixed_binder_count: count,
            invocation_binder_count: 0,
        };
        let value = DynTypeValue {
            ty: ty.clone(),
            life: life.clone(),
            classes: (0..count).map(StorageBinder::Fixed).collect(),
        };
        dynamic_witness::check_type_value(module, &value, scope, b)?;
        let mut origins = classes
            .iter()
            .map(|source| IntrinsicOrigin::DerivedClass {
                source: source.clone(),
            })
            .collect();
        if let Type::Closure(id)=ty {
            if let Some(domain)=self.closure_domain(*id,b)? {
                if domain.life!=*life||domain.origins.len()!=classes.len() {return Err(bad("Closure claim differs from its independently reconstructed complete domain"));}
                origins=domain.origins;
            }
        }
        Ok(CheckedType {
            ty: ty.clone(),
            life: life.clone(),
            classes,
            origins,
            statics: vec![],
        })
    }
    fn attach_child(
        &self,
        environment: &EvalEnv,
        parent: &mut CheckedType,
        child: &CheckedType,
        prefix: Vec<LifeTypeProjection>,
        physical: Option<PlaceProjection>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(
            child
                .classes
                .len()
                .saturating_add(child.life.slots.len())
                .saturating_add(child.life.retained.len())
                .saturating_add(child.life.nominal_arguments.len())
                .saturating_add(child.statics.len()),
        )?;
        let mut map = Vec::new();
        for (i, key) in child.classes.iter().enumerate() {
            let mut found = None;
            for (j, current) in parent.classes.iter().enumerate() {
                if self.same(environment, key, current, b)?
                    && self.same_origin(environment, &parent.origins[j], &child.origins[i], b)?
                {
                    found = Some(j as u32);
                    break;
                }
            }
            let index = match found {
                Some(index) => index,
                None => {
                    let index = u32::try_from(parent.classes.len())
                        .map_err(|_| limit("constructed semantic domain exceeds u32"))?;
                    parent.classes.push(key.clone());
                    parent.origins.push(
                        child
                            .origins
                            .get(i)
                            .cloned()
                            .ok_or_else(|| bad("child intrinsic origin is absent"))?,
                    );
                    index
                }
            };
            map.push(index);
        }
        if let Some(field) = physical {
            for slot in &child.life.slots {
                let mut path = vec![field];
                path.extend_from_slice(&slot.path);
                parent.life.slots.push(StorageLifeSlot {
                    path,
                    binder: *map
                        .get(slot.binder as usize)
                        .ok_or_else(|| bad("child slot binder is outside its domain"))?,
                    mode: slot.mode,
                });
            }
        }
        // Every child class remains a separately selected intrinsic occurrence,
        // even when two outside parent images have proved equal.
        for (i, binder) in map.iter().enumerate() {
            parent.life.retained.push(StorageRetainedBinding {
                path: prefix.clone(),
                class: i as u32,
                binder: *binder,
            });
        }
        for row in &child.life.nominal_arguments {
            let mut path = prefix.clone();
            path.extend_from_slice(&row.path);
            parent.life.nominal_arguments.push(NominalLifeArgument {
                path,
                declaration: row.declaration,
                argument: row.argument,
                binder: *map
                    .get(row.binder as usize)
                    .ok_or_else(|| bad("nominal child binder is absent"))?,
            });
        }
        for row in &child.statics {
            match row {
                StaticOccurrence::Direct { path, leaf, mode } => {
                    let mut full = prefix.clone();
                    full.extend_from_slice(path);
                    parent.statics.push(StaticOccurrence::Direct {
                        path: full,
                        leaf: leaf.clone(),
                        mode: *mode,
                    });
                }
                row @ StaticOccurrence::Logical { .. } => parent.statics.push(row.clone()),
            }
        }
        Ok(())
    }
    pub(crate) fn evaluate_actual(
        &self,
        module: &TypedModule,
        recipe: &SemanticActualRecipe,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedActual> {
        b.spend(1)?;
        Ok(match recipe {
            SemanticActualRecipe::Infer => return Err(unsupported("Infer actual requires its checked original caller body and call/formal slot", None)),
            SemanticActualRecipe::Type(recipe) => CheckedActual::Type(self.evaluate_type(
                module,
                recipe,
                environment,
                external,
                active,
                b,
            )?),
            SemanticActualRecipe::Life(recipe) => {
                CheckedActual::Life(self.life(module, recipe, environment, b)?)
            }
            SemanticActualRecipe::Const { carrier, value } => {
                let carrier =
                    self.evaluate_type(module, carrier, environment, external, active, b)?;
                let value = self.evaluate_constant(
                    module,
                    value,
                    &carrier,
                    environment,
                    external,
                    active,
                    b,
                )?;
                CheckedActual::Const { carrier, value }
            }
        })
    }
    pub(crate) fn evaluate_signature(
        &self,
        module: &TypedModule,
        recipe: &SemanticSignatureRecipe,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedSignature> {
        dynamic_metadata::recipe(recipe, b)?;
        let mut environment = environment.clone();
        let mut bound = Vec::new();
        if recipe.bound_lifetimes > 0 {
            b.spend(recipe.bound_lifetimes as usize)?;
            let frame = active.fresh(b)?;
            bound = (0..recipe.bound_lifetimes)
                .map(|index| ClassKey::Bound { frame, index })
                .collect();
            environment.bounds.insert(
                0,
                BoundFrame {
                    token: frame,
                    classes: bound.clone(),
                },
            );
        }
        b.spend(recipe.invocation_lifetimes as usize)?;
        let frame = active.fresh(b)?;
        let invocation = (0..recipe.invocation_lifetimes)
            .map(|index| ClassKey::Invocation { frame, index })
            .collect::<Vec<_>>();
        environment.invocations = invocation.clone();
        let mut parameters = Vec::new();
        for parameter in &recipe.parameters {
            parameters.push((
                self.evaluate_type(module, &parameter.ty, &environment, external, active, b)?,
                parameter.mode,
            ));
        }
        let result =
            self.evaluate_type(module, &recipe.result, &environment, external, active, b)?;
        Ok(CheckedSignature {
            parameters,
            result,
            result_mode: recipe.result_mode,
            call_kind: recipe.call_kind,
            bound,
            invocation,
        })
    }
    pub(super) fn application(
        &self,
        module: &TypedModule,
        recipe: &SemanticTraitRecipe,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedApplication> {
        let mut arguments = Vec::new();
        for argument in &recipe.arguments {
            arguments.push(self.evaluate_actual(
                module,
                argument,
                environment,
                external,
                active,
                b,
            )?);
        }
        Ok(CheckedApplication {
            declaration: recipe.declaration,
            arguments,
        })
    }
}
