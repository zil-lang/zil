//! Concrete structural premises keep layout separate from synchronous cleanup.
//! A closure's body kind is irrelevant to dropping its stored environment.
use super::*;
use crate::semantic::value::{self, Argument};
use original::Predicate;

impl Check<'_, '_> {
    pub(super) fn qualify_resource_nominal(
        &self,
        owner: SemanticDefinitionId,
        arguments: &[Argument],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal premises exceed their complete depth",
            ));
        }
        let definition = self.inventory.definition(owner, b)?;
        if definition.parameters.len() != arguments.len() {
            return Err(fail("actual resource loses its complete mixed arguments"));
        }
        for (formal, actual) in definition.parameters.iter().zip(arguments) {
            b.spend(1, WORK)?;
            let (SemanticFormal::Type { requirements }, Argument::Type(_)) = (formal, actual)
            else {
                return Err(unknown(
                    "actual resource cleanup needs its complete mixed lifetime and constant substitution",
                ));
            };
            self.nominal_requirements(owner, arguments, requirements, b, depth + 1)?;
        }
        self.nominal_requirements(owner, arguments, &definition.requirements, b, depth + 1)
    }
    /// Adapt the old zero-domain nominal entry to the complete original field
    /// interpreter. Zero is proved from every actual, never from a port count.
    pub(super) fn nominal_actual(
        &self,
        definition: SemanticDefinitionId,
        actuals: &[Type],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Type> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal exceeds its complete substitution depth",
            ));
        }
        original::nominal::actual_qualified(
            definition,
            actuals,
            self.inventory,
            b,
            depth,
            &|owner, actuals, requirements, b, depth| {
                b.spend(1, WORK)?;
                if requirements.is_empty() {
                    return Ok(());
                }
                let mut arguments = Vec::new();
                for ty in actuals {
                    // actual_qualified has checked the complete nominal zero
                    // domain; also inspect actual closure/resource children.
                    self.zero_data(ty, b, depth + 1)?;
                    self.push_value(&mut arguments, Argument::Type(value::scalar(ty.clone())), b)?;
                }
                self.nominal_requirements(owner, &arguments, requirements, b, depth + 1)
            },
        )
    }

    /// Consume the complete original requirement slice in this nominal's
    /// actual argument domain. Candidate provider identity reconstruction is
    /// separate, so an unselected Self never supplies its own premises.
    fn nominal_requirements(
        &self,
        owner: SemanticDefinitionId,
        arguments: &[Argument],
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal premises exceed their complete depth",
            ));
        }
        for requirement in requirements {
            b.spend(1, WORK)?;
            let (recipe, predicate) = match requirement {
                SemanticRequirementRecipe::Trait { ty, application } => {
                    let receiver = self.nominal_field_value(owner, arguments, ty, b, depth + 1)?;
                    let actuals =
                        self.nominal_trait_actuals(owner, arguments, application, b, depth + 1)?;
                    self.actual_provider(
                        &receiver,
                        application.declaration,
                        &actuals,
                        b,
                        depth + 1,
                    )?;
                    continue;
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    if self.nominal_field_value(owner, arguments, left, b, depth + 1)?
                        != self.nominal_field_value(owner, arguments, right, b, depth + 1)?
                    {
                        return Err(fail("actual nominal fails its original type equality"));
                    }
                    continue;
                }
                SemanticRequirementRecipe::Structural { ty, bound } => (
                    ty,
                    match bound {
                        DynStructuralBound::Copy => Predicate::Copy,
                        DynStructuralBound::Value => Predicate::Value,
                        DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                        DynStructuralBound::StoreElement => Predicate::StoreElement,
                    },
                ),
                SemanticRequirementRecipe::Builtin { ty, trait_kind } => (
                    ty,
                    match trait_kind {
                        SemanticBuiltinTrait::Copy => Predicate::Copy,
                        SemanticBuiltinTrait::Value => Predicate::Value,
                        SemanticBuiltinTrait::SyncDrop => Predicate::SyncDrop,
                        SemanticBuiltinTrait::StoreElement => Predicate::StoreElement,
                        _ => {
                            return Err(unknown(
                                "actual nominal needs its complete builtin premise proof",
                            ));
                        }
                    },
                ),
                _ => {
                    return Err(unknown(
                        "actual nominal needs its complete nonstructural premise proof",
                    ));
                }
            };
            let value = self.nominal_field_value(owner, arguments, recipe, b, depth + 1)?;
            if !self.structural_actual(&value.ty, predicate, b, depth + 1)? {
                return Err(fail("actual nominal fails an original structural premise"));
            }
        }
        Ok(())
    }

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
        if matches!(predicate, Predicate::StoreElement) {
            // The sealed element contract includes ordinary formation and
            // synchronous cleanup. Layout alone cannot supply either theorem.
            if !self.structural_actual(ty, Predicate::Value, b, depth + 1)?
                || !self.structural_actual(ty, Predicate::SyncDrop, b, depth + 1)?
            {
                return Ok(false);
            }
            crate::nested_store::semantic_element(self.module, ty, b)?;
            return Ok(true);
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
        let (owner, _) = self.nominal_inventory_source(ty, b)?;
        let declaration = self.inventory.definition(owner, b)?;
        if matches!(
            declaration.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Resource,
                ..
            }
        ) {
            return Err(fail(
                "raw resource payload is not an ordinary structural value",
            ));
        }
        original::resources::ordinary_drop_absent(owner, self.inventory, b)
    }
}
