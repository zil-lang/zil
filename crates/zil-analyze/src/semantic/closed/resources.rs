//! Whole resource scope reconstruction. Only a complete actual payload and its
//! original closure environment can establish a zero child domain; the owning
//! wrapper and an empty physical port list cannot do so.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn zero_data(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "complete zero-domain capture graph exceeds its bounded depth",
            ));
        }
        if ty.is_b0_scalar()
            || *ty == Type::ForeignHandle
            || *ty == crate::generic_mutex::error_type()
        {
            return Ok(());
        }
        match ty {
            Type::Managed(_) => self.resource_zero(ty, b, depth + 1),
            Type::Closure(id) => {
                if !self.capture_directory(*id, b)?.is_empty() {
                    return Err(fail(
                        "zero-domain capture retains a complete original head class",
                    ));
                }
                let closure = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| fail("zero-domain child closure has another actual identity"))?;
                let life = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("zero-domain child environment is Unknown"))?;
                if life.fixed_binder_count != 0 {
                    return Err(fail(
                        "zero-domain child environment retains nominal or phantom classes",
                    ));
                }
                for capture in &closure.captures {
                    self.zero_data(capture, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.zero_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Array { element, .. } => self.zero_data(element, b, depth + 1),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, field) in fields {
                    self.zero_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, fields) in variants {
                    for field in fields {
                        self.zero_data(field, b, depth + 1)?;
                    }
                }
                Ok(())
            }
            Type::BorrowedStorage { storage, life } => {
                self.nominal_inventory(ty, b)?;
                if !empty(life) {
                    return Err(fail(
                        "zero-domain capture retains nominal or phantom classes",
                    ));
                }
                match storage.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        for (_, field) in fields {
                            self.zero_data(field, b, depth + 1)?;
                        }
                        Ok(())
                    }
                    Type::Enum { variants, .. } => {
                        for (_, fields) in variants {
                            for field in fields {
                                self.zero_data(field, b, depth + 1)?;
                            }
                        }
                        Ok(())
                    }
                    _ => Err(unknown(
                        "zero-domain qualified capture needs its complete intrinsic inventory",
                    )),
                }
            }
            Type::ErrorUnion { error, result } => {
                self.zero_data(error, b, depth + 1)?;
                self.zero_data(result, b, depth + 1)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.zero_data(parameter, b, depth + 1)?;
                }
                self.zero_data(result, b, depth + 1)
            }
            _ => Err(unknown(
                "capture has a nonzero, opaque or incomplete intrinsic domain",
            )),
        }
    }
    pub(super) fn resource_zero(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual resource scope exceeds its bounded inventory depth",
            ));
        }
        let Type::Managed(payload) = ty else {
            return Err(fail("actual resource scope has no complete owning carrier"));
        };
        match payload.as_ref() {
            Type::Mutex { payload } => self.mutex_payload(payload, b),
            Type::Task { result, .. } => {
                self.zero_data(result, b, depth + 1)?;
                crate::tasks::semantic_resource_scope(
                    self.module,
                    payload,
                    self.inventory.limits,
                    b,
                )
            }
            Type::ByteBuffer => self.actual_type(ty, b),
            Type::OwnedStore { element } => {
                self.actual_type(ty, b)?;
                crate::stored_flow::validate_plain_nominal_zero(
                    self.module,
                    element,
                    self.inventory.limits,
                    b,
                )
            }
            Type::Record { .. } | Type::BorrowedStorage { .. } => {
                self.resource_nominal(payload, b, depth + 1)
            }
            Type::Registration { environment } | Type::AsyncRegistration { environment } => {
                let Type::Closure(id) = environment.as_ref() else {
                    return Err(unknown(
                        "actual registration scope lacks its real concrete callback environment",
                    ));
                };
                self.ordinary_type_at(environment, b, depth + 1)?;
                if !self.capture_directory(*id, b)?.is_empty() {
                    return Err(fail(
                        "actual registration callback retains a free original head class",
                    ));
                }
                let closure = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| fail("actual registration callback identity is absent"))?;
                let life = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("actual registration environment domain is Unknown"))?;
                if life.fixed_binder_count != 0 {
                    return Err(fail(
                        "actual registration environment retains a nominal or phantom class",
                    ));
                }
                crate::registration::semantic_resource_scope(self.module, payload, b)
            }
            _ => Err(unknown(
                "actual resource needs its complete original payload, cleanup and retained inventory",
            )),
        }
    }
    pub(super) fn resource_nominal(
        &self,
        payload: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal cleanup exceeds its complete inventory depth",
            ));
        }
        crate::stored_flow::validate_plain_nominal_zero(
            self.module,
            payload,
            self.inventory.limits,
            b,
        )?;
        let mut original = None;
        for instance in &self.module.life_instances {
            b.spend(1, WORK)?;
            crate::dyn_metadata::ty(payload, b)?;
            crate::dyn_metadata::ty(&instance.ty, b)?;
            if instance.ty != *payload {
                continue;
            }
            let mut found = None;
            for definition in self.inventory.definitions.values() {
                b.spend(1, WORK)?;
                if let SemanticDefinitionBody::Nominal {
                    declaration,
                    kind: LifeNominalKind::Resource,
                    ..
                } = &definition.body
                {
                    if *declaration == instance.declaration
                        && found.replace(definition.id).is_some()
                    {
                        return Err(fail(
                            "actual resource payload has multiple original definitions",
                        ));
                    }
                }
            }
            let definition = found.ok_or_else(|| {
                fail("actual resource payload is not its original resource declaration")
            })?;
            let mut arguments = Vec::new();
            for actual in &instance.arguments {
                let LifeNominalActual::Type(ty) = actual else {
                    return Err(unknown(
                        "actual resource cleanup needs its complete mixed lifetime and constant substitution",
                    ));
                };
                crate::dyn_metadata::ty(ty, b)?;
                self.push_value(&mut arguments, ty.clone(), b)?;
            }
            let expected =
                original::nominal::actual(definition, &arguments, self.inventory, b, depth + 1)?;
            crate::dyn_metadata::ty(&expected, b)?;
            crate::dyn_metadata::ty(payload, b)?;
            if expected != *payload {
                return Err(fail(
                    "actual resource payload changes its original complete field substitution",
                ));
            }
            if original.replace(definition).is_some() {
                return Err(fail(
                    "actual resource payload has multiple complete nominal instances",
                ));
            }
        }
        let original = original.ok_or_else(|| {
            fail("actual resource payload omits its complete original nominal identity")
        })?;
        let drop = original::resources::drop_method(original, self.inventory, b)?;
        let mut actual = None;
        for resource in &self.module.resources {
            b.spend(1, WORK)?;
            crate::dyn_metadata::ty(&resource.payload, b)?;
            crate::dyn_metadata::ty(payload, b)?;
            if resource.payload == *payload && actual.replace(resource).is_some() {
                return Err(fail(
                    "actual resource payload has multiple cleanup definitions",
                ));
            }
        }
        let actual =
            actual.ok_or_else(|| fail("actual resource payload has no cleanup definition"))?;
        match (drop, actual.teardown) {
            (None, None) => {}
            (Some(original), Some(target)) => {
                let worlds = self.function_worlds(target, b)?;
                if worlds.is_empty() {
                    return Err(fail(
                        "actual resource teardown lacks its original method world",
                    ));
                }
                for world in worlds {
                    if world.definition != original {
                        return Err(fail(
                            "actual resource teardown selects another original method",
                        ));
                    }
                }
                let function = self
                    .inventory
                    .functions
                    .get(&target)
                    .ok_or_else(|| fail("actual resource teardown has no real function"))?;
                if function.call_kind != CallKind::Direct || function.return_type != Type::Unit {
                    return Err(fail(
                        "actual resource teardown changes its synchronous cleanup ABI",
                    ));
                }
            }
            _ => {
                return Err(fail(
                    "actual resource cleanup presence differs from the original Drop declaration",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn resolved(
        &self,
        context: &SemanticInstance,
        input: &SemanticTypeValue,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "resource whole value exceeds its bounded scope depth",
            ));
        }
        let SemanticTypeValue::Resolved { ty, life, classes } = input else {
            return Err(fail(
                "actual resource whole value retains Core-only metadata",
            ));
        };
        match ty {
            Type::MutexGuard { .. } => {
                self.ordinary_type_at(ty, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if life.binder_count != 1
                    || !life.slots.is_empty()
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual Mutex guard changes its opaque intrinsic domain",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                crate::dyn_metadata::ty(ty, b)?;
                Ok(Value {
                    ty: ty.clone(),
                    life: life.clone(),
                    classes: vec![class],
                })
            }
            Type::Address { mutable, inner }
                if matches!(inner.as_ref(), Type::MutexGuard { .. }) =>
            {
                self.ordinary_type_at(inner, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                let [row] = life.retained.as_slice() else {
                    return Err(fail(
                        "Mutex guard reference omits its complete Pointee embedding",
                    ));
                };
                if row.path.as_slice() != [LifeTypeProjection::Pointee]
                    || row.class != 0
                    || classes.len() != life.binder_count as usize
                    || classes.get(row.binder as usize).is_none()
                {
                    return Err(fail(
                        "Mutex guard reference changes its intrinsic class selector",
                    ));
                }
                crate::dyn_metadata::ty(inner, b)?;
                let expected = value::address(
                    *mutable,
                    value::Class::Local(0),
                    Value {
                        ty: inner.as_ref().clone(),
                        life: StorageLifeShape {
                            binder_count: 1,
                            slots: Vec::new(),
                            retained: Vec::new(),
                            nominal_arguments: Vec::new(),
                        },
                        classes: vec![value::Class::Local(row.binder)],
                    },
                    b,
                )?;
                if expected.life != *life {
                    return Err(fail(
                        "Mutex guard reference changes its complete outer occurrence",
                    ));
                }
                let mut images = Vec::new();
                for expression in classes {
                    let image = value::class(context, expression, self.inventory, b, depth + 1)?;
                    self.push_value(&mut images, image, b)?;
                }
                crate::dyn_metadata::ty(ty, b)?;
                Ok(Value {
                    ty: ty.clone(),
                    life: life.clone(),
                    classes: images,
                })
            }
            Type::Desc(inner) => {
                crate::dyn_metadata::ty(inner, b)?;
                self.resource_zero(&Type::Managed(inner.clone()), b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if life.binder_count != 1
                    || !matches!(life.slots.as_slice(), [slot] if slot.path.is_empty() && slot.binder == 0 && slot.mode == StoredBorrowMode::WritableDescription)
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual zero-domain resource description changes its complete weak occurrence",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                crate::dyn_metadata::ty(ty, b)?;
                Ok(Value {
                    ty: ty.clone(),
                    life: life.clone(),
                    classes: vec![class],
                })
            }
            Type::Managed(_) => {
                self.resource_zero(ty, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if !empty(life) || !classes.is_empty() {
                    return Err(fail(
                        "actual zero resource carries another claimed intrinsic domain",
                    ));
                }
                crate::dyn_metadata::ty(ty, b)?;
                Ok(value::scalar(ty.clone()))
            }
            Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(_)) => {
                self.resource_zero(inner, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                let Type::Address { mutable, .. } = ty else {
                    return Err(fail("actual resource reference changes its carrier"));
                };
                if life.binder_count != 1
                    || life.slots.len() != 1
                    || !life.slots[0].path.is_empty()
                    || life.slots[0].binder != 0
                    || life.slots[0].mode
                        != if *mutable {
                            StoredBorrowMode::ExclusiveAddress
                        } else {
                            StoredBorrowMode::SharedAddress
                        }
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual resource reference changes its complete outer occurrence or child domain",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                crate::dyn_metadata::ty(ty, b)?;
                Ok(Value {
                    ty: ty.clone(),
                    life: life.clone(),
                    classes: vec![class],
                })
            }
            _ => value::resolved(context, input, self.inventory, b, depth + 1),
        }
    }
    pub(super) fn pointee(&self, input: &Value, b: &mut Budget) -> CompileResult<Value> {
        if let Type::Address { inner, .. } = &input.ty
            && matches!(inner.as_ref(), Type::Managed(_))
        {
            self.resource_zero(inner, b, 0)?;
            if !input.life.retained.is_empty() {
                return Err(fail(
                    "actual zero resource reference invents child class occurrences",
                ));
            }
            crate::dyn_metadata::ty(inner, b)?;
            return Ok(value::scalar(inner.as_ref().clone()));
        }
        value::pointee(input, self.inventory, b)
    }
}
