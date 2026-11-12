//! Independent finite borrowed dyn checker. Raw tables and contracts grant no
//! authority: every used and unused entry is tied to a checked body.
use super::*;
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

pub(super) fn present(module: &TypedModule) -> bool {
    !module.dyn_declarations.is_empty()
        || !module.dyn_interfaces.is_empty()
        || !module.dyn_vtables.is_empty()
}

fn plain_value(ty: &Type) -> bool {
    ty.is_b0_scalar()
        || *ty == Type::Unit
        || matches!(ty, Type::Tuple(fields) if fields.iter().all(plain_value))
        || matches!(ty, Type::Array { element, .. } if plain_value(element))
}

fn bad(message: &str, span: Option<Span>) -> Diagnostic {
    reject("E-VERIFY-DYN", span, message)
}

fn receiver_position() -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![],
    }
}

fn interface(module: &TypedModule, id: DynInterfaceId) -> Check<&DynInterface> {
    module
        .dyn_interfaces
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| bad("dyn interface identity is absent", None))
}

fn table(module: &TypedModule, id: DynVtableId) -> Check<&DynVtable> {
    module
        .dyn_vtables
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| bad("dyn table identity is absent", None))
}

fn contract(slot: &DynSlot, budget: &mut Budget) -> Check<()> {
    let c = &slot.contract;
    let finite = c.requires.as_ref().is_some_and(|v| v.len() == 1)
        && c.guarantees.as_ref().is_some_and(Vec::is_empty)
        && c.hold.as_ref().is_some_and(Vec::is_empty)
        && c.call.as_ref().is_some_and(|v| {
            v.reads.len() == 1 && v.writes.len() == usize::from(slot.receiver.mutable)
        })
        && c.cleanup.as_ref().is_some_and(|v| {
            !v.environment
                && v.positions.is_empty()
                && v.access.reads.is_empty()
                && v.access.writes.is_empty()
                && v.effects.is_empty()
        })
        && c.effects.as_ref().is_some_and(|v| {
            v.is_empty()
                || v.as_slice() == [DynEffect::Panic]
                || v.as_slice()
                    == [
                        DynEffect::Alloc,
                        DynEffect::Io,
                        DynEffect::Block,
                        DynEffect::Ffi,
                        DynEffect::Reenter,
                        DynEffect::Panic,
                    ]
        });
    if !finite {
        return Err(bad(
            "dyn contract has an unknown or unbounded component",
            None,
        ));
    }
    budget.spend(12)?;
    let position = receiver_position();
    let expected = DynContract {
        requires: Some(vec![if slot.receiver.mutable {
            DynRequirement::Write(position.clone())
        } else {
            DynRequirement::Read(position.clone())
        }]),
        guarantees: Some(vec![]),
        hold: Some(vec![]),
        call: Some(DynCallAccess {
            reads: vec![position.clone()],
            writes: if slot.receiver.mutable {
                vec![position]
            } else {
                vec![]
            },
        }),
        result: Some(if borrowed_result(&slot.result) {
            DynResultSource::Borrow {
                sources: vec![StorageSource::Exact(receiver_position())],
                mutable: false,
            }
        } else {
            DynResultSource::Value
        }),
        cleanup: Some(DynCleanupContract {
            environment: false,
            positions: vec![],
            access: DynCallAccess {
                reads: vec![],
                writes: vec![],
            },
            effects: vec![],
        }),
        effects: c.effects.clone(),
        resume: Some(DynResumeBound::Local),
        threads: Some(DynThreadRequirements {
            transfer: false,
            share: false,
        }),
    };
    if slot.contract != expected {
        return Err(bad(
            "dyn contract is unknown or differs from the independently derived finite body contract",
            None,
        ));
    }
    Ok(())
}

fn borrowed_result(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if inner.is_b0_scalar())
}

fn scalar(ty: &Type) -> bool {
    ty.is_b0_scalar() || *ty == Type::Unit
}

fn closed_owner_shape(
    ty: &Type,
    module: &TypedModule,
    budget: &mut Budget,
    depth: usize,
) -> Check<Option<usize>> {
    if depth > budget.limits.max_call_depth {
        return Err(zil_base::Diagnostic::limit(
            zil_base::Phase::Verify,
            "owned dyn shape depth exceeded",
            None,
        ));
    }
    budget.spend(1)?;
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(Some(0));
    }
    let mut owners = 0usize;
    match ty {
        Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer => {
            budget.spend(module.resources.len())?;
            return Ok(module
                .resources
                .iter()
                .any(|r| {
                    r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
                        && r.payload == Type::ByteBuffer
                        && r.teardown.is_none()
                })
                .then_some(1));
        }
        Type::Managed(payload) => {
            budget.spend(module.resources.len())?;
            if !module.resources.iter().any(|r| r.payload == **payload) {
                return Ok(None);
            }
            match payload.as_ref() {
                Type::Record { .. }
                    if closed_owner_shape(payload, module, budget, depth + 1)? == Some(0) =>
                {
                    return Ok(Some(1));
                }
                Type::OwnedStore { element }
                    if closed_owner_shape(element, module, budget, depth + 1)?.is_some() =>
                {
                    return Ok(Some(1));
                }
                _ => return Ok(None),
            }
        }
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                if closed_owner_shape(field, module, budget, depth + 1)? != Some(0) {
                    return Ok(None);
                }
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                if closed_owner_shape(field, module, budget, depth + 1)? != Some(0) {
                    return Ok(None);
                }
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if closed_owner_shape(field, module, budget, depth + 1)? != Some(0) {
                        return Ok(None);
                    }
                }
            }
        }
        Type::Array { element, length }
            if *length > 0 && (*length as usize) <= budget.limits.max_nodes =>
        {
            budget.spend(*length as usize)?;
            if closed_owner_shape(element, module, budget, depth + 1)? != Some(0) {
                return Ok(None);
            }
        }
        Type::Package { fields, .. } => {
            for (_, field) in fields {
                let Some(n) = closed_owner_shape(field, module, budget, depth + 1)? else {
                    return Ok(None);
                };
                owners = owners.checked_add(n).ok_or_else(|| {
                    zil_base::Diagnostic::limit(
                        zil_base::Phase::Verify,
                        "owned dyn owner count exceeded",
                        None,
                    )
                })?;
            }
            if owners == 0 {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(owners))
}

fn owned_provider(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<bool> {
    let Type::Package { .. } = ty else {
        return Ok(false);
    };
    Ok(closed_owner_shape(ty, module, budget, 0)?.is_some())
}

fn dyn_address(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. }))
}

/// Charge every raw list before cloning, fingerprinting or indexing it. This
/// branch is inactive for old modules, preserving their exact budget path.
pub(super) fn inventory(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    for f in &module.functions {
        budget.spend(1)?;
        if let Some(instance) = &f.dyn_instance {
            crate::dynamic_metadata::recipe(instance, budget)?;
            return Err(bad(
                "dynamic function instance requires its complete definition proof",
                None,
            ));
        }
    }
    for table in &module.dyn_vtables {
        crate::dynamic_metadata::recipe_table(table, budget)?;
        if table.self_life.is_some() || table.implementation_witnesses.is_some() {
            return Err(bad(
                "dynamic implementation recipes require their complete static proof",
                None,
            ));
        }
    }

    if !module.target.is_supported_x64_llvm() {
        return Err(unsupported(
            "shared dyn requires the checked x64 LLVM target profile",
            None,
        ));
    }
    budget.spend(
        module
            .dyn_declarations
            .len()
            .saturating_mul(4)
            .saturating_add(module.dyn_interfaces.len().saturating_mul(8))
            .saturating_add(module.dyn_vtables.len().saturating_mul(8)),
    )?;
    for function in &module.functions {
        budget.spend(1)?;
        if dyn_address(&function.return_type)
            || (function.foreign.is_some()
                && function.parameters.iter().any(|local| {
                    function
                        .locals
                        .get(local.0 as usize)
                        .is_some_and(|decl| dyn_address(&decl.ty))
                }))
        {
            return Err(bad(
                "dyn return and foreign dyn ABI require a separate source contract",
                Some(function.span),
            ));
        }
    }
    for closure in &module.closures {
        budget.spend(closure.captures.len().saturating_add(1))?;
        if closure.captures.iter().any(dyn_address) {
            return Err(bad(
                "captured dyn requires a separate retained-root contract",
                None,
            ));
        }
    }
    budget.dyn_contexts |= module
        .dyn_interfaces
        .iter()
        .any(|item| item.context.is_some());
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for declaration in &module.dyn_declarations {
        crate::dynamic_metadata::declaration(declaration, budget)?;
        if declaration.direct_supertraits.is_some()
            || declaration.requirements.is_some()
            || declaration.associated_definitions.is_some()
            || declaration.method_signatures.is_some()
            || declaration.method_definitions.is_some()
        {
            return Err(bad(
                "dynamic declaration recipes require their complete static proof",
                None,
            ));
        }
        if declaration.kind == DynDeclarationKind::Trait
            && (declaration.implemented_trait.is_some() || declaration.self_pattern.is_some())
        {
            return Err(bad(
                "trait declaration cannot claim an implementation recipe",
                None,
            ));
        }
        budget.spend(
            declaration
                .members
                .len()
                .saturating_add(declaration.parameters.len())
                .saturating_add(declaration.identity.package.len()),
        )?;
        if !ids.insert(declaration.id)
            || !identities.insert(&declaration.identity)
            || module
                .life_declarations
                .iter()
                .any(|other| other.id == declaration.id || other.identity == declaration.identity)
        {
            return Err(bad(
                "dyn declaration collides with a nominal or dyn identity",
                None,
            ));
        }
        let mut member_names = BTreeSet::new();
        for member in &declaration.members {
            budget.spend(member.name.len().saturating_add(1))?;
            if member.name.is_empty()
                || !member_names.insert(&member.name)
                || matches!(&member.kind, DynMemberKind::Method(kinds) if kinds.len() > budget.limits.max_nodes)
            {
                return Err(bad(
                    "finite dyn declarations require named ordinary methods",
                    None,
                ));
            }
        }
    }
    let mut interfaces = BTreeSet::new();
    for item in &module.dyn_interfaces {
        crate::dynamic_metadata::interface(module, item, budget)?;
        budget.spend(1)?;
        if let Some(context) = &item.context {
            crate::dynamic_metadata::ty(context, budget)?;
        }
        crate::dynamic_cleanup::metadata(module, item.owning_cleanup.as_ref(), budget)?;
        crate::dynamic_metadata::retained_shape(&item.retained_life, budget)?;
        crate::dynamic_context::validate(module, item, budget)?;
        budget.spend(
            item.slots
                .len()
                .saturating_mul(8)
                .saturating_add(item.inherited.len())
                .saturating_add(item.associated_types.len())
                .saturating_add(item.associated_constants.len()),
        )?;
        if !interfaces.insert(item.id) {
            return Err(bad("duplicate dyn interface identity", None));
        }
        if item.context.is_some() {
            crate::dynamic_ports::validate_interface(module, item, budget)?;
            continue;
        }
        if item.slots.is_empty()
            || !item.inherited.is_empty()
            || !item.associated_types.is_empty()
            || !item.associated_constants.is_empty()
            || item.retained_life.binder_count != 0
            || !item.retained_life.ports.is_empty()
            || !item.retained_life.retained.is_empty()
            || !item.retained_life.nominal_arguments.is_empty()
            || !item.application.actuals.is_empty()
        {
            return Err(bad(
                "dyn interface exceeds the finite plain method profile",
                None,
            ));
        }
        let declaration = module
            .dyn_declarations
            .iter()
            .find(|d| d.id == item.application.declaration)
            .ok_or_else(|| bad("dyn trait declaration absent", None))?;
        if item.id.module != declaration.id.module
            || declaration.kind != DynDeclarationKind::Trait
            || !declaration.parameters.is_empty()
        {
            return Err(bad(
                "dyn interface must belong to its ordinary nongeneric trait declaration module",
                None,
            ));
        }
        let mut members = BTreeSet::new();
        for slot in &item.slots {
            for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
                if !ty.is_b0_scalar() {
                    check_type_shape(ty, &module.target, &budget.limits.clone(), budget)?;
                }
            }
            budget.spend(
                slot.parameters
                    .len()
                    .saturating_add(slot.parameter_modes.len())
                    .saturating_add(slot.actuals.len()),
            )?;
            let member = declaration
                .members
                .get(slot.member.ordinal as usize)
                .ok_or_else(|| bad("dyn slot member ordinal absent", None))?;
            let mut expected_life = StorageLifeContract {
                static_bindings: Vec::new(),
                fixed_binder_count: 0,
                invocation_binder_count: 1,
                bindings: vec![StorageLifeBinding {
                    position: receiver_position(),
                    binder: StorageBinder::Invocation(0),
                }],
                classes: vec![],
            };
            if borrowed_result(&slot.result) {
                expected_life.bindings.push(StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Result,
                        path: Vec::new(),
                    },
                    binder: StorageBinder::Invocation(0),
                });
            }
            if !slot.storage_life.as_ref().is_some_and(|life| {
                life.bindings.len() == if borrowed_result(&slot.result) { 2 } else { 1 }
                    && life.bindings[0].position.path.is_empty()
                    && life.classes.is_empty()
            }) {
                return Err(bad("dyn slot has an unbounded life relation", None));
            }
            if !members.insert(slot.member.ordinal)
                || slot.member.application != item.application
                || !matches!(&member.kind, DynMemberKind::Method(parameters) if parameters.is_empty())
                || !slot.actuals.is_empty()
                || slot.receiver.mode
                    != if slot.receiver.mutable {
                        PassingMode::BorrowMut
                    } else {
                        PassingMode::Borrow
                    }
                || slot.parameters.len() != slot.parameter_modes.len()
                || !slot.parameters.iter().all(plain_value)
                || !slot
                    .parameter_modes
                    .iter()
                    .all(|mode| *mode == PassingMode::Infer)
                || !(plain_value(&slot.result)
                    || !slot.receiver.mutable && borrowed_result(&slot.result))
                || slot.result_mode != PassingMode::Infer
                || slot.call_kind != CallKind::Direct
                || slot.storage_life.as_ref() != Some(&expected_life)
            {
                return Err(bad(
                    "dyn slot signature or member does not match finite shared dispatch",
                    None,
                ));
            }
            contract(slot, budget)?;
        }
        if members.len() != declaration.members.len() {
            return Err(bad("dyn interface omits an ordinary trait method", None));
        }
    }
    let mut tables = BTreeSet::new();
    for item in &module.dyn_vtables {
        if item.self_classes.len() > budget.limits.max_nodes {
            return Err(limit("dyn Self class inventory exceeds node limit"));
        }
        budget.spend(item.self_classes.len().saturating_add(1))?;
        budget.spend(
            item.entries
                .len()
                .saturating_mul(8)
                .saturating_add(item.actuals.len())
                .saturating_add(item.ports.len())
                .saturating_add(item.self_classes.len()),
        )?;
        if item.self_classes.len() > budget.limits.max_nodes {
            return Err(limit(
                "dyn Self class witness count exceeds selected node bound",
            ));
        }
        if !tables.insert(item.id) {
            return Err(bad("duplicate dyn table identity", None));
        }
        check_type_shape(
            &item.self_type,
            &module.target,
            &budget.limits.clone(),
            budget,
        )?;
        let iface = interface(module, item.interface)?;
        let implementation = module
            .dyn_declarations
            .iter()
            .find(|d| d.id == item.implementation)
            .ok_or_else(|| bad("dyn implementation declaration absent", None))?;
        if iface.context.is_some() {
            crate::dynamic_ports::validate_table(module, item, budget)?;
            if item.id.module != implementation.id.module
                || implementation.kind != DynDeclarationKind::Impl
                || item.owned_storage != Some(DynOwnedStorage::Heap)
                || item.cleanup.is_some()
                || item.entries.len() != iface.slots.len()
            {
                return Err(bad(
                    "Context provider table has an inconsistent implementation or storage route",
                    None,
                ));
            }
            crate::dynamic_ports::validate_entries(module, item, iface, budget)?;
            budget.spend(budget.pending_dyn_tables.len().saturating_add(1))?;
            budget.pending_dyn_tables.insert(item.id);
            continue;
        }
        if implementation
            .implemented_trait
            .as_ref()
            .is_none_or(|recipe| {
                recipe.declaration != iface.application.declaration || !recipe.arguments.is_empty()
            })
        {
            return Err(bad(
                "dyn provider has no proved implemented-trait application",
                None,
            ));
        }
        if item.id.module != implementation.id.module
            || implementation.kind != DynDeclarationKind::Impl
            || !implementation.parameters.is_empty()
            || !item.actuals.is_empty()
            || !item.ports.is_empty()
            || !item.self_classes.is_empty()
            || item.cleanup.is_some()
            || item.entries.len() != iface.slots.len()
            || !(matches!(&item.self_type, Type::Record { fields, .. }
                    if !fields.is_empty() && fields.iter().all(|(_, ty)| scalar(ty)))
                || item.owned_storage == Some(DynOwnedStorage::Heap)
                    && owned_provider(module, &item.self_type, budget)?)
        {
            return Err(bad(
                "dyn table exceeds finite Copy record provider profile",
                None,
            ));
        }
        crate::dynamic_patterns::provider(module, item, implementation, iface, budget)?;
        if item.owned_storage == Some(DynOwnedStorage::Heap) {
            let bound = crate::dynamic_cleanup::bound(iface.owning_cleanup.as_ref(), budget)?;
            crate::dynamic_cleanup::prove(module, &item.self_type, bound, budget)?;
        }
        for (ordinal, entry) in item.entries.iter().enumerate() {
            budget.spend(1)?;
            if entry.slot as usize != ordinal {
                return Err(bad("dyn table slot order or completeness differs", None));
            }
            let function = module
                .functions
                .get(entry.function.0 as usize)
                .ok_or_else(|| bad("dyn table entry function absent", None))?;
            if function.id != entry.function {
                return Err(bad("dyn entry function identity differs", None));
            }
            entry_body(
                module,
                function,
                &item.self_type,
                &iface.slots[ordinal],
                budget,
                &mut BTreeSet::new(),
                0,
            )?;
        }
    }
    Ok(())
}

fn entry_body(
    module: &TypedModule,
    function: &TypedFunction,
    self_type: &Type,
    slot: &DynSlot,
    budget: &mut Budget,
    active: &mut BTreeSet<FunctionId>,
    depth: usize,
) -> Check<()> {
    if depth > budget.limits.max_call_depth {
        return Err(limit(
            "dyn direct provider chain exceeds verification depth",
        ));
    }
    budget.spend(active.len().saturating_add(1))?;
    if !active.insert(function.id) {
        return Err(bad(
            "recursive dyn provider chain has unknown effects",
            Some(function.span),
        ));
    }
    budget.spend(
        function
            .parameters
            .len()
            .saturating_add(function.blocks.len())
            .saturating_add(function.values.len())
            .saturating_add(function.locals.len()),
    )?;
    let receiver_matches = |ty: &Type| matches!(ty, Type::Address { mutable, inner } if *mutable == slot.receiver.mutable && inner.as_ref() == self_type);
    if function.foreign.is_some()
        || function.environment.is_some()
        || function.call_kind != CallKind::Direct
        || function.parameters.len() != slot.parameters.len() + 1
        || function.return_type != slot.result
        || function.result_mode != slot.result_mode
        || !function
            .locals
            .get(function.parameters[0].0 as usize)
            .is_some_and(|l| receiver_matches(&l.ty))
        || function.blocks.len() != 1
    {
        return Err(bad(
            "dyn entry has wrong Self, ABI or callable role",
            Some(function.span),
        ));
    }
    for (local, ty) in function.parameters.iter().skip(1).zip(&slot.parameters) {
        if function.locals.get(local.0 as usize).map(|l| &l.ty) != Some(ty) {
            return Err(bad(
                "dyn entry argument type differs from slot",
                Some(function.span),
            ));
        }
    }
    if function.locals.iter().enumerate().any(|(index, local)| {
        (index != function.parameters[0].0 as usize && !plain_value(&local.ty))
            || local.mutable && !plain_value(&local.ty)
    }) {
        return Err(bad(
            "dyn entry retains a non-scalar local or mutable receiver",
            Some(function.span),
        ));
    }
    // The finite lane accepts bounded scalar reads and writes through the
    // exact receiver. Direct wrappers carry that receiver to a checked target.
    // Ownership, unknown effects and unproved arithmetic remain rejected.
    let fields = match self_type {
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields,
        _ => {
            return Err(bad(
                "dyn entry Self has no checked fields",
                Some(function.span),
            ));
        }
    };
    let mut receiver_values = BTreeSet::new();
    let mut field_values = BTreeSet::new();
    let mut saw_write = false;
    let mut direct_result = None;
    let mut called = false;
    for block in &function.blocks {
        budget.spend(block.instructions.len().saturating_add(1))?;
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Load(local) if *local == function.parameters[0] => {
                    if let Some(result) = instruction.result {
                        receiver_values.insert(result);
                    }
                }
                InstructionKind::ReborrowAddress {
                    address,
                    mutable: false,
                } if field_values.contains(address) => {
                    let input = function.values.get(address.0 as usize);
                    let output = instruction
                        .result
                        .and_then(|v| function.values.get(v.0 as usize));
                    if !matches!((input, output), (Some(Type::Address { inner: a, .. }), Some(Type::Address { mutable: false, inner: b })) if a == b && scalar(a))
                    {
                        return Err(bad(
                            "dyn borrowed field reborrow loses its exact field type",
                            Some(instruction.span),
                        ));
                    }
                    field_values.insert(instruction.result.expect("checked result"));
                }
                InstructionKind::ReborrowAddress { address, mutable } => {
                    if *mutable != slot.receiver.mutable
                        || !receiver_values.contains(address)
                        || !instruction.result.is_some_and(|result| {
                            function
                                .values
                                .get(result.0 as usize)
                                .is_some_and(receiver_matches)
                        })
                    {
                        return Err(bad(
                            "dyn default reborrow loses its exact receiver",
                            Some(instruction.span),
                        ));
                    }
                    receiver_values.insert(instruction.result.unwrap());
                }
                InstructionKind::AddressField { address, index } => {
                    if !receiver_values.contains(address)
                        || !fields
                            .get(*index as usize)
                            .is_some_and(|(_, ty)| scalar(ty))
                    {
                        return Err(bad(
                            "dyn entry reads outside its actual receiver",
                            Some(instruction.span),
                        ));
                    }
                    if let Some(result) = instruction.result {
                        field_values.insert(result);
                    }
                }
                InstructionKind::ReadAddress { address } => {
                    if !field_values.contains(address) {
                        return Err(bad(
                            "dyn entry has an unproved address read",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::WriteAddress { address, value } if slot.receiver.mutable => {
                    let address_ty = function.values.get(address.0 as usize);
                    let value_ty = function.values.get(value.0 as usize);
                    if !field_values.contains(address)
                        || !matches!(address_ty, Some(Type::Address { mutable: true, inner })
                            if inner.is_b0_scalar() && value_ty == Some(inner.as_ref()))
                    {
                        return Err(bad(
                            "mutable dyn entry writes outside its actual scalar Self field",
                            Some(instruction.span),
                        ));
                    }
                    saw_write = true;
                }
                InstructionKind::MakeAggregate { fields } => {
                    budget.spend(fields.len().saturating_add(1))?;
                    if !fields
                        .iter()
                        .all(|id| function.values.get(id.0 as usize).is_some_and(plain_value))
                        || !instruction.result.is_some_and(|id| {
                            function.values.get(id.0 as usize).is_some_and(plain_value)
                        })
                    {
                        return Err(bad(
                            "dyn aggregate has nonvalue fields or result",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::ExtractField { aggregate, .. } => {
                    if !function
                        .values
                        .get(aggregate.0 as usize)
                        .is_some_and(plain_value)
                        || !instruction.result.is_some_and(|id| {
                            function.values.get(id.0 as usize).is_some_and(plain_value)
                        })
                    {
                        return Err(bad(
                            "dyn aggregate projection has nonvalue result",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::Binary { left, right, .. } => {
                    if !slot
                        .contract
                        .effects
                        .as_ref()
                        .is_some_and(|e| e.contains(&DynEffect::Panic))
                        || ![*left, *right]
                            .iter()
                            .all(|id| function.values.get(id.0 as usize).is_some_and(scalar))
                        || !instruction.result.is_some_and(|id| {
                            function.values.get(id.0 as usize).is_some_and(scalar)
                        })
                    {
                        return Err(bad(
                            "dyn arithmetic lacks proved Panic upper bound or scalar operands",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::Unary {
                    op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                    operand,
                } => {
                    if !slot
                        .contract
                        .effects
                        .as_ref()
                        .is_some_and(|e| e.contains(&DynEffect::Panic))
                        || !function.values.get(operand.0 as usize).is_some_and(scalar)
                        || !instruction.result.is_some_and(|id| {
                            function.values.get(id.0 as usize).is_some_and(scalar)
                        })
                    {
                        return Err(bad(
                            "dyn unary operation lacks proved Panic upper bound",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::Call {
                    function: target,
                    arguments,
                } => {
                    if called
                        || !slot.parameters.is_empty()
                        || arguments.len() != 1
                        || !receiver_values.contains(&arguments[0])
                        || (slot.result == Type::Unit && instruction.result.is_some())
                        || (slot.result != Type::Unit && instruction.result.is_none())
                    {
                        return Err(bad(
                            "dyn default wrapper does not pass its exact receiver once",
                            Some(instruction.span),
                        ));
                    }
                    let callee = module
                        .functions
                        .get(target.0 as usize)
                        .filter(|callee| callee.id == *target)
                        .ok_or_else(|| {
                            bad(
                                "dyn default target FunctionId is absent",
                                Some(instruction.span),
                            )
                        })?;
                    entry_body(module, callee, self_type, slot, budget, active, depth + 1)?;
                    called = true;
                    direct_result = instruction.result;
                    if borrowed_result(&slot.result) {
                        field_values.insert(instruction.result.expect("checked non-Unit call"));
                    }
                    if slot.receiver.mutable {
                        saw_write = true;
                    }
                }
                InstructionKind::Constant(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. } => {}
                InstructionKind::Load(local)
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => {
                    let receiver_dead = matches!(&instruction.kind,
                        InstructionKind::StorageDead(id) if *id == function.parameters[0]);
                    if !receiver_dead
                        && !function
                            .locals
                            .get(local.0 as usize)
                            .is_some_and(|l| plain_value(&l.ty))
                    {
                        return Err(bad(
                            "dyn entry has an unproved local address access",
                            Some(instruction.span),
                        ));
                    }
                }
                InstructionKind::Store { local, .. } => {
                    if !function
                        .locals
                        .get(local.0 as usize)
                        .is_some_and(|l| plain_value(&l.ty))
                    {
                        return Err(bad(
                            "dyn entry writes a borrowed or owning local",
                            Some(instruction.span),
                        ));
                    }
                }
                _ => {
                    return Err(bad(
                        "dyn entry body has unproved access, cleanup or effect",
                        Some(instruction.span),
                    ));
                }
            }
        }
        if borrowed_result(&slot.result)
            && !matches!(block.terminator, Terminator::Return(Some(v)) if field_values.contains(&v))
        {
            return Err(bad(
                "dyn borrowed return does not originate at its actual receiver field",
                Some(function.span),
            ));
        }
        if !matches!(block.terminator, Terminator::Return(_)) {
            return Err(bad(
                "dyn entry has an unproved control-flow exit",
                Some(function.span),
            ));
        }
        if let Some(result) = direct_result
            && !matches!(block.terminator, Terminator::Return(Some(value)) if value == result)
        {
            return Err(bad(
                "dyn default wrapper return differs from its checked target",
                Some(function.span),
            ));
        }
        if called
            && slot.result == Type::Unit
            && !matches!(block.terminator, Terminator::Return(None))
        {
            return Err(bad(
                "dyn Unit wrapper has an unproved return",
                Some(function.span),
            ));
        }
    }
    if slot.receiver.mutable && !saw_write {
        return Err(bad(
            "mutable dyn entry has no proven receiver write",
            Some(function.span),
        ));
    }
    active.remove(&function.id);
    Ok(())
}

pub(super) fn type_address(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Address { inner, .. } = ty else {
        return Err(bad("dyn must be a shared address", span));
    };
    let Type::Dyn { interface: id } = inner.as_ref() else {
        return Err(bad("dyn address lacks its interface", span));
    };
    budget.spend(module.dyn_interfaces.len())?;
    interface(module, *id)?;
    Ok(())
}

pub(super) fn closed_owned(
    module: &TypedModule,
    id: DynInterfaceId,
    budget: &mut Budget,
) -> Check<()> {
    type_owned(module, id, budget)?;
    let actual = interface(module, id)?;
    if actual.context.is_some()
        || !actual.retained_life.ports.is_empty()
        || actual.retained_life.binder_count != 0
    {
        return Err(bad(
            "self-contained owner qualification retains external context",
            None,
        ));
    }
    Ok(())
}

pub(super) fn type_owned(
    module: &TypedModule,
    interface_id: DynInterfaceId,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(module.dyn_interfaces.len().saturating_add(1))?;
    let iface = interface(module, interface_id)?;
    if iface.context.is_some() {
        crate::dynamic_ports::validate_cleanup(iface, budget)?;
    } else {
        crate::dynamic_cleanup::bound(iface.owning_cleanup.as_ref(), budget)?;
    }
    Ok(())
}

pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.dyn:v1:DynCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

pub(super) fn instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match &instruction.kind {
        InstructionKind::MakeDynOwned {
            interface: id,
            vtable,
            source,
            route,
        } => {
            budget.spend(module.dyn_vtables.len().saturating_add(4))?;
            interface(module, *id)?;
            let selected = table(module, *vtable)?;
            if *route != DynOwnedStorage::Heap
                || selected.interface != *id
                || selected.owned_storage != Some(*route)
                || value_type(function, *source, span)? != &selected.self_type
                || if interface(module, *id)?.context.is_some() {
                    crate::dynamic_ports::validate_table(module, selected, budget)?;
                    false
                } else {
                    !owned_provider(module, &selected.self_type, budget)?
                }
            {
                return Err(bad(
                    "owned dyn construction lacks exact complete provider and Heap route",
                    span,
                ));
            }
            Ok(Some(Type::ErrorUnion {
                error: Box::new(create_error()),
                result: Box::new(Type::OwnedDyn { interface: *id }),
            }))
        }
        InstructionKind::BorrowDynOwned { local, mutable } => {
            budget.spend(function.locals.len().saturating_add(1))?;
            let actual = function
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("owned dyn borrow lacks its actual owner local", span))?;
            let (interface, exclusive) = match &actual.ty {
                Type::OwnedDyn { interface } => (*interface, actual.mutable),
                Type::Address { mutable, inner } => match inner.as_ref() {
                    Type::OwnedDyn { interface } => (*interface, *mutable),
                    _ => return Err(bad("owned dyn borrow address has no owner pointee", span)),
                },
                _ => return Err(bad("owned dyn borrow lacks its actual owner local", span)),
            };
            if *mutable && !exclusive {
                return Err(bad(
                    "exclusive owned dyn borrow needs a mutable owner local",
                    span,
                ));
            }
            type_owned(module, interface, budget)?;
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(Type::Dyn { interface }),
            }))
        }
        InstructionKind::DropDynOwned { local } => {
            budget.spend(function.locals.len().saturating_add(1))?;
            if !matches!(
                function.locals.get(local.0 as usize).map(|l| &l.ty),
                Some(Type::OwnedDyn { .. })
            ) {
                return Err(bad("owned dyn drop lacks its actual owner local", span));
            }
            Ok(None)
        }
        InstructionKind::MakeDynBorrow {
            interface: id,
            vtable,
            source,
            mutable,
        } => {
            budget.spend(3)?;
            interface(module, *id)?;
            let table = table(module, *vtable)?;
            if table.interface != *id
                || !matches!(value_type(function, *source, span)?,
                Type::Address { mutable: available, inner } if inner.as_ref() == &table.self_type && (!*mutable || *available))
            {
                return Err(bad(
                    "dyn construction source, Self or table interface differs",
                    span,
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(Type::Dyn { interface: *id }),
            }))
        }
        InstructionKind::CallDynSlot {
            callee,
            interface: id,
            slot,
            arguments,
        } => {
            budget.spend(arguments.len().saturating_add(3))?;
            let iface = interface(module, *id)?;
            let selected = iface
                .slots
                .get(*slot as usize)
                .ok_or_else(|| bad("dyn call slot absent", span))?;
            if !matches!(value_type(function, *callee, span)?, Type::Address { mutable,
                inner } if *mutable == selected.receiver.mutable && matches!(inner.as_ref(), Type::Dyn { interface } if interface == id))
                || selected.parameters.len() != arguments.len()
            {
                return Err(bad("dyn call receiver or arity differs", span));
            }
            for (argument, expected) in arguments.iter().zip(&selected.parameters) {
                if value_type(function, *argument, span)? != expected {
                    return Err(bad("dyn call argument type differs", span));
                }
            }
            Ok((selected.result != Type::Unit).then(|| selected.result.clone()))
        }
        _ => unreachable!(),
    }
}
