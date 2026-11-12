//! Independently authenticate concrete provider positions against the complete
//! Context envelope. Public mappings select targets; they never create them.
use super::*;
use std::collections::BTreeSet;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-PORT", None, message)
}
pub(super) fn interface<'a>(
    module: &'a TypedModule,
    id: DynInterfaceId,
    b: &mut Budget,
) -> Check<&'a DynInterface> {
    b.spend(module.dyn_interfaces.len().saturating_add(1))?;
    module
        .dyn_interfaces
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| bad("port carrier interface is absent"))
}
pub(super) fn physical_field(ty: &Type, p: PlaceProjection) -> Check<&Type> {
    let ty = if let Type::BorrowedStorage { storage, .. } = ty {
        storage.as_ref()
    } else {
        ty
    };
    match (ty, p) {
        (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(i as usize),
        (Type::Record { fields, .. } | Type::Package { fields, .. }, PlaceProjection::Field(i)) => {
            fields.get(i as usize).map(|(_, ty)| ty)
        }
        (Type::Array { element, length }, PlaceProjection::Field(i)) if i < *length => {
            Some(element.as_ref())
        }
        (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
            .get(p.variant as usize)
            .and_then(|(_, fields)| fields.get(p.index as usize)),
        (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
            Some(if e { error.as_ref() } else { result.as_ref() })
        }
        _ => None,
    }
    .ok_or_else(|| bad("provider position crosses a pointer or has an absent physical field"))
}
pub(super) fn physical_path<'a>(
    ty: &'a Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    b.spend(path.len())?;
    let mut node = ty;
    for p in path {
        node = physical_field(node, *p)?;
    }
    Ok(node)
}
pub(super) fn source_places(
    source: &StoragePosition,
    b: &mut Budget,
) -> Check<Vec<PlaceProjection>> {
    if source.base != StorageBase::Result {
        return Err(bad(
            "provider port must select its real Self result storage",
        ));
    }
    b.spend(source.path.len().saturating_add(1))?;
    source.path.iter().map(|step| match step {
        ContractProjection::Place(p) => Ok(*p),
        _ => Err(unsupported("provider capture/opaque source needs a complete actual retained class and target route", None)),
    }).collect()
}
pub(super) fn validate_table(module: &TypedModule, table: &DynVtable, b: &mut Budget) -> Check<()> {
    let iface = interface(module, table.interface, b)?;
    if iface.context.is_none() {
        if !table.ports.is_empty() || !table.self_classes.is_empty() {
            return Err(bad(
                "closed dyn provider advertises an unknown external class or port",
            ));
        }
        return Ok(());
    }
    let life = if let Type::BorrowedStorage { life, .. } = &table.self_type {
        let limits = b.limits.clone();
        stored_flow::qualification(module, &table.self_type, &limits, b)?;
        if !life.retained.is_empty() {
            return Err(unsupported(
                "provider nested retained graph needs complete actual leaf-to-port reconstruction",
                None,
            ));
        }
        Some(life)
    } else {
        if !closed_targets(module, &table.self_type, b)? {
            return Err(unsupported(
                "Context provider has unknown unqualified Self dependencies",
                None,
            ));
        }
        None
    };
    let slots = life.map_or(&[][..], |life| life.slots.as_slice());
    let count = life.map_or(0, |life| life.binder_count as usize);
    b.spend(
        table
            .self_classes
            .len()
            .saturating_add(table.ports.len())
            .saturating_add(slots.len()),
    )?;
    if table.self_classes.len() != count || table.ports.len() != iface.retained_life.ports.len() {
        return Err(bad("provider class map or port directory is incomplete"));
    }
    for class in &table.self_classes {
        if !matches!(class, StorageBinder::Fixed(i) if *i < iface.retained_life.binder_count) {
            return Err(bad(
                "provider class map must preserve a complete Context Fixed domain",
            ));
        }
    }
    let mut covered = BTreeSet::new();
    for (ordinal, row) in table.ports.iter().enumerate() {
        if row.port as usize != ordinal {
            return Err(bad(
                "provider ports are missing, duplicated or noncanonical",
            ));
        }
        let port = &iface.retained_life.ports[ordinal];
        for source in &row.sources {
            let path = source_places(source, b)?;
            b.spend(slots.len().saturating_mul(path.len().saturating_add(1)))?;
            let actual = slots
                .iter()
                .find(|slot| slot.path == path)
                .ok_or_else(|| bad("provider source is not an actual external Self leaf"))?;
            let ty = physical_path(&table.self_type, &path, b)?;
            dynamic_metadata::ty(ty, b)?;
            dynamic_metadata::ty(&port.leaf, b)?;
            if *ty != port.leaf
                || actual.mode != port.mode
                || table.self_classes.get(actual.binder as usize)
                    != Some(&StorageBinder::Fixed(port.binder))
            {
                return Err(bad(
                    "provider source full type, mode or invariant class differs from its Context port",
                ));
            }
            b.spend(
                covered
                    .len()
                    .saturating_add(1)
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            if !covered.insert(path) {
                return Err(bad("one real provider leaf is mapped more than once"));
            }
        }
    }
    if covered.len() != slots.len() {
        return Err(bad("provider map omits an actual retained leaf"));
    }
    Ok(())
}

pub(super) fn validate_cleanup(iface: &DynInterface, b: &mut Budget) -> Check<()> {
    let c = iface
        .owning_cleanup
        .as_ref()
        .ok_or_else(|| bad("owning Context cleanup is Unknown"))?;
    if !c.environment || !c.positions.is_empty() {
        return Err(bad(
            "Context cleanup has no complete owning environment route",
        ));
    }
    b.spend(
        c.access
            .reads
            .len()
            .saturating_add(c.access.writes.len())
            .saturating_add(c.effects.len()),
    )?;
    for (writing, positions) in [(false, &c.access.reads), (true, &c.access.writes)] {
        let mut previous = None;
        for position in positions {
            b.spend(position.path.len().saturating_add(1))?;
            let [ContractProjection::RetainedPort(port)] = position.path.as_slice() else {
                return Err(bad(
                    "Context cleanup access is not an actual typed retained port",
                ));
            };
            let actual = iface
                .retained_life
                .ports
                .get(*port as usize)
                .ok_or_else(|| bad("Context cleanup port is absent"))?;
            if position.base != StorageBase::Result
                || previous.is_some_and(|p| p >= *port)
                || writing && actual.mode != StoredBorrowMode::ExclusiveAddress
            {
                return Err(bad(
                    "Context cleanup port order, base or write mode differs",
                ));
            }
            previous = Some(*port);
        }
    }
    if c.effects.iter().any(|effect| {
        !matches!(
            effect,
            DynEffect::Alloc
                | DynEffect::Io
                | DynEffect::Block
                | DynEffect::Ffi
                | DynEffect::Reenter
                | DynEffect::Panic
        )
    }) {
        return Err(bad("Context cleanup has an unknown effect upper bound"));
    }
    Ok(())
}

pub(super) fn closed_targets(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![(ty, 0usize)];
    let mut closures = BTreeSet::new();
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("closed provider graph exceeds selected depth"));
        }
        match ty {
            Type::Address { .. } | Type::Desc(_) => return Ok(false),
            Type::BorrowedStorage { storage, life } => {
                if life.binder_count != 0
                    || !life.slots.is_empty()
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                {
                    return Ok(false);
                }
                b.spend(1)?;
                pending.push((storage, depth + 1));
            }
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. } => {
                b.spend(1)?;
                pending.push((inner, depth + 1));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| (t, depth + 1)));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|t| (t, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)]);
            }
            Type::Closure(id) => {
                b.spend(closures.len().saturating_add(1))?;
                if closures.insert(*id) {
                    let actual = crate::closures::definition(module, *id, None)?;
                    if actual
                        .storage_life
                        .as_ref()
                        .is_some_and(|life| life.fixed_binder_count != 0)
                    {
                        return Ok(false);
                    }
                    b.spend(actual.captures.len())?;
                    pending.extend(actual.captures.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::Dyn { interface: id } | Type::OwnedDyn { interface: id } => {
                let iface = interface(module, *id, b)?;
                if iface.retained_life.binder_count != 0 || !iface.retained_life.ports.is_empty() {
                    return Ok(false);
                }
            }
            Type::ByteBuffer => {}
            ty if ty.is_copy_value() => {}
            _ => return Ok(false),
        }
        if pending.len() > b.limits.max_nodes {
            return Err(limit("closed provider graph exceeds selected node count"));
        }
    }
    Ok(true)
}

fn actuals(
    kinds: &[LifeParameterKind],
    values: &[DynGenericActual],
    fixed: u32,
    invocation: u32,
    b: &mut Budget,
) -> Check<()> {
    b.spend(kinds.len().saturating_add(values.len()))?;
    if kinds.len() != values.len() {
        return Err(bad(
            "dyn generic actual order or cardinality differs from declaration",
        ));
    }
    for (kind, actual) in kinds.iter().zip(values) {
        match (kind, actual) {
            (LifeParameterKind::Type, DynGenericActual::Type(ty)) => dynamic_metadata::ty(ty, b)?,
            (LifeParameterKind::Const(ty), DynGenericActual::Const(value)) => {
                dynamic_metadata::ty(ty, b)?;
                if *ty != value.ty() {
                    return Err(bad(
                        "dyn constant actual full type differs from its ordered declaration",
                    ));
                }
            }
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Fixed(i)))
                if *i < fixed => {}
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Invocation(i)))
                if *i < invocation => {}
            _ => {
                return Err(bad(
                    "dyn actual kind or lexical class domain differs from declaration",
                ));
            }
        }
    }
    Ok(())
}
fn application<'a>(
    module: &'a TypedModule,
    app: &DynTraitApplication,
    fixed: u32,
    b: &mut Budget,
) -> Check<&'a DynDeclaration> {
    b.spend(module.dyn_declarations.len())?;
    let declaration = module
        .dyn_declarations
        .iter()
        .find(|d| d.id == app.declaration && d.kind == DynDeclarationKind::Trait)
        .ok_or_else(|| bad("dyn application lacks its exact actual trait declaration"))?;
    actuals(&declaration.parameters, &app.actuals, fixed, 0, b)?;
    Ok(declaration)
}
pub(super) fn validate_interface(
    module: &TypedModule,
    iface: &DynInterface,
    b: &mut Budget,
) -> Check<()> {
    if !iface.associated_types.is_empty() || !iface.associated_constants.is_empty() {
        return Err(unsupported(
            "associated dyn bindings lack a real implementation-member recipe witness",
            None,
        ));
    }
    if !iface.inherited.is_empty() {
        return Err(unsupported(
            "dyn inherited applications lack a real declaration inheritance witness",
            None,
        ));
    }
    let root = application(
        module,
        &iface.application,
        iface.retained_life.binder_count,
        b,
    )?;
    if iface.id.module != root.id.module {
        return Err(bad("Context interface has a mismatched defining module"));
    }
    let mut applications = vec![&iface.application];
    for inherited in &iface.inherited {
        application(module, inherited, iface.retained_life.binder_count, b)?;
        b.spend(applications.len())?;
        if applications.contains(&inherited) {
            return Err(bad("duplicate dyn trait application"));
        }
        applications.push(inherited);
    }
    let mut selected: Vec<(&DynMember, &[DynGenericActual])> = Vec::new();
    for slot in &iface.slots {
        b.spend(applications.len())?;
        if !applications.contains(&&slot.member.application) {
            return Err(bad(
                "Context slot is outside its full trait application directory",
            ));
        }
        let declaration = application(
            module,
            &slot.member.application,
            iface.retained_life.binder_count,
            b,
        )?;
        let member = declaration
            .members
            .get(slot.member.ordinal as usize)
            .ok_or_else(|| bad("Context slot member is absent"))?;
        let DynMemberKind::Method(kinds) = &member.kind else {
            return Err(bad("Context slot selects a non-method declaration member"));
        };
        let life = slot
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Context method lifetime relation is Unknown"))?;
        actuals(
            kinds,
            &slot.actuals,
            iface.retained_life.binder_count,
            life.invocation_binder_count,
            b,
        )?;
        b.spend(
            selected
                .len()
                .saturating_mul(slot.actuals.len().saturating_add(1)),
        )?;
        if selected
            .iter()
            .any(|(member, args)| **member == slot.member && *args == slot.actuals)
        {
            return Err(bad("duplicate full Context method specialization"));
        }
        selected.push((&slot.member, &slot.actuals));
        if slot.parameters.len() != slot.parameter_modes.len()
            || slot.receiver.mode
                != if slot.receiver.mutable {
                    PassingMode::BorrowMut
                } else {
                    PassingMode::Borrow
                }
            || slot.call_kind != CallKind::Direct
        {
            return Err(bad(
                "Context slot mode, actual argument count or call kind is inconsistent",
            ));
        }
        for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
            dynamic_metadata::ty(ty, b)?;
        }
        if life.fixed_binder_count != iface.retained_life.binder_count
            || life.invocation_binder_count == 0
            || crate::signature_life::outer(life, StorageBase::Parameter(0))
                != Some(StorageBinder::Invocation(0))
        {
            return Err(bad(
                "Context slot receiver and persistent class domains differ",
            ));
        }
        let mut parameters = vec![Type::Address {
            mutable: slot.receiver.mutable,
            inner: Box::new(Type::Dyn {
                interface: iface.id,
            }),
        }];
        b.spend(slot.parameters.len().saturating_add(1))?;
        parameters.extend(slot.parameters.iter().cloned());
        crate::signature_life::charge(life, b)?;
        crate::signature_life::function_relation(
            module,
            &parameters,
            &slot.result,
            life,
            None,
            false,
        )?;
        for class in 0..iface.retained_life.binder_count {
            b.spend(life.classes.len())?;
            if !life.classes.iter().any(|row| {
                row.position.base == StorageBase::Parameter(0)
                    && row.position.path == [LifeTypeProjection::Pointee]
                    && row.position.class == class
                    && row.binder == StorageBinder::Fixed(class)
            }) {
                return Err(bad(
                    "Context receiver class is not the complete invariant interface class",
                ));
            }
        }
        validate_contract(iface, slot, b)?;
    }
    validate_cleanup(iface, b)
}
fn position_leaf(
    iface: &DynInterface,
    slot: &DynSlot,
    position: &StoragePosition,
    b: &mut Budget,
) -> Check<(Type, StorageBinder, bool)> {
    b.spend(position.path.len().saturating_add(1))?;
    let life = slot
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("method source relation is Unknown"))?;
    match position.base {
        StorageBase::Parameter(0) => {
            if position.path.is_empty() {
                return Ok((
                    Type::Address {
                        mutable: slot.receiver.mutable,
                        inner: Box::new(Type::Dyn {
                            interface: iface.id,
                        }),
                    },
                    StorageBinder::Invocation(0),
                    true,
                ));
            }
            let [ContractProjection::RetainedPort(index)] = position.path.as_slice() else {
                return Err(bad(
                    "erased receiver position is not its head or typed port",
                ));
            };
            let port = iface
                .retained_life
                .ports
                .get(*index as usize)
                .ok_or_else(|| bad("erased receiver port is absent"))?;
            dynamic_metadata::ty(&port.leaf, b)?;
            Ok((port.leaf.clone(), StorageBinder::Fixed(port.binder), false))
        }
        StorageBase::Parameter(index) if position.path.is_empty() => {
            let ty = slot
                .parameters
                .get(index.saturating_sub(1) as usize)
                .ok_or_else(|| bad("method source actual parameter is absent"))?;
            if !matches!(ty, Type::Address { .. }) {
                return Err(bad(
                    "method target selector is not an actual borrowed parameter",
                ));
            }
            dynamic_metadata::ty(ty, b)?;
            let binder = crate::signature_life::outer(life, StorageBase::Parameter(index))
                .ok_or_else(|| bad("method parameter source class is Unknown"))?;
            Ok((ty.clone(), binder, false))
        }
        _ => Err(unsupported(
            "nested method source requires complete typed carrier integration",
            None,
        )),
    }
}
fn compatible(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (
            Type::Address {
                mutable: a,
                inner: x,
            },
            Type::Address {
                mutable: z,
                inner: y,
            },
        ) => (!*z || *a) && x == y,
        _ => actual == expected,
    }
}
fn validate_contract(iface: &DynInterface, slot: &DynSlot, b: &mut Budget) -> Check<()> {
    let c = &slot.contract;
    let requires = c
        .requires
        .as_ref()
        .ok_or_else(|| bad("method requirements are Unknown"))?;
    for requirement in requires {
        let (position, writing) = match requirement {
            DynRequirement::Read(p) => (p, false),
            DynRequirement::Write(p) => (p, true),
            _ => {
                return Err(bad(
                    "method requirement has no independent authority implementation",
                ));
            }
        };
        let (leaf, _, head) = position_leaf(iface, slot, position, b)?;
        if writing
            && !(matches!(leaf, Type::Address { mutable: true, .. })
                && (!head || slot.receiver.mutable))
        {
            return Err(bad("method requirement writes a shared actual target"));
        }
    }
    let call = c
        .call
        .as_ref()
        .ok_or_else(|| bad("method call access upper is Unknown"))?;
    for (writing, positions) in [(false, &call.reads), (true, &call.writes)] {
        for position in positions {
            let (leaf, _, head) = position_leaf(iface, slot, position, b)?;
            if writing
                && !(matches!(leaf, Type::Address { mutable: true, .. })
                    && (!head || slot.receiver.mutable))
            {
                return Err(bad("method call writes a shared actual target"));
            }
            let requirement = if writing {
                DynRequirement::Write(position.clone())
            } else {
                DynRequirement::Read(position.clone())
            };
            b.spend(
                requires
                    .len()
                    .saturating_mul(position.path.len().saturating_add(1)),
            )?;
            if !requires.contains(&requirement)
                && !(!writing && requires.contains(&DynRequirement::Write(position.clone())))
            {
                return Err(bad(
                    "method call access is missing its actual caller requirement",
                ));
            }
        }
    }
    let guarantees = c
        .guarantees
        .as_ref()
        .ok_or_else(|| bad("method receiver-after sources are Unknown"))?;
    if guarantees.len() != iface.retained_life.ports.len() {
        return Err(bad("method receiver-after directory omits a port"));
    }
    for (index, (guarantee, port)) in guarantees
        .iter()
        .zip(&iface.retained_life.ports)
        .enumerate()
    {
        let DynGuarantee::StorageSources(row) = guarantee else {
            return Err(bad("method receiver-after row has no actual source proof"));
        };
        let position = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::RetainedPort(index as u32)],
        };
        if row.position != position {
            return Err(bad(
                "method receiver-after rows are not complete canonical ports",
            ));
        }
        if !slot.receiver.mutable && row.sources.as_slice() != [position] {
            return Err(bad("shared receiver cannot change owning source slots"));
        }
        for source in &row.sources {
            let (leaf, binder, head) = position_leaf(iface, slot, source, b)?;
            dynamic_metadata::ty(&port.leaf, b)?;
            if head || binder != StorageBinder::Fixed(port.binder) || !compatible(&leaf, &port.leaf)
            {
                return Err(bad(
                    "receiver-after source has a different full leaf, mode or persistent class",
                ));
            }
        }
    }
    match (&slot.result, &c.result) {
        (Type::Address { .. }, Some(DynResultSource::Borrow { sources, mutable })) => {
            if !matches!(slot.result,Type::Address{mutable:m,..} if m==*mutable) {
                return Err(bad(
                    "method result borrow mode differs from its actual type",
                ));
            }
            let life = slot
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("method result class is Unknown"))?;
            let result = crate::signature_life::outer(life, StorageBase::Result)
                .ok_or_else(|| bad("method borrowed result has no class binding"))?;
            for source in sources {
                let (leaf, binder, head) = position_leaf(iface, slot, source, b)?;
                if head {
                    if result != StorageBinder::Invocation(0) {
                        return Err(bad("receiver head cannot authorize a longer Fixed result"));
                    }
                } else if !compatible(&leaf, &slot.result)
                    || binder != result
                        && !(source.base == StorageBase::Parameter(0)
                            && result == StorageBinder::Invocation(0))
                {
                    return Err(bad(
                        "method result source has a different full leaf, mode or class",
                    ));
                }
            }
        }
        (ty, Some(DynResultSource::Value)) if ty.is_copy_value() => {}
        _ => {
            return Err(bad(
                "method result source has no independently integrated value or borrowed route",
            ));
        }
    }
    if c.hold.as_ref().is_none_or(|v| !v.is_empty())
        || c.resume != Some(DynResumeBound::Local)
        || c.threads
            != Some(DynThreadRequirements {
                transfer: false,
                share: false,
            })
    {
        return Err(bad(
            "method retained execution contract is Unknown or unsupported",
        ));
    }
    let cleanup = c
        .cleanup
        .as_ref()
        .ok_or_else(|| bad("method cleanup is Unknown"))?;
    if cleanup.environment
        || !cleanup.positions.is_empty()
        || !cleanup.access.reads.is_empty()
        || !cleanup.access.writes.is_empty()
        || !cleanup.effects.is_empty()
    {
        return Err(bad("method cleanup has unproved authority"));
    }
    let effects = c
        .effects
        .as_ref()
        .ok_or_else(|| bad("method effects are Unknown"))?;
    b.spend(effects.len())?;
    if effects.iter().any(|effect| {
        !matches!(
            effect,
            DynEffect::Alloc
                | DynEffect::Io
                | DynEffect::Block
                | DynEffect::Ffi
                | DynEffect::Reenter
                | DynEffect::Panic
        )
    }) {
        return Err(bad("method effects exceed the checked synchronous domain"));
    }
    Ok(())
}

fn entry_life(
    table: &DynVtable,
    slot: &DynSlot,
    function: &TypedFunction,
    b: &mut Budget,
) -> Check<()> {
    let actual = function
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("provider actual function class relation is Unknown"))?;
    let public = slot
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("public method class relation is Unknown"))?;
    crate::signature_life::charge(actual, b)?;
    let mut map = Vec::<(StorageBinder, StorageBinder)>::new();
    fn bind(
        map: &mut Vec<(StorageBinder, StorageBinder)>,
        a: StorageBinder,
        z: StorageBinder,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(map.len().saturating_add(1))?;
        if let Some((_, old)) = map.iter().find(|(old, _)| *old == a) {
            if *old != z {
                return Err(bad(
                    "provider requires an input class equality absent from the public method",
                ));
            }
        } else {
            map.push((a, z));
        }
        Ok(())
    }
    for row in &actual.classes {
        let expected = if row.position.base == StorageBase::Parameter(0)
            && row.position.path == [LifeTypeProjection::Pointee]
        {
            *table
                .self_classes
                .get(row.position.class as usize)
                .ok_or_else(|| bad("actual provider receiver class has no complete Self map"))?
        } else {
            b.spend(
                public
                    .classes
                    .len()
                    .saturating_mul(row.position.path.len().saturating_add(1)),
            )?;
            public
                .classes
                .iter()
                .find(|p| p.position == row.position)
                .ok_or_else(|| {
                    bad("provider actual class is absent from public full ordered argument")
                })?
                .binder
        };
        bind(&mut map, row.binder, expected, b)?;
    }
    for row in &actual.bindings {
        if row.position.base == StorageBase::Result {
            continue;
        }
        b.spend(
            public
                .bindings
                .len()
                .saturating_mul(row.position.path.len().saturating_add(1)),
        )?;
        let expected = public
            .bindings
            .iter()
            .find(|p| p.position == row.position)
            .ok_or_else(|| bad("provider borrowed input has no public full-position binding"))?
            .binder;
        bind(&mut map, row.binder, expected, b)?;
    }
    for row in &actual.bindings {
        if row.position.base != StorageBase::Result {
            continue;
        }
        b.spend(public.bindings.len().saturating_add(map.len()))?;
        let expected = public
            .bindings
            .iter()
            .find(|p| p.position == row.position)
            .ok_or_else(|| bad("provider borrowed result has no public full-position binding"))?
            .binder;
        let found = map
            .iter()
            .find(|(a, _)| *a == row.binder)
            .map(|(_, z)| *z)
            .ok_or_else(|| bad("provider result class is not grounded in its actual inputs"))?;
        if found != expected
            && !(expected == StorageBinder::Invocation(0)
                && matches!(found, StorageBinder::Fixed(_))
                && matches!(slot.result, Type::Address { .. }))
        {
            return Err(bad(
                "provider actual borrowed result class cannot instantiate the public method",
            ));
        }
    }
    Ok(())
}

/// Establish complete entries from actual function identities/signatures. The
/// memory/effect kernel later proves the executable accesses and return sources.
pub(super) fn validate_entries(
    module: &TypedModule,
    table: &DynVtable,
    iface: &DynInterface,
    b: &mut Budget,
) -> Check<()> {
    let implementation = module
        .dyn_declarations
        .iter()
        .find(|d| d.id == table.implementation)
        .ok_or_else(|| bad("Context implementation declaration is absent"))?;
    actuals(
        &implementation.parameters,
        &table.actuals,
        iface.retained_life.binder_count,
        0,
        b,
    )?;
    crate::dynamic_patterns::provider(module, table, implementation, iface, b)?;
    for (ordinal, entry) in table.entries.iter().enumerate() {
        b.spend(1)?;
        let function = module
            .functions
            .get(entry.function.0 as usize)
            .filter(|f| f.id == entry.function)
            .ok_or_else(|| bad("Context provider entry is absent"))?;
        let slot = &iface.slots[ordinal];
        if entry.slot as usize != ordinal
            || function.foreign.is_some()
            || function.environment.is_some()
            || function.call_kind != slot.call_kind
            || function.parameters.len() != slot.parameters.len() + 1
            || function.result_mode != slot.result_mode
            || function.return_type != slot.result
        {
            return Err(bad(
                "Context entry identity, calling mode or complete result differs from its slot",
            ));
        }
        entry_life(table, slot, function, b)?;
        b.spend(function.parameters.len())?;
        for (index, local) in function.parameters.iter().enumerate() {
            let actual = &function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("Context entry parameter is absent"))?
                .ty;
            if index == 0 {
                dynamic_metadata::ty(actual, b)?;
                dynamic_metadata::ty(&table.self_type, b)?;
                if !matches!(actual, Type::Address { mutable, inner } if *mutable == slot.receiver.mutable && inner.as_ref() == &table.self_type)
                {
                    return Err(bad("Context entry receiver is not the exact actual Self"));
                }
            } else {
                dynamic_metadata::ty(actual, b)?;
                dynamic_metadata::ty(&slot.parameters[index - 1], b)?;
                if *actual != slot.parameters[index - 1] {
                    return Err(bad("Context entry full ordered argument type differs"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
