//! Concrete structural premises keep layout separate from synchronous cleanup.
//! A closure's body kind is irrelevant to dropping its stored environment.
use super::*;
use original::Predicate;

impl Check<'_, '_> {
    pub(super) fn structural_actual(
        &self,
        ty: &Type,
        predicate: Predicate,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual structural environment exceeds its finite inventory depth",
            ));
        }
        if !matches!(predicate, Predicate::Value | Predicate::SyncDrop) {
            return original::nominal::predicate_actual(ty, predicate, self.inventory, b, depth);
        }
        match ty {
            Type::Closure(id) => {
                self.capture_directory(*id, b)?;
                let closure = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| fail("structural closure has no exact actual definition"))?;
                for capture in &closure.captures {
                    // Every stored field has a finite complete type. Value is
                    // not Copy; SyncDrop separately checks each old occupant.
                    self.ordinary_type_at(capture, b, depth + 1)?;
                    if !self.structural_actual(capture, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Address { .. }
            | Type::Desc(_)
            | Type::FunctionPointer { .. }
            | Type::ForeignCallback { .. } => {
                self.ordinary_type_at(ty, b, depth + 1)?;
                // Dropping the non-owning handle never drops its pointee.
                Ok(true)
            }
            Type::Managed(payload) => {
                self.ordinary_type_at(ty, b, depth + 1)?;
                if matches!(predicate, Predicate::Value) {
                    return Ok(true);
                }
                match payload.as_ref() {
                    Type::ByteBuffer => Ok(true),
                    Type::Mutex { payload } => {
                        self.mutex_payload(payload, b)?;
                        self.structural_actual(payload, predicate, b, depth + 1)
                    }
                    Type::Record { fields, .. } => {
                        // This proves the original authored Drop link and the
                        // actual Direct Unit teardown, not a source name flag.
                        self.resource_nominal(payload, b, depth + 1)?;
                        for (_, field) in fields {
                            if !self.structural_actual(field, predicate, b, depth + 1)? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    Type::OwnedStore { element } => {
                        self.resource_zero(ty, b, depth + 1)?;
                        self.structural_actual(element, predicate, b, depth + 1)
                    }
                    Type::Task { .. } | Type::AsyncRegistration { .. } => Ok(false),
                    _ => Err(unknown(
                        "actual owning SyncDrop needs its complete finite cleanup theorem",
                    )),
                }
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if !self.structural_actual(field, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Array { element, .. } => self.structural_actual(element, predicate, b, depth + 1),
            Type::ErrorUnion { error, result } => {
                Ok(self.structural_actual(error, predicate, b, depth + 1)?
                    && self.structural_actual(result, predicate, b, depth + 1)?)
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                self.structural_nominal_identity(ty, b)?;
                for (_, field) in fields {
                    if !self.structural_actual(field, predicate, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Enum { variants, .. } => {
                self.structural_nominal_identity(ty, b)?;
                for (_, fields) in variants {
                    for field in fields {
                        if !self.structural_actual(field, predicate, b, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::BorrowedStorage { storage, .. } => {
                self.structural_nominal_identity(ty, b)?;
                match storage.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        for (_, field) in fields {
                            if !self.structural_actual(field, predicate, b, depth + 1)? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    Type::Enum { variants, .. } => {
                        for (_, fields) in variants {
                            for field in fields {
                                if !self.structural_actual(field, predicate, b, depth + 1)? {
                                    return Ok(false);
                                }
                            }
                        }
                        Ok(true)
                    }
                    _ => Err(unknown(
                        "qualified structural value has another complete nominal carrier",
                    )),
                }
            }
            _ => original::nominal::predicate_actual(ty, predicate, self.inventory, b, depth + 1),
        }
    }

    fn structural_nominal_identity(&self, ty: &Type, b: &mut Budget) -> CompileResult<()> {
        self.nominal_inventory(ty, b)?;
        let mut declaration = None;
        for entry in &self.module.life_instances {
            crate::dyn_metadata::ty(&entry.ty, b)?;
            crate::dyn_metadata::ty(ty, b)?;
            if entry.ty == *ty && declaration.replace(entry.declaration).is_some() {
                return Err(fail(
                    "structural nominal has competing exact catalogue identities",
                ));
            }
        }
        let declaration = declaration
            .ok_or_else(|| fail("structural nominal has no exact original catalogue identity"))?;
        let mut owner = None;
        for definition in self.inventory.definitions.values() {
            b.spend(1, WORK)?;
            if let SemanticDefinitionBody::Nominal {
                declaration: identity,
                kind,
                ..
            } = &definition.body
            {
                if *identity != declaration {
                    continue;
                }
                if *kind == LifeNominalKind::Resource {
                    return Err(fail(
                        "raw resource payload is not an ordinary structural value",
                    ));
                }
                if owner.replace(definition.id).is_some() {
                    return Err(fail(
                        "structural nominal has competing original definitions",
                    ));
                }
            }
        }
        original::resources::ordinary_drop_absent(
            owner.ok_or_else(|| fail("structural nominal omits its original declaration"))?,
            self.inventory,
            b,
        )
    }
}
