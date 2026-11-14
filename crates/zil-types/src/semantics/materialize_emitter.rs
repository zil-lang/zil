//! Whole Task context proposals at the actual worker preparation event.
//! Original schemes select the domain; actual function/capture products supply
//! layouts. Neither this view nor a proposed header is an execution proof.
use super::domains::{Domain, Occurrence, OccurrenceRoot};
use super::*;

pub(crate) struct PreparedTask {
    pub context: RetainedLifeShape,
    pub bindings: Vec<RetainedPortBinding>,
    pub inputs: SemanticTypeValue,
    pub headers: Vec<(
        ClosureId,
        StorageLifeContract,
        Vec<SemanticHeaderBinding>,
        Vec<(StorageBase, SemanticTypeValue)>,
    )>,
}

fn fee(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "actual Task context preparation work exceeded",
            None,
        )
    })?;
    Ok(())
}
fn origin_matches(target: &SemanticInstanceTarget, origin: super::super::emission::Origin) -> bool {
    match target {
        SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) => {
            origin.module == *module && origin.creator == *function && origin.closure.is_none()
        }
        SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
            module,
            creator,
            expression,
            occurrence: 0,
        }) => {
            origin.module == *module
                && origin.creator == *creator
                && origin.closure == Some(*expression)
        }
        _ => false,
    }
}

fn actual_closures(
    state: &closures::State,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<ClosureDefinition>> {
    fee(left, state.pending.len())?;
    let mut result = Vec::new();
    for pending in &state.pending {
        let Some(function) = &pending.typed else {
            continue;
        };
        let mut parameters = Vec::new();
        for local in &function.parameters {
            let ty = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("actual Task closure has no real parameter local"))?
                .ty;
            inference::charge_type_snapshot(ty, pending.span, limits, left, 1)?;
            parameters.push(ty.clone());
        }
        let mut captures = Vec::new();
        for capture in &pending.captures {
            inference::charge_type_snapshot(&capture.ty, capture.span, limits, left, 2)?;
            captures.push(capture.stored_type());
        }
        inference::charge_type_snapshot(&function.return_type, pending.span, limits, left, 1)?;
        if let Some(header) = &function.storage_life {
            borrowed_fp::charge_contract(header, |work| fee(left, work), 1)?;
        }
        result.push(ClosureDefinition {
            storage_life: function.storage_life.clone(),
            id: pending.id,
            function: function.id,
            captures,
            parameters,
            result: function.return_type.clone(),
            kind: pending.callable_kind(),
            lending_captures: Vec::new(),
        });
    }
    Ok(result)
}

fn bind_instance(
    directory: &mut ActualDirectory<'_>,
    program: &SemanticModule,
    ledger: &Ledger,
    actual: FunctionId,
    preferred: Option<SemanticInstanceId>,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<SemanticInstanceId>> {
    fee(
        left,
        ledger.frames.len().saturating_add(program.instances.len()),
    )?;
    let Some(frame) = ledger.frames.get(&actual) else {
        return Ok(None);
    };
    let mut selected = None;
    for row in &program.instances {
        if preferred.is_some_and(|preferred| row.id != preferred)
            || !origin_matches(&row.target, frame.origin)
        {
            continue;
        }
        if selected.replace(row).is_some() {
            return Err(bad(
                "actual Task preparation has multiple original worlds for one exact emitter origin",
            ));
        }
    }
    let Some(original) = selected else {
        return Ok(None);
    };
    if let Some(previous) = directory.instances.get(&original.id) {
        if matches!(previous.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id != actual)
        {
            // A different actual world needs its own preparation view. It is
            // never selected because its layout or caller image looks equal.
            return Ok(None);
        }
    }
    crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
    let mut row = original.clone();
    row.target = SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(actual));
    if let Some((_, closure)) = frame.parent {
        let environment = row
            .environment
            .as_mut()
            .ok_or_else(|| bad("actual closure preparation lost its original environment"))?;
        environment.closure = SemanticClosureTarget::Typed(closure);
    }
    let id = row.id;
    directory.instances.insert(id, row);
    Ok(Some(id))
}

fn install_callable_actuals(
    directory: &mut ActualDirectory<'_>,
    program: &SemanticModule,
    state: &closures::State,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let mut changes = Vec::new();
    for (id, row) in &directory.instances {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(actual)) = row.target
        else {
            continue;
        };
        fee(
            left,
            state
                .prepared_helpers
                .len()
                .saturating_add(program.definitions.len()),
        )?;
        let Some((_, bindings)) = state.prepared_helpers.get(&actual) else {
            continue;
        };
        let definition = program
            .definitions
            .iter()
            .find(|definition| definition.id == row.definition)
            .ok_or_else(|| {
                bad("actual helper preparation lost its original full formal inventory")
            })?;
        let mut ordinal = 0u32;
        for (full, formal) in definition.parameters.iter().enumerate() {
            if !matches!(formal, SemanticFormal::Type { .. }) {
                continue;
            }
            let binding = bindings.optional(ordinal);
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| bad("actual helper Type ordinal overflows"))?;
            let Some(closure) = binding else {
                continue;
            };
            fee(left, directory.closures.len())?;
            let closure_definition = directory
                .closures
                .iter()
                .find(|definition| definition.id == closure)
                .ok_or_else(|| {
                    unknown("Task worker callable input awaits its real typed closure definition")
                })?;
            let Some(header) = &closure_definition.storage_life else {
                continue;
            };
            borrowed_fp::charge_contract(header, |work| fee(left, work), 1)?;
            let full = u32::try_from(full)
                .map_err(|_| bad("actual helper full mixed ordinal overflows"))?;
            fee(left, header.fixed_binder_count as usize)?;
            let classes = (0..header.fixed_binder_count)
                .map(|class| SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::TypeArgument {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: full,
                    },
                    path: Vec::new(),
                    class,
                })
                .collect();
            changes.push((
                *id,
                full,
                SemanticActual::Type(SemanticTypeValue::Resolved {
                    ty: Type::Closure(closure),
                    life: StorageLifeShape {
                        binder_count: header.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    classes,
                }),
            ));
        }
    }
    for (instance, ordinal, actual) in changes {
        let row = directory
            .instances
            .get_mut(&instance)
            .ok_or_else(|| bad("actual helper preparation world disappeared"))?;
        let slot = row
            .actuals
            .get_mut(ordinal as usize)
            .ok_or_else(|| bad("actual helper preparation omits full mixed actual positions"))?;
        *slot = actual;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_task(
    state: &closures::State,
    caller: &TypedFunction,
    prior: &[&TypedFunction],
    life_instances: &[LifeNominalInstance],
    declarations: &[DynDeclaration],
    interfaces: &[DynInterface],
    worker: FunctionId,
    parameters: &[Type],
    result: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<PreparedTask>> {
    let ledger = &state.semantic_emission;
    let Some(context) = ledger.original.context() else {
        return Ok(None);
    };
    let expression = ledger
        .active_expression(caller.id, left, caller.span)?
        .ok_or_else(|| bad("Task worker preparation has no real active Core call"))?;
    let scheme = ledger
        .original_task_worker(caller.id, expression, limits, left)?
        .ok_or_else(|| {
            unknown("Task worker preparation awaits its exact checked original call scheme")
        })?;
    fee(
        left,
        ledger
            .frames
            .len()
            .saturating_add(context.semantic.instances.len()),
    )?;
    let actual_worker = ledger
        .frames
        .get(&worker)
        .ok_or_else(|| bad("Task worker has no real emitter origin"))?;
    if !context.semantic.instances.iter().any(|row| {
        row.definition == scheme.worker && origin_matches(&row.target, actual_worker.origin)
    }) {
        return Err(bad(
            "Task preparation changes its exact original worker target",
        ));
    }
    let mut functions = Vec::new();
    fee(
        left,
        prior
            .len()
            .saturating_add(state.pending.len())
            .saturating_add(state.helper_functions.len())
            .saturating_add(state.thin.len())
            .saturating_add(1),
    )?;
    functions.push(caller);
    functions.extend(prior.iter().copied());
    functions.extend(
        state
            .pending
            .iter()
            .filter_map(|pending| pending.typed.as_ref()),
    );
    functions.extend(&state.helper_functions);
    functions.extend(state.thin.iter().filter_map(Option::as_ref));
    let closures = actual_closures(state, limits, left)?;
    let mut directory = ActualDirectory::building(
        functions,
        closures,
        life_instances,
        declarations,
        interfaces,
        BTreeMap::new(),
        limits,
        left,
    )?;
    bind_instance(
        &mut directory,
        &context.semantic,
        ledger,
        caller.id,
        Some(scheme.instance),
        limits,
        left,
    )?
    .ok_or_else(|| bad("Task caller preparation lost its exact original world"))?;
    for function in prior {
        bind_instance(
            &mut directory,
            &context.semantic,
            ledger,
            function.id,
            None,
            limits,
            left,
        )?;
    }
    for definition in directory
        .closures
        .iter()
        .map(|row| (row.id, row.function))
        .collect::<Vec<_>>()
    {
        bind_instance(
            &mut directory,
            &context.semantic,
            ledger,
            definition.1,
            None,
            limits,
            left,
        )?;
    }
    // A previously prepared real closure keeps the same complete original
    // embedding when another TaskCreate observes it. A header without its
    // exact original class bridge is never treated as a closed environment.
    for closure in directory.closures.iter() {
        let Some((header, embedding, _)) = ledger.formed_headers.get(&closure.id) else {
            continue;
        };
        let actual = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("previously formed closure lost its actual header"))?;
        if actual != header {
            return Err(bad("reused Task input closure changes its complete header"));
        }
        ledger.validate_formed_capture(
            closure.id,
            &closure.parameters,
            &closure.captures,
            &closure.result,
            actual,
            limits,
            left,
        )?;
        fee(left, directory.instances.len())?;
        let mut instance = None;
        for row in directory.instances.values() {
            if matches!(&row.environment, Some(env) if env.closure == SemanticClosureTarget::Typed(closure.id))
            {
                if instance.replace(row.id).is_some() {
                    return Err(bad(
                        "reused Task closure has multiple original preparation worlds",
                    ));
                }
            }
        }
        let Some(instance) = instance else {
            continue;
        };
        for row in embedding {
            crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 1)?;
        }
        let row = directory
            .instances
            .get_mut(&instance)
            .ok_or_else(|| bad("reused Task original world disappeared"))?;
        if !row.header_embedding.is_empty() && &row.header_embedding != embedding {
            return Err(bad(
                "reused Task closure changes its original Fixed mapping",
            ));
        }
        row.header_embedding = embedding.clone();
    }
    install_callable_actuals(&mut directory, &context.semantic, state, limits, left)?;
    let mut headers = Vec::new();
    // Actual capture dependencies form a finite creation graph. Revisit only
    // when a previous complete directory was formed; missing dependencies are
    // never assigned zero classes.
    for _ in 0..=directory.closures.len() {
        let mut progress = false;
        let candidates = directory
            .closures
            .iter()
            .filter(|row| row.storage_life.is_none())
            .map(|row| (row.id, row.function))
            .collect::<Vec<_>>();
        fee(left, candidates.len())?;
        for (closure, entry) in candidates {
            let instance = directory.instances.values().find(|row| matches!(row.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == entry));
            let Some(instance) = instance else {
                continue;
            };
            let Some(function) = directory.function(entry) else {
                continue;
            };
            let Some((header, embedding, values)) = super::super::headers::form_emitted_header(
                &context.semantic,
                &directory,
                ledger,
                instance,
                closure,
                function,
                limits,
                left,
            )?
            else {
                continue;
            };
            borrowed_fp::charge_contract(&header, |work| fee(left, work), 2)?;
            for row in &embedding {
                crate::dyn_metadata::semantic_operand_snapshot(row, limits, left, 2)?;
            }
            let instance_id = instance.id;
            directory
                .closures
                .to_mut()
                .iter_mut()
                .find(|row| row.id == closure)
                .ok_or_else(|| bad("formed Task input closure disappeared"))?
                .storage_life = Some(header.clone());
            directory
                .instances
                .get_mut(&instance_id)
                .ok_or_else(|| bad("formed Task input original world disappeared"))?
                .header_embedding = embedding.clone();
            headers.push((closure, header, embedding, values));
            progress = true;
        }
        install_callable_actuals(&mut directory, &context.semantic, state, limits, left)?;
        if !progress {
            break;
        }
    }
    let mut eval = Eval {
        input: &context.semantic,
        module: &directory,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
    };
    let (shape, bindings, inputs) = eval.task_inputs(
        scheme.instance,
        &scheme.signature,
        &scheme.result_sources,
        parameters,
        result,
    )?;
    // Reserve the actual success-context clone, failure input snapshot and
    // canonical failure identity before the builder copies these products.
    crate::dyn_metadata::semantic_operand_snapshot(&inputs, limits, left, 3)?;
    for port in &shape.ports {
        inference::charge_type_snapshot(&port.leaf, caller.span, limits, left, 1)?;
    }
    fee(left, shape.ports.len().saturating_add(bindings.len()))?;
    for binding in &bindings {
        fee(left, binding.source.path.len())?;
    }
    for row in &shape.retained {
        fee(left, row.path.len().saturating_add(1))?;
    }
    for row in &shape.nominal_arguments {
        fee(left, row.path.len().saturating_add(1))?;
    }
    Ok(Some(PreparedTask {
        context: shape,
        bindings,
        inputs,
        headers,
    }))
}

impl Eval<'_, '_, '_> {
    fn task_inputs(
        &mut self,
        scope: SemanticInstanceId,
        signature: &SemanticSignatureRecipe,
        sources: &[super::super::original::TaskReturnSources],
        parameters: &[Type],
        result: &Type,
    ) -> R<(
        RetainedLifeShape,
        Vec<RetainedPortBinding>,
        SemanticTypeValue,
    )> {
        self.fee(parameters.len().saturating_add(signature.parameters.len()))?;
        if parameters.len() != signature.parameters.len() {
            return Err(bad(
                "actual Task worker changes its complete original parameter arity",
            ));
        }
        let mut domains = Vec::new();
        let mut input_maps = Vec::new();
        let mut selectors = Vec::new();
        let mut classes = Vec::new();
        let mut ports = Vec::new();
        let mut bindings = Vec::new();
        let mut retained = Vec::new();
        let mut nominal_arguments = Vec::new();
        for (ordinal, (parameter, actual)) in
            signature.parameters.iter().zip(parameters).enumerate()
        {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| bad("Task actual parameter ordinal overflows"))?;
            let occurrence = Occurrence {
                root: OccurrenceRoot::TaskParameter(ordinal),
                path: Vec::new(),
            };
            let domain = self.type_domain_at(scope, &parameter.ty, Some(actual), 0, &occurrence)?;
            let map = self.join_domain(&mut selectors, &mut classes, &domain)?;
            let SemanticTypeValue::Resolved { ty, life, .. } = &domain.value else {
                unreachable!()
            };
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(actual, Span::default(), self.limits, self.left, 1)?;
            if ty != actual {
                return Err(bad(
                    "actual Task input changes its complete original worker parameter type",
                ));
            }
            for (class, binder) in map.iter().enumerate() {
                retained.push(StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Parameter(ordinal)],
                    class: class as u32,
                    binder: *binder,
                });
            }
            for row in &life.nominal_arguments {
                self.fee(row.path.len().saturating_add(2))?;
                let mut path = vec![LifeTypeProjection::Parameter(ordinal)];
                path.extend_from_slice(&row.path);
                nominal_arguments.push(NominalLifeArgument {
                    path,
                    declaration: row.declaration,
                    argument: row.argument,
                    binder: *map
                        .get(row.binder as usize)
                        .ok_or_else(|| bad("Task input nominal class leaves its full domain"))?,
                });
            }
            let projected = crate::tasks::context_ports::from_type(
                self.module,
                ty,
                life,
                self.limits,
                self.left,
            )?;
            self.fee(projected.len())?;
            for projected in projected {
                let binder = *map
                    .get(projected.port.binder as usize)
                    .ok_or_else(|| bad("Task input port leaves its whole child domain"))?;
                bindings.push(RetainedPortBinding {
                    port: u32::try_from(ports.len())
                        .map_err(|_| bad("Task input port ordinal overflows"))?,
                    source: StoragePosition {
                        base: StorageBase::Parameter(ordinal),
                        path: projected.path,
                    },
                });
                ports.push(RetainedBorrowPort {
                    binder,
                    ..projected.port
                });
            }
            input_maps.push(map);
            domains.push(domain);
        }
        let output = self.type_domain_at(
            scope,
            &signature.result,
            Some(result),
            0,
            &Occurrence {
                root: OccurrenceRoot::TaskResult,
                path: Vec::new(),
            },
        )?;
        let SemanticTypeValue::Resolved {
            ty,
            life,
            classes: images,
        } = &output.value
        else {
            unreachable!()
        };
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        inference::charge_type_snapshot(result, Span::default(), self.limits, self.left, 1)?;
        if ty != result || life.binder_count as usize != output.selectors.len() {
            return Err(bad(
                "actual Task worker changes its complete original result type",
            ));
        }
        let mut result_map = Vec::new();
        for (class, (selector, image)) in output.selectors.iter().zip(images).enumerate() {
            let binder =
                if let Some(binder) = self.find_selector(&selectors, &classes, selector, image)? {
                    binder
                } else {
                    self.task_return_class(
                        scope,
                        signature,
                        sources,
                        &domains,
                        &input_maps,
                        &output,
                        class as u32,
                    )?
                };
            self.fee(result_map.len().saturating_add(1))?;
            if result_map.contains(&binder) {
                return Err(bad("Task result collapses two complete intrinsic indices"));
            }
            result_map.push(binder);
            retained.push(StorageRetainedBinding {
                path: vec![LifeTypeProjection::Result],
                class: class as u32,
                binder,
            });
        }
        let binder_count =
            u32::try_from(selectors.len()).map_err(|_| bad("Task input joint domain overflows"))?;
        let inputs = self.compose_tuple(domains)?;
        Ok((
            RetainedLifeShape {
                binder_count,
                ports,
                retained,
                nominal_arguments,
            },
            bindings,
            inputs.value,
        ))
    }
}
