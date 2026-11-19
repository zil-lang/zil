//! Rigid entries of root functions with immutable, closed type actuals.
//! An occurrence still checks the callee premises and projects every retained
//! world through its own caller boundary.
use super::*;

type Target = (SemanticDefinitionId, FunctionId);

#[derive(Default)]
pub(in crate::semantic) struct RigidWorlds {
    by_target: Option<BTreeMap<Target, Vec<usize>>>,
    eligible: BTreeMap<SemanticDefinitionId, bool>,
    signatures: BTreeMap<SemanticInstanceId, (Vec<CheckedActual>, CheckedSignature)>,
}

fn lookup(len: usize) -> usize {
    len.checked_ilog2().map_or(1, |n| n as usize + 2)
}

impl CheckedContext {
    pub(super) fn world_candidates(
        &self,
        module: &TypedModule,
        program: &SemanticModule,
        function: FunctionId,
        definition: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<Vec<usize>> {
        let key = (definition, function);
        if !self.site_index.same_input(module, b) || program.instances.len() < 8 {
            b.spend(program.instances.len())?;
            return Ok(program
                .instances
                .iter()
                .enumerate()
                .filter_map(|(i, row)| {
                    (row.definition == definition
                        && row.target
                            == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(
                                function,
                            )))
                    .then_some(i)
                })
                .collect());
        }
        let mut cache = self.rigid_worlds.borrow_mut();
        if cache.by_target.is_none() {
            let mut positions: BTreeMap<Target, Vec<usize>> = BTreeMap::new();
            for (index, row) in program.instances.iter().enumerate() {
                b.spend(1)?;
                if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) =
                    row.target
                {
                    b.spend(lookup(positions.len()))?;
                    positions
                        .entry((row.definition, id))
                        .or_default()
                        .push(index);
                }
            }
            cache.by_target = Some(positions);
        }
        let positions = cache.by_target.as_ref().unwrap();
        b.spend(lookup(positions.len()))?;
        let rows = positions.get(&key).map(Vec::as_slice).unwrap_or_default();
        b.spend(rows.len())?;
        Ok(rows.to_vec())
    }

    pub(super) fn rigid_entry_signature(
        &self,
        module: &TypedModule,
        definition: &SemanticDefinition,
        recipe: &SemanticSignatureRecipe,
        world: &CallWorld,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedSignature> {
        let env = &world.environment;
        let actuals = match env.lexical.as_slice() {
            [frame]
                if frame.key == ScopeKey::Instance(world.instance)
                    && frame.definition == definition.id
                    && frame.environment.is_empty()
                    && frame.self_type.is_none() =>
            {
                &frame.actuals
            }
            _ => return self.entry_signature(module, definition, recipe, env, external, active, b),
        };
        if !self.site_index.same_input(module, b)
            || self.domain_only.get()
            || self.original.borrow().is_none()
            || active.depth() != 0
            || env.assumptions.as_slice() != [ScopeKey::Instance(world.instance)]
            || !env.images.is_empty()
            || !env.bounds.is_empty()
            || env.at.is_some()
            || !matches!(&definition.body, SemanticDefinitionBody::FunctionWithBody { signature, .. }
                if std::ptr::eq(signature, recipe))
        {
            return self.entry_signature(module, definition, recipe, env, external, active, b);
        }
        // environment_for_instance always creates the real code's invocation frame.
        // A root signature creates its own invocation domain inside entry_signature;
        // eligibility below requires that domain to be empty, and permits only
        // declared Implicit heads. The function's independent header check still
        // relates those heads to the current actual invocation at its boundary.
        let mut cache = self.rigid_worlds.borrow_mut();
        b.spend(lookup(cache.signatures.len()))?;
        if let Some((inputs, prior)) = cache.signatures.get(&world.instance) {
            b.spend(inputs.len().saturating_add(1))?;
            let mut same = inputs.len() == actuals.len();
            for (old, current) in inputs.iter().zip(actuals) {
                let (CheckedActual::Type(old), CheckedActual::Type(current)) = (old, current)
                else {
                    same = false;
                    break;
                };
                if current.life != empty_life()
                    || !current.statics.is_empty()
                    || !current.origins.is_empty()
                    || !current.classes.is_empty()
                    || !old.same_raw_type(current, b)?
                {
                    same = false;
                    break;
                }
            }
            if same {
                active.fresh(b)?; // This use still owns its empty invocation frame.
                charge_signature(prior, b)?;
                return Ok(prior.clone());
            }
        }
        b.spend(lookup(cache.eligible.len()))?;
        let eligible = match cache.eligible.get(&definition.id) {
            Some(known) => *known,
            None => {
                let eligible = independent_recipe(self, module, definition, recipe, b)?;
                cache.eligible.insert(definition.id, eligible);
                eligible
            }
        };
        drop(cache);
        let mut immutable = eligible;
        if immutable {
            for actual in actuals {
                b.spend(1)?;
                let CheckedActual::Type(value) = actual else {
                    immutable = false;
                    break;
                };
                if value.life != empty_life()
                    || !value.classes.is_empty()
                    || !value.origins.is_empty()
                    || !value.statics.is_empty()
                    || !match value.ty {
                        Type::Closure(id) => instances::closed_callback(module, id, b)?,
                        _ => type_storage::closed_storage(&value.ty, b)?,
                    }
                {
                    immutable = false;
                    break;
                }
            }
        }
        let signature =
            self.entry_signature(module, definition, recipe, env, external, active, b)?;
        if immutable && signature.bound.is_empty() && signature.invocation.is_empty() {
            let scope = ScopeKey::Instance(world.instance);
            let mut stable = true;
            for value in signature
                .parameters
                .iter()
                .map(|(ty, _)| ty)
                .chain([&signature.result])
            {
                b.spend(
                    value
                        .classes
                        .len()
                        .saturating_add(value.origins.len())
                        .saturating_add(1),
                )?;
                stable &= value.statics.is_empty() && value.classes.iter()
                    .all(|key| matches!(key, ClassKey::Implicit { scope: owner, .. } if *owner == scope))
                    && value.origins.iter().all(|origin| matches!(origin,
                        IntrinsicOrigin::DerivedClass { source: ClassKey::Implicit {scope: owner, ..} } if *owner == scope));
            }
            if stable {
                charge_signature(&signature, b)?;
                for actual in actuals {
                    if let CheckedActual::Type(value) = actual {
                        value.charge_clone(b)?;
                    }
                }
                self.rigid_worlds
                    .borrow_mut()
                    .signatures
                    .insert(world.instance, (actuals.clone(), signature.clone()));
            }
        }
        Ok(signature)
    }
}

fn charge_signature(signature: &CheckedSignature, b: &mut Budget) -> Check<()> {
    b.spend(signature.parameters.len().saturating_add(1))?;
    for (parameter, _) in &signature.parameters {
        parameter.charge_clone(b)?;
    }
    signature.result.charge_clone(b)
}

fn independent_recipe(
    context: &CheckedContext,
    module: &TypedModule,
    root: &SemanticDefinition,
    recipe: &SemanticSignatureRecipe,
    b: &mut Budget,
) -> Check<bool> {
    if root.kind != SemanticDefinitionKind::Function
        || root.parent.is_some()
        || !root.environment.is_empty()
        || !root.requirements.is_empty()
        || recipe.bound_lifetimes != 0
        || recipe.invocation_lifetimes != 0
    {
        return Ok(false);
    }
    for (ordinal, formal) in root.parameters.iter().enumerate() {
        b.spend(1)?;
        let SemanticFormal::Type { requirements } = formal else {
            return Ok(false);
        };
        b.spend(requirements.len())?;
        // The caller of this cache always interprets each complete formal requirement
        // in this exact world, including callable obligations and their proof requests.
        if !requirements.iter().all(|requirement| {
            let ty = match requirement {
                SemanticRequirementRecipe::Structural { ty, .. } |
                SemanticRequirementRecipe::Callable { ty, kind: CallableKind::Fn, .. } => ty,
                _ => return false,
            };
            matches!(ty, SemanticTypeRecipe::Formal(SemanticFormalRef { scope: SemanticScopeRef::Ancestor(0), ordinal: i }) if *i as usize == ordinal)
        }) { return Ok(false); }
    }
    for ty in recipe
        .parameters
        .iter()
        .map(|p| &p.ty)
        .chain([&recipe.result])
    {
        let closed = match ty {
            SemanticTypeRecipe::Address {
                lifetime:
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    }),
                inner,
                ..
            } => {
                b.spend(root.implicit_lifetimes.len())?;
                if !root
                    .implicit_lifetimes
                    .iter()
                    .any(|row| row.ordinal == *ordinal)
                {
                    return Ok(false);
                }
                inner.as_ref()
            }
            ty => ty,
        };
        if !nominal::closed_fields::independent_signature_type(context, module, root, closed, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_index_keeps_every_distinct_world_of_the_same_physical_entry() {
        let limits = Limits::default();
        let (mut module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../fixtures/ordinary-identity-complete-call-sites.wire"),
            &limits,
        )
        .unwrap();
        let program = module.semantic.as_mut().unwrap();
        let first = program
            .instances
            .iter()
            .find(|row| {
                matches!(
                    row.target,
                    SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
                )
            })
            .unwrap()
            .clone();
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(entry)) = first.target
        else {
            unreachable!()
        };
        let mut next = program.instances.iter().map(|r| r.id.index).max().unwrap() + 1;
        for _ in 0..8 {
            let mut world = first.clone();
            world.id.index = next;
            next += 1;
            program.instances.push(world);
        }
        let expected: Vec<_> = program
            .instances
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                (row.definition == first.definition && row.target == first.target).then_some(i)
            })
            .collect();
        let mut b = Budget::new(&limits);
        let context = CheckedContext::definition_catalogue(&module, None, &mut b).unwrap();
        let program = module.semantic.as_ref().unwrap();
        // Both the cold index and the subsequent lookup retain the last instance.
        for _ in 0..2 {
            assert_eq!(
                context
                    .world_candidates(&module, program, entry, first.definition, &mut b)
                    .unwrap(),
                expected
            );
        }
        let mut absent = first.definition;
        absent.index = u32::MAX;
        assert!(
            context
                .world_candidates(&module, program, entry, absent, &mut b)
                .unwrap()
                .is_empty()
        );
    }
}
