//! Cleanup predicates over the checked original Gamma. These helpers neither
//! infer expression types nor inspect actual instances or emitted Drop nodes.
use super::*;

fn with_check<T>(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    limits: &Limits,
    left: &mut usize,
    run: impl FnOnce(&mut Check<'_, '_>) -> R<T>,
) -> R<T> {
    *left = left
        .checked_sub(proof.complete.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original cleanup context work exceeded", None)
        })?;
    if !proof.complete.contains(&owner) {
        return Err(bad(
            "cleanup obligation lacks its complete original owner body",
        ));
    }
    let mut check = Check {
        program,
        limits,
        left,
        proof: ProofAccess::Owned(std::mem::take(proof)),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: vec![owner],
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        escape_active: Vec::new(),
    };
    let result = run(&mut check);
    // Restore even on rejection or resource exhaustion. No speculative fact
    // from an actual specialization is ever installed in this context.
    *proof = std::mem::take(&mut check.proof).into_owned()?;
    result
}
pub(super) fn copy(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    with_check(program, proof, owner, limits, left, |check| {
        check.copy_type(owner, ty, 0)
    })
}
pub(super) fn capability(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    with_check(program, proof, owner, limits, left, |check| {
        check.cleanup_capability(owner, ty, 0)
    })
}
/// Exact product fields, in declaration order and in the use definition's
/// original domain. None is an unknown/non-product, never an empty inventory.
pub(super) fn fields(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<Vec<(String, SemanticTypeRecipe)>>> {
    with_check(program, proof, owner, limits, left, |check| {
        check.cleanup_fields(owner, ty, 0)
    })
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Copy,
    View,
    Own,
}

pub(super) fn category(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<Category> {
    with_check(program, proof, owner, limits, left, |check| {
        let observed = check.observation_type(owner, ty, 0)?;
        check.cleanup_category(owner, &observed, 1)
    })
}
pub(super) fn variant(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    name: &str,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<Vec<SemanticTypeRecipe>>> {
    with_check(program, proof, owner, limits, left, |check| {
        check.cleanup_variant(owner, ty, name, 0)
    })
}
pub(super) fn error_arms(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<(SemanticTypeRecipe, SemanticTypeRecipe)>> {
    with_check(program, proof, owner, limits, left, |check| {
        let ty = check.observation_type(owner, ty, 0)?;
        match ty {
            SemanticTypeRecipe::ErrorUnion { error, result } => Ok(Some((*error, *result))),
            _ => Ok(None),
        }
    })
}
pub(super) fn task_head(
    program: &SemanticModule,
    proof: &mut Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<bool>> {
    with_check(program, proof, owner, limits, left, |check| {
        check.cleanup_task_head(owner, ty, 0)
    })
}
impl Check<'_, '_> {
    fn cleanup_task_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::Opaque {
            definition, hidden, ..
        } = ty
        {
            return self.cleanup_task_head(*definition, hidden, depth + 1);
        }
        let ty = self.observation_type(owner, ty, depth + 1)?;
        match &ty {
            SemanticTypeRecipe::Task { .. } | SemanticTypeRecipe::Atom(Type::TaskI32) => {
                Ok(Some(false))
            }
            SemanticTypeRecipe::Managed(inner) => Ok(self
                .cleanup_task_head(owner, inner, depth + 1)?
                .map(|_| true)),
            SemanticTypeRecipe::Address { inner, .. } | SemanticTypeRecipe::Desc { inner, .. } => {
                Ok(self
                    .cleanup_task_head(owner, inner, depth + 1)?
                    .map(|_| false))
            }
            SemanticTypeRecipe::Qualified { storage, .. } => {
                self.cleanup_task_head(owner, storage, depth + 1)
            }
            _ => Ok(None),
        }
    }
    fn cleanup_category(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Category> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::Exact(value) = ty {
            crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
            if value.ty.is_copy_value() {
                return Ok(Category::Copy);
            }
            if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) {
                return Ok(Category::View);
            }
        }
        if let SemanticTypeRecipe::Qualified { storage, .. } = ty {
            return self.cleanup_category(owner, storage, depth + 1);
        }
        if matches!(
            ty,
            SemanticTypeRecipe::Address { .. }
                | SemanticTypeRecipe::Desc { .. }
                | SemanticTypeRecipe::Slice(_)
                | SemanticTypeRecipe::Dyn { owned: false, .. }
        ) {
            return Ok(Category::View);
        }
        Ok(if self.copy_type(owner, ty, depth + 1)? {
            Category::Copy
        } else {
            Category::Own
        })
    }
    fn cleanup_variant(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        name: &str,
        depth: usize,
    ) -> R<Option<Vec<SemanticTypeRecipe>>> {
        self.pay(name.len().saturating_add(1), depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
        if let SemanticTypeRecipe::Qualified { storage, .. } = &ty {
            return self.cleanup_variant(owner, storage, name, depth + 1);
        }
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = &ty
        else {
            return Ok(None);
        };
        let original = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &original.body
        else {
            return Ok(None);
        };
        self.call_requirements(owner, original, arguments, depth + 1)?;
        self.pay(variants.len(), depth)?;
        let mut selected = None;
        for variant in variants {
            self.pay(variant.name.len(), depth)?;
            if variant.name == name {
                if selected.replace(variant).is_some() {
                    return Err(bad("cleanup nominal has duplicate original variant names"));
                }
            }
        }
        let Some(variant) = selected else {
            return Err(bad(
                "cleanup pattern selects an absent original nominal variant",
            ));
        };
        self.pay(variant.fields.len(), depth)?;
        let mut out = Vec::with_capacity(variant.fields.len());
        for field in &variant.fields {
            out.push(super::super::instantiate::ty(
                self.program,
                &self.proof,
                field,
                super::super::instantiate::DomainMapping {
                    source: original.id,
                    use_definition: owner,
                    arguments,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?);
        }
        Ok(Some(out))
    }

    pub(super) fn cleanup_capability(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(hops),
            ordinal,
        } = ty
        {
            let mut holder = owner;
            for step in 0..*hops {
                self.pay(1, depth + step as usize)?;
                holder = self.definition(holder)?.parent.ok_or_else(|| {
                    bad("cleanup environment escapes its actual lexical ancestry")
                })?;
            }
            let (parent, source) = self
                .proof
                .environments
                .get(&(holder, *ordinal))
                .ok_or_else(|| {
                    pending("cleanup requires its sealed original capture Type source")
                })?;
            let parent = *parent;
            let source = copy_recipe(source, self.limits, self.left)?;
            self.contexts.push(parent);
            let result = self.cleanup_capability(parent, &source, depth + 1);
            self.contexts.pop();
            return result;
        }
        let ty = self.observation_type(owner, ty, depth + 1)?;
        // Releasing an address/description ends that view; it does not destroy
        // the referent and does not establish SyncDrop of its payload type.
        if matches!(
            ty,
            SemanticTypeRecipe::Address { .. }
                | SemanticTypeRecipe::Desc { .. }
                | SemanticTypeRecipe::Code { .. }
                | SemanticTypeRecipe::Slice(_)
                | SemanticTypeRecipe::Dyn { owned: false, .. }
        ) {
            return Ok(true);
        }
        if let SemanticTypeRecipe::Qualified { storage, .. } = &ty {
            return self.cleanup_capability(owner, storage, depth + 1);
        }
        if self.copy_type(owner, &ty, depth + 1)? {
            return Ok(true);
        }
        for requirement in self.original_requirements(owner, depth + 1)? {
            self.pay(1, depth)?;
            let premise = match &requirement {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound:
                        DynStructuralBound::SyncDrop
                        | DynStructuralBound::Copy
                        | DynStructuralBound::StoreElement,
                } => Some(ty),
                SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind:
                        SemanticBuiltinTrait::SyncDrop
                        | SemanticBuiltinTrait::Copy
                        | SemanticBuiltinTrait::StoreElement,
                } => Some(ty),
                _ => None,
            };
            if let Some(premise) = premise {
                if self.premise_type_equal(owner, &ty, premise, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        match &ty {
            SemanticTypeRecipe::Exact(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(&ty, self.limits, self.left, 1)?;
                // The complete original carrier uses the ordinary physical
                // Copy rule; its class domain is neither erased nor merged.
                if value.ty.is_copy_value()
                    || matches!(value.ty, Type::Address { .. } | Type::Desc(_))
                {
                    return Ok(true);
                }
            }
            SemanticTypeRecipe::Dyn { owned: true, .. } => {
                // Every legal erased owner carries its synchronous owning
                // cleanup bound. Construction proves exact concrete teardown
                // on failure and transfers that responsibility on success;
                // provider admission independently checks the whole bound.
                // Releasing this owner establishes no property of an erased
                // provider type and grants no external leaf ownership.
                return Ok(true);
            }
            // These rules release only a genuine initialized owner/view. They
            // never export a payload SyncDrop premise or a protocol phase.
            SemanticTypeRecipe::MutexGuard { .. } => return Ok(true),
            SemanticTypeRecipe::Managed(payload) => {
                return self.cleanup_owned_payload(owner, payload, depth + 1);
            }
            SemanticTypeRecipe::Closure {
                definition,
                environment,
                ..
            } => {
                let Some(moved) = self.proof.closure_move.get(definition).copied() else {
                    return Ok(false);
                };
                if !self.proof.complete.contains(definition) {
                    return Ok(false);
                }
                if !moved {
                    return Ok(true);
                }
                let child = self.definition(*definition)?;
                let names = self.proof.capture_names.get(definition).ok_or_else(|| {
                    pending("closure cleanup lacks its complete original capture directory")
                })?;
                self.pay(
                    names
                        .iter()
                        .fold(names.len(), |n, name| n.saturating_add(name.len())),
                    depth,
                )?;
                let names = self
                    .proof
                    .capture_names
                    .get(definition)
                    .ok_or_else(|| bad("original closure capture directory disappeared"))?
                    .clone();
                for name in names {
                    self.pay(child.environment.len(), depth)?;
                    let mut selected = None;
                    for (slot, formal) in child.environment.iter().enumerate() {
                        if matches!(&formal.source,SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{source:SemanticOriginalTypeSource::NameAt{name:source,..},..}) if source==&name)
                        {
                            if selected.replace(slot).is_some() {
                                return Err(bad(
                                    "one physical capture has ambiguous original whole-Type slots",
                                ));
                            }
                        }
                    }
                    let Some(slot) = selected else {
                        return Ok(false);
                    };
                    let Some(SemanticActualRecipe::Type(actual)) = environment.get(slot) else {
                        return Ok(false);
                    };
                    if !self.cleanup_capability(owner, actual, depth + 1)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                let callee = self.definition(*definition)?;
                if !self.proof.complete.contains(definition) {
                    return Ok(false);
                }
                let result = self
                    .proof
                    .types
                    .get(&(*definition, Query::Result))
                    .ok_or_else(|| {
                        pending("opaque cleanup lacks the independently checked defining result")
                    })?;
                crate::dyn_metadata::semantic_operand_snapshot(result, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    hidden.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                if result != hidden.as_ref() {
                    return Err(bad(
                        "opaque cleanup hidden type differs from its original all-return theorem",
                    ));
                }
                self.call_requirements(owner, callee, arguments, depth + 1)?;
                self.contexts.push(*definition);
                let result = self.cleanup_capability(*definition, hidden, depth + 1);
                self.contexts.pop();
                return result;
            }
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                for field in fields {
                    if !self.cleanup_capability(owner, field, depth + 1)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            SemanticTypeRecipe::Array { element, .. } => {
                return self.cleanup_capability(owner, element, depth + 1);
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                return Ok(self.cleanup_capability(owner, error, depth + 1)?
                    && self.cleanup_capability(owner, result, depth + 1)?);
            }
            SemanticTypeRecipe::Nominal { definition, .. } => {
                let original = self.definition(*definition)?;
                if let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Enum { variants },
                    ..
                } = &original.body
                {
                    self.pay(variants.len(), depth)?;
                    for variant in variants {
                        let fields = self
                            .cleanup_variant(owner, &ty, &variant.name, depth + 1)?
                            .ok_or_else(|| bad("cleanup enum lost its original declaration"))?;
                        for field in fields {
                            if !self.cleanup_capability(owner, &field, depth + 1)? {
                                return Ok(false);
                            }
                        }
                    }
                    return Ok(true);
                }
                if matches!(
                    original.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Record | LifeNominalKind::Package,
                        body: SemanticNominalBody::Product { .. },
                        ..
                    }
                ) {
                    let fields = self
                        .cleanup_fields(owner, &ty, depth + 1)?
                        .ok_or_else(|| bad("cleanup product lost its actual source declaration"))?;
                    for (_, field) in fields {
                        if !self.cleanup_capability(owner, &field, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
            }
            _ => {}
        }
        if let Some(value) =
            self.sealed_capability(owner, &ty, SemanticBuiltinTrait::SyncDrop, depth + 1)?
        {
            return Ok(value);
        }
        Ok(false)
    }
    fn cleanup_owned_payload(
        &mut self,
        owner: SemanticDefinitionId,
        payload: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let payload = self.observation_type(owner, payload, depth + 1)?;
        match &payload {
            SemanticTypeRecipe::Atom(Type::ByteBuffer) => return Ok(true),
            SemanticTypeRecipe::OwnedStore(element) | SemanticTypeRecipe::Mutex(element) => {
                return self.cleanup_capability(owner, element, depth + 1);
            }
            // A legal synchronous registration owner preserves the constructor's
            // callback cleanup invariant. This does not prove SyncDrop(F).
            SemanticTypeRecipe::Registration(_) => return Ok(true),
            SemanticTypeRecipe::AsyncRegistration(_)
            | SemanticTypeRecipe::Task { .. }
            | SemanticTypeRecipe::Atom(Type::TaskI32) => return Ok(false),
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let declaration = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    kind: LifeNominalKind::Resource,
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &declaration.body
                else {
                    return Ok(false);
                };
                self.call_requirements(owner, declaration, arguments, depth + 1)?;
                self.pay(
                    fields.len().saturating_add(self.program.definitions.len()),
                    depth,
                )?;
                for field in fields {
                    let ty = super::super::instantiate::ty(
                        self.program,
                        &self.proof,
                        &field.ty,
                        super::super::instantiate::DomainMapping {
                            source: declaration.id,
                            use_definition: owner,
                            arguments,
                            parent_call: None,
                        },
                        self.limits,
                        self.left,
                    )?;
                    if !self.cleanup_capability(owner, &ty, depth + 1)? {
                        return Ok(false);
                    }
                }
                let mut selected = None;
                for scope in &self.program.definitions {
                    if !matches!(
                        scope.body,
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl {
                            trait_kind: SemanticBuiltinTrait::Drop,
                            ..
                        })
                    ) {
                        continue;
                    }
                    match self.solve_parent(owner, scope, &payload, depth + 1) {
                        Ok(_) => {}
                        Err(diagnostic)
                            if diagnostic.category == zil_base::ResultCategory::SafeReject =>
                        {
                            continue;
                        }
                        Err(diagnostic) => return Err(diagnostic),
                    }
                    self.pay(self.program.definitions.len(), depth)?;
                    let mut methods = self.program.definitions.iter().filter(|method| {
                        method.parent == Some(scope.id)
                            && method.kind == SemanticDefinitionKind::Method
                    });
                    let Some(method) = methods.next() else {
                        return Err(bad("original resource Drop provider has no method"));
                    };
                    if methods.next().is_some() || selected.replace(scope.id).is_some() {
                        return Err(bad(
                            "original resource cleanup has ambiguous Drop providers",
                        ));
                    }
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature,
                        body: SemanticOriginalFunctionBody::Defined(_),
                        ..
                    } = &method.body
                    else {
                        return Ok(false);
                    };
                    if signature.call_kind != CallKind::Direct
                        || !self.proof.complete.contains(&method.id)
                    {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    fn cleanup_fields(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<Vec<(String, SemanticTypeRecipe)>>> {
        self.pay(1, depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
        if let SemanticTypeRecipe::Qualified { storage, .. } = &ty {
            return self.cleanup_fields(owner, storage, depth + 1);
        }
        if let SemanticTypeRecipe::Tuple(fields) = &ty {
            self.pay(fields.len().saturating_mul(32), depth)?;
            let mut out = Vec::with_capacity(fields.len());
            for (index, field) in fields.iter().enumerate() {
                out.push((
                    index.to_string(),
                    copy_recipe(field, self.limits, self.left)?,
                ));
            }
            return Ok(Some(out));
        }
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = &ty
        else {
            return Ok(None);
        };
        let original = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Record | LifeNominalKind::Package,
            body: SemanticNominalBody::Product { fields },
            ..
        } = &original.body
        else {
            return Ok(None);
        };
        self.call_requirements(owner, original, arguments, depth + 1)?;
        self.pay(fields.len(), depth)?;
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            self.pay(field.name.len().saturating_add(1), depth)?;
            let ty = super::super::instantiate::ty(
                self.program,
                &self.proof,
                &field.ty,
                super::super::instantiate::DomainMapping {
                    source: original.id,
                    use_definition: owner,
                    arguments,
                    parent_call: None,
                },
                self.limits,
                self.left,
            )?;
            out.push((field.name.clone(), ty));
        }
        Ok(Some(out))
    }
}
