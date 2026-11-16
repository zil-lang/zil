//! Instantiate the original method contract and its complete receiver Context.
//! This reconstructs untrusted declaration uppers; it does not prove a provider
//! body, an actual source, an access permission, or a cleanup obligation.
use super::*;
fn resolved_type(value: &SemanticTypeValue) -> R<&Type> {
    match value {
        SemanticTypeValue::Resolved { ty, .. } => Ok(ty),
        _ => Err(bad("Dyn contract source projection remains unresolved")),
    }
}

struct Contract<'a, 'b> {
    method: &'a SemanticDefinition,
    signature: &'a SemanticSignatureRecipe,
    parent: &'a SemanticDefinition,
    parent_actuals: &'a [DynGenericActual],
    interface: &'a DynInterface,
    slot: &'a DynSlot,
    parameters: &'a [std::rc::Rc<SemanticTypeValue>],
    result: &'a SemanticTypeValue,
    limits: &'a Limits,
    left: &'b mut usize,
}
impl Contract<'_, '_> {
    fn push<T>(&mut self, values: &mut Vec<T>, value: T) -> R<()> {
        if values.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Dyn contract contextual inventory exceeds node limit",
                None,
            ));
        }
        provider_fee(
            self.left,
            1 + if values.len() == values.capacity() {
                values.len()
            } else {
                0
            },
        )?;
        values.try_reserve(1).map_err(|_| {
            Diagnostic::limit(
                Phase::Types,
                "Dyn contract contextual allocation failed",
                None,
            )
        })?;
        values.push(value);
        Ok(())
    }
    fn leaf(&mut self, pattern: &DynTypePattern) -> R<Type> {
        let selected = match pattern {
            DynTypePattern::Concrete(ty) => ty,
            DynTypePattern::Semantic { scope, ty } if *scope == self.method.id => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    ty.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(
                    &self.signature.result,
                    self.limits,
                    self.left,
                    1,
                )?;
                let mut selected =
                    (ty.as_ref() == &self.signature.result).then_some(resolved_type(self.result)?);
                for (ordinal, parameter) in self.signature.parameters.iter().enumerate() {
                    provider_fee(self.left, 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        ty.as_ref(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &parameter.ty,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if &parameter.ty != ty.as_ref() {
                        continue;
                    }
                    // A retained source leaf selects a real result/argument,
                    // never an erased receiver pretending to be its payload.
                    if ordinal == 0 {
                        continue;
                    }
                    let actual = self
                        .parameters
                        .get(ordinal)
                        .ok_or_else(|| bad("Dyn source leaf loses its exact parameter ordinal"))?;
                    let actual = resolved_type(actual)?;
                    if selected.is_some_and(|old| old != actual) {
                        return Err(bad(
                            "Dyn source leaf has contradictory complete actual signature types",
                        ));
                    }
                    selected = Some(actual);
                }
                selected.ok_or_else(|| {
                    unknown(
                        "Dyn source leaf needs its complete original signature type substitution",
                    )
                })?
            }
            _ => {
                return Err(unknown(
                    "Dyn source leaf needs its complete original scoped recipe",
                ));
            }
        };
        inference::charge_type_snapshot(selected, Span::default(), self.limits, self.left, 1)?;
        Ok(selected.clone())
    }
    fn source(&mut self, source: &DynStorageSourceRecipe) -> R<StorageSource> {
        provider_fee(self.left, 1)?;
        Ok(match source {
            DynStorageSourceRecipe::Exact(p) => StorageSource::Exact(p.clone()),
            DynStorageSourceRecipe::PlaceAddress { place, mode } => StorageSource::PlaceAddress {
                place: place.clone(),
                mode: *mode,
            },
            DynStorageSourceRecipe::OwnedView { head, leaf, mode } => StorageSource::OwnedView {
                head: head.clone(),
                leaf: self.leaf(leaf)?,
                mode: *mode,
            },
            DynStorageSourceRecipe::ReborrowedLeaf { source, head } => {
                StorageSource::ReborrowedLeaf {
                    source: source.clone(),
                    head: head.clone(),
                }
            }
            DynStorageSourceRecipe::Static { leaf, mode } => StorageSource::Static {
                leaf: self.leaf(leaf)?,
                mode: *mode,
            },
        })
    }
    fn sources(&mut self, source: &[DynStorageSourceRecipe]) -> R<Vec<StorageSource>> {
        let mut values = Vec::new();
        for source in source {
            let value = self.source(source)?;
            self.push(&mut values, value)?;
        }
        Ok(values)
    }
    fn effects(&mut self, source: &[DynEffectRecipe]) -> R<Vec<DynEffect>> {
        let mut values = Vec::new();
        for effect in source {
            let value = match effect {
                DynEffectRecipe::Alloc => DynEffect::Alloc,
                DynEffectRecipe::Io => DynEffect::Io,
                DynEffectRecipe::Block => DynEffect::Block,
                DynEffectRecipe::Suspend => DynEffect::Suspend,
                DynEffectRecipe::Spawn => DynEffect::Spawn,
                DynEffectRecipe::Ffi => DynEffect::Ffi,
                DynEffectRecipe::Reenter => DynEffect::Reenter,
                DynEffectRecipe::Panic => DynEffect::Panic,
                DynEffectRecipe::Call(position) => DynEffect::Call(position.clone()),
                DynEffectRecipe::Cleanup(ty) => DynEffect::Cleanup(self.leaf(ty)?),
            };
            self.push(&mut values, value)?;
        }
        Ok(values)
    }
    fn base(&mut self, recipe: &DynContractRecipe) -> R<DynContract> {
        let requires = if let Some(source) = &recipe.requires {
            let mut values = Vec::new();
            for row in source {
                let value = match row {
                    DynRequirementRecipe::Read(p) => DynRequirement::Read(p.clone()),
                    DynRequirementRecipe::Write(p) => DynRequirement::Write(p.clone()),
                    DynRequirementRecipe::Manage(p) => DynRequirement::Manage(p.clone()),
                    DynRequirementRecipe::Apart(a, b) => {
                        DynRequirement::Apart(a.clone(), b.clone())
                    }
                    DynRequirementRecipe::State {
                        position,
                        ty,
                        variant,
                    } => DynRequirement::State {
                        position: position.clone(),
                        ty: self.leaf(ty)?,
                        variant: *variant,
                    },
                };
                self.push(&mut values, value)?;
            }
            Some(values)
        } else {
            None
        };
        let guarantees = if let Some(source) = &recipe.guarantees {
            let mut values = Vec::new();
            for row in source {
                let value = match row {
                    DynGuaranteeRecipe::StorageSources(binding) => {
                        DynGuarantee::StorageSources(StorageSourceBinding {
                            position: binding.position.clone(),
                            sources: self.sources(&binding.sources)?,
                        })
                    }
                    DynGuaranteeRecipe::ManageResult => DynGuarantee::ManageResult,
                    DynGuaranteeRecipe::State {
                        position,
                        ty,
                        variant,
                    } => DynGuarantee::State {
                        position: position.clone(),
                        ty: self.leaf(ty)?,
                        variant: *variant,
                    },
                };
                self.push(&mut values, value)?;
            }
            Some(values)
        } else {
            None
        };
        let result = match &recipe.result {
            Some(DynResultSourceRecipe::Value) => Some(DynResultSource::Value),
            Some(DynResultSourceRecipe::Create) => Some(DynResultSource::Create),
            Some(DynResultSourceRecipe::Give) => Some(DynResultSource::Give),
            Some(DynResultSourceRecipe::Borrow { sources, mutable }) => {
                Some(DynResultSource::Borrow {
                    sources: self.sources(sources)?,
                    mutable: *mutable,
                })
            }
            None => None,
        };
        let cleanup = if let Some(row) = &recipe.cleanup {
            Some(DynCleanupContract {
                environment: row.environment,
                positions: row.positions.clone(),
                access: row.access.clone(),
                effects: self.effects(&row.effects)?,
            })
        } else {
            None
        };
        Ok(DynContract {
            requires,
            guarantees,
            hold: recipe.hold.clone(),
            call: recipe.call.clone(),
            result,
            cleanup,
            effects: recipe
                .effects
                .as_ref()
                .map(|source| self.effects(source))
                .transpose()?,
            resume: recipe.resume,
            threads: recipe.threads.clone(),
        })
    }
    fn binder(&mut self, lifetime: &SemanticOuterLifeRecipe) -> R<Option<StorageBinder>> {
        let signature = self.signature;
        provider_fee(self.left, self.signature.parameters.len().saturating_add(1))?;
        let SemanticOuterLifeRecipe::Class(lifetime) = lifetime else {
            return Ok(None);
        };
        if let SemanticLifeRecipe::Formal(SemanticFormalRef { scope, ordinal }) = lifetime {
            let (formals, actuals) = match scope {
                SemanticScopeRef::Ancestor(0) => (
                    self.method.parameters.as_slice(),
                    self.slot.actuals.as_slice(),
                ),
                SemanticScopeRef::Ancestor(1) => {
                    (self.parent.parameters.as_slice(), self.parent_actuals)
                }
                _ => {
                    return Err(unknown(
                        "Dyn contract lifetime needs its complete lexical ancestor map",
                    ));
                }
            };
            if !matches!(
                formals.get(*ordinal as usize),
                Some(SemanticFormal::Lifetime)
            ) {
                return Err(bad(
                    "Dyn contract lifetime changes its full mixed formal kind",
                ));
            }
            let Some(DynGenericActual::Life(binder)) = actuals.get(*ordinal as usize) else {
                return Err(bad(
                    "Dyn contract lifetime omits its complete original actual",
                ));
            };
            if matches!(binder,StorageBinder::Fixed(class) if *class>=self.interface.retained_life.binder_count)
                || matches!(binder,StorageBinder::Invocation(class) if *class>=self.slot.storage_life.as_ref().ok_or_else(||bad("Dyn contract omits its complete lifetime header"))?.invocation_binder_count)
                || matches!(
                    (scope, binder),
                    (SemanticScopeRef::Ancestor(1), StorageBinder::Invocation(_))
                )
            {
                return Err(bad(
                    "Dyn contract lifetime leaves its complete lexical or Context domain",
                ));
            }
            return Ok(Some(*binder));
        }
        if let SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ..
        } = lifetime
        {
            let mut invocation = Vec::<&SemanticLifeRecipe>::new();
            for parameter in &signature.parameters {
                let SemanticTypeRecipe::Address {
                    lifetime: SemanticOuterLifeRecipe::Class(candidate),
                    ..
                } = &parameter.ty
                else {
                    continue;
                };
                if !matches!(
                    candidate,
                    SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ..
                    }
                ) {
                    continue;
                }
                if !invocation.contains(&candidate) {
                    self.push(&mut invocation, candidate)?;
                }
                if candidate == lifetime {
                    let index = invocation
                        .iter()
                        .position(|old| *old == candidate)
                        .ok_or_else(|| bad("Dyn invocation occurrence disappeared"))?;
                    return Ok(Some(StorageBinder::Invocation(
                        u32::try_from(index).map_err(|_| bad("Dyn invocation domain overflows"))?,
                    )));
                }
            }
        }
        Err(unknown(
            "Dyn contract lifetime needs its complete original invocation/result relation",
        ))
    }
    fn extend(&mut self, contract: &mut DynContract) -> R<()> {
        let signature = self.signature;
        let interface = self.interface;
        if self.interface.retained_life.ports.is_empty() {
            return Ok(());
        }
        let mut arguments = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate().skip(1) {
            if let SemanticTypeRecipe::Address { lifetime, .. } = &parameter.ty {
                let binder = self.binder(lifetime)?;
                let index =
                    u32::try_from(index).map_err(|_| bad("Dyn parameter ordinal overflows"))?;
                self.push(&mut arguments, (index, binder))?;
            }
        }
        let result_binder =
            if let SemanticTypeRecipe::Address { lifetime, .. } = &self.signature.result {
                self.binder(lifetime)?
            } else {
                None
            };
        for (ordinal, port) in interface.retained_life.ports.iter().enumerate() {
            inference::charge_type_snapshot(
                &port.leaf,
                Span::default(),
                self.limits,
                self.left,
                1,
            )?;
            let ordinal =
                u32::try_from(ordinal).map_err(|_| bad("Dyn Context port ordinal overflows"))?;
            let position = StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::RetainedPort(ordinal)],
            };
            let requires = contract
                .requires
                .as_mut()
                .ok_or_else(|| unknown("Dyn Context has no complete original requirement upper"))?;
            self.push(requires, DynRequirement::Read(position.clone()))?;
            let call = contract
                .call
                .as_mut()
                .ok_or_else(|| unknown("Dyn Context has no complete original call-access upper"))?;
            self.push(&mut call.reads, position.clone())?;
            if self.slot.receiver.mutable && port.mode == StoredBorrowMode::ExclusiveAddress {
                self.push(&mut call.writes, position.clone())?;
                self.push(requires, DynRequirement::Write(position.clone()))?;
            }
            let mut sources = Vec::new();
            self.push(&mut sources, StorageSource::Exact(position.clone()))?;
            if self.slot.receiver.mutable {
                for (index, binder) in &arguments {
                    if *binder == Some(StorageBinder::Fixed(port.binder))
                        && self
                            .parameters
                            .get(*index as usize)
                            .map(|value| resolved_type(value))
                            .transpose()?
                            == Some(&port.leaf)
                    {
                        self.push(
                            &mut sources,
                            StorageSource::Exact(StoragePosition {
                                base: StorageBase::Parameter(*index),
                                path: Vec::new(),
                            }),
                        )?;
                    }
                }
            }
            let guarantees = contract.guarantees.as_mut().ok_or_else(|| {
                unknown("Dyn Context has no complete original after-source upper")
            })?;
            self.push(
                guarantees,
                DynGuarantee::StorageSources(StorageSourceBinding {
                    position: position.clone(),
                    sources,
                }),
            )?;
            if let Some(DynResultSource::Borrow { sources, .. }) = &mut contract.result {
                match result_binder {
                    Some(StorageBinder::Fixed(class))
                        if class == port.binder
                            && port.mode == StoredBorrowMode::SharedAddress
                            && &port.leaf == resolved_type(self.result)? =>
                    {
                        self.push(sources, StorageSource::Exact(position))?
                    }
                    Some(StorageBinder::Invocation(0))
                        if matches!((&port.leaf,resolved_type(self.result)?),(Type::Address{inner:a,..},Type::Address{mutable:false,inner:b})if a==b)
                            && matches!(
                                port.mode,
                                StoredBorrowMode::SharedAddress
                                    | StoredBorrowMode::ExclusiveAddress
                            ) =>
                    {
                        self.push(
                            sources,
                            StorageSource::ReborrowedLeaf {
                                source: position,
                                head: StoragePosition {
                                    base: StorageBase::Parameter(0),
                                    path: Vec::new(),
                                },
                            },
                        )?
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

pub(super) fn compare(
    program: &SemanticModule,
    method: &SemanticDefinition,
    parent_actuals: &[DynGenericActual],
    interface: &DynInterface,
    slot: &DynSlot,
    recipe: &DynContractRecipe,
    parameters: &[std::rc::Rc<SemanticTypeValue>],
    result: &SemanticTypeValue,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
    let context = &interface.retained_life;
    for count in [
        context.binder_count as usize,
        context.ports.len(),
        context.retained.len(),
        context.nominal_arguments.len(),
        slot.parameters.len(),
    ] {
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Dyn contextual contract metadata exceeds node limit",
                None,
            ));
        }
        provider_fee(left, count.saturating_add(1))?;
    }
    for port in &context.ports {
        inference::charge_type_snapshot(&port.leaf, Span::default(), limits, left, 1)?;
    }
    for path in context
        .retained
        .iter()
        .map(|row| row.path.as_slice())
        .chain(
            context
                .nominal_arguments
                .iter()
                .map(|row| row.path.as_slice()),
        )
    {
        if path.len() > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Dyn contextual contract metadata path exceeds depth limit",
                None,
            ));
        }
        provider_fee(left, path.len().saturating_add(1))?;
    }
    for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
        inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
    }

    provider_fee(left, program.definitions.len())?;
    let parent_id = method
        .parent
        .ok_or_else(|| bad("Dyn contract method has no original lexical parent"))?;
    let mut parents = program
        .definitions
        .iter()
        .filter(|definition| definition.id == parent_id);
    let parent = parents
        .next()
        .ok_or_else(|| bad("Dyn contract original parent is absent"))?;
    if parents.next().is_some()
        || parent.parameters.len() != parent_actuals.len()
        || method.parameters.len() != slot.actuals.len()
    {
        return Err(bad(
            "Dyn contract changes its complete original mixed scope",
        ));
    }
    if parameters.len() != slot.parameters.len().saturating_add(1)
        || resolved_type(result)? != &slot.result
    {
        return Err(bad(
            "Dyn contract result differs from its independently interpreted original signature",
        ));
    }
    for (value, actual) in parameters.iter().skip(1).zip(&slot.parameters) {
        if resolved_type(value)? != actual {
            return Err(bad(
                "Dyn contract parameter differs from its independently interpreted original signature",
            ));
        }
    }
    let signature = original_signature(method)?;
    let mut evaluator = Contract {
        method,
        signature,
        parent,
        parent_actuals,
        interface,
        slot,
        parameters,
        result,
        limits,
        left,
    };
    let mut expected = evaluator.base(recipe)?;
    evaluator.extend(&mut expected)?;
    if expected != slot.contract {
        return Err(bad(
            "Dyn contextual contract changes its complete original requirement, source, access, cleanup, or effect upper",
        ));
    }
    Ok(())
}
