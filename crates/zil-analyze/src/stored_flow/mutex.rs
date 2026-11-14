//! Mutex manager reach, guard storage and protected payloads are three distinct
//! origins. Only an actual lock followed by a mutable live guard projection can
//! create a payload window; descriptors never contain that permission.
use super::*;
use zil_ir::ContractProjection;

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
        || matches!(ty,Type::ErrorUnion{error,result} if error.as_ref()==&crate::generic_mutex::error_type() && owner(result));
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
            complete(
                &source.pointer,
                "Mutex payload has no actual mutable guard storage head",
                b,
            )?;
            complete(
                &source.held,
                "Mutex payload has no complete actual locked manager origin",
                b,
            )?;
            out.pointer = select(source, &[], b)?;
            // The fresh payload loan is published at this opcode's result ID
            // by the common transfer tail, preserving these complete parents.
        }
        InstructionKind::MutexUnlock { guard: head } => {
            complete(
                &get(*head)?.held,
                "Mutex unlock has no complete actual retained manager",
                b,
            )?;
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
            complete(
                &original.held,
                "borrowed Mutex guard omits its actual retained manager",
                b,
            )?;
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
