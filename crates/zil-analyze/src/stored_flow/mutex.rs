//! Mutex manager reach, guard storage and protected payloads are three distinct
//! origins. Only an actual lock followed by a mutable live guard projection can
//! create a payload window; descriptors never contain that permission.
use super::*;
use zil_ir::ContractProjection;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Returns {
    sources: Vec<Target>,
    missing: bool,
}
pub(super) fn work(value: &Returns, b: &mut Budget) -> CompileResult<()> {
    fee(b, value.sources.len().saturating_add(1))?;
    for source in &value.sources {
        fee(b, target_width(source).saturating_add(1))?;
    }
    Ok(())
}
pub(super) fn merge(to: &mut Returns, from: &Returns, b: &mut Budget) -> CompileResult<bool> {
    work(from, b)?;
    let mut changed = false;
    if from.missing && !to.missing {
        to.missing = true;
        changed = true;
    }
    for source in &from.sources {
        fee(
            b,
            to.sources
                .len()
                .saturating_mul(target_width(source).saturating_add(1)),
        )?;
        if !to.sources.contains(source) {
            fee(b, target_width(source).saturating_add(1))?;
            to.sources.push(source.clone());
            changed = true;
        }
    }
    Ok(changed)
}

/// The stored-address route returns before the ordinary synchronization
/// schedule. Keep the complete actual worker graph, supported target, acquire/
/// release signatures and failure-cleanup checks on this route as well.
pub(super) fn module(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if b.generic_mutex || b.scoped_threads {
        crate::scoped_thread::check(module, b)?;
        crate::generic_mutex::check(module, b)?;
    }
    Ok(())
}

fn thread_scope(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::WithScopedThread {
        worker,
        argument,
        body,
    } = i.kind
    else {
        return Ok(false);
    };
    if !description(value_type(f, argument)?) {
        return Ok(false);
    }
    if !b.scoped_threads || !b.generic_mutex {
        return Err(failure(
            "Mutex thread scope is absent from its complete actual synchronization schedule",
        ));
    }
    let worker = module
        .functions
        .get(worker.0 as usize)
        .filter(|row| row.id == worker)
        .ok_or_else(|| failure("Mutex thread worker identity is absent"))?;
    fee(b, worker.parameters.len().saturating_add(1))?;
    if worker.parameters.len() != 1
        || worker
            .parameters
            .first()
            .and_then(|id| worker.locals.get(id.0 as usize))
            .map(|row| &row.ty)
            != Some(value_type(f, argument)?)
        || worker.return_type != Type::I32
        || worker.result_mode != PassingMode::Infer
        || worker.call_kind != CallKind::Direct
        || worker.foreign.is_some()
        || worker.environment.is_some()
    {
        return Err(failure(
            "Mutex thread scope changes its exact actual root worker signature or execution kind",
        ));
    }
    let Type::Closure(id) = value_type(f, body)? else {
        return Err(failure(
            "Mutex thread parent body is not an actual concrete closure",
        ));
    };
    let definition = module
        .closures
        .get(id.0 as usize)
        .filter(|row| row.id == *id)
        .ok_or_else(|| failure("Mutex thread parent environment is absent"))?;
    let entry = module
        .functions
        .get(definition.function.0 as usize)
        .filter(|row| row.id == definition.function)
        .ok_or_else(|| failure("Mutex thread parent executable entry is absent"))?;
    fee(
        b,
        definition
            .captures
            .len()
            .saturating_add(definition.parameters.len())
            .saturating_add(entry.parameters.len())
            .saturating_add(1),
    )?;
    if !definition.parameters.is_empty()
        || definition.result != Type::I32
        || !definition.lending_captures.is_empty()
        || entry.environment != Some(*id)
        || !entry.parameters.is_empty()
        || entry.return_type != Type::I32
        || entry.result_mode != PassingMode::Infer
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
    {
        return Err(failure(
            "Mutex thread parent body changes its complete actual concrete entry or scalar result",
        ));
    }
    for capture in &definition.captures {
        b.type_work(capture, false, "Mutex scoped parent capture domain")?;
        if !description(capture) && !capture.is_b0_scalar() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "Mutex thread parent capture needs its complete stored payload access and cleanup substitution",
                Some(i.span),
            )]);
        }
    }
    if f.call_kind != CallKind::Direct
        || i.result.and_then(|id| f.values.get(id.0 as usize))
            != Some(&Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]))
    {
        return Err(failure(
            "Mutex thread scope changes its complete synchronous result carrier",
        ));
    }
    Ok(true)
}

fn guard(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::MutexGuard {
        payload,
        retained_life,
    } = ty
    else {
        fee(b, 1)?;
        return Ok(false);
    };
    b.type_work(ty, false, "complete Mutex guard identity")?;
    if payload.as_ref() != &Type::I32 {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "Mutex stored payload needs its complete non-scalar storage and cleanup transport",
            None,
        )]);
    }
    fee(
        b,
        retained_life
            .ports
            .len()
            .saturating_add(retained_life.retained.len())
            .saturating_add(retained_life.nominal_arguments.len())
            .saturating_add(1),
    )?;
    if retained_life.binder_count != 1
        || !retained_life.retained.is_empty()
        || !retained_life.nominal_arguments.is_empty()
        || !matches!(retained_life.ports.as_slice(),[port] if port.leaf==(Type::Mutex{payload:Box::new(Type::I32)}) && port.mode==StoredBorrowMode::SharedAddress && port.binder==0)
    {
        return Err(failure(
            "Mutex guard changes its complete sealed retained manager domain",
        ));
    }
    Ok(true)
}
fn manager(ty: &Type) -> bool {
    matches!(ty,Type::Mutex{payload} if payload.as_ref()==&Type::I32)
}
fn owner(ty: &Type) -> bool {
    matches!(ty,Type::Managed(payload) if manager(payload))
}
fn description(ty: &Type) -> bool {
    matches!(ty,Type::Desc(payload) if manager(payload))
}
fn create_union(ty: &Type) -> bool {
    matches!(ty,Type::ErrorUnion{error,result} if error.as_ref()==&crate::generic_mutex::error_type() && owner(result))
}
fn union_local<'a>(f: &'a TypedFunction, local: LocalId) -> CompileResult<&'a Type> {
    f.locals
        .get(local.0 as usize)
        .filter(|row| row.id == local)
        .map(|row| &row.ty)
        .ok_or_else(|| failure("Mutex allocation union local is absent"))
}
fn union_arm(ty: &Type, is_error: bool) -> CompileResult<&Type> {
    let Type::ErrorUnion { error, result } = ty else {
        return Err(failure("Mutex allocation arm has no complete union"));
    };
    if !create_union(ty) {
        return Err(failure(
            "Mutex allocation arm selects another complete error/result family",
        ));
    }
    Ok(if is_error {
        error.as_ref()
    } else {
        result.as_ref()
    })
}
fn guard_head(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    match ty {
        Type::Address { inner, .. } => guard(inner, b),
        _ => Ok(false),
    }
}
fn resource(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    let payload = Type::Mutex {
        payload: Box::new(Type::I32),
    };
    b.type_work(&payload, false, "Mutex sealed resource identity")?;
    let name = format!("@builtin:Mutex<{}>", Type::I32.fingerprint());
    fee(b, name.len().saturating_add(module.resources.len()))?;
    let mut found = 0usize;
    for actual in &module.resources {
        b.type_work(&actual.payload, false, "Mutex complete resource candidate")?;
        fee(b, actual.name.len())?;
        if actual.payload == payload || actual.name == name {
            if actual.payload != payload || actual.name != name || actual.teardown.is_some() {
                return Err(failure(
                    "Mutex resource changes its exact sealed payload or synchronous cleanup role",
                ));
            }
            found = found
                .checked_add(1)
                .ok_or_else(|| failure("Mutex resource inventory overflows"))?;
        }
    }
    if found > 1 {
        return Err(failure(
            "Mutex repeats its actual sealed manager resource identity",
        ));
    }
    // Mutex is a sealed language owner, not a user ResourceDefinition. Source
    // may correctly emit no resource row. A row cannot replace the operation,
    // manager-origin, affine or epoch proofs, and a conflicting row is never
    // an alternative definition of the builtin.
    Ok(())
}
pub(super) fn admitted(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let selected = owner(ty)
        || description(ty)
        || guard(ty, b)?
        || guard_head(ty, b)?
        || matches!(ty,Type::Address{inner,..} if owner(inner))
        || create_union(ty);
    if selected {
        resource(module, b)?;
    }
    Ok(selected)
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    fee(b, 1)?;
    let output = i.result.map(|id| value_type(f, id)).transpose()?;
    let guard_ty = || Type::MutexGuard {
        payload: Box::new(Type::I32),
        retained_life: RetainedLifeShape {
            binder_count: 1,
            ports: vec![RetainedBorrowPort {
                leaf: Type::Mutex {
                    payload: Box::new(Type::I32),
                },
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
    };
    let expected = match &i.kind {
        InstructionKind::ErrorTag { value } if create_union(value_type(f, *value)?) => {
            Some(Type::Bool)
        }
        InstructionKind::ReadErrorUnion { value, is_error }
            if create_union(value_type(f, *value)?) =>
        {
            if !*is_error {
                return Err(failure(
                    "Mutex allocation success owner cannot be copied by ReadErrorUnion",
                ));
            }
            Some(union_arm(value_type(f, *value)?, true)?.clone())
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if create_union(value_type(f, *value)?) =>
        {
            Some(union_arm(value_type(f, *value)?, *is_error)?.clone())
        }
        InstructionKind::MakeErrorUnion { value, is_error } if output.is_some_and(create_union) => {
            let union =
                output.ok_or_else(|| failure("Mutex allocation union constructor omits result"))?;
            if value_type(f, *value)? != union_arm(union, *is_error)? {
                return Err(failure(
                    "Mutex allocation union input changes its selected full arm",
                ));
            }
            Some(union.clone())
        }
        InstructionKind::PlaceTagLocal { local, path } if create_union(union_local(f, *local)?) => {
            if !path.is_empty() {
                return Err(failure(
                    "Mutex allocation tag observation changes its actual root place",
                ));
            }
            Some(Type::I32)
        }
        InstructionKind::TakeVariantField { local, path }
            if create_union(union_local(f, *local)?) =>
        {
            let [PlaceProjection::ErrorArm(is_error)] = path.as_slice() else {
                return Err(failure(
                    "Mutex allocation extraction changes its exact complete selected arm",
                ));
            };
            Some(union_arm(union_local(f, *local)?, *is_error)?.clone())
        }
        InstructionKind::BorrowPlaceLocal {
            local,
            path,
            mutable,
        } if create_union(union_local(f, *local)?) => {
            let [PlaceProjection::ErrorArm(is_error)] = path.as_slice() else {
                return Err(failure(
                    "Mutex allocation borrow changes its exact complete selected arm",
                ));
            };
            if *mutable && !f.locals[local.0 as usize].mutable {
                return Err(failure(
                    "Mutex allocation place borrow upgrades an immutable actual local",
                ));
            }
            Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(union_arm(union_local(f, *local)?, *is_error)?.clone()),
            })
        }
        InstructionKind::DropErrorUnion { local } if create_union(union_local(f, *local)?) => None,
        InstructionKind::WithScopedThread { .. } if thread_scope(module, f, i, b)? => {
            Some(Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]))
        }
        InstructionKind::MutexCreate { value } => {
            if value_type(f, *value)? != &Type::I32 {
                return Err(failure(
                    "Mutex construction lacks its complete supported actual payload",
                ));
            }
            Some(Type::ErrorUnion {
                error: Box::new(crate::generic_mutex::error_type()),
                result: Box::new(Type::Managed(Box::new(Type::Mutex {
                    payload: Box::new(Type::I32),
                }))),
            })
        }
        InstructionKind::MutexLock { mutex } => {
            if !description(value_type(f, *mutex)?) {
                return Err(failure(
                    "Mutex lock requires its exact actual weak manager descriptor",
                ));
            }
            Some(guard_ty())
        }
        InstructionKind::MutexValue { guard: head } => {
            if !matches!(value_type(f,*head)?,Type::Address{mutable:true,inner} if guard(inner,b)?)
            {
                return Err(failure(
                    "Mutex data projection requires an exact mutable live guard head",
                ));
            }
            Some(Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            })
        }
        InstructionKind::MutexUnlock { guard: head } => {
            if !guard(value_type(f, *head)?, b)? {
                return Err(failure(
                    "Mutex unlock requires its complete actual affine guard",
                ));
            }
            Some(Type::Unit)
        }
        InstructionKind::DropMutexGuard { local } => {
            let ty = &f
                .locals
                .get(local.0 as usize)
                .filter(|row| row.id == *local)
                .ok_or_else(|| failure("Mutex guard cleanup local is absent"))?
                .ty;
            if !guard(ty, b)? {
                return Err(failure("Mutex cleanup selects a different actual local"));
            }
            None
        }
        InstructionKind::DescribeLocal { local }
            if f.locals
                .get(local.0 as usize)
                .is_some_and(|row| owner(&row.ty)) =>
        {
            Some(Type::Desc(Box::new(Type::Mutex {
                payload: Box::new(Type::I32),
            })))
        }
        InstructionKind::DescribeCapture { index }
            if f.environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|row| row.captures.get(*index as usize))
                .is_some_and(owner) =>
        {
            Some(Type::Desc(Box::new(Type::Mutex {
                payload: Box::new(Type::I32),
            })))
        }
        InstructionKind::DescribeAddress { address } if matches!(value_type(f,*address)?,Type::Address{inner,..} if owner(inner)) => {
            Some(Type::Desc(Box::new(Type::Mutex {
                payload: Box::new(Type::I32),
            })))
        }
        InstructionKind::Load(local)
            if f.locals
                .get(local.0 as usize)
                .is_some_and(|row| matches!(row.ty, Type::MutexGuard { .. })) =>
        {
            return Err(failure(
                "Mutex affine guard cannot be copied by an ordinary local read",
            ));
        }
        _ => return Ok(false),
    };
    resource(module, b)?;
    if matches!(
        i.kind,
        InstructionKind::MutexCreate { .. }
            | InstructionKind::MutexLock { .. }
            | InstructionKind::MutexValue { .. }
            | InstructionKind::MutexUnlock { .. }
            | InstructionKind::DropMutexGuard { .. }
    ) && f.call_kind != CallKind::Direct
    {
        return Err(failure(
            "actual Mutex lock, payload and cleanup cannot enter a suspended frame",
        ));
    }
    if output != expected.as_ref() {
        return Err(failure(
            "Mutex actual opcode changes its complete result carrier",
        ));
    }
    Ok(true)
}
fn target(base: HoldRoot) -> Target {
    Target {
        retained_port: None,
        base,
        stored: vec![],
        place: vec![],
        heap_parents: vec![],
        heap_element: None,
    }
}
fn payload(manager: &Permission, b: &mut Budget) -> CompileResult<Permission> {
    let mut result = Permission::default();
    for actual in &manager.targets {
        fee(b, target_width(actual).saturating_add(2))?;
        let mut child = actual.clone();
        match &child.retained_port {
            None => {
                child.retained_port = Some(PortOrigin::MutexPayload {
                    carrier: Vec::new(),
                })
            }
            Some(PortOrigin::MutexPayload { carrier }) if carrier.is_empty() => {}
            _ => {
                return Err(failure(
                    "Mutex payload source crosses an unproved opaque carrier namespace",
                ));
            }
        }
        ordered(b, result.targets.len(), target_width(&child))?;
        result.targets.insert(child);
    }
    fee(b, manager.loans.len())?;
    result.loans.extend(&manager.loans);
    Ok(result)
}
fn complete(permission: &Permission, text: &str, b: &mut Budget) -> CompileResult<()> {
    fee(b, permission.targets.len().saturating_add(1))?;
    if permission.targets.is_empty() {
        return Err(failure(text));
    }
    Ok(())
}
fn allocation_value(value: &Value, b: &mut Budget) -> CompileResult<()> {
    // The exact I32 allocation carrier has no embedded address, opaque class
    // or environment. This checks the actual transfer fact as well as its
    // type; unknown/nonempty rows cannot disappear under an error-tag test.
    fee(
        b,
        value
            .slots
            .len()
            .saturating_add(value.environment.len())
            .saturating_add(value.task_ports.len())
            .saturating_add(value.dyn_ports.len())
            .saturating_add(1),
    )?;
    for permission in [&value.pointer, &value.held, &value.store_description] {
        target_work(&permission.targets, b)?;
        fee(b, permission.loans.len())?;
        if !permission.targets.is_empty() || !permission.loans.is_empty() {
            return Err(failure(
                "Mutex allocation carrier unexpectedly retains actual stored permissions",
            ));
        }
    }
    if !value.slots.is_empty()
        || !value.environment.is_empty()
        || !value.task_ports.is_empty()
        || !value.dyn_ports.is_empty()
    {
        return Err(failure(
            "Mutex allocation carrier has an unproved intrinsic source directory",
        ));
    }
    Ok(())
}
pub(super) fn seed(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Value,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    if description(ty) {
        resource(module, b)?;
        fee(b, 1)?;
        out.store_description.targets.insert(target(base));
    } else if guard(ty, b)? || guard_head(ty, b)? {
        resource(module, b)?;
        let mut source = Permission::default();
        fee(b, 1)?;
        source.targets.insert(target(base));
        let protected = payload(&source, b)?;
        let id = loans.len();
        target_work(&protected.targets, b)?;
        fee(b, 1)?;
        loans.push(Loan {
            targets: protected.targets.clone(),
            parents: Loans::new(),
            mutable: false,
            receiver_head: false,
        });
        union(&mut out.held, &protected, b)?;
        fee(b, 1)?;
        out.held.loans.insert(id);
    }
    Ok(())
}
/// This query substitutes an already proved callee payload source. A raw weak
/// descriptor cannot manufacture the guard/token required by a returned view.
pub(super) fn select(
    value: &Value,
    carrier: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    fee(b, carrier.len().saturating_add(1))?;
    if !carrier.is_empty() {
        return Err(failure(
            "Mutex payload summary needs its complete nested carrier transport",
        ));
    }
    complete(
        &value.held,
        "Mutex payload summary has no actual guard-retained source",
        b,
    )?;
    let mut out = Permission::default();
    for source in &value.held.targets {
        fee(b, target_width(source).saturating_add(1))?;
        if !matches!(&source.retained_port,Some(PortOrigin::MutexPayload{carrier}) if carrier.is_empty())
        {
            return Err(failure(
                "Mutex payload summary substitutes another actual dependency namespace",
            ));
        }
    }
    union(&mut out, &value.held, b)?;
    fee(b, value.pointer.loans.len())?;
    out.loans.extend(&value.pointer.loans);
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("Mutex actual SSA origin is absent"))
    };
    let mut out = Value::default();
    match &i.kind {
        InstructionKind::ErrorTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. }
            if create_union(value_type(f, *value)?) =>
        {
            allocation_value(get(*value)?, b)?;
        }
        InstructionKind::MakeErrorUnion { value, .. }
            if i.result
                .map(|id| value_type(f, id))
                .transpose()?
                .is_some_and(create_union) =>
        {
            allocation_value(get(*value)?, b)?;
        }
        InstructionKind::PlaceTagLocal { local, .. } if create_union(union_local(f, *local)?) => {
            if let Some(value) = state.locals.get(local) {
                allocation_value(value, b)?;
            }
        }
        InstructionKind::TakeVariantField { local, .. }
        | InstructionKind::DropErrorUnion { local }
            if create_union(union_local(f, *local)?) =>
        {
            if let Some(value) = state.locals.remove(local) {
                allocation_value(&value, b)?;
            }
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::BorrowPlaceLocal { local, path, .. }
            if create_union(union_local(f, *local)?) =>
        {
            if let Some(value) = state.locals.get(local) {
                allocation_value(value, b)?;
            }
            fee(b, path.len().saturating_add(1))?;
            let mut root = target(local_capture_base(state, *local, b)?);
            root.place = path.clone();
            out.pointer.targets.insert(root);
        }
        InstructionKind::WithScopedThread { .. } if thread_scope(module, f, i, b)? => {
            // Joining the actual worker is proved by the synchronization
            // kernel; this scalar result exports neither a guard nor data.
        }
        InstructionKind::MutexCreate { .. } => {}
        InstructionKind::MutexLock { mutex } => {
            let source = &get(*mutex)?.store_description;
            complete(
                source,
                "Mutex lock has no actual descriptor manager origin",
                b,
            )?;
            for manager in &source.targets {
                fee(b, target_width(manager).saturating_add(1))?;
                if manager.retained_port.is_some() || manager.base == HoldRoot::Static {
                    return Err(failure(
                        "Mutex lock manager has no ordinary actual live resource route",
                    ));
                }
            }
            out.held = payload(source, b)?;
            union(&mut out.store_description, source, b)?;
            let id = i
                .result
                .ok_or_else(|| failure("Mutex lock omits its affine guard SSA"))?
                .0 as usize;
            let loan = loans.get_mut(id).ok_or_else(|| {
                failure("Mutex guard manager pin is outside the actual loan arena")
            })?;
            let mut current = Permission {
                targets: std::mem::take(&mut loan.targets),
                loans: std::mem::take(&mut loan.parents),
            };
            union(&mut current, source, b)?;
            loan.targets = current.targets;
            loan.parents = current.loans;
            loan.mutable = false;
            fee(b, 1)?;
            out.held.loans.insert(id);
        }
        InstructionKind::MutexValue { guard: head } => {
            let source = get(*head)?;
            if !source.pointer.targets.is_empty() && !source.held.targets.is_empty() {
                out.pointer = select(source, &[], b)?;
            }
            // The fresh payload loan is published at this opcode's result ID
            // by the common transfer tail, preserving these complete parents.
        }
        InstructionKind::MutexUnlock { guard: head } => {
            let _ = get(*head)?; // Final access requires the grounded manager.
        }
        InstructionKind::DropMutexGuard { local } => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
        }
        InstructionKind::DescribeLocal { local }
            if f.locals
                .get(local.0 as usize)
                .is_some_and(|row| owner(&row.ty)) =>
        {
            fee(b, 1)?;
            out.store_description
                .targets
                .insert(target(local_capture_base(state, *local, b)?));
        }
        InstructionKind::DescribeCapture { index }
            if f.environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|definition| definition.captures.get(*index as usize))
                .is_some_and(owner) =>
        {
            fee(b, 1)?;
            out.store_description
                .targets
                .insert(target(HoldRoot::Capture(*index)));
        }
        InstructionKind::DescribeAddress { address } if matches!(value_type(f,*address)?,Type::Address{inner,..} if owner(inner)) =>
        {
            let original = get(*address)?;
            complete(
                &original.pointer,
                "Mutex description has no actual live manager head",
                b,
            )?;
            union(&mut out.store_description, &original.pointer, b)?;
        }
        _ => return Ok(None),
    }
    if !instruction(module, f, i, b)? {
        return Err(failure(
            "Mutex transport lacks its exact actual opcode and result proof",
        ));
    }
    Ok(Some(out))
}
/// Ordinary moves/stores use the common whole Value transport. These are only
/// the operations whose public type omits the guard's retained data directory.
pub(super) fn propagate(
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    match &i.kind {
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local }
            if guard(&f.locals[local.0 as usize].ty, b)? =>
        {
            ordered(b, state.locals.len(), 0)?;
            let original = state
                .locals
                .get(local)
                .ok_or_else(|| failure("borrowed Mutex guard is unavailable"))?;
            union(&mut out.held, &original.held, b)?;
            union(&mut out.store_description, &original.store_description, b)?;
        }
        InstructionKind::ReborrowAddress { address, .. }
            if guard_head(value_type(f, *address)?, b)? =>
        {
            let original = values
                .get(address.0 as usize)
                .ok_or_else(|| failure("Mutex guard reborrow source is absent"))?;
            union(&mut out.held, &original.held, b)?;
            union(&mut out.store_description, &original.store_description, b)?;
        }
        _ => {}
    }
    Ok(())
}
/// These checks run against the instruction-before active set. Constructing a
/// payload window checks both guard storage and the protected data, so a live
/// sibling projection cannot be hidden behind the same manager descriptor.
pub(super) fn accesses(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Vec<(Permission, bool)>> {
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("Mutex access SSA is absent"))
    };
    let mut out = Vec::new();
    match &i.kind {
        InstructionKind::BorrowPlaceLocal {
            local,
            path,
            mutable,
        } if create_union(union_local(f, *local)?) => {
            fee(b, path.len().saturating_add(1))?;
            let mut root = target(local_capture_base(state, *local, b)?);
            root.place = path.clone();
            out.push((
                Permission {
                    targets: Targets::from([root]),
                    loans: Loans::new(),
                },
                *mutable,
            ));
        }
        InstructionKind::TakeVariantField { local, .. }
        | InstructionKind::DropErrorUnion { local }
            if create_union(union_local(f, *local)?) =>
        {
            fee(b, 1)?;
            out.push((
                Permission {
                    targets: Targets::from([target(local_capture_base(state, *local, b)?)]),
                    loans: Loans::new(),
                },
                true,
            ));
        }
        InstructionKind::WithScopedThread { argument, .. } if thread_scope(module, f, i, b)? => {
            let manager = &get(*argument)?.store_description;
            complete(
                manager,
                "Mutex thread scope has no actual live manager descriptor source",
                b,
            )?;
            out.push((copy_permission(manager, b)?, false));
        }
        InstructionKind::MutexLock { mutex } => {
            let manager = &get(*mutex)?.store_description;
            complete(manager, "Mutex lock access has no actual manager origin", b)?;
            out.push((copy_permission(manager, b)?, false));
        }
        InstructionKind::MutexValue { guard: head } => {
            let original = get(*head)?;
            complete(
                &original.pointer,
                "Mutex payload access has no actual mutable guard head",
                b,
            )?;
            complete(
                &original.held,
                "Mutex payload access has no actual locked payload origin",
                b,
            )?;
            out.push((copy_permission(&original.pointer, b)?, true));
            out.push((copy_permission(&original.held, b)?, true));
        }
        InstructionKind::MutexUnlock { guard: head } => {
            let original = get(*head)?;
            complete(
                &original.held,
                "Mutex unlock has no actual locked payload origin",
                b,
            )?;
            out.push((copy_permission(&original.held, b)?, true));
        }
        InstructionKind::DropMutexGuard { local } => {
            let mut head = Permission::default();
            fee(b, 1)?;
            head.targets
                .insert(target(local_capture_base(state, *local, b)?));
            out.push((head, true));
            // Guarded cleanup after a real move/unlock is inactive. The
            // resource ledger proves that conditional effect; this pass still
            // checks live children of the old physical guard storage.
            if let Some(original) = state.locals.get(local) {
                complete(
                    &original.held,
                    "Mutex cleanup has no complete retained manager source",
                    b,
                )?;
                out.push((copy_permission(&original.held, b)?, true));
            }
        }
        _ => return Ok(out),
    }
    if !instruction(module, f, i, b)? {
        return Err(failure(
            "Mutex access lacks complete actual opcode validation",
        ));
    }
    fee(b, out.len())?;
    Ok(out)
}
fn guard_parameter(f: &TypedFunction, index: u32, b: &mut Budget) -> CompileResult<()> {
    let local = f
        .parameters
        .get(index as usize)
        .ok_or_else(|| failure("Mutex payload source parameter is absent"))?;
    let declaration = f
        .locals
        .get(local.0 as usize)
        .filter(|row| row.id == *local)
        .ok_or_else(|| failure("Mutex payload source local is absent"))?;
    if !matches!(&declaration.ty,Type::Address{mutable:true,inner} if guard(inner,b)?) {
        return Err(failure(
            "returned Mutex payload must retain the caller's actual mutable guard head",
        ));
    }
    Ok(())
}
pub(super) fn source_binding(
    f: &TypedFunction,
    source: &Target,
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let Some(PortOrigin::MutexPayload { carrier }) = &source.retained_port else {
        return Err(failure(
            "Mutex source binding selects another origin namespace",
        ));
    };
    fee(b, target_width(source).saturating_add(1))?;
    if !carrier.is_empty()
        || !source.stored.is_empty()
        || !source.place.is_empty()
        || !source.heap_parents.is_empty()
        || source.heap_element.is_some()
    {
        return Err(failure(
            "Mutex source binding requires its complete unprojected guard payload",
        ));
    }
    let HoldRoot::Parameter(index) = source.base else {
        return Err(failure("Mutex source binding is not an actual guard input"));
    };
    guard_parameter(f, index, b)?;
    let life = f.storage_life.as_ref().ok_or_else(|| {
        failure("Mutex payload result has no complete actual header class relation")
    })?;
    crate::signature_life::charge(life, b)?;
    crate::signature_life::outer(life, StorageBase::Parameter(index))
        .ok_or_else(|| failure("Mutex payload result omits its actual guard-head occurrence"))
}
fn guard_carrier(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(failure(
            "Mutex result carrier exceeds its complete finite type depth",
        ));
    }
    match ty {
        Type::MutexGuard { .. } => guard(ty, b),
        Type::Address { .. } | Type::Desc(_) => Ok(false),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, child) in fields {
                if guard_carrier(child, b, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Tuple(fields) => {
            for child in fields {
                if guard_carrier(child, b, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for child in fields {
                    if guard_carrier(child, b, depth + 1)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            Ok(guard_carrier(error, b, depth + 1)? || guard_carrier(result, b, depth + 1)?)
        }
        Type::Array { element, .. } | Type::Managed(element) => {
            guard_carrier(element, b, depth + 1)
        }
        _ => Ok(false),
    }
}
/// Returning a borrowed payload must preserve every actual guard-head parent.
/// A manager descriptor parameter alone cannot keep a local lock activation
/// alive after Return. Owning guard return requires a separate complete
/// retained-owner summary and remains explicitly unavailable here.
pub(super) fn returning(
    f: &TypedFunction,
    value: &Value,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    if guard(&f.return_type, b)? {
        return guard_return(f, value, loans, b);
    }
    if guard_carrier(&f.return_type, b, 0)? {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "returning an affine Mutex guard requires its complete manager-retained owner summary",
            None,
        )]);
    }
    let mut permissions = vec![&value.pointer, &value.held];
    fee(
        b,
        value
            .slots
            .len()
            .saturating_add(value.environment.len())
            .saturating_add(value.task_ports.len())
            .saturating_add(value.dyn_ports.len())
            .saturating_add(2),
    )?;
    permissions.extend(value.slots.values());
    permissions.extend(value.environment.values());
    permissions.extend(value.task_ports.values());
    permissions.extend(value.dyn_ports.values());
    for permission in permissions {
        let mut protected = false;
        for source in &permission.targets {
            fee(b, target_width(source).saturating_add(1))?;
            if matches!(source.retained_port, Some(PortOrigin::MutexPayload { .. })) {
                protected = true;
                source_binding(f, source, b)?;
            }
        }
        if !protected {
            continue;
        }
        if permission.loans.is_empty() {
            return Err(failure(
                "Mutex payload return has no actual guard-head parent loan",
            ));
        }
        let mut guarded = false;
        for id in ancestry(&permission.loans, loans, b)? {
            let parent = loans
                .get(id)
                .ok_or_else(|| failure("Mutex result parent is outside actual loan arena"))?;
            for source in &parent.targets {
                fee(b, target_width(source).saturating_add(1))?;
                if !matches!(source.base, HoldRoot::Parameter(_)) {
                    return Err(failure(
                        "Mutex payload return retains a local lock or captured guard activation",
                    ));
                }
                if source.retained_port.is_none() {
                    let HoldRoot::Parameter(index) = source.base else {
                        unreachable!()
                    };
                    guard_parameter(f, index, b)?;
                    guarded = true;
                }
            }
        }
        if !guarded {
            return Err(failure(
                "Mutex payload return omits its actual caller guard storage head",
            ));
        }
    }
    Ok(())
}

fn input_type(f: &TypedFunction, index: u32) -> CompileResult<&Type> {
    let local = f
        .parameters
        .get(index as usize)
        .ok_or_else(|| failure("returned guard manager input is absent"))?;
    union_local(f, *local)
}
fn manager_binding(
    f: &TypedFunction,
    base: StorageBase,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let life = f.storage_life.as_ref().ok_or_else(|| {
        failure("returned guard manager omits its complete actual class signature")
    })?;
    crate::signature_life::charge(life, b)?;
    if description(ty) {
        return crate::signature_life::outer(life, base).ok_or_else(|| {
            failure("returned guard descriptor omits its real outer class occurrence")
        });
    }
    if !guard(ty, b)? {
        return Err(failure(
            "returned guard source is not an affine guard or a manager descriptor",
        ));
    }
    let mut selected = None;
    for row in &life.classes {
        fee(b, row.position.path.len().saturating_add(1))?;
        if row.position.base == base && row.position.path.is_empty() && row.position.class == 0 {
            if selected.replace(row.binder).is_some() {
                return Err(failure(
                    "returned guard manager repeats its intrinsic class occurrence",
                ));
            }
        }
    }
    selected
        .ok_or_else(|| failure("returned guard manager omits its actual retained class occurrence"))
}
fn guard_source(f: &TypedFunction, source: &Target, b: &mut Budget) -> CompileResult<u32> {
    fee(b, target_width(source).saturating_add(1))?;
    if !matches!(&source.retained_port,Some(PortOrigin::MutexPayload{carrier}) if carrier.is_empty())
        || !source.stored.is_empty()
        || !source.place.is_empty()
        || !source.heap_parents.is_empty()
        || source.heap_element.is_some()
    {
        return Err(failure(
            "returned guard manager needs its complete unprojected actual input route",
        ));
    }
    let HoldRoot::Parameter(index) = source.base else {
        return Err(failure(
            "returned guard would outlive an activation-local or captured manager",
        ));
    };
    let input = input_type(f, index)?;
    let result = manager_binding(f, StorageBase::Result, &f.return_type, b)?;
    if manager_binding(f, StorageBase::Parameter(index), input, b)? != result {
        return Err(failure(
            "returned guard manager differs from its complete original input/result class relation",
        ));
    }
    Ok(index)
}
fn guard_return(
    f: &TypedFunction,
    value: &Value,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    if !value.pointer.targets.is_empty()
        || !value.pointer.loans.is_empty()
        || !value.slots.is_empty()
        || !value.environment.is_empty()
        || !value.task_ports.is_empty()
        || !value.dyn_ports.is_empty()
    {
        return Err(failure(
            "affine guard result carries an unrelated pointer or opaque source directory",
        ));
    }
    // An empty intermediate fixed-point proposal supplies no root or token.
    // Final returns/grounding below require every actual return to be rooted.
    if value.held.targets.is_empty() {
        return Ok(());
    }
    if value.held.loans.is_empty() {
        return Err(failure(
            "returned guard omits its actual manager-retaining parent loan",
        ));
    }
    for source in &value.held.targets {
        guard_source(f, source, b)?;
    }
    for id in ancestry(&value.held.loans, loans, b)? {
        let parent = loans
            .get(id)
            .ok_or_else(|| failure("returned guard parent is outside the actual loan arena"))?;
        for source in &parent.targets {
            fee(b, target_width(source).saturating_add(1))?;
            let HoldRoot::Parameter(index) = source.base else {
                return Err(failure(
                    "returned guard retains activation-local guard storage or a local manager",
                ));
            };
            let input = input_type(f, index)?;
            if !description(input) && !guard(input, b)? {
                return Err(failure(
                    "returned guard retains a borrowed guard head instead of moving its unlock obligation",
                ));
            }
            if !source.stored.is_empty()
                || !source.place.is_empty()
                || !source.heap_parents.is_empty()
                || source.heap_element.is_some()
            {
                return Err(failure(
                    "returned guard parent needs its complete projected manager lifetime transport",
                ));
            }
            if let Some(origin) = &source.retained_port {
                if !matches!(origin,PortOrigin::MutexPayload{carrier} if carrier.is_empty()) {
                    return Err(failure(
                        "returned guard parent changes its manager dependency namespace",
                    ));
                }
            }
        }
    }
    Ok(())
}
pub(super) fn summarize(
    f: &TypedFunction,
    value: &Value,
    loans: &[Loan],
    out: &mut Returns,
    b: &mut Budget,
) -> CompileResult<()> {
    if !guard(&f.return_type, b)? {
        return Ok(());
    }
    guard_return(f, value, loans, b)?;
    // Every reachable return is a separate obligation. A rooted branch
    // cannot cover an empty proposal on another branch.
    out.missing |= value.held.targets.is_empty();
    for source in &value.held.targets {
        guard_source(f, source, b)?;
        fee(
            b,
            out.sources
                .len()
                .saturating_mul(target_width(source).saturating_add(1)),
        )?;
        if !out.sources.contains(source) {
            fee(b, target_width(source).saturating_add(1))?;
            out.sources.push(source.clone());
        }
    }
    Ok(())
}
pub(super) fn returns(f: &TypedFunction, summary: &Returns, b: &mut Budget) -> CompileResult<()> {
    work(summary, b)?;
    if guard(&f.return_type, b)? {
        if summary.sources.is_empty() || summary.missing {
            return Err(failure(
                "affine guard return has no grounded actual manager source; recursive cycles grant no unlock obligation",
            ));
        }
        for source in &summary.sources {
            guard_source(f, source, b)?;
        }
    } else if !summary.sources.is_empty() {
        return Err(failure(
            "nonguard result invents an affine guard source summary",
        ));
    }
    Ok(())
}
pub(super) fn grounded(
    f: &TypedFunction,
    summary: &Returns,
    summaries: &[Facts],
    b: &mut Budget,
) -> CompileResult<bool> {
    work(summary, b)?;
    if guard(&f.return_type, b)? && (summary.sources.is_empty() || summary.missing) {
        return Ok(false);
    }
    for block in &f.blocks {
        fee(b, block.instructions.len().saturating_add(1))?;
        for instruction in &block.instructions {
            if let InstructionKind::Call { function, .. } = instruction.kind
                && instruction
                    .result
                    .and_then(|id| f.values.get(id.0 as usize))
                    .is_some_and(|ty| matches!(ty, Type::MutexGuard { .. }))
            {
                let callee = summaries.get(function.0 as usize).ok_or_else(|| {
                    failure("guard result call has no complete actual callee summary")
                })?;
                work(&callee.mutex_returns, b)?;
                if !callee.complete
                    || callee.mutex_returns.sources.is_empty()
                    || callee.mutex_returns.missing
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
fn manager_from_guard(value: &Value, b: &mut Budget) -> CompileResult<Permission> {
    let mut result = Permission::default();
    for source in &value.held.targets {
        fee(b, target_width(source).saturating_add(1))?;
        if !matches!(&source.retained_port,Some(PortOrigin::MutexPayload{carrier}) if carrier.is_empty())
        {
            return Err(failure("moved guard changes its actual manager namespace"));
        }
        let mut manager = source.clone();
        manager.retained_port = None;
        ordered(b, result.targets.len(), target_width(&manager))?;
        result.targets.insert(manager);
    }
    fee(b, value.held.loans.len())?;
    result.loans.extend(&value.held.loans);
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    i: &Instruction,
    callee: &TypedFunction,
    summary: &Returns,
    arguments: &[ValueId],
    values: &[Value],
    _state: &State,
    out: &mut Value,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    if !guard(&callee.return_type, b)? {
        return Ok(());
    }
    work(summary, b)?;
    let mut manager = Permission::default();
    for source in &summary.sources {
        let index = guard_source(callee, source, b)?;
        let id = arguments
            .get(index as usize)
            .ok_or_else(|| failure("guard call manager input is absent"))?;
        let actual = values
            .get(id.0 as usize)
            .ok_or_else(|| failure("guard call actual manager SSA is absent"))?;
        if description(input_type(callee, index)?) {
            union(&mut manager, &actual.store_description, b)?;
        } else {
            union(&mut manager, &manager_from_guard(actual, b)?, b)?;
        }
    }
    if manager.targets.is_empty() {
        return Ok(());
    }
    let id = i
        .result
        .ok_or_else(|| failure("guard call omits its actual affine result SSA"))?
        .0 as usize;
    let pin = loans
        .get_mut(id)
        .ok_or_else(|| failure("guard call manager pin is outside actual SSA loan arena"))?;
    let mut retained = Permission {
        targets: std::mem::take(&mut pin.targets),
        loans: std::mem::take(&mut pin.parents),
    };
    union(&mut retained, &manager, b)?;
    pin.targets = retained.targets;
    pin.parents = retained.loans;
    pin.mutable = false;
    let protected = payload(&manager, b)?;
    union(&mut out.held, &protected, b)?;
    fee(b, 1)?;
    out.held.loans.insert(id);
    union(&mut out.store_description, &manager, b)?;
    Ok(())
}
/// The ordinary resource kernel consumes this exact selector projection of the
/// completed source proof. It is not an empty fallback for unknown returns.
pub(super) fn publish(module: &TypedModule, facts: &[Facts], b: &mut Budget) -> CompileResult<()> {
    if !b.generic_mutex {
        return Ok(());
    }
    if facts.len() != module.functions.len() {
        return Err(failure(
            "Mutex return publication omits actual function summaries",
        ));
    }
    fee(b, module.functions.len())?;
    let mut rows = Vec::new();
    for (function, fact) in module.functions.iter().zip(facts) {
        let mut selectors = BTreeSet::new();
        if guard(&function.return_type, b)? {
            if !fact.complete {
                return Err(failure(
                    "Mutex guard source publication precedes actual fixed-point completion",
                ));
            }
            returns(function, &fact.mutex_returns, b)?;
            for source in &fact.mutex_returns.sources {
                let index = guard_source(function, source, b)?;
                ordered(b, selectors.len(), 0)?;
                selectors.insert(index);
            }
        }
        fee(b, 1)?;
        rows.push(selectors);
    }
    b.guard_return_sources = rows;
    Ok(())
}
