//! Independent, finite borrowed-dyn admission. Raw tables and producer contracts
//! are inventory only: entry signatures, body reads and borrow sources are
//! reconstructed here before the ordinary Hold walk consumes the instructions.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use zil_base::CompileResult;
use zil_ir::*;

use crate::{bad, budget::Budget};

const WORK: &str = "finite shared dyn admission";

pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.dyn:v1:DynCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
    )
}

fn closed_owner_shape(
    ty: &Type,
    module: &TypedModule,
    budget: &mut Budget,
    depth: usize,
) -> CompileResult<Option<usize>> {
    if depth > budget.partial_depth {
        return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "owned dyn shape depth exceeded",
            None,
        )]);
    }
    budget.spend(1, WORK)?;
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(Some(0));
    }
    let mut owners = 0usize;
    match ty {
        Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer => {
            budget.spend(module.resources.len(), WORK)?;
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
            if *length > 0 && (*length as usize) <= budget.helper_nodes =>
        {
            budget.spend(*length as usize, WORK)?;
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
                    vec![zil_base::Diagnostic::limit(
                        zil_base::Phase::Analyze,
                        "owned dyn owner count exceeded",
                        None,
                    )]
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

fn shared(ty: &Type, pointee: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if inner.as_ref() == pointee)
}

fn position() -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(0),
        path: Vec::new(),
    }
}

fn closed_contract(mutable: bool) -> DynContract {
    let receiver = position();
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

fn receiver_life() -> StorageLifeContract {
    StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![StorageLifeBinding {
            position: position(),
            binder: StorageBinder::Invocation(0),
        }],
        classes: Vec::new(),
    }
}

fn scalar_mode(mode: PassingMode) -> bool {
    matches!(mode, PassingMode::Copy | PassingMode::Infer)
}

/// Preflight's sole new inventory walk. Empty legacy modules return without a
/// charge; all table nodes are paid before any collection or lookup.
pub(super) fn inventory(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if module.dyn_declarations.is_empty()
        && module.dyn_interfaces.is_empty()
        && module.dyn_vtables.is_empty()
    {
        return Ok(());
    }
    if !module.target.is_supported_x64_llvm() {
        return Err(bad("finite shared dyn requires a checked x64 LLVM target"));
    }
    budget.spend(
        module
            .dyn_declarations
            .len()
            .saturating_add(module.dyn_interfaces.len())
            .saturating_add(module.dyn_vtables.len()),
        WORK,
    )?;
    let mut declarations = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for declaration in &module.dyn_declarations {
        budget.spend(
            declaration
                .identity
                .module_path
                .len()
                .saturating_add(declaration.identity.declaration_path.len())
                .saturating_add(declaration.identity.generation_path.len())
                .saturating_add(declaration.members.len()),
            WORK,
        )?;
        budget.spend(
            declaration
                .members
                .len()
                .saturating_add(declaration.parameters.len())
                .saturating_add(declaration.identity.package.len())
                .saturating_add(
                    declaration
                        .identity
                        .module_path
                        .iter()
                        .fold(0usize, |n, s| n.saturating_add(s.len())),
                )
                .saturating_add(
                    declaration
                        .identity
                        .declaration_path
                        .iter()
                        .fold(0usize, |n, s| n.saturating_add(s.len())),
                )
                .saturating_add(declaration.identity.generation_path.len())
                .saturating_add(
                    declaration
                        .members
                        .iter()
                        .fold(0usize, |n, m| n.saturating_add(m.name.len())),
                )
                .saturating_add(1),
            WORK,
        )?;
        budget.lookup(declarations.len(), WORK)?;
        budget.lookup(identities.len(), WORK)?;
        if declarations.insert(declaration.id, declaration).is_some()
            || !identities.insert(declaration.identity.clone())
        {
            return Err(bad("dyn declaration identity is repeated"));
        }
        if !declaration.parameters.is_empty()
            || declaration
                .members
                .iter()
                .any(|m| !matches!(&m.kind, DynMemberKind::Method(actuals) if actuals.is_empty()))
        {
            return Err(bad(
                "dyn declaration is outside finite ordinary method profile",
            ));
        }
        let mut member_names = BTreeSet::new();
        for member in &declaration.members {
            budget.lookup(member_names.len(), WORK)?;
            if member.name.is_empty() || !member_names.insert(member.name.as_str()) {
                return Err(bad("dyn declaration has repeated or empty member identity"));
            }
        }
    }
    let mut interfaces = BTreeSet::new();
    for interface in &module.dyn_interfaces {
        budget.lookup(declarations.len(), WORK)?;
        if interface.id.module != interface.application.declaration.module
            || !interfaces.insert(interface.id)
            || !declarations
                .get(&interface.application.declaration)
                .is_some_and(|d| d.kind == DynDeclarationKind::Trait)
        {
            return Err(bad("dyn interface has no unique actual trait declaration"));
        }
        budget.spend(interface.slots.len().saturating_add(1), WORK)?;
        if !interface.inherited.is_empty()
            || !interface.associated_types.is_empty()
            || !interface.associated_constants.is_empty()
            || !interface.application.actuals.is_empty()
            || interface.retained_life.binder_count != 0
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || !interface.retained_life.nominal_arguments.is_empty()
        {
            return Err(bad("dyn interface is outside the finite shared profile"));
        }
        for slot in &interface.slots {
            budget.spend(
                slot.parameters
                    .len()
                    .saturating_add(slot.parameter_modes.len())
                    .saturating_add(24),
                WORK,
            )?;
            if slot.member.application != interface.application
                || !slot.actuals.is_empty()
                || slot.receiver.mode
                    != if slot.receiver.mutable {
                        PassingMode::BorrowMut
                    } else {
                        PassingMode::Borrow
                    }
                || !slot.parameters.iter().all(scalar)
                || slot.parameter_modes.iter().any(|m| !scalar_mode(*m))
                || slot.parameters.len() != slot.parameter_modes.len()
                || !scalar(&slot.result)
                || !scalar_mode(slot.result_mode)
                || slot.call_kind != CallKind::Direct
                || slot.storage_life.as_ref() != Some(&receiver_life())
                || slot.contract != closed_contract(slot.receiver.mutable)
            {
                return Err(bad("dyn slot lacks a body-derived closed shared contract"));
            }
            budget.lookup(declarations.len(), WORK)?;
            let declaration = declarations
                .get(&slot.member.application.declaration)
                .ok_or_else(|| bad("dyn slot trait declaration absent"))?;
            if !matches!(declaration.members.get(slot.member.ordinal as usize), Some(DynMemberDeclaration { kind: DynMemberKind::Method(actuals), .. }) if actuals.is_empty())
            {
                return Err(bad("dyn slot does not select an actual ordinary method"));
            }
        }
    }
    if interfaces.is_empty() && !module.dyn_vtables.is_empty() {
        return Err(bad("dyn tables require an admitted interface"));
    }
    budget.dyn_interfaces = interfaces;
    Ok(())
}

fn value<'a>(f: &'a TypedFunction, id: ValueId, budget: &mut Budget) -> CompileResult<&'a Type> {
    budget.lookup(f.values.len(), WORK)?;
    f.values
        .get(id.0 as usize)
        .ok_or_else(|| bad("dyn operand is outside actual value table"))
}

fn body(
    module: &TypedModule,
    f: &TypedFunction,
    receiver: &Type,
    mutable: bool,
    result: &Type,
    visiting: &mut BTreeSet<FunctionId>,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.lookup(visiting.len(), WORK)?;
    if visiting.len() >= budget.helper_depth || !visiting.insert(f.id) {
        return Err(bad(
            "dyn helper call chain is cyclic or exceeds finite depth",
        ));
    }
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.parameters.is_empty()
        || f.blocks.is_empty()
        || &f.return_type != result
        || !scalar_mode(f.result_mode)
    {
        return Err(bad("dyn entry is not a finite ordinary body"));
    }
    let first = f.parameters[0];
    if !f.locals.get(first.0 as usize).is_some_and(|l| {
        matches!(&l.ty, Type::Address { mutable: actual, inner }
            if *actual == mutable && inner.as_ref() == receiver)
    }) {
        return Err(bad("dyn entry actual Self differs from table Self"));
    }
    budget.spend(1, WORK)?;
    let mut queue = VecDeque::from([f.entry]);
    let mut seen = BTreeSet::new();
    let mut receiver_values = BTreeSet::new();
    let mut field_values = BTreeMap::new();
    let mut value_results = BTreeSet::new();
    let mut saw_return = false;
    let mut saw_write = false;
    let mut called = false;
    while let Some(id) = queue.pop_front() {
        budget.lookup(seen.len(), WORK)?;
        if !seen.insert(id) {
            continue;
        }
        let block = f
            .blocks
            .get(id.0 as usize)
            .ok_or_else(|| bad("dyn entry has missing reachable block"))?;
        if block.id != id {
            return Err(bad("dyn entry reachable block identity differs"));
        }
        budget.spend(block.instructions.len().saturating_add(1), WORK)?;
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Load(local) if *local == first => {
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn receiver load lacks result"))?;
                    if !matches!(value(f, result, budget)?, Type::Address { mutable: actual, inner }
                        if *actual == mutable && inner.as_ref() == receiver)
                    {
                        return Err(bad("dyn receiver load type differs"));
                    }
                    receiver_values.insert(result);
                }
                InstructionKind::Load(local)
                    if f.locals
                        .get(local.0 as usize)
                        .is_some_and(|l| scalar(&l.ty)) =>
                {
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn scalar load lacks result"))?;
                    if value(f, result, budget)? != &f.locals[local.0 as usize].ty {
                        return Err(bad("dyn scalar load type differs"));
                    }
                    value_results.insert(result);
                }
                InstructionKind::ReborrowAddress {
                    address,
                    mutable: reborrow,
                } if receiver_values.contains(address) => {
                    if *reborrow != mutable {
                        return Err(bad("dyn receiver reborrow changes access permission"));
                    }
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn receiver reborrow lacks result"))?;
                    if !shared(value(f, result, budget)?, receiver) {
                        return Err(bad("dyn receiver reborrow type differs"));
                    }
                    receiver_values.insert(result);
                }
                InstructionKind::AddressField { address, index }
                    if receiver_values.contains(address) =>
                {
                    let fields = match receiver {
                        Type::Record { fields, .. } | Type::Package { fields, .. } => fields,
                        _ => return Err(bad("dyn Self is not a finite provider")),
                    };
                    let field = fields
                        .get(*index as usize)
                        .ok_or_else(|| bad("dyn receiver field is absent"))?;
                    if !scalar(&field.1) {
                        return Err(bad("dyn receiver field is not scalar"));
                    }
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn field address lacks result"))?;
                    if !matches!(value(f, result, budget)?, Type::Address { mutable: actual, inner }
                        if *actual == mutable && inner.as_ref() == &field.1)
                    {
                        return Err(bad("dyn field address type differs"));
                    }
                    field_values.insert(result, field.1.clone());
                }
                InstructionKind::ReadAddress { address } if field_values.contains_key(address) => {
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn field read lacks result"))?;
                    if value(f, result, budget)? != field_values.get(address).unwrap() {
                        return Err(bad("dyn field read result differs from actual field"));
                    }
                    value_results.insert(result);
                }
                InstructionKind::WriteAddress {
                    address,
                    value: written,
                } if mutable && field_values.contains_key(address) => {
                    if value(f, *written, budget)? != field_values.get(address).unwrap() {
                        return Err(bad("dyn write value differs from actual scalar Self field"));
                    }
                    saw_write = true;
                }
                InstructionKind::Constant(constant) if scalar(&constant.ty()) => {
                    // A body need not exercise the slot's shared Read upper
                    // bound. A literal proves an empty actual access set.
                    let result = instruction
                        .result
                        .ok_or_else(|| bad("dyn scalar literal lacks result"))?;
                    if value(f, result, budget)? != &constant.ty() {
                        return Err(bad("dyn scalar literal actual type differs"));
                    }
                    value_results.insert(result);
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len().saturating_add(1), WORK)?;
                    if called
                        || f.parameters.len() != 1
                        || arguments.len() != 1
                        || !receiver_values.contains(&arguments[0])
                    {
                        return Err(bad(
                            "dyn helper call does not pass through the actual shared receiver",
                        ));
                    }
                    called = true;
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| bad("dyn helper actual FunctionId is absent"))?;
                    if target.id != *function
                        || target.parameters.len() != 1
                        || target.return_type != f.return_type
                        || target.result_mode != f.result_mode
                        || (result == &Type::Unit && instruction.result.is_some())
                        || (result != &Type::Unit
                            && !instruction.result.is_some_and(|output| {
                                f.values.get(output.0 as usize) == Some(&target.return_type)
                            }))
                        || !target.parameters.iter().all(|id| {
                            target.locals.get(id.0 as usize).is_some_and(|l| {
                                matches!(&l.ty, Type::Address { mutable: actual, inner }
                                if *actual == mutable && inner.as_ref() == receiver)
                            })
                        })
                    {
                        return Err(bad("dyn helper actual entry signature differs"));
                    }
                    body(module, target, receiver, mutable, result, visiting, budget)?;
                    if let Some(output) = instruction.result {
                        value_results.insert(output);
                    }
                    if mutable {
                        saw_write = true;
                    }
                }
                InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_) => {}
                _ => {
                    return Err(bad(
                        "dyn entry reachable body has unproved access or effect",
                    ));
                }
            }
        }
        match &block.terminator {
            Terminator::Return(None) if *result == Type::Unit => saw_return = true,
            Terminator::Return(Some(result))
                if value_results.contains(result) && scalar(value(f, *result, budget)?) =>
            {
                saw_return = true
            }
            Terminator::Jump(next) => {
                budget.spend(1, WORK)?;
                queue.push_back(*next);
            }
            _ => {
                return Err(bad(
                    "dyn entry reachable exit is outside scalar read profile",
                ));
            }
        }
    }
    if !saw_return || (mutable && !saw_write) {
        return Err(bad("dyn entry has no proven scalar receiver read return"));
    }
    visiting.remove(&f.id);
    Ok(())
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if budget.dyn_interfaces.is_empty() {
        return Ok(());
    }
    let mut tables = BTreeMap::new();
    for table in &module.dyn_vtables {
        budget.lookup(tables.len(), WORK)?;
        if table.id.module != table.implementation.module
            || tables.insert(table.id, table).is_some()
        {
            return Err(bad("dyn vtable identity is repeated"));
        }
        budget.spend(
            module
                .dyn_interfaces
                .len()
                .saturating_add(module.dyn_declarations.len())
                .saturating_add(table.entries.len())
                .saturating_add(1),
            WORK,
        )?;
        let interface = module
            .dyn_interfaces
            .iter()
            .find(|i| i.id == table.interface)
            .ok_or_else(|| bad("dyn vtable interface is absent"))?;
        let (name, fields, owning_package) = match &table.self_type {
            Type::Record { name, fields } => (name, fields, false),
            Type::Package { name, fields } => (name, fields, true),
            _ => return Err(bad("dyn table Self is not a concrete provider")),
        };
        budget.spend(
            name.len().saturating_add(fields.len()).saturating_add(1),
            WORK,
        )?;
        let mut names = BTreeSet::new();
        if name.is_empty() || fields.is_empty() {
            return Err(bad("dyn table Self lacks finite nominal storage"));
        }
        for (field, ty) in fields {
            budget.spend(field.len().saturating_add(1), WORK)?;
            budget.lookup(names.len(), WORK)?;
            budget.spend(module.resources.len(), WORK)?;
            let admissible = if owning_package {
                closed_owner_shape(ty, module, budget, 1)?.is_some()
            } else {
                scalar(ty)
            };
            if field.is_empty() || !admissible || !names.insert(field) {
                return Err(bad("dyn table Self has an invalid field"));
            }
        }
        if owning_package
            && (table.owned_storage != Some(DynOwnedStorage::Heap)
                || closed_owner_shape(&table.self_type, module, budget, 0)?.is_none())
        {
            return Err(bad(
                "owning dyn Package lacks Heap route or real resource debt",
            ));
        }
        if !module
            .dyn_declarations
            .iter()
            .any(|d| d.id == table.implementation && d.kind == DynDeclarationKind::Impl)
            || !table.actuals.is_empty()
            || !table.ports.is_empty()
            || table.cleanup.is_some()
            || table.entries.len() != interface.slots.len()
        {
            return Err(bad("dyn vtable is outside finite concrete shared profile"));
        }
        let mut entry_slots = BTreeSet::new();
        for entry in &table.entries {
            budget.lookup(entry_slots.len(), WORK)?;
            if !entry_slots.insert(entry.slot) {
                return Err(bad("dyn vtable slot repeated"));
            }
            let slot = interface
                .slots
                .get(entry.slot as usize)
                .ok_or_else(|| bad("dyn vtable slot is absent"))?;
            let f = module
                .functions
                .get(entry.function.0 as usize)
                .ok_or_else(|| bad("dyn vtable entry function absent"))?;
            budget.spend(
                f.parameters
                    .len()
                    .saturating_add(slot.parameters.len())
                    .saturating_add(1),
                WORK,
            )?;
            if f.id != entry.function
                || f.return_type != slot.result
                || f.result_mode != slot.result_mode
                || f.parameters.len() != slot.parameters.len() + 1
                || !f
                    .parameters
                    .iter()
                    .skip(1)
                    .zip(&slot.parameters)
                    .all(|(id, ty)| f.locals.get(id.0 as usize).is_some_and(|l| &l.ty == ty))
            {
                return Err(bad("dyn entry actual signature differs from selected slot"));
            }
            body(
                module,
                f,
                &table.self_type,
                slot.receiver.mutable,
                &slot.result,
                &mut BTreeSet::new(),
                budget,
            )?;
        }
    }
    for f in &module.functions {
        budget.spend(1, WORK)?;
        let mut definitions = BTreeMap::new();
        for block in &f.blocks {
            budget.spend(block.instructions.len().saturating_add(1), WORK)?;
            for instruction in &block.instructions {
                if let Some(id) = instruction.result {
                    budget.lookup(definitions.len(), WORK)?;
                    if definitions.insert(id, &instruction.kind).is_some() {
                        return Err(bad("dyn function has repeated actual value definition"));
                    }
                }
            }
        }
        for block in &f.blocks {
            for instruction in &block.instructions {
                match &instruction.kind {
                    InstructionKind::MakeDynBorrow {
                        interface,
                        vtable,
                        source,
                        mutable,
                    } => {
                        budget.spend(1, WORK)?;
                        let table = tables
                            .get(vtable)
                            .ok_or_else(|| bad("dyn borrow table is absent"))?;
                        let result = instruction
                            .result
                            .ok_or_else(|| bad("dyn borrow lacks result"))?;
                        if table.interface != *interface
                            || !shared(value(f, *source, budget)?, &table.self_type)
                            || !matches!(value(f, *source, budget)?, Type::Address { mutable: available, .. } if !*mutable || *available)
                            || !shared(
                                value(f, result, budget)?,
                                &Type::Dyn {
                                    interface: *interface,
                                },
                            )
                            || !matches!(value(f, result, budget)?, Type::Address { mutable: actual, .. } if actual == mutable)
                        {
                            return Err(bad("dyn borrow actual source, table or result differs"));
                        }
                        // A direct receiver origin is required. Hold independently
                        // carries its root through the resulting fat address.
                        budget.lookup(definitions.len(), WORK)?;
                        if !matches!(
                            definitions.get(source),
                            Some(
                                InstructionKind::BorrowLocal { .. }
                                    | InstructionKind::BorrowLocalMut { .. }
                                    | InstructionKind::ReborrowAddress { .. }
                            )
                        ) {
                            return Err(bad("dyn borrow source lacks an actual shared loan"));
                        }
                    }
                    InstructionKind::CallDynSlot {
                        callee,
                        interface,
                        slot,
                        arguments,
                    } => {
                        budget.spend(arguments.len().saturating_add(1), WORK)?;
                        let selected = module
                            .dyn_interfaces
                            .iter()
                            .find(|i| i.id == *interface)
                            .and_then(|i| i.slots.get(*slot as usize))
                            .ok_or_else(|| bad("dyn call slot absent"))?;
                        if !matches!(value(f, *callee, budget)?, Type::Address { mutable, inner }
                            if *mutable == selected.receiver.mutable && matches!(inner.as_ref(), Type::Dyn { interface: actual } if actual == interface))
                            || arguments.len() != selected.parameters.len()
                            || !arguments
                                .iter()
                                .zip(&selected.parameters)
                                .all(|(id, ty)| f.values.get(id.0 as usize) == Some(ty))
                            || (selected.result != Type::Unit
                                && instruction
                                    .result
                                    .and_then(|id| f.values.get(id.0 as usize))
                                    != Some(&selected.result))
                            || (selected.result == Type::Unit && instruction.result.is_some())
                        {
                            return Err(bad("dyn call actual callee, arguments or result differs"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// The ordinary Hold analysis reconstructs actual SSA/local provenance and
/// windows. Its candidate facts are used here only for Analyze's own final
/// grounding; Verify rebuilds both facts independently.
pub(super) fn check_roots(
    module: &TypedModule,
    plans: &[FunctionPlan],
    budget: &mut Budget,
) -> CompileResult<()> {
    if budget.dyn_interfaces.is_empty() {
        return Ok(());
    }
    for (f, plan) in module.functions.iter().zip(plans) {
        budget.spend(f.blocks.len().saturating_add(1), WORK)?;
        if matches!(&f.return_type, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. }))
        {
            return Err(bad(
                "dyn borrowed carrier return is outside the finite scalar-result profile",
            ));
        }
        for block in &f.blocks {
            budget.spend(block.instructions.len(), WORK)?;
            for instruction in &block.instructions {
                let roots = |id: ValueId| -> CompileResult<&[HoldRoot]> {
                    let hold = plan
                        .holds
                        .get(id.0 as usize)
                        .ok_or_else(|| bad("dyn value has no actual Hold fact"))?;
                    if hold.value != id {
                        return Err(bad("dyn Hold identity differs"));
                    }
                    Ok(&hold.roots)
                };
                match instruction.kind {
                    InstructionKind::MakeDynBorrow { source, .. } => {
                        let result = instruction
                            .result
                            .ok_or_else(|| bad("dyn borrow result absent"))?;
                        budget.spend(
                            roots(source)?.len().saturating_add(roots(result)?.len()),
                            WORK,
                        )?;
                        if roots(source)?.is_empty() || roots(source)? != roots(result)? {
                            return Err(bad("dyn borrow does not retain its actual source root"));
                        }
                    }
                    InstructionKind::CallDynSlot { callee, .. } => {
                        budget.spend(roots(callee)?.len(), WORK)?;
                        if roots(callee)?.is_empty() {
                            return Err(bad("dyn call callee has no actual borrow root"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile};

    fn inst(result: Option<u32>, kind: InstructionKind) -> Instruction {
        Instruction {
            result: result.map(ValueId),
            kind,
            span: Span::default(),
        }
    }

    fn local(id: u32, ty: Type) -> LocalDecl {
        LocalDecl {
            id: LocalId(id),
            name: format!("l{id}"),
            ty,
            mutable: false,
            span: Span::default(),
        }
    }

    fn fixture() -> TypedModule {
        let owner = Type::Record {
            name: "A".into(),
            fields: vec![("n".into(), Type::I32)],
        };
        let address = Type::Address {
            mutable: false,
            inner: Box::new(owner.clone()),
        };
        let interface_id = DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        };
        let table_id = DynVtableId {
            module: ModuleId(0),
            index: 0,
        };
        let dyn_address = Type::Address {
            mutable: false,
            inner: Box::new(Type::Dyn {
                interface: interface_id,
            }),
        };
        let trait_id = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(1),
            compiler: false,
        };
        let impl_id = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(2),
            compiler: false,
        };
        let application = DynTraitApplication {
            declaration: trait_id,
            actuals: vec![],
        };
        let identity = |name: &str| LifeDeclarationIdentity {
            package: "p".into(),
            module_path: vec!["m".into()],
            declaration_path: vec![name.into()],
            generation_path: vec![],
        };
        let entry = TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(0),
            name: "A.read".into(),
            foreign: None,
            environment: None,
            parameters: vec![LocalId(0)],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![local(0, address.clone())].into(),
            values: vec![
                address.clone(),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                Type::I32,
            ]
            .into(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    inst(Some(0), InstructionKind::Load(LocalId(0))),
                    inst(
                        Some(1),
                        InstructionKind::AddressField {
                            address: ValueId(0),
                            index: 0,
                        },
                    ),
                    inst(
                        Some(2),
                        InstructionKind::ReadAddress {
                            address: ValueId(1),
                        },
                    ),
                ],
                terminator: Terminator::Return(Some(ValueId(2))),
            }]
            .into(),
            entry: BlockId(0),
            span: Span::default(),
        };
        let caller = TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(1),
            name: "use_dyn".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![local(0, owner.clone())].into(),
            values: vec![address, dyn_address, Type::I32].into(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    inst(Some(0), InstructionKind::BorrowLocal { local: LocalId(0) }),
                    inst(
                        Some(1),
                        InstructionKind::MakeDynBorrow {
                            interface: interface_id,
                            vtable: table_id,
                            source: ValueId(0),
                            mutable: false,
                        },
                    ),
                    inst(
                        Some(2),
                        InstructionKind::CallDynSlot {
                            callee: ValueId(1),
                            interface: interface_id,
                            slot: 0,
                            arguments: vec![],
                        },
                    ),
                ],
                terminator: Terminator::Return(Some(ValueId(2))),
            }]
            .into(),
            entry: BlockId(0),
            span: Span::default(),
        };
        TypedModule {
            dyn_declarations: vec![
                DynDeclaration {
                    id: trait_id,
                    identity: identity("Read"),
                    kind: DynDeclarationKind::Trait,
                    parameters: vec![],
                    members: vec![DynMemberDeclaration {
                        name: "read".into(),
                        kind: DynMemberKind::Method(vec![]),
                    }],
                },
                DynDeclaration {
                    id: impl_id,
                    identity: identity("impl_Read_A"),
                    kind: DynDeclarationKind::Impl,
                    parameters: vec![],
                    members: vec![DynMemberDeclaration {
                        name: "read".into(),
                        kind: DynMemberKind::Method(vec![]),
                    }],
                },
            ],
            dyn_interfaces: vec![DynInterface {
                id: interface_id,
                application: application.clone(),
                inherited: vec![],
                associated_types: vec![],
                associated_constants: vec![],
                retained_life: RetainedLifeShape {
                    binder_count: 0,
                    ports: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                slots: vec![DynSlot {
                    member: DynMember {
                        application,
                        ordinal: 0,
                    },
                    actuals: vec![],
                    receiver: DynReceiver {
                        mutable: false,
                        mode: PassingMode::Borrow,
                    },
                    parameters: vec![],
                    parameter_modes: vec![],
                    result: Type::I32,
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                    storage_life: Some(receiver_life()),
                    contract: closed_contract(false),
                }],
            }],
            dyn_vtables: vec![DynVtable {
                id: table_id,
                interface: interface_id,
                self_type: owner,
                implementation: impl_id,
                actuals: vec![],
                ports: vec![],
                entries: vec![DynEntry {
                    slot: 0,
                    function: FunctionId(0),
                }],
                cleanup: None,
                owned_storage: None,
            }],
            life_declarations: vec![],
            life_instances: vec![],
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            functions: vec![entry, caller],
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        }
    }

    fn admitted(module: &TypedModule) -> CompileResult<()> {
        let mut budget = Budget::new(&Limits::default());
        inventory(module, &mut budget)?;
        check(module, &mut budget)
    }

    #[test]
    fn finite_entry_and_actual_borrow_are_admitted() {
        admitted(&fixture()).unwrap();
    }

    #[test]
    fn same_layout_wrong_self_and_unknown_contract_are_rejected() {
        let mut m = fixture();
        m.functions[0].locals[0].ty = Type::Address {
            mutable: false,
            inner: Box::new(Type::Record {
                name: "B".into(),
                fields: vec![("n".into(), Type::I32)],
            }),
        };
        assert!(admitted(&m).is_err());
        let mut m = fixture();
        m.dyn_interfaces[0].slots[0].contract.effects = None;
        assert!(admitted(&m).is_err());
    }

    #[test]
    fn literal_body_has_empty_actual_access_but_unknown_call_and_wrong_slot_fail() {
        let mut m = fixture();
        m.functions[0].blocks[0].instructions =
            vec![inst(Some(2), InstructionKind::Constant(Constant::I32(17)))];
        admitted(&m).unwrap();
        m.functions[0].blocks[0].instructions.insert(
            0,
            inst(
                None,
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![],
                },
            ),
        );
        assert!(admitted(&m).is_err());
        let mut m = fixture();
        m.dyn_vtables[0].entries[0].slot = 1;
        assert!(admitted(&m).is_err());
    }

    #[test]
    fn wrapper_follows_actual_function_id_and_receiver_with_cycle_closed() {
        let mut m = fixture();
        let mut helper = m.functions[0].clone();
        helper.id = FunctionId(2);
        helper.name = "unrelated_safe_helper".into();
        helper.blocks[0].instructions =
            vec![inst(Some(2), InstructionKind::Constant(Constant::I32(9)))];
        m.functions.push(helper);
        m.functions[0].blocks[0].instructions = vec![
            inst(Some(0), InstructionKind::Load(LocalId(0))),
            inst(
                Some(2),
                InstructionKind::Call {
                    function: FunctionId(2),
                    arguments: vec![ValueId(0)],
                },
            ),
        ];
        admitted(&m).unwrap();

        let mut wrong_receiver = m.clone();
        if let InstructionKind::Call { arguments, .. } =
            &mut wrong_receiver.functions[0].blocks[0].instructions[1].kind
        {
            arguments[0] = ValueId(1);
        }
        assert!(admitted(&wrong_receiver).is_err());

        let mut cyclic = m.clone();
        cyclic.functions[2].blocks[0].instructions = vec![
            inst(Some(0), InstructionKind::Load(LocalId(0))),
            inst(
                Some(2),
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![ValueId(0)],
                },
            ),
        ];
        assert!(admitted(&cyclic).is_err());

        let mut unknown = m;
        unknown.functions[2].call_kind = CallKind::Suspend;
        assert!(admitted(&unknown).is_err());
    }

    #[test]
    fn forged_source_and_unused_bad_table_are_rejected() {
        let mut m = fixture();
        m.functions[1].blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(7));
        assert!(admitted(&m).is_err());
        let mut m = fixture();
        if let InstructionKind::CallDynSlot { interface, .. } =
            &mut m.functions[1].blocks[0].instructions[2].kind
        {
            interface.index = 1;
        }
        assert!(admitted(&m).is_err());
        let mut m = fixture();
        let mut bad = m.dyn_vtables[0].clone();
        bad.id.index = 1;
        bad.entries[0].function = FunctionId(99);
        m.dyn_vtables.push(bad);
        assert!(admitted(&m).is_err());
    }

    #[test]
    fn actual_owner_window_survives_call_but_rejects_early_death() {
        let mut m = fixture();
        m.functions[1].parameters = vec![LocalId(0)];
        crate::analyze(&m, &Limits::default()).unwrap();
        m.functions[1].blocks[0]
            .instructions
            .insert(2, inst(None, InstructionKind::StorageDead(LocalId(0))));
        assert!(crate::analyze(&m, &Limits::default()).is_err());
    }

    #[test]
    fn fresh_dyn_budget_has_exact_n_and_n_minus_one_boundary() {
        let mut m = fixture();
        m.functions[1].parameters = vec![LocalId(0)];
        let baseline = Limits::default();
        let mut budget = Budget::new(&baseline);
        crate::analyze_budgeted(&m, &baseline, &mut budget, None).unwrap();
        let spent = baseline.max_analysis_steps - budget.remaining();
        let mut exact = baseline.clone();
        exact.max_analysis_steps = spent;
        crate::analyze(&m, &exact).unwrap();
        exact.max_analysis_steps -= 1;
        assert!(crate::analyze(&m, &exact).is_err());
    }

    #[test]
    fn local_dyn_escape_and_nested_carrier_are_closed() {
        let mut m = fixture();
        m.functions[1].parameters = vec![LocalId(0)];
        m.functions[1].return_type = m.functions[1].values[1].clone();
        m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        assert!(crate::analyze(&m, &Limits::default()).is_err());

        let mut m = fixture();
        m.functions[1].parameters = vec![LocalId(0)];
        let held = m.functions[1].values[1].clone();
        m.functions[1].locals.push(local(
            1,
            Type::Record {
                name: "Escape".into(),
                fields: vec![("held".into(), held)],
            },
        ));
        assert!(crate::analyze(&m, &Limits::default()).is_err());
    }
}
