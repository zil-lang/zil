//! Request-local Task frame metadata and outstanding constructor obligations.
//! Metadata is not WF, a worker entry, a root, a loan, or a Closed fact.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FrameKey {
    pub(crate) worker: FunctionId,
    pub(crate) instance: Option<SemanticInstanceId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selector {
    Outer {
        owner: SemanticDefinitionId,
        life: SemanticOuterLifeRecipe,
    },
    Whole {
        owner: SemanticDefinitionId,
        origin: IntrinsicOrigin,
    },
    RawOuter(StorageBinder),
    RawWhole {
        parameter: u32,
        class: u32,
    },
}
#[derive(Clone)]
pub(crate) struct FrameMetadata {
    pub(crate) key: FrameKey,
    pub(crate) definition: Option<SemanticDefinitionId>,
    pub(crate) arguments: Vec<CheckedActual>,
    pub(crate) parameters: Vec<CheckedType>,
    pub(crate) parameter_types: Vec<Type>,
    pub(crate) result: Option<CheckedType>,
    pub(crate) result_type: Type,
    pub(crate) shape: RetainedLifeShape,
    pub(crate) port_sources: Vec<RetainedPortBinding>,
    pub(crate) input_maps: Vec<Vec<u32>>,
    pub(crate) result_map: Vec<u32>,
    pub(crate) selectors: Vec<Selector>,
    pub(crate) worker_checked: bool,
}
pub(crate) enum FrameState {
    Building,
    MetadataReady(FrameMetadata),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ConstructorKey {
    pub(crate) caller: Option<SemanticInstanceId>,
    pub(crate) function: FunctionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: u32,
    pub(crate) result: ValueId,
    pub(crate) worker: FunctionId,
    pub(crate) owned: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObligationState {
    Pending,
    Checking,
    Discharged,
}
pub(crate) struct ConstructorObligation {
    pub(crate) key: ConstructorKey,
    pub(crate) ty: Type,
    pub(crate) frame: Option<FrameKey>,
    pub(crate) frames: Vec<FrameKey>,
    pub(crate) state: ObligationState,
    pub(crate) raw_checked: bool,
    pub(crate) semantic_checked: bool,
}

impl CheckedContext {
    pub(super) fn with_domain_metadata<T>(&self, run: impl FnOnce() -> Check<T>) -> Check<T> {
        let previous = self.domain_only.replace(true);
        let result = run();
        self.domain_only.set(previous);
        result
    }
    /// Only complete declared domains are constructed here. Full WF is kept
    /// pending on the exact FrameKey and is checked by the normal worker pass.
    pub(super) fn metadata_value(
        &self,
        _module: &TypedModule,
        ty: &Type,
        life: &StorageLifeShape,
        classes: Vec<ClassKey>,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        dynamic_metadata::ty(ty, b)?;
        if classes.len() != life.binder_count as usize || classes.len() > b.limits.max_nodes {
            return Err(bad(
                "Task metadata loses its complete declared class domain",
            ));
        }
        b.spend(
            classes
                .len()
                .saturating_add(life.slots.len())
                .saturating_add(life.retained.len())
                .saturating_add(life.nominal_arguments.len()),
        )?;
        for row in &life.slots {
            b.spend(row.path.len())?;
            if row.binder >= life.binder_count {
                return Err(bad("Task metadata slot escapes its declared domain"));
            }
        }
        for row in &life.retained {
            b.spend(row.path.len())?;
            if row.binder >= life.binder_count {
                return Err(bad("Task metadata child escapes its declared domain"));
            }
        }
        for row in &life.nominal_arguments {
            b.spend(row.path.len())?;
            if row.binder >= life.binder_count {
                return Err(bad(
                    "Task metadata nominal occurrence escapes its declared domain",
                ));
            }
        }
        Ok(CheckedType {
            ty: ty.clone(),
            life: life.clone(),
            origins: classes
                .iter()
                .map(|source| IntrinsicOrigin::DerivedClass {
                    source: source.clone(),
                })
                .collect(),
            classes,
            statics: vec![],
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn metadata_closure_type(
        &self,
        module: &TypedModule,
        definition: SemanticDefinitionId,
        arguments: &[SemanticActualRecipe],
        sealed: &[SemanticActualRecipe],
        env: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<CheckedType> {
        let original = self.definition(module, definition, b)?;
        if original.kind != SemanticDefinitionKind::Closure
            || arguments.len() != original.parameters.len()
            || sealed.len() != original.environment.len()
        {
            return Err(bad(
                "Task closure metadata changes its exact original full scope",
            ));
        }
        let mut actuals = Vec::new();
        for recipe in arguments {
            actuals.push(self.evaluate_actual(module, recipe, env, external, active, b)?);
        }
        let mut environment = Vec::new();
        for recipe in sealed {
            environment.push(self.evaluate_actual(module, recipe, env, external, active, b)?);
        }
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("Task closure metadata has no semantic inventory"))?;
        b.spend(program.instances.len())?;
        let mut selected = None;
        for instance in &program.instances {
            if instance.definition != definition {
                continue;
            }
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
                instance.target
            else {
                continue;
            };
            let entry = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| bad("Task closure metadata entry absent"))?;
            let Some(id) = entry.environment else {
                continue;
            };
            if original.parent.is_some_and(|parent| {
                !env.lexical.iter().any(|frame| {
                    frame.definition == parent
                        && matches!(frame.key,ScopeKey::Instance(key)if Some(key)==instance.parent)
                })
            }) {
                continue;
            }
            // All matching worlds must denote the same actual opaque entry;
            // no first-world or same-ABI choice is made here.
            if selected.is_some_and(|old| old != id) {
                return Err(bad(
                    "Task closure metadata has conflicting actual opaque entries",
                ));
            }
            selected = Some(id);
        }
        let id = selected.ok_or_else(|| bad("Task closure metadata has no exact actual entry"))?;
        b.spend(module.closures.len())?;
        let mut rows = module.closures.iter().filter(|row| row.id == id);
        let closure = rows
            .next()
            .ok_or_else(|| bad("Task closure domain is absent"))?;
        if rows.next().is_some() {
            return Err(bad("Task closure metadata identity duplicated"));
        }
        let header = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Task closure full persistent domain is Unknown"))?;
        signature_life::charge(header, b)?;
        if header.fixed_binder_count as usize > b.limits.max_nodes {
            return Err(limit("Task closure metadata domain exceeded"));
        }
        let scope = ScopeKey::Witness {
            definition,
            frame: active.fresh(b)?,
        };
        let classes = (0..header.fixed_binder_count)
            .map(|class| ClassKey::Intrinsic {
                scope,
                operand: OperandKey::Receiver,
                path: vec![],
                class,
            })
            .collect();
        let _ = (actuals, environment); // Exact actuals remain part of Frame WF.
        self.metadata_value(
            module,
            &Type::Closure(id),
            &StorageLifeShape {
                binder_count: header.fixed_binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes,
            b,
        )
    }

    pub(super) fn prepare_task_contexts(
        &self,
        module: &TypedModule,
        _external: &mut dynamic_witness::semantic_external::Proofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        register_constructors(module, b)?;
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("Task metadata lacks its exact source scopes"))?;
        let workers = workers(module, b)?;
        for worker in workers {
            b.spend(program.instances.len())?;
            let mut found = false;
            for instance in &program.instances {
                if !matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==worker)
                {
                    continue;
                }
                found = true;
                let key = FrameKey {
                    worker,
                    instance: Some(instance.id),
                };
                if b.task_frames.contains_key(&key) {
                    continue;
                }
                if b.task_frames.len() >= b.limits.max_nodes {
                    return Err(limit(
                        "Task frame metadata inventory exceeds selected bound",
                    ));
                }
                b.spend(1)?;
                b.task_frames.insert(key, FrameState::Building);
                let result = self.with_domain_metadata(|| {
                    let env = self.metadata_instance_environment(module, instance.id, active, b)?;
                    let definition = self.definition(module, instance.definition, b)?;
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                        &definition.body
                    else {
                        return Err(bad(
                            "Task worker metadata omits its complete original signature",
                        ));
                    };
                    let mut external = dynamic_witness::semantic_external::Proofs::default();
                    let checked =
                        self.evaluate_signature(module, signature, &env, &mut external, active, b)?;
                    build_frame(
                        self,
                        module,
                        key,
                        Some(definition.id),
                        env.lexical
                            .first()
                            .map_or_else(Vec::new, |frame| frame.actuals.clone()),
                        signature,
                        &checked,
                        &env,
                        b,
                    )
                });
                match result {
                    Ok(frame) => {
                        b.task_frames.insert(key, FrameState::MetadataReady(frame));
                    }
                    Err(error) => {
                        b.task_frames.remove(&key);
                        return Err(error);
                    }
                }
            }
            if !found {
                return Err(bad("Task worker has no exact original instantiation world"));
            }
        }
        Ok(())
    }

    fn metadata_instance_environment(
        &self,
        module: &TypedModule,
        id: SemanticInstanceId,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<EvalEnv> {
        let mut chain = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            b.spend(chain.len().saturating_add(1))?;
            if chain.contains(&id) {
                return Err(bad("Task metadata scope parent cycle"));
            }
            if chain.len() >= b.limits.max_call_depth.min(128) {
                return Err(limit("Task metadata lexical depth exceeded"));
            }
            chain.push(id);
            next = self.instance(module, id, b)?.parent;
        }
        let mut lexical = Vec::new();
        for id in chain.into_iter().rev() {
            let instance = self.instance(module, id, b)?;
            let definition = self.definition(module, instance.definition, b)?;
            if instance.actuals.len() != definition.parameters.len() {
                return Err(bad("Task metadata omits full mixed formal order"));
            }
            let scope = ScopeKey::Instance(id);
            let mut actuals = Vec::new();
            for (ordinal, (formal, actual)) in definition
                .parameters
                .iter()
                .zip(&instance.actuals)
                .enumerate()
            {
                actuals.push(metadata_actual(
                    self,
                    module,
                    scope,
                    ordinal as u32,
                    formal,
                    actual,
                    false,
                    b,
                )?);
            }
            let mut environment = Vec::new();
            if let Some(sealed) = &instance.environment {
                if sealed.actuals.len() != definition.environment.len() {
                    return Err(bad("Task metadata omits a complete ghost environment"));
                }
                for (ordinal, (formal, actual)) in definition
                    .environment
                    .iter()
                    .zip(&sealed.actuals)
                    .enumerate()
                {
                    environment.push(metadata_actual(
                        self,
                        module,
                        scope,
                        ordinal as u32,
                        &formal.kind,
                        actual,
                        true,
                        b,
                    )?);
                }
            } else if !definition.environment.is_empty() {
                return Err(bad("Task metadata missing its original ghost environment"));
            }
            lexical.insert(
                0,
                LexicalFrame {
                    definition: definition.id,
                    key: scope,
                    actuals,
                    environment,
                    self_type: None,
                },
            );
            let recipe = match &definition.body {
                SemanticDefinitionBody::Scope(
                    SemanticScopeRecipe::InherentImpl { self_type, .. }
                    | SemanticScopeRecipe::TraitImpl { self_type, .. }
                    | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                ) => Some(self_type),
                _ => None,
            };
            if let Some(recipe) = recipe {
                let env = EvalEnv {
                    lexical: lexical.clone(),
                    assumptions: vec![],
                    images: vec![],
                    bounds: vec![],
                    invocations: vec![],
                    at: None,
                };
                let mut external = dynamic_witness::semantic_external::Proofs::default();
                lexical[0].self_type =
                    Some(self.evaluate_type(module, recipe, &env, &mut external, active, b)?);
            }
        }
        Ok(EvalEnv {
            lexical,
            assumptions: vec![],
            images: vec![],
            bounds: vec![],
            invocations: vec![],
            at: None,
        })
    }

    pub(super) fn mark_task_worker_checked(
        &self,
        module: &TypedModule,
        instance: &SemanticInstance,
        signature: &CheckedSignature,
        b: &mut Budget,
    ) -> Check<()> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(worker)) =
            instance.target
        else {
            return Ok(());
        };
        let key = FrameKey {
            worker,
            instance: Some(instance.id),
        };
        let Some(state) = b.task_frames.get_mut(&key) else {
            return Ok(());
        };
        let FrameState::MetadataReady(frame) = state else {
            return Err(bad("Task worker WF re-enters unfinished frame metadata"));
        };
        if frame.definition != Some(instance.definition)
            || frame.parameter_types.len() != signature.parameters.len()
            || frame.result_type != signature.result.ty
        {
            return Err(bad(
                "actual Task worker world differs from its complete source metadata",
            ));
        }
        for (expected, (actual, _)) in frame.parameter_types.iter().zip(&signature.parameters) {
            if *expected != actual.ty {
                return Err(bad("Task worker parameter changes a complete source type"));
            }
        }
        let _ = module;
        frame.worker_checked = true;
        Ok(())
    }
}

fn metadata_actual(
    context: &CheckedContext,
    module: &TypedModule,
    scope: ScopeKey,
    ordinal: u32,
    formal: &SemanticFormal,
    actual: &SemanticActual,
    environment: bool,
    b: &mut Budget,
) -> Check<CheckedActual> {
    b.spend(1)?;
    Ok(match (formal, actual) {
        (SemanticFormal::Type { .. }, SemanticActual::Type(value)) => {
            let (ty, life, classes) = resolved(value)?;
            if classes.len() != life.binder_count as usize {
                return Err(bad(
                    "Task metadata Type actual loses its full persistent domain",
                ));
            }
            let operand = if environment {
                OperandKey::Environment { scope, ordinal }
            } else {
                OperandKey::TypeArgument { scope, ordinal }
            };
            let keys = (0..life.binder_count)
                .map(|class| ClassKey::Intrinsic {
                    scope,
                    operand: operand.clone(),
                    path: vec![],
                    class,
                })
                .collect();
            let mut value = context.metadata_value(module, ty, life, keys, b)?;
            value.origins = (0..life.binder_count)
                .map(|class| IntrinsicOrigin::Existing {
                    scope,
                    operand: operand.clone(),
                    path: vec![],
                    class,
                })
                .collect();
            CheckedActual::Type(value)
        }
        (SemanticFormal::Lifetime, SemanticActual::Life(_)) => {
            CheckedActual::Life(if environment {
                ClassKey::Intrinsic {
                    scope,
                    operand: OperandKey::Environment { scope, ordinal },
                    path: vec![],
                    class: 0,
                }
            } else {
                ClassKey::Formal { scope, ordinal }
            })
        }
        (SemanticFormal::Const { .. }, SemanticActual::Const { ty, value }) => {
            let (carrier, life, classes) = resolved(ty)?;
            if !classes.is_empty() || life.binder_count != 0 || *carrier != value.ty() {
                return Err(bad(
                    "Task metadata Const changes its complete scalar carrier",
                ));
            }
            scalar::closed(
                &DynConstPattern::Value(value.clone()),
                carrier,
                module.target.pointer_width,
                b,
            )?
            .ok_or_else(|| bad("Task metadata Const is not a finite scalar"))?;
            CheckedActual::Const {
                carrier: context.metadata_value(module, carrier, life, vec![], b)?,
                value: value.clone(),
            }
        }
        _ => {
            return Err(bad(
                "Task metadata changes an original full-ordinal argument kind",
            ));
        }
    })
}

pub(super) fn selector(
    context: &CheckedContext,
    owner: SemanticDefinitionId,
    recipe: &SemanticTypeRecipe,
    value: &CheckedType,
    class: u32,
    b: &mut Budget,
) -> Check<Selector> {
    b.spend(1)?;
    if class >= value.life.binder_count {
        return Err(bad("Task selector escapes its full child domain"));
    }
    match recipe {
        SemanticTypeRecipe::Address { lifetime, .. }
        | SemanticTypeRecipe::Desc { lifetime, .. } => {
            let outer = value
                .life
                .slots
                .iter()
                .any(|slot| slot.path.is_empty() && slot.binder == class);
            let child = value
                .life
                .retained
                .iter()
                .any(|row| row.path == [LifeTypeProjection::Pointee] && row.binder == class);
            if outer && child {
                return Err(bad(
                    "Task input aliases an outer occurrence with a frozen pointee intrinsic class",
                ));
            }
            if outer {
                return Ok(Selector::Outer {
                    owner,
                    life: lifetime.clone(),
                });
            }
        }
        _ => {}
    }
    dynamic_metadata::recipe(recipe, b)?;
    let origin = value
        .origins
        .get(class as usize)
        .ok_or_else(|| bad("Task selector loses its complete source intrinsic occurrence"))?;
    Ok(Selector::Whole {
        owner,
        origin: context.canonical_origin(origin, b)?,
    })
}

fn build_frame(
    context: &CheckedContext,
    module: &TypedModule,
    key: FrameKey,
    definition: Option<SemanticDefinitionId>,
    arguments: Vec<CheckedActual>,
    recipe: &SemanticSignatureRecipe,
    signature: &CheckedSignature,
    env: &EvalEnv,
    b: &mut Budget,
) -> Check<FrameMetadata> {
    if recipe.parameters.len() != signature.parameters.len() {
        return Err(bad("Task metadata loses a complete parameter"));
    }
    let owner = definition.ok_or_else(|| bad("source Task frame has no original scope"))?;
    let mut shape = RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    };
    let mut selectors = Vec::new();
    let mut input_maps = Vec::new();
    let mut port_sources = Vec::new();
    for (ordinal, (original, (input, _))) in recipe
        .parameters
        .iter()
        .zip(&signature.parameters)
        .enumerate()
    {
        b.spend(input.classes.len().saturating_add(1))?;
        let mut map = Vec::new();
        for class in 0..input.life.binder_count {
            let selected = selector(context, owner, &original.ty, input, class, b)?;
            b.spend(selectors.len().saturating_add(1))?;
            let binder = match selectors.iter().position(|old| old == &selected) {
                Some(index) => index as u32,
                None => {
                    if selectors.len() >= b.limits.max_nodes {
                        return Err(limit("Task complete frame domain exceeded"));
                    }
                    let index = selectors.len() as u32;
                    selectors.push(selected);
                    index
                }
            };
            map.push(binder);
            shape.retained.push(StorageRetainedBinding {
                path: vec![LifeTypeProjection::Parameter(ordinal as u32)],
                class,
                binder,
            });
        }
        for row in &input.life.nominal_arguments {
            b.spend(row.path.len().saturating_add(1))?;
            let mut row = row.clone();
            row.path
                .insert(0, LifeTypeProjection::Parameter(ordinal as u32));
            row.binder = *map
                .get(row.binder as usize)
                .ok_or_else(|| bad("Task input phantom row loses its whole class map"))?;
            shape.nominal_arguments.push(row);
        }
        let leaves = input_ports(context, module, input, b)?;
        for (leaf, mode, class, path) in leaves {
            let binder = *map
                .get(class as usize)
                .ok_or_else(|| bad("Task input port has no full-domain class"))?;
            let port = shape.ports.len() as u32;
            shape.ports.push(RetainedBorrowPort { leaf, mode, binder });
            port_sources.push(RetainedPortBinding {
                port,
                source: StoragePosition {
                    base: StorageBase::Parameter(ordinal as u32),
                    path,
                },
            });
        }
        input_maps.push(map);
    }
    let mut result_map = Vec::new();
    for class in 0..signature.result.life.binder_count {
        let result = selector(context, owner, &recipe.result, &signature.result, class, b)?;
        b.spend(selectors.len())?;
        let binder = selectors
            .iter()
            .position(|row| row == &result)
            .ok_or_else(|| {
                bad("Task result class/phantom lacks its exact original input-domain selector")
            })? as u32;
        result_map.push(binder);
        shape.retained.push(StorageRetainedBinding {
            path: vec![LifeTypeProjection::Result],
            class,
            binder,
        });
    }
    shape.binder_count = selectors.len() as u32;
    shape.retained.sort();
    shape.nominal_arguments.sort();
    let _ = env;
    Ok(FrameMetadata {
        key,
        definition,
        arguments,
        parameters: signature
            .parameters
            .iter()
            .map(|(value, _)| value.clone())
            .collect(),
        parameter_types: signature
            .parameters
            .iter()
            .map(|(value, _)| value.ty.clone())
            .collect(),
        result: Some(signature.result.clone()),
        result_type: signature.result.ty.clone(),
        shape,
        port_sources,
        input_maps,
        result_map,
        selectors,
        worker_checked: false,
    })
}

pub(super) fn input_ports(
    context: &CheckedContext,
    module: &TypedModule,
    value: &CheckedType,
    b: &mut Budget,
) -> Check<Vec<(Type, StoredBorrowMode, u32, Vec<ContractProjection>)>> {
    let mut result = Vec::new();
    if !value.statics.is_empty() {
        return Err(unsupported(
            "Task constant-source frame input requires its complete source obligations",
            None,
        ));
    }
    for slot in &value.life.slots {
        b.spend(slot.path.len().saturating_add(1))?;
        let path = slot
            .path
            .iter()
            .copied()
            .map(LifeTypeProjection::Place)
            .collect::<Vec<_>>();
        let leaf = lifetimes::select(&value.ty, &path, b)?;
        dynamic_metadata::ty(leaf, b)?;
        result.push((
            leaf.clone(),
            slot.mode,
            slot.binder,
            slot.path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect(),
        ));
    }
    let mut payload = &value.ty;
    let mut prefix = Vec::new();
    loop {
        match payload {
            Type::BorrowedStorage { storage, .. } => payload = storage,
            Type::Managed(inner) => {
                payload = inner;
                prefix.push(LifeTypeProjection::Pointee);
            }
            _ => break,
        }
    }
    let opaque = match payload {
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            Some(retained_life)
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            b.spend(module.dyn_interfaces.len())?;
            Some(
                &module
                    .dyn_interfaces
                    .iter()
                    .find(|row| row.id == *interface)
                    .ok_or_else(|| bad("Task input opaque interface is absent"))?
                    .retained_life,
            )
        }
        _ => None,
    };
    if let Some(shape) = opaque {
        dynamic_metadata::retained_shape(shape, b)?;
        for (index, port) in shape.ports.iter().enumerate() {
            let class = if prefix.is_empty() {
                port.binder
            } else {
                let mut rows = value
                    .life
                    .retained
                    .iter()
                    .filter(|row| row.path == prefix && row.class == port.binder);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Task opaque input loses its complete child map"))?;
                if rows.next().is_some() {
                    return Err(bad("Task opaque child class mapping duplicated"));
                }
                row.binder
            };
            result.push((
                port.leaf.clone(),
                port.mode,
                class,
                vec![ContractProjection::RetainedPort(index as u32)],
            ));
        }
    }
    if let Type::Closure(id) = payload {
        b.spend(module.closures.len())?;
        let mut definitions = module.closures.iter().filter(|row| row.id == *id);
        let closure = definitions
            .next()
            .ok_or_else(|| bad("Task closure input has no exact capture inventory"))?;
        if definitions.next().is_some() {
            return Err(bad("Task closure input has duplicate original identity"));
        }
        let header = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Task closure complete capture domain is Unknown"))?;
        signature_life::charge(header, b)?;
        for binding in &header.bindings {
            let StorageBase::Capture(ordinal) = binding.position.base else {
                continue;
            };
            let StorageBinder::Fixed(class) = binding.binder else {
                return Err(bad(
                    "Task closure capture port escapes its persistent domain",
                ));
            };
            if class >= header.fixed_binder_count {
                return Err(bad(
                    "Task closure capture port exceeds its full fixed domain",
                ));
            }
            let capture = closure
                .captures
                .get(ordinal as usize)
                .ok_or_else(|| bad("Task closure port selects a missing actual capture"))?;
            let leaf = contract_leaf(module, capture, &binding.position.path, b)?;
            let mode = match &leaf {
                Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                Type::Desc(_) => StoredBorrowMode::ReadOnlyDescription,
                _ => {
                    return Err(bad(
                        "Task closure port does not select a complete borrowed leaf",
                    ));
                }
            };
            let class = if prefix.is_empty() {
                class
            } else {
                let mut rows = value
                    .life
                    .retained
                    .iter()
                    .filter(|row| row.path == prefix && row.class == class);
                let row = rows.next().ok_or_else(|| {
                    bad("Task closure input loses its exact complete child-domain map")
                })?;
                if rows.next().is_some() {
                    return Err(bad("Task closure child domain map is duplicated"));
                }
                row.binder
            };
            b.spend(binding.position.path.len().saturating_add(1))?;
            if result.len() >= b.limits.max_nodes {
                return Err(limit("Task complete capture-port inventory exceeded"));
            }
            let mut path = vec![ContractProjection::Capture(ordinal)];
            path.extend_from_slice(&binding.position.path);
            result.push((leaf, mode, class, path));
        }
    }
    let _ = context;
    Ok(result)
}

/// Select only a declared logical/physical borrow occurrence. This never
/// computes a GEP or proves that the selected actual root remains live.
fn contract_leaf(
    module: &TypedModule,
    root: &Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<Type> {
    b.spend(path.len().saturating_add(1))?;
    let mut current = root;
    for projection in path {
        while let Type::BorrowedStorage { storage, .. } = current {
            current = storage;
        }
        match projection {
            ContractProjection::Place(field) => {
                current = lifetimes::select(current, &[LifeTypeProjection::Place(*field)], b)?
            }
            ContractProjection::Capture(ordinal) => {
                let Type::Closure(id) = current else {
                    return Err(bad(
                        "Task logical capture selector crosses another constructor",
                    ));
                };
                b.spend(module.closures.len())?;
                let mut definitions = module.closures.iter().filter(|row| row.id == *id);
                let closure = definitions
                    .next()
                    .ok_or_else(|| bad("Task nested capture inventory absent"))?;
                if definitions.next().is_some() {
                    return Err(bad("Task nested capture identity duplicated"));
                }
                current = closure
                    .captures
                    .get(*ordinal as usize)
                    .ok_or_else(|| bad("Task nested capture ordinal absent"))?;
            }
            ContractProjection::RetainedPort(ordinal) => {
                while let Type::Managed(inner) | Type::Desc(inner) = current {
                    current = inner;
                }
                let shape = match current {
                    Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                        retained_life
                    }
                    Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                        b.spend(module.dyn_interfaces.len())?;
                        let mut interfaces = module
                            .dyn_interfaces
                            .iter()
                            .filter(|row| row.id == *interface);
                        let row = interfaces
                            .next()
                            .ok_or_else(|| bad("Task nested port interface absent"))?;
                        if interfaces.next().is_some() {
                            return Err(bad("Task nested port interface duplicated"));
                        }
                        &row.retained_life
                    }
                    _ => {
                        return Err(bad(
                            "Task retained-port selector crosses another constructor",
                        ));
                    }
                };
                dynamic_metadata::retained_shape(shape, b)?;
                current = &shape
                    .ports
                    .get(*ordinal as usize)
                    .ok_or_else(|| bad("Task retained-port ordinal absent"))?
                    .leaf;
            }
            ContractProjection::RetainedClass(_) => {
                return Err(bad(
                    "Task class-only selector cannot supply a borrowed port",
                ));
            }
        }
    }
    dynamic_metadata::ty(current, b)?;
    Ok(current.clone())
}

fn workers(module: &TypedModule, b: &mut Budget) -> Check<BTreeSet<FunctionId>> {
    let mut result = BTreeSet::new();
    for function in &module.functions {
        for block in &function.blocks {
            b.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                if let InstructionKind::TaskCreate { worker, .. }
                | InstructionKind::TaskCreateOwned { worker, .. } = instruction.kind
                {
                    result.insert(worker);
                }
            }
        }
    }
    Ok(result)
}

fn register_constructors(module: &TypedModule, b: &mut Budget) -> Check<()> {
    for function in &module.functions {
        let callers = if let Some(program) = &module.semantic {
            b.spend(program.instances.len())?;
            program.instances.iter().filter_map(|instance|if matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==function.id){Some(Some(instance.id))}else{None}).collect::<Vec<_>>()
        } else {
            vec![None]
        };
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1)?;
                let (worker, owned) = match instruction.kind {
                    InstructionKind::TaskCreate { worker, .. } => (worker, false),
                    InstructionKind::TaskCreateOwned { worker, .. } => (worker, true),
                    _ => continue,
                };
                let result = instruction
                    .result
                    .ok_or_else(|| bad("Task constructor has no actual result slot"))?;
                let ty = function
                    .values
                    .get(result.0 as usize)
                    .ok_or_else(|| bad("Task constructor result type is absent"))?;
                dynamic_metadata::ty(ty, b)?;
                for caller in &callers {
                    let key = ConstructorKey {
                        caller: *caller,
                        function: function.id,
                        block: block.id,
                        instruction: index as u32,
                        result,
                        worker,
                        owned,
                    };
                    if b.task_constructors.contains_key(&key) {
                        continue;
                    }
                    if b.task_constructors.len() >= b.limits.max_nodes {
                        return Err(limit(
                            "Task constructor obligation inventory exceeds selected bound",
                        ));
                    }
                    b.spend(1)?;
                    b.task_constructors.insert(
                        key,
                        ConstructorObligation {
                            key,
                            ty: ty.clone(),
                            frame: None,
                            frames: vec![],
                            state: ObligationState::Pending,
                            raw_checked: false,
                            semantic_checked: false,
                        },
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn prepare_raw_task_contexts(module: &TypedModule, b: &mut Budget) -> Check<()> {
    if module.semantic.is_some() {
        return Err(bad(
            "raw Task metadata entry cannot substitute for semantic worlds",
        ));
    }
    register_constructors(module, b)?;
    for worker in workers(module, b)? {
        let function = module
            .functions
            .get(worker.0 as usize)
            .ok_or_else(|| bad("raw Task worker absent"))?;
        // The mature raw profile remains the authoritative complete worker
        // type interpreter. Its frame metadata never records a WF success.
        let shape = crate::tasks::raw_frame_shape(module, function, b)?;
        let key = FrameKey {
            worker,
            instance: None,
        };
        let sources = crate::tasks::raw_frame_sources(module, function, b)?;
        let parameter_types = function
            .parameters
            .iter()
            .map(|id| {
                function
                    .locals
                    .get(id.0 as usize)
                    .map(|local| local.ty.clone())
                    .ok_or_else(|| bad("raw Task parameter local absent"))
            })
            .collect::<Check<Vec<_>>>()?;
        b.task_frames.insert(
            key,
            FrameState::MetadataReady(FrameMetadata {
                key,
                definition: None,
                arguments: vec![],
                parameters: vec![],
                parameter_types,
                result: None,
                result_type: function.return_type.clone(),
                shape,
                port_sources: sources,
                input_maps: vec![],
                result_map: vec![],
                selectors: vec![],
                worker_checked: false,
            }),
        );
    }
    Ok(())
}

pub(crate) fn metadata_candidates(worker: FunctionId, b: &mut Budget) -> Check<Vec<FrameMetadata>> {
    b.spend(b.task_frames.len())?;
    let frames = std::mem::take(&mut b.task_frames);
    let result = (|| {
        let mut result = Vec::new();
        for (key, state) in &frames {
            if key.worker != worker {
                continue;
            }
            let FrameState::MetadataReady(frame) = state else {
                return Err(bad("Task query re-enters Building metadata"));
            };
            dynamic_metadata::retained_shape(&frame.shape, b)?;
            for ty in frame
                .parameter_types
                .iter()
                .chain(std::iter::once(&frame.result_type))
            {
                dynamic_metadata::ty(ty, b)?;
            }
            b.spend(
                frame
                    .selectors
                    .len()
                    .saturating_add(frame.port_sources.len())
                    .saturating_add(frame.input_maps.iter().map(Vec::len).sum::<usize>())
                    .saturating_add(frame.result_map.len()),
            )?;
            result.push(frame.clone());
        }
        if result.is_empty() {
            return Err(bad("Task query has no complete frame metadata"));
        }
        Ok(result)
    })();
    b.task_frames = frames;
    result
}

pub(crate) fn metadata_zero_type(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<Option<bool>> {
    dynamic_metadata::ty(ty, b)?;
    let mut pending = vec![ty];
    let mut found = false;
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::Task {
                result,
                retained_life,
            } => {
                found = true;
                dynamic_metadata::retained_shape(retained_life, b)?;
                if retained_life.binder_count != 0 {
                    return Ok(Some(false));
                }
                pending.push(result);
            }
            Type::BorrowedStorage { storage, life } => {
                if life.binder_count != 0 {
                    return Ok(Some(false));
                }
                pending.push(storage);
            }
            Type::Address { .. } | Type::Desc(_) => return Ok(Some(false)),
            Type::Managed(inner)
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::OwnedStore { element: inner }
            | Type::Opaque { hidden: inner, .. } => pending.push(inner),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty))
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields)
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::Closure(id) => {
                b.spend(module.closures.len())?;
                let mut rows = module.closures.iter().filter(|row| row.id == *id);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Task ABI metadata closure absent"))?;
                if rows.next().is_some() {
                    return Err(bad("Task ABI closure metadata duplicated"));
                }
                let header = row
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| bad("Task ABI closure domain is Unknown"))?;
                signature_life::charge(header, b)?;
                if header.fixed_binder_count != 0 {
                    return Ok(Some(false));
                }
                pending.extend(&row.captures);
            }
            Type::Named(_) | Type::Unsupported(_) => return Ok(None),
            _ => {}
        }
    }
    Ok(if found { Some(true) } else { None })
}

/// Type WF uses a complete frame cursor and queues every constructor; it must
/// not recursively execute the constructor kernel from inside checked_value.
pub(crate) fn task_metadata_domain(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<u32> {
    let Type::Task {
        result,
        retained_life,
    } = ty
    else {
        return Err(bad("Task domain cursor selects another type"));
    };
    dynamic_metadata::retained_shape(retained_life, b)?;
    let mut found = false;
    let workers = workers(module, b)?;
    for worker in workers {
        for frame in metadata_candidates(worker, b)? {
            if frame.result_type == **result && frame.shape == *retained_life {
                found = true;
            }
        }
    }
    if !found {
        return Err(bad(
            "Task type has no complete source frame metadata; WF remains pending",
        ));
    }
    Ok(retained_life.binder_count)
}

pub(crate) fn record_raw_constructor(
    function: &TypedFunction,
    instruction: &Instruction,
    expected: &Type,
    b: &mut Budget,
) -> Check<()> {
    let Some(result) = instruction.result else {
        return Err(bad("Task constructor lacks a result"));
    };
    let mut selected = Vec::new();
    b.spend(b.task_constructors.len())?;
    for (key, obligation) in &b.task_constructors {
        if key.function == function.id && key.result == result {
            if obligation.ty != *expected {
                return Err(bad(
                    "Task constructor differs from its complete independently built type",
                ));
            }
            selected.push(*key);
        }
    }
    if selected.is_empty() {
        return Err(bad(
            "Task constructor is absent from the exact-site obligation inventory",
        ));
    }
    for key in selected {
        let obligation = b.task_constructors.get_mut(&key).unwrap();
        if obligation.state == ObligationState::Checking {
            return Err(bad("Task constructor WF forms an active cycle"));
        }
        obligation.raw_checked = true;
        if key.caller.is_none() {
            obligation.frames = vec![FrameKey {
                worker: key.worker,
                instance: None,
            }];
            obligation.state = ObligationState::Discharged;
        }
    }
    Ok(())
}

pub(crate) fn drain(module: &TypedModule, b: &mut Budget) -> Check<()> {
    register_constructors(module, b)?;
    b.spend(
        b.task_frames
            .len()
            .saturating_add(b.task_constructors.len()),
    )?;
    for state in b.task_frames.values() {
        if !matches!(state, FrameState::MetadataReady(_)) {
            return Err(bad(
                "Task metadata remains Building at the success boundary",
            ));
        }
    }
    for obligation in b.task_constructors.values() {
        if !obligation.raw_checked
            || module.semantic.is_some() && !obligation.semantic_checked
            || obligation.state != ObligationState::Discharged
        {
            return Err(bad(
                "Task constructor retains an undischarged exact-site WF/source obligation",
            ));
        }
        for key in &obligation.frames {
            let Some(FrameState::MetadataReady(frame)) = b.task_frames.get(key) else {
                return Err(bad("Task constructor names unfinished frame metadata"));
            };
            if module.semantic.is_some() && !frame.worker_checked {
                return Err(bad(
                    "Task frame has no completely checked exact worker world",
                ));
            }
        }
    }
    Ok(())
}
