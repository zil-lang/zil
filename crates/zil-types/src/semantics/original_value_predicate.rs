//! Value means complete ordinary inline storage. Copy, StoreElement, closed
//! environments, transfer and cleanup remain separate original obligations.
use super::*;
impl Check<'_, '_> {
    pub(super) fn value_recipe(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(self.predicate_active.len().saturating_add(1), depth)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(hops),
            ordinal,
        } = ty
        {
            let mut environment_owner = owner;
            for step in 0..*hops {
                self.pay(1, depth + step as usize)?;
                environment_owner = self
                    .definition(environment_owner)?
                    .parent
                    .ok_or_else(|| bad("Value environment escapes its real lexical ancestry"))?;
            }
            let (parent, source) = self
                .proof
                .environments
                .get(&(environment_owner, *ordinal))
                .ok_or_else(|| pending("Value needs its checked sealed whole-Type environment"))?;
            let parent = *parent;
            let source = copy_recipe(source, self.limits, self.left)?;
            return self.value_recipe(parent, &source, depth + 1);
        }
        let ty = self.resolve(owner, ty, depth + 1)?;
        if self
            .predicate_active
            .iter()
            .any(|(domain, kind, active)| *domain == owner && *kind == 1 && active == &ty)
        {
            return Ok(false);
        }
        let active = copy_recipe(&ty, self.limits, self.left)?;
        self.predicate_active.push((owner, 1, active));
        let result = self.value_recipe_inner(owner, &ty, depth + 1);
        self.predicate_active.pop();
        result
    }
    fn value_recipe_inner(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Atom(ty) => {
                crate::inference::charge_type_snapshot(
                    ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                ty.is_copy_value()
            }
            SemanticTypeRecipe::Exact(exact) => self.exact_value_shape(&exact.ty, depth + 1)?,
            SemanticTypeRecipe::Address { .. }
            | SemanticTypeRecipe::Closure { .. }
            | SemanticTypeRecipe::Code { .. }
            | SemanticTypeRecipe::MutexGuard { .. } => true,
            SemanticTypeRecipe::Dyn { owned, .. } => *owned,
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.value_recipe(owner, field, depth + 1)?;
                }
                all
            }
            SemanticTypeRecipe::Array { element, .. } => {
                self.value_recipe(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                let error = self.value_recipe(owner, error, depth + 1)?;
                let result = self.value_recipe(owner, result, depth + 1)?;
                error && result
            }
            SemanticTypeRecipe::Qualified { storage, .. } => {
                self.value_recipe(owner, storage, depth + 1)?
            }
            SemanticTypeRecipe::Environment { .. } | SemanticTypeRecipe::OriginalType { .. } => {
                self.value_recipe(owner, ty, depth + 1)?
            }
            SemanticTypeRecipe::Formal(_)
            | SemanticTypeRecipe::AssociatedType { .. }
            | SemanticTypeRecipe::SelfType { .. } => {
                let mut found = false;
                for requirement in self.original_requirements(owner, depth + 1)? {
                    let premise = match &requirement {
                        SemanticRequirementRecipe::Structural {
                            ty,
                            bound:
                                DynStructuralBound::Value
                                | DynStructuralBound::StoreElement
                                | DynStructuralBound::Copy,
                        } => Some(ty),
                        SemanticRequirementRecipe::Builtin {
                            ty,
                            trait_kind:
                                SemanticBuiltinTrait::Value
                                | SemanticBuiltinTrait::StoreElement
                                | SemanticBuiltinTrait::Copy,
                        } => Some(ty),
                        _ => None,
                    };
                    if let Some(premise) = premise {
                        found |= self.premise_type_equal(owner, ty, premise, depth + 1)?;
                    }
                }
                if !found {
                    for equal in self.equal_types(owner, ty, depth + 1)? {
                        if self.value_recipe(owner, &equal, depth + 1)? {
                            found = true;
                            break;
                        }
                    }
                }
                found
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let original = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal { body, .. } = &original.body else {
                    return Err(bad("Value nominal recipe names a non-nominal declaration"));
                };
                let count = match body {
                    SemanticNominalBody::Product { fields } => fields.len(),
                    SemanticNominalBody::Enum { variants } => {
                        self.pay(variants.len(), depth)?;
                        variants.iter().try_fold(0usize, |count, variant| {
                            count
                                .checked_add(variant.fields.len())
                                .ok_or_else(|| bad("Value nominal field inventory overflows"))
                        })?
                    }
                };
                self.pay(count, depth)?;
                let mut all = true;
                match body {
                    SemanticNominalBody::Product { fields } => {
                        for field in fields {
                            let field = self.instantiate_field(
                                owner,
                                original.id,
                                &field.ty,
                                arguments,
                                depth + 1,
                            )?;
                            all &= self.value_recipe(owner, &field, depth + 1)?;
                        }
                    }
                    SemanticNominalBody::Enum { variants } => {
                        for variant in variants {
                            for field in &variant.fields {
                                let field = self.instantiate_field(
                                    owner,
                                    original.id,
                                    field,
                                    arguments,
                                    depth + 1,
                                )?;
                                all &= self.value_recipe(owner, &field, depth + 1)?;
                            }
                        }
                    }
                }
                all
            }
            SemanticTypeRecipe::Managed(payload) => {
                if !self.resource_head(owner, payload, depth + 1)? {
                    false
                } else if let SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } = payload.as_ref()
                {
                    let original = self.definition(*definition)?;
                    let SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Resource,
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } = &original.body
                    else {
                        return Ok(false);
                    };
                    self.pay(fields.len(), depth)?;
                    let mut all = true;
                    for field in fields {
                        let field = self.instantiate_field(
                            owner,
                            original.id,
                            &field.ty,
                            arguments,
                            depth + 1,
                        )?;
                        all &= self.copy_type(owner, &field, depth + 1)?;
                    }
                    all
                } else {
                    true
                }
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                // Body evidence and predicates are first interpreted in the
                // opaque definition's lexical domain. Explicit actuals may
                // specialize only that proved original result.
                if !self.proof.complete.contains(definition) && *definition != owner {
                    return Err(pending(
                        "Value opaque result needs its complete original definition proof",
                    ));
                }
                let result =
                    if let Some(result) = self.proof.types.get(&(*definition, Query::Result)) {
                        copy_recipe(result, self.limits, self.left)?
                    } else if *definition == owner {
                        copy_recipe(hidden, self.limits, self.left)?
                    } else {
                        return Err(pending(
                            "Value opaque result omits its all-return original equation",
                        ));
                    };
                if self.value_recipe(*definition, &result, depth + 1)? {
                    true
                } else {
                    let specialized = super::super::instantiate::ty(
                        self.program,
                        &self.proof,
                        &result,
                        super::super::instantiate::DomainMapping {
                            source: *definition,
                            use_definition: owner,
                            arguments,
                            parent_call: None,
                        },
                        self.limits,
                        self.left,
                    )?;
                    self.value_recipe(owner, &specialized, depth + 1)?
                }
            }
            SemanticTypeRecipe::Desc { .. }
            | SemanticTypeRecipe::Slice(_)
            | SemanticTypeRecipe::OwnedStore(_)
            | SemanticTypeRecipe::Mutex(_)
            | SemanticTypeRecipe::Task { .. }
            | SemanticTypeRecipe::Registration(_)
            | SemanticTypeRecipe::AsyncRegistration(_) => false,
            SemanticTypeRecipe::UnresolvedNamed(_) | SemanticTypeRecipe::Unsupported(_) => {
                return Err(pending(
                    "Value needs its complete original carrier declaration",
                ))
            }
        })
    }
    fn exact_value_shape(&mut self, ty: &Type, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match ty {
            Type::Address { .. }
            | Type::Closure(_)
            | Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. }
            | Type::ForeignCallback { .. }
            | Type::OwnedDyn { .. }
            | Type::MutexGuard { .. } => true,
            Type::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.exact_value_shape(field, depth + 1)?;
                }
                all
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                let mut all = true;
                for (_, field) in fields {
                    all &= self.exact_value_shape(field, depth + 1)?;
                }
                all
            }
            Type::Enum { variants, .. } => {
                let mut all = true;
                for (_, fields) in variants {
                    for field in fields {
                        all &= self.exact_value_shape(field, depth + 1)?;
                    }
                }
                all
            }
            Type::Array { element, .. }
            | Type::BorrowedStorage {
                storage: element, ..
            }
            | Type::Opaque {
                hidden: element, ..
            } => self.exact_value_shape(element, depth + 1)?,
            Type::ErrorUnion { error, result } => {
                let error = self.exact_value_shape(error, depth + 1)?;
                let result = self.exact_value_shape(result, depth + 1)?;
                error && result
            }
            Type::Managed(payload) => {
                let mut payload = payload.as_ref();
                while let Type::BorrowedStorage { storage, .. } = payload {
                    self.pay(1, depth)?;
                    payload = storage;
                }
                match payload {
                    Type::ByteBuffer
                    | Type::OwnedStore { .. }
                    | Type::Mutex { .. }
                    | Type::TaskI32
                    | Type::Task { .. }
                    | Type::Registration { .. }
                    | Type::AsyncRegistration { .. } => true,
                    Type::Record { .. } => {
                        crate::inference::charge_type_snapshot(
                            payload,
                            Span::default(),
                            self.limits,
                            self.left,
                            depth + 1,
                        )?;
                        payload.is_copy_value()
                    }
                    _ => false,
                }
            }
            _ => {
                crate::inference::charge_type_snapshot(
                    ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                ty.is_copy_value()
            }
        })
    }
}
