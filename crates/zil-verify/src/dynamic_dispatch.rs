//! Independent finite shared dyn checker. Raw tables and contracts grant no
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

fn bad(message: &str, span: Option<Span>) -> Diagnostic {
    reject("E-VERIFY-DYN", span, message)
}

fn receiver_position() -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![],
    }
}

fn interface<'a>(module: &'a TypedModule, id: DynInterfaceId) -> Check<&'a DynInterface> {
    module
        .dyn_interfaces
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| bad("dyn interface identity is absent", None))
}

fn table<'a>(module: &'a TypedModule, id: DynVtableId) -> Check<&'a DynVtable> {
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
        && c.call
            .as_ref()
            .is_some_and(|v| v.reads.len() == 1 && v.writes.is_empty())
        && c.cleanup.as_ref().is_some_and(|v| {
            !v.environment
                && v.positions.is_empty()
                && v.access.reads.is_empty()
                && v.access.writes.is_empty()
                && v.effects.is_empty()
        })
        && c.effects.as_ref().is_some_and(Vec::is_empty);
    if !finite {
        return Err(bad(
            "dyn contract has an unknown or unbounded component",
            None,
        ));
    }
    budget.spend(12)?;
    let position = receiver_position();
    let expected = DynContract {
        requires: Some(vec![DynRequirement::Read(position.clone())]),
        guarantees: Some(vec![]),
        hold: Some(vec![]),
        call: Some(DynCallAccess {
            reads: vec![position],
            writes: vec![],
        }),
        result: Some(DynResultSource::Value),
        cleanup: Some(DynCleanupContract {
            environment: false,
            positions: vec![],
            access: DynCallAccess {
                reads: vec![],
                writes: vec![],
            },
            effects: vec![],
        }),
        effects: Some(vec![]),
        resume: Some(DynResumeBound::Local),
        threads: Some(DynThreadRequirements {
            transfer: false,
            share: false,
        }),
    };
    if slot.contract != expected {
        return Err(bad(
            "dyn contract is unknown or differs from the independently derived finite read-only body contract",
            None,
        ));
    }
    Ok(())
}

fn scalar(ty: &Type) -> bool {
    ty.is_b0_scalar() || *ty == Type::Unit
}

/// Charge every raw list before cloning, fingerprinting or indexing it. This
/// branch is inactive for old modules, preserving their exact budget path.
pub(super) fn inventory(module: &TypedModule, budget: &mut Budget) -> Check<()> {
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
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for declaration in &module.dyn_declarations {
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
        for member in &declaration.members {
            budget.spend(member.name.len().saturating_add(1))?;
            if member.name.is_empty() || !matches!(member.kind, DynMemberKind::Method(_)) {
                return Err(bad(
                    "finite dyn declarations require named ordinary methods",
                    None,
                ));
            }
        }
    }
    let mut interfaces = BTreeSet::new();
    for item in &module.dyn_interfaces {
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
        if !item.inherited.is_empty()
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
        if declaration.kind != DynDeclarationKind::Trait || !declaration.parameters.is_empty() {
            return Err(bad(
                "dyn interface requires an ordinary nongeneric trait",
                None,
            ));
        }
        let mut members = BTreeSet::new();
        for slot in &item.slots {
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
            let expected_life = StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 1,
                bindings: vec![StorageLifeBinding {
                    position: receiver_position(),
                    binder: StorageBinder::Invocation(0),
                }],
                classes: vec![],
            };
            if !slot.storage_life.as_ref().is_some_and(|life| {
                life.bindings.len() == 1
                    && life.bindings[0].position.path.is_empty()
                    && life.classes.is_empty()
            }) {
                return Err(bad("dyn slot has an unbounded life relation", None));
            }
            if !members.insert(slot.member.ordinal)
                || slot.member.application != item.application
                || !matches!(&member.kind, DynMemberKind::Method(parameters) if parameters.is_empty())
                || !slot.actuals.is_empty()
                || slot.receiver.mutable
                || !matches!(slot.receiver.mode, PassingMode::Borrow)
                || slot.parameters.len() != slot.parameter_modes.len()
                || !slot.parameters.iter().all(scalar)
                || !slot
                    .parameter_modes
                    .iter()
                    .all(|mode| *mode == PassingMode::Infer)
                || !scalar(&slot.result)
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
        budget.spend(
            item.entries
                .len()
                .saturating_mul(8)
                .saturating_add(item.actuals.len())
                .saturating_add(item.ports.len()),
        )?;
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
        if implementation.kind != DynDeclarationKind::Impl
            || !implementation.parameters.is_empty()
            || !item.actuals.is_empty()
            || !item.ports.is_empty()
            || item.cleanup.is_some()
            || item.entries.len() != iface.slots.len()
            || !matches!(&item.self_type, Type::Record { fields, .. } if !fields.is_empty() && fields.iter().all(|(_, ty)| scalar(ty)))
        {
            return Err(bad(
                "dyn table exceeds finite Copy record provider profile",
                None,
            ));
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
            entry_body(function, &item.self_type, &iface.slots[ordinal], budget)?;
        }
    }
    Ok(())
}

fn entry_body(
    function: &TypedFunction,
    self_type: &Type,
    slot: &DynSlot,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(
        function
            .parameters
            .len()
            .saturating_add(function.blocks.len())
            .saturating_add(function.values.len())
            .saturating_add(function.locals.len()),
    )?;
    let receiver_matches = |ty: &Type| matches!(ty, Type::Address { mutable: false, inner } if inner.as_ref() == self_type);
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
        (index != function.parameters[0].0 as usize && !scalar(&local.ty))
            || local.mutable && !scalar(&local.ty)
    }) {
        return Err(bad(
            "dyn entry retains a non-scalar local or mutable receiver",
            Some(function.span),
        ));
    }
    // The first lane accepts one real scalar field read through the exact
    // receiver, with scalar-only local movement and arithmetic around it.
    // Calls, mutation, ownership and unknown effects are rejected even if a
    // public contract asserts an empty effect set.
    let Type::Record { fields, .. } = self_type else {
        unreachable!()
    };
    let mut receiver_values = BTreeSet::new();
    let mut field_values = BTreeSet::new();
    for block in &function.blocks {
        budget.spend(block.instructions.len().saturating_add(1))?;
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Load(local) if *local == function.parameters[0] => {
                    if let Some(result) = instruction.result {
                        receiver_values.insert(result);
                    }
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
                InstructionKind::Constant(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. } => {}
                InstructionKind::Load(local)
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => {
                    if !function
                        .locals
                        .get(local.0 as usize)
                        .is_some_and(|l| scalar(&l.ty))
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
                        .is_some_and(|l| scalar(&l.ty))
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
        if !matches!(block.terminator, Terminator::Return(_)) {
            return Err(bad(
                "dyn entry has an unproved control-flow exit",
                Some(function.span),
            ));
        }
    }
    Ok(())
}

pub(super) fn type_address(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Address {
        mutable: false,
        inner,
    } = ty
    else {
        return Err(bad("dyn must be a shared address", span));
    };
    let Type::Dyn { interface: id } = inner.as_ref() else {
        return Err(bad("dyn address lacks its interface", span));
    };
    budget.spend(module.dyn_interfaces.len())?;
    interface(module, *id)?;
    Ok(())
}

pub(super) fn instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match &instruction.kind {
        InstructionKind::MakeDynBorrow {
            interface: id,
            vtable,
            source,
            mutable,
        } => {
            budget.spend(3)?;
            if *mutable {
                return Err(bad(
                    "mutable dyn construction is outside shared profile",
                    span,
                ));
            }
            interface(module, *id)?;
            let table = table(module, *vtable)?;
            if table.interface != *id
                || !matches!(value_type(function, *source, span)?,
                Type::Address { mutable: false, inner } if inner.as_ref() == &table.self_type)
            {
                return Err(bad(
                    "dyn construction source, Self or table interface differs",
                    span,
                ));
            }
            Ok(Some(Type::Address {
                mutable: false,
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
            if !matches!(value_type(function, *callee, span)?, Type::Address { mutable: false,
                inner } if matches!(inner.as_ref(), Type::Dyn { interface } if interface == id))
                || selected.parameters.len() != arguments.len()
            {
                return Err(bad("dyn call receiver or arity differs", span));
            }
            for (argument, expected) in arguments.iter().zip(&selected.parameters) {
                if value_type(function, *argument, span)? != expected {
                    return Err(bad("dyn call argument type differs", span));
                }
            }
            Ok(Some(selected.result.clone()))
        }
        _ => unreachable!(),
    }
}
