//! Finite borrowed dyn admission. Core tables and slot claims are untrusted.
use super::*;

pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.dyn:v1:DynCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

pub(super) fn owned_result(interface: DynInterfaceId) -> Type {
    Type::ErrorUnion {
        error: Box::new(create_error()),
        result: Box::new(Type::OwnedDyn { interface }),
    }
}

fn plain_value(ty: &Type) -> bool {
    ty.is_b0_scalar()
        || *ty == Type::Unit
        || matches!(ty, Type::Tuple(fields) if fields.iter().all(plain_value))
        || matches!(ty, Type::Array { element, .. } if plain_value(element))
}

pub(super) fn borrowed_result(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar())
}

pub(super) fn slot_life(slot: &DynSlot) -> StorageLifeContract {
    let mut life = shared_slot_life();
    if borrowed_result(&slot.result) {
        life.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: Vec::new(),
            },
            binder: StorageBinder::Invocation(0),
        });
    }
    life
}

fn closed_owner_shape(
    ty: &Type,
    left: &mut usize,
    depth: usize,
    limits: &Limits,
) -> R<Option<usize>> {
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "owned dyn provider depth exceeded",
            None,
        ));
    }
    *left = left
        .checked_sub(1)
        .ok_or_else(|| Diagnostic::limit(Phase::Types, "owned dyn provider work exceeded", None))?;
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(Some(0));
    }
    let mut count = 0usize;
    match ty {
        Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer => return Ok(Some(1)),
        Type::Managed(payload) => match payload.as_ref() {
            Type::Record { .. }
                if closed_owner_shape(payload, left, depth + 1, limits)? == Some(0) =>
            {
                return Ok(Some(1));
            }
            Type::OwnedStore { element }
                if closed_owner_shape(element, left, depth + 1, limits)?.is_some() =>
            {
                return Ok(Some(1));
            }
            _ => return Ok(None),
        },
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                let Some(n) = closed_owner_shape(field, left, depth + 1, limits)? else {
                    return Ok(None);
                };
                if n != 0 {
                    return Ok(None);
                }
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                let Some(n) = closed_owner_shape(field, left, depth + 1, limits)? else {
                    return Ok(None);
                };
                if n != 0 {
                    return Ok(None);
                }
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    let Some(n) = closed_owner_shape(field, left, depth + 1, limits)? else {
                        return Ok(None);
                    };
                    if n != 0 {
                        return Ok(None);
                    }
                }
            }
        }
        Type::Array { element, length }
            if *length > 0 && (*length as usize) <= limits.max_nodes =>
        {
            let Some(n) = closed_owner_shape(element, left, depth + 1, limits)? else {
                return Ok(None);
            };
            if n != 0 {
                return Ok(None);
            }
            *left = left.checked_sub(*length as usize).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "owned dyn array work exceeded", None)
            })?;
        }
        Type::Package { fields, .. } => {
            for (_, field) in fields {
                let Some(n) = closed_owner_shape(field, left, depth + 1, limits)? else {
                    return Ok(None);
                };
                count = count.checked_add(n).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "owned dyn owner count exceeded", None)
                })?;
            }
            if count == 0 {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(count))
}

fn receiver_position() -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(0),
        path: Vec::new(),
    }
}

pub(super) fn scalar_contract(mutable: bool) -> DynContract {
    let receiver = receiver_position();
    DynContract {
        requires: Some(vec![if mutable {
            DynRequirement::Write(receiver.clone())
        } else {
            DynRequirement::Read(receiver.clone())
        }]),
        guarantees: Some(Vec::new()),
        hold: Some(Vec::new()),
        call: Some(DynCallAccess {
            reads: vec![receiver.clone()],
            writes: if mutable { vec![receiver] } else { Vec::new() },
        }),
        result: Some(DynResultSource::Value),
        cleanup: Some(DynCleanupContract {
            environment: false,
            positions: Vec::new(),
            access: DynCallAccess {
                reads: Vec::new(),
                writes: Vec::new(),
            },
            effects: Vec::new(),
        }),
        effects: Some(Vec::new()),
        resume: Some(DynResumeBound::Local),
        threads: Some(DynThreadRequirements {
            transfer: false,
            share: false,
        }),
    }
}

pub(super) fn valid_scalar_contract(slot: &DynSlot) -> bool {
    let mut expected = scalar_contract(slot.receiver.mutable);
    if slot.contract.effects.as_deref() == Some(&[DynEffect::Panic]) {
        expected.effects = Some(vec![DynEffect::Panic]);
    } else if slot.contract.effects.as_deref()
        == Some(&[
            DynEffect::Alloc,
            DynEffect::Io,
            DynEffect::Block,
            DynEffect::Ffi,
            DynEffect::Reenter,
            DynEffect::Panic,
        ])
    {
        expected.effects = slot.contract.effects.clone();
    }
    if borrowed_result(&slot.result) {
        expected.result = Some(DynResultSource::Borrow {
            sources: vec![receiver_position()],
            mutable: false,
        });
    }
    slot.contract == expected
}

fn scalar_expression(
    expr: &CoreExpr,
    function: &CoreFunction,
    fields: &[(String, Type)],
    panic: bool,
    left: &mut usize,
    max_depth: usize,
    depth: usize,
) -> R<bool> {
    *left = left
        .checked_sub(1 + fields.len() + function.parameters.len())
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "dyn expression budget exceeded",
                Some(expr.span),
            )
        })?;
    // Depth is independently bounded even when a raw producer supplies a deep tree.
    if depth > max_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "dyn expression depth exceeded",
            Some(expr.span),
        ));
    }
    Ok(match &expr.kind {
        CoreExprKind::Unit
        | CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::Literal(_) => true,
        CoreExprKind::Name(name) => function
            .parameters
            .iter()
            .skip(1)
            .any(|p| p.name == *name && plain_value(&p.ty)),
        CoreExprKind::Member {
            object,
            member,
            owner: None,
        } => {
            matches!(&object.kind, CoreExprKind::Name(name) if name == &function.parameters[0].name)
                && fields
                    .iter()
                    .any(|(name, ty)| name == member && ty.is_b0_scalar())
        }
        CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
            for value in values {
                if !scalar_expression(value, function, fields, panic, left, max_depth, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        CoreExprKind::Binary {
            left: a, right: b, ..
        } if panic => {
            scalar_expression(a, function, fields, panic, left, max_depth, depth + 1)?
                && scalar_expression(b, function, fields, panic, left, max_depth, depth + 1)?
        }
        CoreExprKind::Unary {
            op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
            operand,
        } if panic => {
            scalar_expression(operand, function, fields, panic, left, max_depth, depth + 1)?
        }
        _ => false,
    })
}

pub(super) fn shared_slot_life() -> StorageLifeContract {
    StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![StorageLifeBinding {
            position: receiver_position(),
            binder: StorageBinder::Invocation(0),
        }],
        classes: Vec::new(),
    }
}

pub(super) fn inventory(
    core: &CoreModule,
    signatures: &BTreeMap<String, Signature>,
    limits: &Limits,
) -> R<Vec<DynVtable>> {
    inventory_in_modules(core, signatures, limits, &[core], None)
}

fn inventory_in_modules(
    core: &CoreModule,
    signatures: &BTreeMap<String, Signature>,
    limits: &Limits,
    modules: &[&CoreModule],
    program_signatures: Option<&BTreeMap<zil_base::ModuleId, BTreeMap<String, Signature>>>,
) -> R<Vec<DynVtable>> {
    if core.dyn_declarations.is_empty()
        && core.dyn_interfaces.is_empty()
        && core.dyn_vtables.is_empty()
    {
        return Ok(Vec::new());
    }
    let mut left = limits.max_analysis_steps;
    let spend = |left: &mut usize, cost: usize| {
        *left = left.checked_sub(cost).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "dyn inventory work budget exceeded", None)
        })?;
        Ok::<(), Diagnostic>(())
    };
    let mut declarations = std::collections::BTreeSet::new();
    for declaration in &core.dyn_declarations {
        spend(
            &mut left,
            1 + declaration.members.len() + declaration.parameters.len(),
        )?;
        if !declarations.insert(declaration.id) {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "duplicate dyn declaration identity",
            ));
        }
    }
    let mut interfaces = std::collections::BTreeSet::new();
    for interface in &core.dyn_interfaces {
        spend(&mut left, 1 + interface.slots.len())?;
        crate::dyn_context::validate(interface, modules, limits, &mut left)?;
        if interface.context.is_some()
            || !interfaces.insert(interface.id)
            || !declarations.contains(&interface.application.declaration)
            || core
                .dyn_declarations
                .iter()
                .find(|d| d.id == interface.application.declaration)
                .is_none_or(|declaration| {
                    declaration.kind != DynDeclarationKind::Trait
                        || !declaration.parameters.is_empty()
                })
            || !interface.application.actuals.is_empty()
            || !interface.inherited.is_empty()
            || interface.slots.is_empty()
            || !interface.associated_types.is_empty()
            || !interface.associated_constants.is_empty()
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || interface.retained_life.binder_count != 0
            || !interface.retained_life.nominal_arguments.is_empty()
        {
            return Err(unsupported(
                Span::default(),
                "dyn interface is outside closed shared profile",
            ));
        }
        let declaration = core
            .dyn_declarations
            .iter()
            .find(|d| d.id == interface.application.declaration)
            .ok_or_else(|| {
                error(
                    "E-DYN-DECLARATION",
                    Span::default(),
                    "dyn interface declaration absent",
                )
            })?;
        if interface.slots.len() != declaration.members.len() {
            return Err(unsupported(
                Span::default(),
                "dyn interface omits an actual declared method",
            ));
        }
        for (ordinal, slot) in interface.slots.iter().enumerate() {
            for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
                if !ty.is_b0_scalar() {
                    inference::charge_type_snapshot(ty, Span::default(), limits, &mut left, 3)?;
                }
            }
            spend(&mut left, 256 + slot.parameters.len())?;
            if !valid_scalar_contract(slot)
                || slot.receiver.mode
                    != if slot.receiver.mutable {
                        PassingMode::BorrowMut
                    } else {
                        PassingMode::Borrow
                    }
                || !slot.actuals.is_empty()
                || slot.parameters.iter().any(|ty| !plain_value(ty))
                || !(plain_value(&slot.result)
                    || !slot.receiver.mutable && borrowed_result(&slot.result))
                || slot
                    .parameter_modes
                    .iter()
                    .any(|mode| *mode != PassingMode::Infer)
                || slot.call_kind != CallKind::Direct
                || slot.storage_life.as_ref() != Some(&slot_life(slot))
                || slot.member.application != interface.application
                || slot.member.ordinal as usize != ordinal
                || core
                    .dyn_declarations
                    .iter()
                    .find(|d| d.id == slot.member.application.declaration)
                    .and_then(|d| d.members.get(slot.member.ordinal as usize))
                    .is_none_or(|member| !matches!(member.kind, DynMemberKind::Method(_)))
            {
                return Err(unsupported(
                    Span::default(),
                    "dyn slot has unknown or unproven closed contract",
                ));
            }
        }
    }
    let mut output = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for table in &core.dyn_vtables {
        spend(&mut left, 1 + table.entries.len())?;
        let interface = core
            .dyn_interfaces
            .iter()
            .find(|interface| interface.id == table.interface)
            .ok_or_else(|| {
                error(
                    "E-DYN-INTERFACE",
                    Span::default(),
                    "dyn table has no actual interface",
                )
            })?;
        if !ids.insert(table.id)
            || table.entries.len() != interface.slots.len()
            || table.cleanup.is_some()
            || !table.ports.is_empty()
            || !table.actuals.is_empty()
            || core
                .dyn_declarations
                .iter()
                .find(|d| d.id == table.implementation)
                .is_none_or(|declaration| declaration.kind != DynDeclarationKind::Impl)
        {
            return Err(unsupported(
                Span::default(),
                "dyn table has invalid slot or cleanup inventory",
            ));
        }
        if table.owned_storage == Some(DynOwnedStorage::Heap) {
            let bound = interface.owning_cleanup.as_ref().ok_or_else(|| {
                unsupported(
                    Span::default(),
                    "owned dyn whole-owner cleanup contract is Unknown",
                )
            })?;
            spend(&mut left, bound.effects.len().saturating_add(1))?;
            if bound.effects.len() > 6
                || !bound.environment
                || !bound.positions.is_empty()
                || !bound.access.reads.is_empty()
                || !bound.access.writes.is_empty()
                || bound.effects.iter().any(|e| {
                    !matches!(
                        e,
                        DynEffect::Alloc
                            | DynEffect::Io
                            | DynEffect::Block
                            | DynEffect::Ffi
                            | DynEffect::Reenter
                            | DynEffect::Panic
                    )
                })
            {
                return Err(unsupported(
                    Span::default(),
                    "owned dyn cleanup bound has unproved authority",
                ));
            }
        }
        let mut entries = Vec::new();
        let mut canonical_self = None;
        for (ordinal, entry) in table.entries.iter().enumerate() {
            spend(&mut left, 1)?;
            if entry.slot as usize != ordinal || entry.module != core.module_id {
                return Err(unsupported(
                    Span::default(),
                    "dyn entry needs exact local target identity",
                ));
            }
            let function = core
                .functions
                .iter()
                .find(|function| function.id == entry.function)
                .ok_or_else(|| {
                    error(
                        "E-DYN-ENTRY",
                        Span::default(),
                        "dyn entry target body absent",
                    )
                })?;
            let typed_id = signatures
                .get(&function.name)
                .ok_or_else(|| error("E-DYN-ENTRY", function.span, "dyn entry signature absent"))?
                .concrete_id(function.span)?;
            let Some(Type::Address { mutable, inner }) = function.parameters.first().map(|p| &p.ty)
            else {
                return Err(unsupported(
                    function.span,
                    "dyn provider receiver must be shared concrete Address",
                ));
            };
            let (name, fields, owning_package) = match inner.as_ref() {
                Type::Record { name, fields } => (name, fields, false),
                Type::Package { name, fields } => (name, fields, true),
                _ => {
                    return Err(unsupported(
                        function.span,
                        "dyn provider Self must resolve to concrete record or owning Package",
                    ));
                }
            };
            let Type::Named(raw_self) = &table.self_type else {
                return Err(unsupported(
                    function.span,
                    "dyn raw Self needs nominal declaration",
                ));
            };
            if *name != format!("@module{}:{}", table.id.module.0, raw_self)
                || if owning_package {
                    table.owned_storage != Some(DynOwnedStorage::Heap)
                        || closed_owner_shape(inner, &mut left, 0, limits)?.is_none()
                } else {
                    fields.iter().any(|(_, ty)| !ty.is_b0_scalar())
                }
                || canonical_self
                    .as_ref()
                    .is_some_and(|previous| previous != inner.as_ref())
            {
                return Err(error(
                    "E-DYN-SELF",
                    function.span,
                    "dyn provider Self differs from actual table declaration",
                ));
            }
            canonical_self = Some(inner.as_ref().clone());
            let slot = &interface.slots[ordinal];
            if *mutable != slot.receiver.mutable
                || function.foreign.is_some()
                || !function.type_parameters.is_empty()
                || !function.state_parameters.is_empty()
                || !function.callable_parameters.is_empty()
                || function.return_type != slot.result
                || function.parameters.len() != slot.parameters.len() + 1
                || function.parameters[0].mode != slot.receiver.mode
                || function
                    .parameters
                    .iter()
                    .skip(1)
                    .zip(&slot.parameters)
                    .any(|(p, ty)| p.ty != *ty)
                || !scalar_body(
                    ScalarBodyContext {
                        core,
                        modules,
                        table,
                        interface,
                        signatures,
                        program_signatures,
                        max_depth: limits.max_call_depth,
                    },
                    function,
                    fields,
                    slot.receiver.mutable,
                    &slot.result,
                    slot.contract
                        .effects
                        .as_ref()
                        .is_some_and(|e| e.contains(&DynEffect::Panic)),
                    &mut left,
                )?
            {
                return Err(unsupported(
                    function.span,
                    "dyn provider body or signature is outside proven scalar profile",
                ));
            }
            entries.push(DynEntry {
                slot: entry.slot,
                function: typed_id,
            });
        }
        let self_type =
            canonical_self.ok_or_else(|| unsupported(Span::default(), "dyn table is empty"))?;
        if table.owned_storage == Some(DynOwnedStorage::Heap) {
            crate::dyn_cleanup::prove(
                &self_type,
                modules,
                interface
                    .owning_cleanup
                    .as_ref()
                    .expect("checked whole owner bound"),
                &mut left,
                limits,
                0,
            )?;
        }
        spend(&mut left, output.len())?;
        if output.iter().any(|prior: &DynVtable| {
            prior.interface == table.interface && prior.self_type == self_type
        }) {
            return Err(error(
                "E-DYN-PROVIDER",
                Span::default(),
                "dyn interface has duplicate concrete Self tables",
            ));
        }
        output.push(DynVtable {
            id: table.id,
            interface: table.interface,
            self_type,
            self_classes: table.self_classes.clone(),
            implementation: table.implementation,
            actuals: table.actuals.clone(),
            ports: Vec::new(),
            entries,
            cleanup: None,
            owned_storage: table.owned_storage,
        });
    }
    Ok(output)
}

pub(super) fn inventory_program(
    program: &CoreProgram,
    signatures: &BTreeMap<zil_base::ModuleId, BTreeMap<String, Signature>>,
    limits: &Limits,
) -> R<(Vec<DynDeclaration>, Vec<DynInterface>, Vec<DynVtable>)> {
    let mut declarations = Vec::new();
    let mut interfaces = Vec::new();
    let mut left = limits.max_analysis_steps;
    let mut declaration_ids = std::collections::BTreeSet::new();
    let mut declaration_identities = std::collections::BTreeSet::new();
    for module in &program.modules {
        let core = &module.core;
        let spend = |left: &mut usize, count: usize| {
            *left = left.checked_sub(count).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn project inventory budget exceeded", None)
            })?;
            Ok::<(), Diagnostic>(())
        };
        for declaration in &core.dyn_declarations {
            spend(
                &mut left,
                1 + declaration.parameters.len() + declaration.members.len(),
            )?;
            spend(
                &mut left,
                declaration.identity.package.len()
                    + declaration.identity.module_path.len().saturating_mul(8)
                    + declaration
                        .identity
                        .declaration_path
                        .len()
                        .saturating_mul(8)
                    + declaration.identity.generation_path.len().saturating_mul(8),
            )?;
            for name in declaration
                .identity
                .module_path
                .iter()
                .chain(&declaration.identity.declaration_path)
            {
                spend(&mut left, name.len())?;
            }
            if !declaration.parameters.is_empty() {
                return Err(unsupported(
                    Span::default(),
                    "generic dyn declaration needs a fixed application",
                ));
            }
            if !declaration_ids.insert(declaration.id)
                || !declaration_identities.insert(&declaration.identity)
            {
                return Err(error(
                    "E-DYN-DECLARATION",
                    Span::default(),
                    "dyn declaration identity is duplicated",
                ));
            }
            for member in &declaration.members {
                spend(&mut left, member.name.len() + 1)?;
                if !matches!(&member.kind, DynMemberKind::Method(parameters) if parameters.is_empty())
                {
                    return Err(unsupported(
                        Span::default(),
                        "dyn declaration member is outside fixed method profile",
                    ));
                }
            }
        }
        for interface in &core.dyn_interfaces {
            spend(
                &mut left,
                1 + interface.slots.len()
                    + interface.inherited.len()
                    + interface.application.actuals.len()
                    + interface.associated_types.len()
                    + interface.associated_constants.len()
                    + interface.retained_life.ports.len()
                    + interface.retained_life.retained.len()
                    + interface.retained_life.nominal_arguments.len(),
            )?;
            if interface.context.is_some()
                || !interface.application.actuals.is_empty()
                || !interface.inherited.is_empty()
                || !interface.associated_types.is_empty()
                || !interface.associated_constants.is_empty()
                || !interface.retained_life.ports.is_empty()
                || !interface.retained_life.retained.is_empty()
                || !interface.retained_life.nominal_arguments.is_empty()
            {
                return Err(unsupported(
                    Span::default(),
                    "dyn interface has unresolved retained or generic facts",
                ));
            }
            for slot in &interface.slots {
                for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
                    if !ty.is_b0_scalar() {
                        inference::charge_type_snapshot(ty, Span::default(), limits, &mut left, 3)?;
                    }
                }
                spend(
                    &mut left,
                    256 + slot.parameters.len()
                        + slot.parameter_modes.len()
                        + slot.actuals.len()
                        + slot.member.application.actuals.len(),
                )?;
                if !valid_scalar_contract(slot)
                    || !slot.actuals.is_empty()
                    || !slot.member.application.actuals.is_empty()
                    || slot.parameters.iter().any(|ty| !plain_value(ty))
                    || !(plain_value(&slot.result)
                        || !slot.receiver.mutable && borrowed_result(&slot.result))
                {
                    return Err(unsupported(
                        Span::default(),
                        "dyn slot has unknown or non-scalar contract",
                    ));
                }
            }
        }
        for table in &core.dyn_vtables {
            spend(
                &mut left,
                1 + table.entries.len() + table.actuals.len() + table.ports.len(),
            )?;
            if !table.actuals.is_empty() || !table.ports.is_empty() || table.cleanup.is_some() {
                return Err(unsupported(
                    Span::default(),
                    "dyn table has unsupported actuals or cleanup",
                ));
            }
        }
        declarations.extend(core.dyn_declarations.iter().cloned());
        interfaces.extend(core.dyn_interfaces.iter().cloned());
    }
    if declarations.is_empty()
        && interfaces.is_empty()
        && program
            .modules
            .iter()
            .all(|module| module.core.dyn_vtables.is_empty())
    {
        return Ok((declarations, interfaces, Vec::new()));
    }
    if interfaces.is_empty() {
        return Err(unsupported(
            Span::default(),
            "dyn project declaration has no actual interface",
        ));
    }
    let metadata_work = limits.max_analysis_steps - left;
    let mut tables = Vec::new();
    for module in &program.modules {
        if module.core.dyn_vtables.is_empty() {
            continue;
        }
        // A provider table belongs to its implementation module, while its
        // exact interface declaration may be in a different module.
        left = left
            .checked_sub(metadata_work.saturating_add(module.core.functions.len()))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn project snapshot budget exceeded", None)
            })?;
        let mut local = inference::snapshot_context_module(&module.core, limits, &mut left)?;
        local.dyn_declarations = declarations.clone();
        local.dyn_interfaces = interfaces.clone();
        left = left.checked_sub(program.modules.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "dyn project module lookup budget exceeded",
                None,
            )
        })?;
        let related: Vec<&CoreModule> = std::iter::once(&local)
            .chain(
                program
                    .modules
                    .iter()
                    .filter(|other| other.core.module_id != local.module_id)
                    .map(|other| &other.core),
            )
            .collect();
        for table in inventory_in_modules(
            &local,
            &signatures[&module.core.module_id],
            limits,
            &related,
            Some(signatures),
        )? {
            left = left.checked_sub(tables.len()).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "dyn project table match budget exceeded",
                    None,
                )
            })?;
            if tables.iter().any(|prior: &DynVtable| {
                prior.id == table.id
                    || prior.interface == table.interface && prior.self_type == table.self_type
            }) {
                return Err(error(
                    "E-DYN-PROVIDER",
                    Span::default(),
                    "dyn project has duplicate table identity or Self",
                ));
            }
            tables.push(table);
        }
    }
    Ok((declarations, interfaces, tables))
}

struct ScalarBodyContext<'a> {
    core: &'a CoreModule,
    modules: &'a [&'a CoreModule],
    table: &'a CoreDynVtable,
    interface: &'a DynInterface,
    signatures: &'a BTreeMap<String, Signature>,
    program_signatures: Option<&'a BTreeMap<zil_base::ModuleId, BTreeMap<String, Signature>>>,
    max_depth: usize,
}

fn scalar_body(
    context: ScalarBodyContext<'_>,
    function: &CoreFunction,
    fields: &[(String, Type)],
    mutable: bool,
    result: &Type,
    panic: bool,
    left: &mut usize,
) -> R<bool> {
    let ScalarBodyContext {
        core,
        modules,
        table,
        interface,
        signatures,
        program_signatures,
        max_depth,
    } = context;
    let mut current = function;
    let mut current_module = core.module_id;
    let mut seen = Vec::new();
    let value = loop {
        *left = left.checked_sub(1 + seen.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "dyn reachable body budget exceeded",
                Some(current.span),
            )
        })?;
        if seen.contains(&(current_module, current.id)) {
            return Ok(false);
        }
        if seen.len() >= max_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn helper depth exceeded",
                Some(current.span),
            ));
        }
        seen.push((current_module, current.id));
        if mutable
            && matches!(current.body.statements.len(), 1 | 2)
            && matches!(
                current.body.statements[0].kind,
                CoreStatementKind::Assign { .. }
            )
        {
            *left = left
                .checked_sub(
                    1 + current.body.statements.len()
                        + fields.len().saturating_mul(3)
                        + current.parameters.len(),
                )
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "dyn mutable body budget exceeded",
                        Some(current.span),
                    )
                })?;
            let assignment = &current.body.statements[0];
            let returned = current.body.statements.get(1);
            let CoreStatementKind::Assign { target, value } = &assignment.kind else {
                return Ok(false);
            };
            let CoreExprKind::Member {
                object,
                member,
                owner: None,
            } = &target.kind
            else {
                return Ok(false);
            };
            if !matches!(&object.kind, CoreExprKind::Name(name) if name == &current.parameters[0].name)
                || !scalar_expression(value, current, fields, panic, left, max_depth, 0)?
                || !fields
                    .iter()
                    .any(|(name, ty)| name == member && ty.is_b0_scalar())
            {
                return Ok(false);
            }
            let returns_unit = *result == Type::Unit
                && returned.is_none_or(|returned| {
                    matches!(
                        &returned.kind,
                        CoreStatementKind::Return(None)
                            | CoreStatementKind::Return(Some(CoreExpr {
                                kind: CoreExprKind::Unit,
                                ..
                            }))
                    )
                });
            let returns_field = returned.is_some_and(|returned| matches!(&returned.kind,
                CoreStatementKind::Return(Some(CoreExpr { kind: CoreExprKind::Member { object, member, owner: None }, .. }))
                    if matches!(&object.kind, CoreExprKind::Name(name) if name == &current.parameters[0].name)
                        && fields.iter().any(|(name, ty)| name == member && ty == result)));
            return Ok(returns_unit || returns_field);
        }

        let [statement] = current.body.statements.as_slice() else {
            return Ok(false);
        };
        if !mutable
            && *result == Type::Unit
            && matches!(
                &statement.kind,
                CoreStatementKind::Return(None)
                    | CoreStatementKind::Return(Some(CoreExpr {
                        kind: CoreExprKind::Unit,
                        ..
                    }))
            )
        {
            return Ok(true);
        }
        let CoreStatementKind::Return(Some(value)) = &statement.kind else {
            return Ok(false);
        };
        let CoreExprKind::Call { callee, arguments } = &value.kind else {
            break value;
        };
        if current.parameters.len() != 1 {
            return Ok(false);
        }
        *left = left
            .checked_sub(
                modules
                    .iter()
                    .map(|m| m.functions.len())
                    .sum::<usize>()
                    .saturating_add(core.dyn_declarations.len())
                    .saturating_add(
                        core.dyn_declarations
                            .iter()
                            .map(|d| d.members.len())
                            .sum::<usize>(),
                    )
                    .saturating_add(table.entries.len()),
            )
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "dyn reachable target budget exceeded",
                    Some(value.span),
                )
            })?;
        let (next_module, next) = match &callee.kind {
            CoreExprKind::Name(target_name)
                if arguments.len() == 1
                    && matches!(&arguments[0].kind,
                    CoreExprKind::Name(name) if name == &current.parameters[0].name) =>
            {
                let mut targets = modules.iter().flat_map(|module| {
                    module
                        .functions
                        .iter()
                        .filter(move |candidate| candidate.name == *target_name)
                        .map(move |function| (module.module_id, function))
                });
                let Some(found) = targets.next() else {
                    return Ok(false);
                };
                if targets.next().is_some() {
                    return Ok(false);
                }
                found
            }
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if arguments.len() == 1
                && matches!(&object.kind, CoreExprKind::Name(_))
                && matches!(&arguments[0].kind,
                        CoreExprKind::Name(name) if name == &current.parameters[0].name) =>
            {
                let Some(trait_module) = modules
                    .iter()
                    .find(|m| m.module_id == table.interface.module)
                else {
                    return Ok(false);
                };
                let mut targets = trait_module.functions.iter().filter(|f| f.name == *member);
                let Some(found) = targets.next() else {
                    return Ok(false);
                };
                if targets.next().is_some() {
                    return Ok(false);
                }
                (trait_module.module_id, found)
            }
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if arguments.is_empty()
                && matches!(&object.kind,
                    CoreExprKind::Name(name) if name == &current.parameters[0].name) =>
            {
                let Some(declaration) = core
                    .dyn_declarations
                    .iter()
                    .find(|d| d.id == interface.application.declaration)
                else {
                    return Ok(false);
                };
                let Some(ordinal) = declaration.members.iter().position(|m| m.name == *member)
                else {
                    return Ok(false);
                };
                let Some(entry) = table
                    .entries
                    .get(ordinal)
                    .filter(|e| e.slot as usize == ordinal)
                else {
                    return Ok(false);
                };
                let Some(found) = modules
                    .iter()
                    .find(|m| m.module_id == entry.module)
                    .and_then(|m| m.functions.iter().find(|f| f.id == entry.function))
                else {
                    return Ok(false);
                };
                (entry.module, found)
            }
            _ => return Ok(false),
        };
        let target_signatures = if next_module == core.module_id {
            Some(signatures)
        } else {
            program_signatures.and_then(|all| all.get(&next_module))
        };
        if target_signatures
            .and_then(|all| all.get(&next.name))
            .is_none_or(|signature| signature.id.is_none())
            || next.foreign.is_some()
            || !next.type_parameters.is_empty()
            || !next.state_parameters.is_empty()
            || !next.callable_parameters.is_empty()
            || next.parameters.len() != 1
            || next.parameters[0].ty != current.parameters[0].ty
            || next.parameters[0].mode != current.parameters[0].mode
            || next.return_type != current.return_type
            || next.result_mode != current.result_mode
        {
            return Ok(false);
        }
        current = next;
        current_module = next_module;
    };
    if mutable {
        return Ok(false);
    }
    if let Type::Address {
        mutable: false,
        inner,
    } = result
    {
        return Ok(
            matches!(&value.kind, CoreExprKind::Unary { op: UnaryOp::Borrow, operand }
            if matches!(&operand.kind, CoreExprKind::Member { object, member, owner: None }
                if matches!(&object.kind, CoreExprKind::Name(name) if name == &current.parameters[0].name)
                    && fields.iter().any(|(name, ty)| name == member && ty == inner.as_ref()))),
        );
    }
    if scalar_expression(value, current, fields, panic, left, max_depth, 0)? {
        return Ok(true);
    }
    Ok(match &value.kind {
        CoreExprKind::Member {
            object,
            member,
            owner: None,
        } => {
            matches!(&object.kind, CoreExprKind::Name(name) if name == &current.parameters[0].name)
                && fields
                    .iter()
                    .any(|(name, ty)| name == member && ty == result)
        }
        CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::Literal(_) => true,
        _ => false,
    })
}

impl Builder<'_, '_> {
    pub(super) fn dyn_method_slot(
        &mut self,
        object: &CoreExpr,
        member: &str,
    ) -> R<Option<(DynInterfaceId, u32)>> {
        let Some(actual) = self.known_type_checked(object)? else {
            return Ok(None);
        };
        let result = (|| {
            let interface = match actual {
                Type::Address { inner, .. } => match *inner {
                    Type::Dyn { interface } | Type::OwnedDyn { interface } => interface,
                    _ => return None,
                },
                Type::OwnedDyn { interface } => interface,
                _ => return None,
            };
            let definition = self
                .dyn_interfaces
                .iter()
                .find(|candidate| candidate.id == interface)?;
            let mut found = None;
            for (index, slot) in definition.slots.iter().enumerate() {
                let declaration = self
                    .dyn_declarations
                    .iter()
                    .find(|declaration| declaration.id == slot.member.application.declaration)?;
                if declaration.members.get(slot.member.ordinal as usize)?.name == member {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((interface, index as u32));
                }
            }
            found
        })();
        Ok(result)
    }

    pub(super) fn dyn_coerce(
        &mut self,
        source: ValueId,
        actual: &Type,
        interface: DynInterfaceId,
        mutable: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let table = self
            .dyn_vtables
            .iter()
            .find(|table| table.interface == interface && &table.self_type == actual)
            .ok_or_else(|| {
                error(
                    "E-DYN-PROVIDER",
                    span,
                    "concrete shared receiver has no exact dyn provider",
                )
            })?;
        let ty = Type::Address {
            mutable,
            inner: Box::new(Type::Dyn { interface }),
        };
        let value = self.value(
            InstructionKind::MakeDynBorrow {
                interface,
                vtable: table.id,
                source,
                mutable,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(super) fn dyn_make_borrow(
        &mut self,
        interface: DynInterfaceId,
        vtable: DynVtableId,
        source: &CoreExpr,
        mutable: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let (source, ty) = self.expr(source, None)?;
        let Type::Address {
            mutable: available,
            inner,
        } = ty
        else {
            return Err(error(
                "E-DYN-RECEIVER",
                span,
                "dyn source must be a real shared concrete address",
            ));
        };
        if mutable && !available {
            return Err(error(
                "E-DYN-RECEIVER",
                span,
                "shared receiver cannot create mutable dyn",
            ));
        }
        let table = self
            .dyn_vtables
            .iter()
            .find(|table| {
                table.id == vtable && table.interface == interface && table.self_type == *inner
            })
            .ok_or_else(|| {
                error(
                    "E-DYN-PROVIDER",
                    span,
                    "dyn source does not match exact selected table",
                )
            })?;
        let ty = Type::Address {
            mutable,
            inner: Box::new(Type::Dyn { interface }),
        };
        let value = self.value(
            InstructionKind::MakeDynBorrow {
                interface,
                vtable: table.id,
                source,
                mutable,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(super) fn dyn_make_owned(
        &mut self,
        interface: DynInterfaceId,
        vtable: Option<DynVtableId>,
        source: &CoreExpr,
        route: DynOwnedStorage,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let (source_value, actual) = self.transfer_expr(source, None)?;
        if !matches!(&actual, Type::Package { .. }) {
            return Err(error(
                "E-DYN-OWNER",
                span,
                "owned dyn needs a complete Package with closed built-in ByteBuffer cleanup",
            ));
        }
        let table = self
            .dyn_vtables
            .iter()
            .find(|table| {
                table.interface == interface
                    && table.self_type == actual
                    && table.owned_storage == Some(route)
                    && vtable.is_none_or(|id| table.id == id)
            })
            .ok_or_else(|| {
                error(
                    "E-DYN-PROVIDER",
                    span,
                    "owned dyn has no exact owning provider table",
                )
            })?;
        let result = owned_result(interface);
        let value = self.value(
            InstructionKind::MakeDynOwned {
                interface,
                vtable: table.id,
                source: source_value,
                route,
            },
            result.clone(),
            span,
        )?;
        Ok((value, result))
    }

    pub(super) fn dyn_call(
        &mut self,
        callee: &CoreExpr,
        interface: DynInterfaceId,
        ordinal: u32,
        arguments: &[CoreExpr],
        span: Span,
    ) -> R<(ValueId, Type)> {
        let slot = self
            .dyn_interfaces
            .iter()
            .find(|candidate| candidate.id == interface)
            .and_then(|interface| interface.slots.get(ordinal as usize))
            .ok_or_else(|| {
                error(
                    "E-DYN-SLOT",
                    span,
                    "dyn call slot absent from actual interface",
                )
            })?;
        self.closures
            .snapshot(slot.parameters.len().saturating_add(1), span)?;
        let parameters = slot.parameters.clone();
        let result = slot.result.clone();
        if arguments.len() != parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "dyn slot argument count mismatch",
            ));
        }
        let (receiver, actual) = if let CoreExprKind::Name(name) = &callee.kind {
            if let Some(local) = self.lookup(name)
                && self
                    .function
                    .locals
                    .get(local.0 as usize)
                    .is_some_and(|l| l.ty == Type::OwnedDyn { interface } || matches!(&l.ty, Type::Address { inner, .. } if **inner == Type::OwnedDyn { interface }))
            {
                if slot.receiver.mutable && !match &self.function.locals[local.0 as usize].ty {
                    Type::Address { mutable, .. } => *mutable,
                    _ => self.function.locals[local.0 as usize].mutable,
                } {
                    return Err(error(
                        "E-DYN-RECEIVER",
                        callee.span,
                        "exclusive dyn method needs a mutable owner local",
                    ));
                }
                let ty = Type::Address {
                    mutable: slot.receiver.mutable,
                    inner: Box::new(Type::Dyn { interface }),
                };
                let value = self.value(
                    InstructionKind::BorrowDynOwned {
                        local,
                        mutable: slot.receiver.mutable,
                    },
                    ty.clone(),
                    callee.span,
                )?;
                (value, ty)
            } else {
                self.expr(callee, None)?
            }
        } else {
            self.expr(callee, None)?
        };
        let expected = Type::Address {
            mutable: slot.receiver.mutable,
            inner: Box::new(Type::Dyn { interface }),
        };
        let (receiver, actual) = if matches!(&actual, Type::Address { mutable: true, inner } if **inner == Type::Dyn { interface })
            && !slot.receiver.mutable
        {
            let value = self.value(
                InstructionKind::ReborrowAddress {
                    address: receiver,
                    mutable: false,
                },
                expected.clone(),
                callee.span,
            )?;
            (value, expected.clone())
        } else {
            (receiver, actual)
        };
        self.same(&actual, &expected, callee.span)?;
        let mut values = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&parameters) {
            let (value, actual) = self.argument_expr(argument, parameter)?;
            self.same(&actual, parameter, argument.span)?;
            values.push(self.stage_argument(value, &actual, argument.span)?);
        }
        let values = self.commit_arguments(values, span)?;
        let instruction = InstructionKind::CallDynSlot {
            callee: receiver,
            interface,
            slot: ordinal,
            arguments: values,
        };
        if result == Type::Unit {
            self.emit(instruction, None, span)?;
            self.constant(Constant::Unit, span)
        } else {
            Ok((self.value(instruction, result.clone(), span)?, result))
        }
    }
}
