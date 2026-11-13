//! Input head contracts select no persistent data domain. Ordinary loans,
//! resource states and effects remain independent executable obligations.
use super::*;
use crate::tasks::input_scope::{self, Domain};

fn zero_data(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(ty, true, "ordinary head complete data type")?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1, "ordinary head actual data scope")?;
        match ty {
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(
                    module.dyn_interfaces.len(),
                    "ordinary head actual dynamic interface",
                )?;
                let mut found = module
                    .dyn_interfaces
                    .iter()
                    .filter(|item| item.id == *interface);
                let Some(item) = found.next() else {
                    return Ok(false);
                };
                if found.next().is_some()
                    || item.context.is_some()
                    || item.retained_life
                        != (zil_ir::RetainedLifeShape {
                            binder_count: 0,
                            ports: vec![],
                            retained: vec![],
                            nominal_arguments: vec![],
                        })
                {
                    return Ok(false);
                }
            }
            Type::Task { .. } => {
                crate::tasks::require_closed_context(ty)?;
                if input_scope::domain(ty, b)? != Domain::Known(0) {
                    return Ok(false);
                }
            }
            Type::Managed(inner)
            | Type::Array { element: inner, .. }
            | Type::OwnedStore { element: inner }
            | Type::Slice { element: inner }
            | Type::Mutex { payload: inner } => pending.push(inner),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len(), "ordinary head actual product children")?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len(), "ordinary head actual Tuple children")?;
                pending.extend(fields)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(
                        fields.len().saturating_add(1),
                        "ordinary head actual Sum alternatives",
                    )?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Opaque { .. } => return Ok(false),
            other => {
                if input_scope::domain(other, b)? != Domain::Known(0) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
fn head_value(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    // Only a whole SSA/input head has this role. Nested Address/Desc fields
    // are dependencies and do not become empty by their payload's shape.
    match ty {
        Type::Address { inner, .. } | Type::Desc(inner) => zero_data(module, inner, b),
        other => zero_data(module, other, b),
    }
}

pub(super) fn relation(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(c) = &f.storage_life else {
        return Ok(false);
    };
    charge(c, b)?;
    if c.fixed_binder_count != 0
        || !c.classes.is_empty()
        || c.invocation_binder_count == 0
        || c.bindings.iter().any(|row| {
            !matches!(row.position.base, StorageBase::Parameter(_)) || !row.position.path.is_empty()
        })
    {
        return Ok(false);
    }
    let mut rows = c.bindings.iter();
    let mut next = 0;
    for (ordinal, local) in f.parameters.iter().enumerate() {
        b.spend(1, "ordinary input head complete signature")?;
        let ty = &f
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("ordinary head parameter local absent"))?
            .ty;
        if !head_value(module, ty, b)? {
            return Ok(false);
        }
        if matches!(ty, Type::Address { .. }) {
            let Some(row) = rows.next() else {
                return Err(bad("ordinary head relation omits an actual Address input"));
            };
            if row.position.base
                != StorageBase::Parameter(
                    u32::try_from(ordinal)
                        .map_err(|_| bad("ordinary head parameter ordinal overflow"))?,
                )
            {
                return Err(bad("ordinary head relation reorders actual inputs"));
            }
            let StorageBinder::Invocation(index) = row.binder else {
                return Err(bad("ordinary input head requires an Invocation domain"));
            };
            if index >= c.invocation_binder_count || index > next {
                return Err(bad("ordinary input head domain is noncanonical"));
            }
            if index == next {
                next += 1;
            }
        }
    }
    if rows.next().is_some() || next != c.invocation_binder_count {
        return Err(bad(
            "ordinary head relation has extra rows or unused domains",
        ));
    }
    if !head_value(module, &f.return_type, b)? {
        return Ok(false);
    }
    // The full ordinary signature validator still runs. Task's complete zero
    // data shape is checked independently of its outer head Borrow above.
    for local in &f.parameters {
        b.type_work(
            &f.locals[local.0 as usize].ty,
            true,
            "ordinary head signature snapshot",
        )?;
    }
    let parameters = f
        .parameters
        .iter()
        .map(|local| f.locals[local.0 as usize].ty.clone())
        .collect::<Vec<_>>();
    function_relation_resource_scopes(
        &parameters,
        &f.return_type,
        c,
        !module.closures.is_empty(),
        module,
        false,
        b,
    )?;
    Ok(true)
}

pub(crate) fn module(module: &TypedModule, limits: &Limits, b: &mut Budget) -> CompileResult<bool> {
    if !b.ordinary_input_head_candidates || b.capture_relations {
        return Ok(false);
    }
    b.spend(module.functions.len(), "ordinary input header inventory")?;
    if !module
        .functions
        .iter()
        .any(|function| function.storage_life.is_some())
    {
        return Ok(false);
    }
    // These actual operations need additional stored-target proofs even when
    // their semantic data domain happens to be zero.
    if b.shared_byte_borrow
        || b.nested_store_windows
        || b.store_option_results
        || b.stored_slot_updates
        || b.stored_enum_addresses
        || b.borrowed_function_pointers
    {
        return Ok(false);
    }
    if !module.closures.is_empty() && b.task_input_scopes.is_none() {
        b.task_input_scopes = Some(input_scope::build(module, limits, b)?);
    }
    for function in &module.functions {
        if function.storage_life.is_some()
            && !input_scope::checked_empty_function(function, b)?
            && !relation(module, function, b)?
        {
            return Ok(false);
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(&function.values)
            .chain(std::iter::once(&function.return_type))
        {
            if !head_value(module, ty, b)? {
                return Ok(false);
            }
        }
        for block in &function.blocks {
            b.spend(
                block.instructions.len(),
                "ordinary head actual operation inventory",
            )?;
            if block.instructions.iter().any(|i| {
                matches!(
                    i.kind,
                    InstructionKind::LoadStoredBorrow { .. }
                        | InstructionKind::StoreStoredBorrow { .. }
                        | InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                )
            }) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
