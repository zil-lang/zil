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
        let previous=b.diagnostic_progress.as_ref().and_then(|progress|progress.recipe);
        if let Some(progress)=&mut b.diagnostic_progress{
            progress.recipes_started=progress.recipes_started.saturating_add(1);
            progress.recipe=Some((recipe as *const SemanticTypeRecipe as usize,std::mem::discriminant(recipe),environment.lexical.first().map(|frame|frame.definition),environment.lexical.first().map(|frame|frame.key),environment.at));
        }
        let result=(||{
            dynamic_metadata::recipe(recipe,b)?;
            let key=DependencyKey{kind:QueryKind::Type,token:active.fresh(b)?};
            active.with_query(key,b,|active,b|self.type_inner(module,recipe,environment,external,active,b))
        })();
        if let Some(progress)=&mut b.diagnostic_progress{
            if result.is_ok(){progress.recipes_completed=progress.recipes_completed.saturating_add(1);}
            progress.recipe=previous;
        }
        result
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
            SemanticTypeRecipe::OriginalType { scope, source } => {
                let frame = self.frame(environment, *scope, b)?;
                let definition = frame.definition;
                if !self.domain_only.get() {
                    let parameter=self.original.borrow().as_ref().ok_or_else(||bad("original source lacks its completed universal body"))?
                        .parameter_source(definition,source,b)?;
                    if let Some(parameter)=parameter {
                        let values=self.occurrences.borrow();
                        if let Some(value)=values.get(&(frame.key,OperandKey::Parameter(parameter))) {
                            value.charge_clone(b)?;
                            return Ok(value.clone());
                        }
                    }
                    if let Some(value)=self.original_call_result(module,definition,source,frame.key,environment,active,b)?{
                        return Ok(value);
                    }
                    if let Some(value) = self.original_closure_value(
                        module,
                        definition,
                        source,
                        frame.key,
                        environment,
                        b,
                    )? {
                        return Ok(value);
                    }
                }
                let recipe = self
                    .original
                    .borrow()
                    .as_ref()
                    .ok_or_else(|| bad("original type body has not been independently checked"))?
                    .type_recipe(module, definition, source, environment, b)?;
                self.evaluate_type(module, &recipe, environment, external, active, b)
            }
            SemanticTypeRecipe::Dyn{owned,interface}=>self.evaluate_dyn(module,*owned,interface,environment,external,active,b),
            SemanticTypeRecipe::Code{kind,signature}=>{
                let signature=self.evaluate_signature(module,signature,environment,external,active,b)?;
                if *kind==SemanticCodeKind::ForeignCallback{self.foreign_callback_code(module,&signature,b)}else{self.ordinary_code(module,*kind,&signature,b)}
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
                Ok(CheckedType {type_cell: crate::semantic::CheckedTypeData::new(ty.clone()),
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
                self.construct_value(module, &exact.ty, &exact.life, classes, b)
            }
            SemanticTypeRecipe::Formal(reference) => {
                match self.formal(environment, reference, b)? {
                    CheckedActual::Type(value) => {
                        value.charge_clone(b)?;
                        Ok(value.clone())
                    }
                    _ => Err(bad("Type recipe selects a different formal kind")),
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                match self.environment_actual(environment, *scope, *ordinal, b)? {
                    CheckedActual::Type(value) => {
                        value.charge_clone(b)?;
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
                value.charge_clone(b)?;
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
                    _ if matches!(lifetime,SemanticOuterLifeRecipe::Static)=>StoredBorrowMode::ReadOnlyDescription,
                    _ => StoredBorrowMode::WritableDescription,
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
                let carrier = CheckedType {type_cell: crate::semantic::CheckedTypeData::new(Type::Usize),
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
                let mut value = CheckedType {type_cell: crate::semantic::CheckedTypeData::new(Type::Array {
                        element: Box::new(child.raw_type_copy(b)?),
                        length,
                    }),
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
                            element: Box::new(child.raw_type_copy(b)?),
                        },
                        LifeTypeProjection::Element,
                    ),
                    SemanticTypeRecipe::OwnedStore(_) => (
                        Type::OwnedStore {
                            element: Box::new(child.raw_type_copy(b)?),
                        },
                        LifeTypeProjection::Element,
                    ),
                    SemanticTypeRecipe::Mutex(_) => (
                        Type::Mutex {
                            payload: Box::new(child.raw_type_copy(b)?),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                    SemanticTypeRecipe::Registration(_) => (
                        Type::Registration {
                            environment: Box::new(child.raw_type_copy(b)?),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                    _ => (
                        Type::AsyncRegistration {
                            environment: Box::new(child.raw_type_copy(b)?),
                        },
                        LifeTypeProjection::Pointee,
                    ),
                };
                let mut value = CheckedType {type_cell: crate::semantic::CheckedTypeData::new(ty),
                    life: empty_life(),
                    classes: vec![],
                    origins: vec![],
                    statics: vec![],
                };
                self.attach_child(environment, &mut value, &child, vec![projection], None, b)?;
                value.life.binder_count = value.classes.len() as u32;
                Ok(value)
            }
            SemanticTypeRecipe::Task { result, retained }
            | SemanticTypeRecipe::MutexGuard {
                payload: result,
                retained,
            } => {
                let child = self.evaluate_type(module, result, environment, external, active, b)?;
                let checked = if matches!(recipe, SemanticTypeRecipe::Task { .. }) {
                    self.retained_task_shape(module, retained, environment, external, active, b)?
                } else {
                    self.retained_shape(module, retained, environment, external, active, b)?
                };
                if !child.statics.is_empty() {
                    return Err(unsupported(
                        "opaque result/payload retains unproved complete static occurrences",
                        None,
                    ));
                }
                if matches!(recipe, SemanticTypeRecipe::Task { .. }) {
                    for (class, (key, origin)) in
                        child.classes.iter().zip(&child.origins).enumerate()
                    {
                        b.spend(checked.shape.retained.len().saturating_add(1))?;
                        let mut rows = checked.shape.retained.iter().filter(|row| {
                            row.path == [LifeTypeProjection::Result] && row.class == class as u32
                        });
                        let row = rows.next().ok_or_else(|| {
                            bad("Task directory omits a complete result class/phantom occurrence")
                        })?;
                        if rows.next().is_some() {
                            return Err(bad("Task result class occurrence is duplicated"));
                        }
                        let actual = checked.classes.get(row.binder as usize).ok_or_else(|| {
                            bad("Task result class maps outside its complete domain")
                        })?;
                        let actual_origin = checked
                            .origins
                            .get(row.binder as usize)
                            .ok_or_else(|| bad("Task result intrinsic origin is absent"))?;
                        if !self.same(environment, key, actual, b)?
                            || !self.same_origin(environment, origin, actual_origin, b)?
                        {
                            return Err(bad(
                                "Task result class changes its pre-image identity or outside image",
                            ));
                        }
                    }
                } else if !child.classes.is_empty() {
                    return Err(unsupported(
                        "guard payload requires its complete retained child domain and manager relation",
                        None,
                    ));
                }
                let ty = if matches!(recipe, SemanticTypeRecipe::Task { .. }) {
                    Type::Task {
                        result: Box::new(child.into_raw_type(b)?),
                        retained_life: checked.shape,
                    }
                } else {
                    Type::MutexGuard {
                        payload: Box::new(child.into_raw_type(b)?),
                        retained_life: checked.shape,
                    }
                };
                let life = StorageLifeShape {
                    binder_count: checked.classes.len() as u32,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                };
                let mut value = self.construct_value(module, &ty, &life, checked.classes, b)?;
                value.origins = checked.origins;
                Ok(value)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.evaluate_type(module, error, environment, external, active, b)?;
                let result =
                    self.evaluate_type(module, result, environment, external, active, b)?;
                let mut value = CheckedType {type_cell: crate::semantic::CheckedTypeData::new(Type::ErrorUnion {
                        error: Box::new(error.raw_type_copy(b)?),
                        result: Box::new(result.raw_type_copy(b)?),
                    }),
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
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => self.opaque_type(
                module,
                *definition,
                arguments,
                hidden,
                environment,
                external,
                active,
                b,
            ),
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment: sealed,
            } => {
                if self.domain_only.get() {
                    self.metadata_closure_type(
                        module,
                        *definition,
                        arguments,
                        sealed,
                        environment,
                        external,
                        active,
                        b,
                    )
                } else {
                    self.closure_type(
                        module,
                        *definition,
                        arguments,
                        sealed,
                        environment,
                        external,
                        active,
                        b,
                    )
                }
            }
            SemanticTypeRecipe::UnresolvedNamed(_) | SemanticTypeRecipe::Unsupported(_) => {
                Err(bad("executable semantic recipe retains unresolved syntax"))
            }
            _ => Err(unsupported(
                "semantic recipe requires its complete nominal/opaque/qualified interpreter",
                None,
            )),
        }
    }
    pub(super) fn construct_value(
        &self,
        module: &TypedModule,
        ty: &Type,
        life: &StorageLifeShape,
        classes: Vec<ClassKey>,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        if self.domain_only.get() {
            self.metadata_value(module, ty, life, classes, b)
        } else {
            self.checked_value(module, ty, life, classes, b)
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
        let type_cell=self.shared_type_cell(ty,b)?;
        let count =
            u32::try_from(classes.len()).map_err(|_| limit("semantic class domain exceeds u32"))?;
        let scope = DynInstantiationScope {
            fixed_binder_count: count,
            invocation_binder_count: 0,
        };
        b.spend(count as usize)?;
        let inventory_classes: Vec<_> = (0..count).map(StorageBinder::Fixed).collect();
        dynamic_witness::check_type_parts(module, ty, life, &inventory_classes, scope, b)?;
        // The returned value still owns its full shape; only the temporary
        // validation payload was removed.
        dynamic_metadata::life_shape(life,b)?;
        let mut origins = self.typed_origins(module,ty,life,&classes,b)?;
        let mut statics = vec![];
        if let Type::Closure(id) = ty {
            if let Some(domain) = self.closure_domain(*id, b)? {
                if domain.life != *life || domain.origins.len() != classes.len() {
                    return Err(bad(
                        "Closure claim differs from its independently reconstructed complete domain",
                    ));
                }
                origins = domain.origins;
                statics = domain.statics;
            }
        }
        Ok(CheckedType {type_cell,
            life: life.clone(),
            classes,
            origins,
            statics,
        })
    }
    pub(super) fn attach_child(
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
        // A whole child occurrence does not replace an ordinary wrapper's
        // own complete descendants. Keep every original local index under
        // the new prefix; an opaque child retains its own separate scope.
        if !matches!(child.ty,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}) {
            for row in &child.life.retained {
                b.spend(prefix.len().saturating_add(row.path.len()).saturating_add(1))?;
                let mut path=prefix.clone();path.extend_from_slice(&row.path);
                let binder=*map.get(row.binder as usize).ok_or_else(||bad("constructed child descendant escapes its complete original class map"))?;
                parent.life.retained.push(StorageRetainedBinding{path,class:row.class,binder});
            }
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
            SemanticActualRecipe::Infer => {
                return Err(unsupported(
                    "Infer actual requires its checked original caller body and call/formal slot",
                    None,
                ));
            }
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
        for (ordinal, parameter) in recipe.parameters.iter().enumerate() {
            let value =
                self.evaluate_type(module, &parameter.ty, &environment, external, active, b)?;
            let value = self.signature_mode_type(
                module,
                &parameter.ty,
                value,
                parameter.mode,
                Some(ordinal as u32),
                &environment,
                active,
                b,
            )?;
            parameters.push((value, parameter.mode));
        }
        let result =
            self.evaluate_type(module, &recipe.result, &environment, external, active, b)?;
        let result = self.signature_mode_type(
            module,
            &recipe.result,
            result,
            recipe.result_mode,
            None,
            &environment,
            active,
            b,
        )?;
        Ok(CheckedSignature {
            parameters,
            result,
            result_mode: recipe.result_mode,
            call_kind: recipe.call_kind,
            bound,
            invocation,
        })
    }
    pub(crate) fn application(
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

#[cfg(test)]
mod constructed_task_descendant_tests {
    use super::*;
    #[test]
    fn nested_task_capture_headers_use_only_the_real_opaque_class_rows_raw_wire(){
        let limits=Limits::default();
        for tuple in [false,true]{
            let(mut module,_)=crate::tests::baseline();
            let leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)};
            let owner=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{
                binder_count:2,ports:vec![RetainedBorrowPort{leaf:leaf.clone(),mode:StoredBorrowMode::SharedAddress,binder:1}],
                retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),nominal_arguments:vec![],
            }}));
            let projection=if tuple{PlaceProjection::Field(0)}else{PlaceProjection::ErrorArm(false)};
            let capture=if tuple{Type::Tuple(vec![owner])}else{Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(owner)}};
            // The first physical port introduces Fixed0. The ghost child
            // class is still κ0 but appears as Fixed1 in this canonical header.
            let header=StorageLifeContract{fixed_binder_count:2,invocation_binder_count:0,
                bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(0),path:vec![ContractProjection::Place(projection),ContractProjection::RetainedPort(0)]},binder:StorageBinder::Fixed(0)}],
                classes:(0..2).map(|class|SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Capture(0),path:vec![LifeTypeProjection::Place(projection),LifeTypeProjection::Pointee],class},binder:StorageBinder::Fixed(1-class)}).collect(),static_bindings:vec![],
            };
            module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(header.clone());
            module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![capture],parameters:vec![],result:Type::I32,kind:CallableKind::FnOnce,storage_life:Some(header),lending_captures:vec![]});
            let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();let(wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
            for actual in [&module,&wire]{
                let life=StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![]};let ty=Type::Closure(ClosureId(0));
                let ports=task_domains::raw_input_ports(actual,&ty,&life,&mut Budget::new(&limits)).unwrap();
                assert_eq!(ports,vec![(leaf.clone(),StoredBorrowMode::SharedAddress,0,vec![ContractProjection::Capture(0),ContractProjection::Place(projection),ContractProjection::RetainedPort(0)])]);
                for attack in 0..2{
                    let mut forged=actual.clone();let header=forged.closures[0].storage_life.as_mut().unwrap();
                    if attack==0{header.classes.pop();}else{header.classes[1].binder=header.classes[0].binder;}
                    forged.functions[0].storage_life=forged.closures[0].storage_life.clone();
                    assert!(task_domains::raw_input_ports(&forged,&ty,&life,&mut Budget::new(&limits)).is_err(),"a true opaque child cannot omit or coalesce an intrinsic class");
                }
                let mut swapped=actual.clone();let header=swapped.closures[0].storage_life.as_mut().unwrap();header.classes[0].binder=StorageBinder::Fixed(0);header.classes[1].binder=StorageBinder::Fixed(1);
                swapped.functions[0].storage_life=swapped.closures[0].storage_life.clone();
                let changed=task_domains::raw_input_ports(&swapped,&ty,&life,&mut Budget::new(&limits)).unwrap();
                assert_eq!(changed[0].2,1);
                // A bare map may permute a child's images. It must report
                // that distinct original index, never silently use κ0. The
                // exact original header/Fixed contract separately rejects a
                // permutation inconsistent with its unchanged port binding.
                if !tuple {
                    let check_header=|module:&TypedModule|{
                        let definition=&module.closures[0];
                        crate::signature_life::closure_relation_resource_scopes(module,&definition.parameters,&definition.captures,definition.kind,&definition.result,definition.storage_life.as_ref().unwrap(),Some(module.functions[0].span),&mut Budget::new(&limits))
                    };
                    check_header(actual).unwrap();
                    assert!(check_header(&swapped).is_err());
                }
            }
        }
    }
    #[test]
    fn error_union_preserves_every_task_wrapper_occurrence_without_merging_equal_images_raw_wire(){
        let limits=Limits::default();
        let(base,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&base,&mut Budget::new(&limits)).unwrap();
        let scope=ScopeKey::Instance(base.semantic.as_ref().unwrap().instances[0].id);
        let environment=EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let leaf=Type::Address{mutable:true,inner:Box::new(Type::I32)};
        let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{
            binder_count:2,ports:vec![RetainedBorrowPort{leaf:leaf.clone(),mode:StoredBorrowMode::ExclusiveAddress,binder:1}],
            retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),
            nominal_arguments:vec![NominalLifeArgument{path:vec![LifeTypeProjection::Parameter(0)],declaration:zil_ir::builtin_life::OPTION,argument:0,binder:0}],
        }};
        let owner=Type::Managed(Box::new(task));
        for ty in [owner.clone(),Type::Address{mutable:true,inner:Box::new(owner.clone())},Type::Desc(Box::new(owner.clone())),Type::Managed(Box::new(owner))] {
            let mut module=base.clone();module.functions[0].values.push(ty);
            let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();
            let(wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
            for actual in [&module,&wire]{
                let ty=actual.functions[0].values.last().unwrap();let mut b=Budget::new(&limits);
                let life=crate::tasks::frame_shape::declared_life(actual,ty,&mut b).unwrap();let count=life.binder_count;
                let image=ClassKey::Formal{scope,ordinal:0};
                // Synthetic metadata facts: different intrinsic child indices
                // have equal outside images. No target or actual loan follows.
                let child=CheckedType{type_cell:CheckedTypeData::new(ty.clone()),life:life.clone(),classes:vec![image;count as usize],origins:(0..count).map(|class|IntrinsicOrigin::Existing{scope,operand:OperandKey::Receiver,path:vec![],class}).collect(),statics:vec![]};
                let mut parent=CheckedType{type_cell:CheckedTypeData::new(Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(ty.clone())}),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
                let prefix=LifeTypeProjection::Place(PlaceProjection::ErrorArm(false));
                context.attach_child(&environment,&mut parent,&child,vec![prefix],Some(PlaceProjection::ErrorArm(false)),&mut b).unwrap();
                parent.life.binder_count=parent.classes.len() as u32;
                assert_eq!(parent.life.binder_count,count);assert_eq!(parent.origins,child.origins);
                assert_eq!(parent.life.retained.len(),count as usize+child.life.retained.len());
                for row in &child.life.retained{let mut path=vec![prefix];path.extend_from_slice(&row.path);assert!(parent.life.retained.contains(&StorageRetainedBinding{path,class:row.class,binder:row.binder}));}
                let ports=task_domains::raw_input_ports(actual,&parent.ty,&parent.life,&mut b).unwrap();
                let expected_class=if matches!(ty,Type::Address{..}|Type::Desc(_)){2}else{1};
                assert!(ports.iter().any(|(actual_leaf,mode,class,path)|actual_leaf==&leaf&&*mode==StoredBorrowMode::ExclusiveAddress&&*class==expected_class&&path.as_slice()==[ContractProjection::Place(PlaceProjection::ErrorArm(false)),ContractProjection::RetainedPort(0)]));
                for attack in 0..3{
                    let mut forged=parent.life.clone();let end=forged.retained.len();
                    match attack{0=>{forged.retained.pop();},1=>{forged.retained[end-1].binder=forged.retained[end-2].binder;},2=>{let binder=forged.retained[end-1].binder;forged.retained[end-1].binder=forged.retained[end-2].binder;forged.retained[end-2].binder=binder;},_=>unreachable!()}
                    assert!(task_domains::raw_input_ports(actual,&parent.ty,&forged,&mut b).is_err(),"wrapper child attack {attack} cannot use equal images to omit, coalesce or swap κ");
                }
            }
        }
    }
}
