//! Rigid entries of root functions with immutable, closed type actuals.
//! Each occurrence establishes its callee's premises in the current journal
//! and projects every retained world through its own caller boundary.
use super::*;

type Target = (SemanticDefinitionId, FunctionId);

#[derive(Default)]
pub(in crate::semantic) struct RigidWorlds {
    by_target: Option<BTreeMap<Target, Vec<usize>>>,
    eligible: BTreeMap<SemanticDefinitionId, bool>,
    signatures: BTreeMap<SemanticInstanceId, Entry>,
    projected: BTreeMap<(SemanticDefinitionId, FunctionId), std::rc::Rc<CheckedSignature>>,
}

struct Entry {
    inputs: Vec<CheckedActual>,
    signature: std::rc::Rc<CheckedSignature>,
    projected: Option<std::rc::Rc<CheckedSignature>>,
    declarations: Option<DeclarationPrefix>,
}

/// Valid only within a single actual call and its particular witness scope.
pub(super) struct Projection {
    model: std::rc::Rc<CheckedSignature>,
    image: ScopeKey,
}

pub(super) enum EntryResult {
    Checked(CheckedSignature, Option<Projection>),
    SameProjection,
}

struct DeclarationPrefix {
    scope: std::rc::Rc<()>,
    length: usize,
    callable_frames: usize,
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

    fn rigid_inputs<'w>(
        &self,
        module: &TypedModule,
        definition: &SemanticDefinition,
        recipe: &SemanticSignatureRecipe,
        world: &'w CallWorld,
        active: &ActiveQueries,
        b: &Budget,
    ) -> Option<&'w Vec<CheckedActual>> {
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
            _ => return None,
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
            return None;
        }
        Some(actuals)
    }

    pub(super) fn world_declarations_present(
        &self,
        module: &TypedModule,
        definition: &SemanticDefinition,
        recipe: &SemanticSignatureRecipe,
        world: &CallWorld,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<bool> {
        let Some(inputs) = self.rigid_inputs(module, definition, recipe, world, active, b) else {
            return Ok(false);
        };
        let rigid = self.rigid_worlds.borrow();
        b.spend(lookup(rigid.signatures.len()))?;
        let Some(Entry {
            inputs: prior,
            declarations: Some(prefix),
            ..
        }) = rigid.signatures.get(&world.instance)
        else {
            return Ok(false);
        };
        let graph = self.premises.borrow();
        b.spend(lookup(graph.len()))?;
        let Some(rows) = graph.get(&ScopeKey::Instance(world.instance)) else {
            return Ok(false);
        };
        if !std::rc::Rc::ptr_eq(&prefix.scope, &rows.prefix)
            || rows.external.len() < prefix.length
            || !same_inputs(prior, inputs, b)?
        {
            return Ok(false);
        }
        // Every admitted Callable in this journal has its own empty invocation
        // frame. Caller obligations and the actual closure remain independent.
        for _ in 0..prefix.callable_frames {
            active.fresh(b)?;
        }
        Ok(true)
    }

    pub(super) fn rigid_entry_signature(
        &self,
        module: &TypedModule,
        definition: &SemanticDefinition,
        recipe: &SemanticSignatureRecipe,
        world: &CallWorld,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        selected: Option<&Projection>,
        b: &mut Budget,
    ) -> Check<EntryResult> {
        let env = &world.environment;
        let scope = ScopeKey::Instance(world.instance);
        let image = world.boundary.sole_scope(scope);
        let Some(actuals) = self.rigid_inputs(module, definition, recipe, world, active, b) else {
            return self.entry_signature(module, definition, recipe, env, external, active, b).map(|s| EntryResult::Checked(s, None));
        };
        // environment_for_instance always creates the real code's invocation frame.
        // A root signature creates its own invocation domain inside entry_signature;
        // eligibility below requires that domain to be empty, and permits only
        // declared Implicit heads. The function's independent header check still
        // relates those heads to the current actual invocation at its boundary.
        let mut cache = self.rigid_worlds.borrow_mut();
        b.spend(lookup(cache.signatures.len()))?;
        if let Some(prior) = cache.signatures.get(&world.instance)
            && same_inputs(&prior.inputs, actuals, b)?
        {
            active.fresh(b)?;
            let projection = prior.projected.as_ref().zip(image);
            if let (Some((model, to)), Some(first)) = (projection, selected) {
                b.spend(2)?;
                if to == first.image && std::rc::Rc::ptr_eq(model, &first.model) {
                    return Ok(EntryResult::SameProjection);
                }
            }
            charge_signature(&prior.signature, b)?;
            return Ok(EntryResult::Checked((*prior.signature).clone(), projection.map(|(model,image)|
                Projection { model: model.clone(), image })));
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
        let mut projected = None;
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
                let signature = std::rc::Rc::new(signature.clone());
                let instance = self.instance(module, world.instance, b)?;
                let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = instance.target
                    else { return Err(bad("rigid world lacks its actual entry")); };
                let mut worlds = self.rigid_worlds.borrow_mut();
                b.spend(lookup(worlds.projected.len()).saturating_add(1))?;
                let model = match worlds.projected.get(&(definition.id, function)) {
                    Some(model) if signatures_correspond(model, &signature, b)? => Some(model.clone()),
                    Some(_) => None,
                    None => {
                        b.spend(lookup(worlds.projected.len()).saturating_add(2))?;
                        worlds.projected.insert((definition.id, function), signature.clone());
                        Some(signature.clone())
                    }
                };
                projected = model.as_ref().zip(image).map(|(model,image)| Projection { model: model.clone(), image });
                worlds.signatures.insert(world.instance, Entry {
                    inputs: actuals.clone(), signature, projected: model,
                    declarations: self.declaration_prefix(definition, scope, b)?,
                });
            }
        }
        Ok(EntryResult::Checked(signature, projected))
    }
}

impl CheckedContext {
    // The complete original declaration was just checked in this exact world.
    // For these leaf recipes its only lasting effect was appending each
    // Structural/Callable request to this scope's external premise journal.
    fn declaration_prefix(
        &self,
        definition: &SemanticDefinition,
        scope: ScopeKey,
        b: &mut Budget,
    ) -> Check<Option<DeclarationPrefix>> {
        let graph = self.premises.borrow();
        b.spend(lookup(graph.len()))?;
        let Some(rows) = graph.get(&scope) else {
            return Ok(None);
        };
        let mut frames = 0;
        for formal in &definition.parameters {
            let SemanticFormal::Type { requirements } = formal else {
                return Ok(None);
            };
            b.spend(requirements.len().saturating_add(1))?;
            for req in requirements {
                // independent_recipe has already checked that the receiver
                // is exactly this formal of this root declaration.
                match req {
                    SemanticRequirementRecipe::Structural { .. } => {}
                    SemanticRequirementRecipe::Callable {
                        kind: CallableKind::Fn,
                        signature,
                        ..
                    } => {
                        if signature.bound_lifetimes != 0
                            || signature.invocation_lifetimes != 0
                            || signature.call_kind != CallKind::Direct
                        {
                            return Ok(None);
                        }
                        b.spend(signature.parameters.len().saturating_add(1))?;
                        for ty in signature
                            .parameters
                            .iter()
                            .map(|p| &p.ty)
                            .chain([&signature.result])
                        {
                            match ty {
                                SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() => {}
                                SemanticTypeRecipe::Formal(SemanticFormalRef {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    ordinal,
                                }) if (*ordinal as usize) < definition.parameters.len() => {}
                                _ => return Ok(None),
                            }
                        }
                        frames += 1;
                    }
                    _ => return Ok(None),
                }
            }
        }
        Ok(Some(DeclarationPrefix {
            scope: rows.prefix.clone(),
            length: rows.external.len(),
            callable_frames: frames,
        }))
    }
}

fn same_inputs(inputs: &[CheckedActual], actuals: &[CheckedActual], b: &mut Budget) -> Check<bool> {
    b.spend(inputs.len().saturating_add(1))?;
    let mut same = inputs.len() == actuals.len();
    for (old, current) in inputs.iter().zip(actuals) {
        let (CheckedActual::Type(old), CheckedActual::Type(current)) = (old, current) else {
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
    Ok(same)
}

/// Both entry proofs have no quantifiers, statics, or occurrences from other
/// scopes. Compare the complete observed signatures, retaining every class and
/// intrinsic origin ordinal. The sole-scope boundary subsequently maps just
/// their owning scopes to the same actual call witness.
fn signatures_correspond(a: &CheckedSignature, z: &CheckedSignature, b: &mut Budget) -> Check<bool> {
    if a.call_kind != z.call_kind || a.result_mode != z.result_mode || a.parameters.len() != z.parameters.len()
        || !a.bound.is_empty() || !z.bound.is_empty() || !a.invocation.is_empty() || !z.invocation.is_empty() {
        return Ok(false);
    }
    let value = |a: &CheckedType, z: &CheckedType, b: &mut Budget| -> Check<bool> {
        if !a.same_raw_type(z,b)? { return Ok(false); }
        dynamic_metadata::life_shape(&a.life,b)?;
        dynamic_metadata::life_shape(&z.life,b)?;
        b.spend(a.classes.len().saturating_add(a.origins.len()).saturating_add(1))?;
        let ordinal = |key: &ClassKey| match key { ClassKey::Implicit {ordinal,..} => Some(*ordinal), _ => None };
        let source = |origin: &IntrinsicOrigin| match origin {
            IntrinsicOrigin::DerivedClass {source} => ordinal(source), _ => None,
        };
        Ok(a.life == z.life && a.statics.is_empty() && z.statics.is_empty()
            && a.classes.len() == z.classes.len() && a.origins.len() == z.origins.len()
            && a.classes.iter().zip(&z.classes).all(|(a,z)|ordinal(a).is_some_and(|o|ordinal(z)==Some(o)))
            && a.origins.iter().zip(&z.origins).all(|(a,z)|source(a).is_some_and(|o|source(z)==Some(o))))
    };
    for ((a,am), (z,zm)) in a.parameters.iter().zip(&z.parameters) {
        b.spend(1)?;
        if am != zm || !value(a,z,b)? { return Ok(false); }
    }
    value(&a.result,&z.result,b)
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
        // Every formal requirement must have been established in this exact
        // world. Caller-side proof requests are checked by the calling occurrence.
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
    fn joint_signature_preserves_relative_ordinals_modes_and_the_actual_call_witness() {
        let limits=Limits::default();
        let mut b=Budget::new(&limits);
        let scope=|index| ScopeKey::Instance(SemanticInstanceId{module:zil_base::ModuleId(0),index});
        let make=|owner, ordinal, origin, mode| {
            let mut value=atomic(&Type::I32,&mut Budget::new(&limits)).unwrap();
            value.replace_type(Type::Address{mutable:false,inner:Box::new(Type::I32)});
            value.life.slots.push(StorageLifeSlot{path:vec![],mode:StoredBorrowMode::SharedAddress,binder:0});
            value.classes.push(ClassKey::Implicit {scope:owner,ordinal});
            value.origins.push(IntrinsicOrigin::DerivedClass{source:ClassKey::Implicit{scope:owner,ordinal:origin}});
            value.life.binder_count=1;
            CheckedSignature{parameters:vec![(value,mode)],result:atomic(&Type::Unit,&mut Budget::new(&limits)).unwrap(),
                result_mode:PassingMode::Move,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]}
        };
        let a=make(scope(1),2,4,PassingMode::Borrow);
        assert!(signatures_correspond(&a,&make(scope(2),2,4,PassingMode::Borrow),&mut b).unwrap());
        assert!(!signatures_correspond(&a,&make(scope(2),2,3,PassingMode::Borrow),&mut b).unwrap());
        assert!(!signatures_correspond(&a,&make(scope(2),4,4,PassingMode::Borrow),&mut b).unwrap());
        assert!(!signatures_correspond(&a,&make(scope(2),2,4,PassingMode::BorrowMut),&mut b).unwrap());
        let mut boundary=trait_members::Boundary::default();
        boundary.scope(scope(1),scope(2),&mut b).unwrap();
        assert_eq!(boundary.sole_scope(scope(1)),Some(scope(2)));
        assert_eq!(boundary.sole_scope(scope(3)),None);
        boundary.scope(scope(3),scope(4),&mut b).unwrap();
        assert_eq!(boundary.sole_scope(scope(1)),None);
    }
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
