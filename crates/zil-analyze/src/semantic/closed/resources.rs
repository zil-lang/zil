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
        if ty.is_b0_scalar() || *ty == Type::ForeignHandle {
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
