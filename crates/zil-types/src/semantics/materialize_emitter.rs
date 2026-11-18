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

// Preparation needs the complete transitive closure of the actual input,
// result and caller-capture Types. Other completed bodies remain mandatory at
// finish, after their deferred nominal applications have been registered.
fn needed_closure_type(
    ty: &Type,
    directory: &ActualDirectory<'_>,
    needed: &mut BTreeSet<ClosureId>,
    seen_interfaces: &mut BTreeSet<DynInterfaceId>,
    seen_nominals: &mut BTreeSet<usize>,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<()> {
    fee(left, 1)?;
    if depth > limits.max_call_depth || needed.len() > limits.max_functions {
        return Err(Diagnostic::limit(
            Phase::Types,
            "Task preparation closure dependency limit exceeded",
            None,
        ));
    }
    // Nominal Type arguments can be phantom in the visible layout. Follow
    // every exact application's complete actual vector as well as its fields.
    if matches!(
        ty,
        Type::BorrowedStorage { .. }
            | Type::Record { .. }
            | Type::Package { .. }
            | Type::Enum { .. }
    ) {
        fee(left, directory.life_instances.len())?;
        for (position, row) in directory.life_instances.iter().enumerate() {
            inference::charge_type_snapshot(ty, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
            if &row.ty != ty {
                continue;
            }
            fee(
                left,
                seen_nominals.len().saturating_add(1).ilog2() as usize + 1,
            )?;
            if !seen_nominals.insert(position) {
                continue;
            }
            fee(left, row.arguments.len())?;
            for argument in &row.arguments {
                if let LifeNominalActual::Type(child)
                | LifeNominalActual::QualifiedType { ty: child, .. } = argument
                {
                    needed_closure_type(
                        child,
                        directory,
                        needed,
                        seen_interfaces,
                        seen_nominals,
                        limits,
                        left,
                        depth + 1,
                    )?;
                }
            }
        }
    }
    let mut visit = |child: &Type| {
        needed_closure_type(
            child,
            directory,
            needed,
            seen_interfaces,
            seen_nominals,
            limits,
            left,
            depth + 1,
        )
    };
    match ty {
        Type::BorrowedStorage { storage: inner, .. }
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Opaque { hidden: inner, .. } => visit(inner)?,
        Type::Task {
            result,
            retained_life,
        }
        | Type::MutexGuard {
            payload: result,
            retained_life,
        } => {
            visit(result)?;
            for port in &retained_life.ports {
                visit(&port.leaf)?;
            }
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                visit(parameter)?;
            }
            visit(result)?;
        }
        Type::Tuple(fields) => {
            for field in fields {
                visit(field)?;
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                visit(field)?;
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    visit(field)?;
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            visit(error)?;
            visit(result)?;
        }
        Type::Closure(id) => {
            fee(left, needed.len().saturating_add(1).ilog2() as usize + 1)?;
            if needed.insert(*id) {
                fee(left, directory.closures.len())?;
                let closure = directory
                    .closures
                    .iter()
                    .find(|row| row.id == *id)
                    .ok_or_else(|| {
                        unknown("Task preparation dependency has no actual closure definition")
                    })?;
                for child in closure
                    .captures
                    .iter()
                    .chain(&closure.parameters)
                    .chain(std::iter::once(&closure.result))
                {
                    needed_closure_type(
                        child,
                        directory,
                        needed,
                        seen_interfaces,
                        seen_nominals,
                        limits,
                        left,
                        depth + 1,
                    )?;
                }
            }
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            fee(
                left,
                seen_interfaces.len().saturating_add(1).ilog2() as usize + 1,
            )?;
            if seen_interfaces.insert(*interface) {
                fee(left, directory.dyn_interfaces.len())?;
                let row = directory
                    .dyn_interfaces
                    .iter()
                    .find(|row| row.id == *interface)
                    .ok_or_else(|| {
                        unknown("Task preparation dependency has no complete dyn interface")
                    })?;
                if let Some(context) = &row.context {
                    needed_closure_type(
                        context,
                        directory,
                        needed,
                        seen_interfaces,
                        seen_nominals,
                        limits,
                        left,
                        depth + 1,
                    )?;
                }
                for port in &row.retained_life.ports {
                    needed_closure_type(
                        &port.leaf,
                        directory,
                        needed,
                        seen_interfaces,
                        seen_nominals,
                        limits,
                        left,
                        depth + 1,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
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
    bind_instance_at(directory, program, ledger, actual, preferred, limits, left,
        0, &mut BTreeSet::new())
}

fn bind_instance_at(
    directory: &mut ActualDirectory<'_>, program: &SemanticModule, ledger: &Ledger,
    actual: FunctionId, preferred: Option<SemanticInstanceId>, limits: &Limits,
    left: &mut usize, depth: usize, active: &mut BTreeSet<FunctionId>,
) -> R<Option<SemanticInstanceId>> {
    fee(left, active.len().saturating_add(1).ilog2() as usize + 2)?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types,
            "actual creator frame dependency depth exceeded", None));
    }
    if !active.insert(actual) {
        return Err(bad("actual creator frames have a cyclic creation dependency"));
    }
    let result = (|| {
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
        if let Some((creator, _)) = frame.parent {
            let parent = original.parent
                .ok_or_else(|| bad("actual closure preparation loses its original lexical creator"))?;
            fee(left, program.definitions.len().saturating_add(program.instances.len()))?;
            let child = program.definitions.iter().find(|row| row.id == original.definition)
                .ok_or_else(|| bad("actual creator binding has no original child definition"))?;
            let parent_row = directory.original_instance(program, parent)
                .ok_or_else(|| bad("actual creator binding has no original parent instance"))?;
            if child.kind != SemanticDefinitionKind::Closure || child.parent != Some(parent_row.definition)
                || !matches!(original.environment.as_ref(), Some(environment)
                    if matches!(environment.closure, SemanticClosureTarget::Core { .. }
                        | SemanticClosureTarget::Typed(_)))
            {
                return Err(bad("actual closure creator changes its complete original lexical owner"));
            }
            if directory.function(creator).is_none() {
                // A nonneeded completed child may refer to a creator which
                // is still being emitted. It stays cold until real finish;
                // a needed child is independently required below.
                return Ok(None);
            }
            // The creator is the exact emitted parent, not an equal input type or
            // a first helper sharing a template name. Bind it before the child.
            if bind_instance_at(directory, program, ledger, creator, Some(parent), limits, left,
                depth + 1, active)? != Some(parent)
            {
                return Ok(None);
            }
            let actual_parent = directory.original_instance(program, parent)
                .ok_or_else(|| bad("actual creator binding lost its completed parent"))?;
            if actual_parent.target != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(creator)) {
                return Err(bad("actual child preparation changes its exact Typed creator"));
            }
        }
        crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
        let mut row = original.clone();
        row.target = SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(actual));
        if let Some((_, Some(closure))) = frame.parent {
            let environment = row
                .environment
                .as_mut()
                .ok_or_else(|| bad("actual closure preparation lost its original environment"))?;
            environment.closure = SemanticClosureTarget::Typed(closure);
        }
        let id = row.id;
        directory.instances.insert(id, row);
        Ok(Some(id))
    })();
    active.remove(&actual);
    result
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

/// Scheduling uses real actual Types and the real helper's original mixed
/// positions, never a failed materialization or an asserted self-formal as a
/// substitute. A missing dependency grants no header or zero-domain fact.
fn header_dependencies(
    directory: &ActualDirectory<'_>, program: &SemanticModule, state: &closures::State,
    instance: &SemanticInstance, closure: ClosureId, limits: &Limits, left: &mut usize,
) -> R<BTreeSet<ClosureId>> {
    fee(left, directory.closures.len())?;
    let actual = directory.closures.iter().find(|row| row.id == closure)
        .ok_or_else(|| bad("Task header dependency has no exact actual closure"))?;
    let mut dependencies = BTreeSet::new();
    let mut interfaces = BTreeSet::new();
    let mut nominals = BTreeSet::new();
    for ty in actual.captures.iter().chain(&actual.parameters).chain(std::iter::once(&actual.result)) {
        needed_closure_type(ty, directory, &mut dependencies, &mut interfaces, &mut nominals,
            limits, left, 0)?;
    }
    // Even a phantom callable actual in the creator's full mixed directory
    // requires its actual class header before Formal can be interpreted.
    let mut owner = Some(instance.id);
    let mut visited = BTreeSet::new();
    while let Some(scope) = owner {
        fee(left, visited.len().saturating_add(1).ilog2() as usize + 2)?;
        if visited.len() > limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types,
                "Task header dependency lexical depth exceeded", None));
        }
        if !visited.insert(scope) {
            return Err(bad("Task header dependency changes its finite lexical parent chain"));
        }
        fee(left, directory.instances.len().saturating_add(1).ilog2() as usize
            + program.instances.len().saturating_add(2))?;
        let row = directory.original_instance(program, scope)
            .ok_or_else(|| bad("Task header dependency loses its original lexical scope"))?;
        if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = row.target {
            fee(left, state.prepared_helpers.len().saturating_add(1).ilog2() as usize + 1)?;
            if let Some((_, bindings)) = state.prepared_helpers.get(&function) {
                fee(left, program.definitions.len())?;
                let definition = program.definitions.iter().find(|definition| definition.id == row.definition)
                    .ok_or_else(|| bad("Task helper dependency loses its full original formal directory"))?;
                let mut type_ordinal = 0u32;
                fee(left, definition.parameters.len())?;
                for formal in &definition.parameters {
                    if !matches!(formal, SemanticFormal::Type { .. }) { continue; }
                    let bound = bindings.optional(type_ordinal);
                    type_ordinal = type_ordinal.checked_add(1)
                        .ok_or_else(|| bad("Task helper dependency Type ordinal overflows"))?;
                    if let Some(child) = bound {
                        needed_closure_type(&Type::Closure(child), directory, &mut dependencies,
                            &mut interfaces, &mut nominals, limits, left, 0)?;
                    }
                }
            }
        }
        owner = row.parent;
    }
    Ok(dependencies)
}

#[allow(clippy::too_many_arguments)]
type FormedHeaders = Vec<(
    ClosureId,
    StorageLifeContract,
    Vec<SemanticHeaderBinding>,
    Vec<(StorageBase, SemanticTypeValue)>,
)>;
#[allow(clippy::too_many_arguments)]
fn emitter_directory<'m>(
    state: &'m closures::State,
    caller: crate::FunctionRef<'m>,
    prior: &[crate::FunctionRef<'m>],
    life_instances: &'m [LifeNominalInstance],
    declarations: &'m [DynDeclaration],
    interfaces: &'m [DynInterface],
    program: &SemanticModule,
    caller_instance: SemanticInstanceId,
    needed_types: impl IntoIterator<Item = &'m Type>,
    limits: &Limits,
    left: &mut usize,
) -> R<(ActualDirectory<'m>, FormedHeaders)> {
    let ledger = &state.semantic_emission;
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
            .filter_map(|pending| pending.typed.as_ref())
            .map(crate::FunctionRef::from),
    );
    functions.extend(state.helper_functions.iter().map(crate::FunctionRef::from));
    functions.extend(
        state
            .thin
            .iter()
            .filter_map(Option::as_ref)
            .map(crate::FunctionRef::from),
    );
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
    let mut needed = BTreeSet::new();
    let mut seen_interfaces = BTreeSet::new();
    let mut seen_nominals = BTreeSet::new();
    for ty in needed_types {
        inference::charge_type_snapshot(ty, caller.span, limits, left, 1)?;
        needed_closure_type(
            ty,
            &directory,
            &mut needed,
            &mut seen_interfaces,
            &mut seen_nominals,
            limits,
            left,
            0,
        )?;
    }
    if let Some(id) = caller.environment {
        fee(left, state.pending.len())?;
        let pending = state
            .pending
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| bad("Task preparation lost its actual caller capture directory"))?;
        for capture in &pending.captures {
            inference::charge_type_snapshot(&capture.ty, caller.span, limits, left, 1)?;
            needed_closure_type(
                &capture.ty,
                &directory,
                &mut needed,
                &mut seen_interfaces,
                &mut seen_nominals,
                limits,
                left,
                0,
            )?;
        }
    }
    bind_instance(
        &mut directory,
        program,
        ledger,
        caller.id,
        Some(caller_instance),
        limits,
        left,
    )?
    .ok_or_else(|| bad("Task caller preparation lost its exact original world"))?;
    loop {
        let before = needed.len();
        fee(left, directory.closures.len().saturating_mul(
            needed.len().saturating_add(1).ilog2() as usize + 1,
        ).saturating_add(needed.len().saturating_mul(std::mem::size_of::<(ClosureId, FunctionId)>())))?;
        let mut required = Vec::with_capacity(needed.len());
        for row in directory.closures.iter() {
            if needed.contains(&row.id) { required.push((row.id, row.function)); }
        }
        for (closure, entry) in required {
            let scope = bind_instance(&mut directory, program, ledger, entry, None, limits, left)?
                .ok_or_else(|| unknown("needed Task closure lacks one exact original creator context"))?;
            fee(left, directory.instances.len().saturating_add(1).ilog2() as usize + 1)?;
            let instance = directory.instances.get(&scope)
                .ok_or_else(|| bad("needed Task closure lost its actual preparation world"))?;
            let dependencies = header_dependencies(&directory, program, state, instance, closure, limits, left)?;
            for dependency in dependencies {
                fee(left, (needed.len().saturating_add(1).ilog2() as usize + 1).saturating_mul(2))?;
                if !needed.contains(&dependency) && needed.len() >= limits.max_functions {
                    return Err(Diagnostic::limit(Phase::Types,
                        "complete Task header dependency count exceeded", None));
                }
                needed.insert(dependency);
            }
        }
        if needed.len() == before { break; }
    }
    for function in prior {
        bind_instance(
            &mut directory,
            program,
            ledger,
            function.id,
            None,
            limits,
            left,
        )?;
    }
    fee(left, directory.closures.len().saturating_mul(
        std::mem::size_of::<FunctionId>().saturating_add(1),
    ))?;
    let entries = directory.closures.iter().map(|row| row.function).collect::<Vec<_>>();
    for entry in entries {
        bind_instance(
            &mut directory,
            program,
            ledger,
            entry,
            None,
            limits,
            left,
        )?;
    }
    for function in state.helper_functions.iter().chain(state.thin.iter().filter_map(Option::as_ref)) {
        bind_instance(&mut directory, program, ledger, function.id, None, limits, left)?;
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
    install_callable_actuals(&mut directory, program, state, limits, left)?;
    let mut headers = Vec::new();
    // Actual capture dependencies form a finite creation graph. Revisit only
    // when a previous complete directory was formed; missing dependencies are
    // never assigned zero classes.
    for _ in 0..=directory.closures.len() {
        let mut progress = false;
        fee(
            left,
            directory
                .closures
                .len()
                .saturating_mul(needed.len().saturating_add(1).ilog2() as usize + 1),
        )?;
        fee(left, needed.len().saturating_mul(
            std::mem::size_of::<(ClosureId, FunctionId)>(),
        ))?;
        let mut candidates = Vec::with_capacity(needed.len());
        for row in directory.closures.iter() {
            if needed.contains(&row.id) && row.storage_life.is_none() {
                candidates.push((row.id, row.function));
            }
        }
        fee(left, candidates.len())?;
        for (closure, entry) in candidates {
            fee(left, directory.instances.len())?;
            let mut worlds = directory.instances.values().filter(|row| matches!(row.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == entry));
            let Some(instance) = worlds.next() else { continue; };
            if worlds.next().is_some() {
                return Err(bad("Task closure header has multiple exact original preparation worlds"));
            }
            let dependencies = header_dependencies(&directory, program, state, instance, closure, limits, left)?;
            fee(left, dependencies.len().saturating_mul(
                directory.closures.len().saturating_add(needed.len().saturating_add(1).ilog2() as usize + 2),
            ))?;
            let mut ready = true;
            for dependency in dependencies {
                if !needed.contains(&dependency) {
                    return Err(bad("Task header source changes its complete prepared dependency closure"));
                }
                ready &= directory.closures.iter()
                    .any(|row| row.id == dependency && row.storage_life.is_some());
            }
            if !ready { continue; }
            let Some(function) = directory.function(entry) else {
                continue;
            };
            let (header, embedding, values) = match super::super::headers::form_emitted_header(
                program, &directory, ledger, instance, closure, function, limits, left,
            )? {
                super::super::headers::HeaderFormation::Complete { header, embedding, values } =>
                    (header, embedding, values),
                super::super::headers::HeaderFormation::NotReady { .. } => continue,
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
            // A later candidate in this same round may capture this exact
            // helper actual. Publish the complete actual vector immediately,
            // not after a consumer has re-entered its unresolved self-formal.
            install_callable_actuals(&mut directory, program, state, limits, left)?;
        }
        install_callable_actuals(&mut directory, program, state, limits, left)?;
        if !progress {
            break;
        }
    }
    for id in needed {
        fee(left, directory.closures.len())?;
        if !directory
            .closures
            .iter()
            .any(|row| row.id == id && row.storage_life.is_some())
        {
            return Err(unknown(
                "Task preparation closure dependency has no complete actual header",
            ));
        }
    }
    Ok((directory, headers))
}

pub(crate) fn prepare_task(
    state: &closures::State,
    caller: crate::FunctionRef<'_>,
    prior: &[crate::FunctionRef<'_>],
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
    let (directory, headers) = emitter_directory(
        state,
        caller,
        prior,
        life_instances,
        declarations,
        interfaces,
        &context.semantic,
        scheme.instance,
        parameters.iter().chain(std::iter::once(result)),
        limits,
        left,
    )?;
    let mut eval = Eval {
        input: &context.semantic,
        module: &directory,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
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
            let mut input_shape = StorageLifeShape {
                binder_count: u32::try_from(classes.len())
                    .map_err(|_| bad("Task input joint domain overflows"))?,
                ..empty()
            };
            inherit_child_shape(
                &mut input_shape,
                life,
                LifeTypeProjection::Parameter(ordinal),
                &map,
                self.limits,
                self.left,
            )?;
            retained.extend(input_shape.retained);
            nominal_arguments.extend(input_shape.nominal_arguments);
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
        }
        let binder_count =
            u32::try_from(selectors.len()).map_err(|_| bad("Task input joint domain overflows"))?;
        let mut result_shape = StorageLifeShape {
            binder_count,
            slots: Vec::new(),
            retained,
            nominal_arguments,
        };
        inherit_child_shape(
            &mut result_shape,
            life,
            LifeTypeProjection::Result,
            &result_map,
            self.limits,
            self.left,
        )?;
        let inputs = self.compose_tuple(domains)?;
        Ok((
            RetainedLifeShape {
                binder_count,
                ports,
                retained: result_shape.retained,
                nominal_arguments: result_shape.nominal_arguments,
            },
            bindings,
            inputs.value,
        ))
    }
}

pub(crate) struct PreparedPoll {
    pub instance: LifeNominalInstance,
    pub headers: FormedHeaders,
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_poll(
    state: &closures::State,
    caller: crate::FunctionRef<'_>,
    prior: &[crate::FunctionRef<'_>],
    life_instances: &[LifeNominalInstance],
    declarations: &[DynDeclaration],
    interfaces: &[DynInterface],
    name: &str,
    task: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<PreparedPoll>> {
    use super::super::original::TaskOp;
    let ledger = &state.semantic_emission;
    let Some(context) = ledger.original.context() else {
        return Ok(None);
    };
    let expression = ledger
        .active_expression(caller.id, left, caller.span)?
        .ok_or_else(|| bad("Task poll has no real active source call"))?;
    let (scope, op, recipe) = ledger
        .original_task_payload(caller.id, expression, limits, left)?
        .ok_or_else(|| unknown("Task poll awaits its exact original payload scheme"))?;
    let close = match (name, op) {
        ("task_poll", TaskOp::Poll) => false,
        ("task_poll_close", TaskOp::PollClose) => true,
        _ => return Err(bad("actual Task poll changes the exact original operation")),
    };
    let Type::Task { result, .. } = task else {
        return Err(bad("Task poll lost its actual full carrier"));
    };
    let (directory, headers) = emitter_directory(
        state,
        caller,
        prior,
        life_instances,
        declarations,
        interfaces,
        &context.semantic,
        scope,
        std::iter::once(task),
        limits,
        left,
    )?;
    let mut eval = Eval {
        input: &context.semantic,
        module: &directory,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    };
    let mut child = eval.type_domain(scope, &recipe, Some(result), 0)?;
    if close {
        child = eval.type_domain(
            scope,
            &SemanticTypeRecipe::Atom(Type::Unit),
            Some(&Type::Unit),
            0,
        )?;
    }
    // The declared Poll<T> payload is a whole-Type formal boundary. Every
    // intrinsic child index survives, including classes with equal images.
    // The nominal TypeArgument map below preserves the entire formal domain.
    // Physical ordinary wrappers keep their real retained paths; an opaque
    // qualified child has its own whole boundary in the shared composer.
    child.frozen = false;
    let SemanticTypeValue::Resolved {
        ty: payload,
        life: payload_life,
        ..
    } = &child.value
    else {
        unreachable!()
    };
    inference::charge_type_snapshot(payload, caller.span, limits, eval.left, 3)?;
    let payload = payload.clone();
    let payload_life = eval.copy_shape(payload_life)?;
    let SemanticTypeValue::Resolved { life: mut life, .. } = eval.compose_tuple(vec![child])?.value
    else {
        unreachable!()
    };
    let variant = PlaceProjection::Variant(VariantProjection {
        variant: 1,
        index: 0,
    });
    for slot in &mut life.slots {
        match slot.path.first_mut() {
            Some(head @ PlaceProjection::Field(0)) => *head = variant.clone(),
            _ => {
                return Err(bad(
                    "Poll field composition changes its actual payload path",
                ));
            }
        }
    }
    for row in &mut life.retained {
        match row.path.first_mut() {
            Some(LifeTypeProjection::Place(head @ PlaceProjection::Field(0))) => {
                *head = variant.clone()
            }
            _ => return Err(bad("Poll child domain changes its declared Ready position")),
        }
    }
    for row in &mut life.nominal_arguments {
        match row.path.first_mut() {
            Some(LifeTypeProjection::Place(head @ PlaceProjection::Field(0))) => {
                *head = variant.clone()
            }
            _ => {
                return Err(bad(
                    "Poll nominal child changes its declared Ready position",
                ));
            }
        }
    }
    eval.fee(life.binder_count as usize)?;
    let map = (0..life.binder_count).collect::<Vec<_>>();
    inherit_child_shape(
        &mut life,
        &payload_life,
        LifeTypeProjection::TypeArgument(0),
        &map,
        limits,
        eval.left,
    )?;
    let storage = Type::Enum {
        name: crate::tasks::poll_name(&payload),
        variants: vec![
            ("Pending".into(), Vec::new()),
            ("Ready".into(), vec![payload.clone()]),
        ],
    };
    let ty = if life.binder_count == 0 {
        storage
    } else {
        Type::BorrowedStorage {
            storage: Box::new(storage),
            life,
        }
    };
    inference::charge_type_snapshot(&ty, caller.span, limits, eval.left, 3)?;
    // The separate root occurrence directory belongs to the actual argument,
    // even when its erased physical type is Address or Desc.
    let instance = LifeNominalInstance {
        declaration: zil_ir::builtin_life::POLL,
        ty,
        arguments: vec![LifeNominalActual::QualifiedType {
            ty: payload,
            life: payload_life,
        }],
    };
    Ok(Some(PreparedPoll { instance, headers }))
}
