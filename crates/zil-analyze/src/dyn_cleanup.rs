//! Reconstruct the erased owner's synchronous cleanup from concrete resources.
//! The public upper bound never supplies a destructor or a managed position.
use crate::{bad, budget::Budget};
use std::collections::BTreeSet;
use zil_base::CompileResult;
use zil_ir::*;

const WORK: &str = "dyn whole owner cleanup";

fn allows(bound: &DynCleanupContract, effect: DynEffect) -> CompileResult<()> {
    if bound.effects.contains(&effect) {
        Ok(())
    } else {
        Err(bad(
            "owned dyn cleanup exceeds its actual effect upper bound",
        ))
    }
}

pub(super) fn metadata(
    claim: Option<&DynCleanupContract>,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(1, WORK)?;
    let Some(c) = claim else {
        return Ok(());
    };
    for count in [
        c.positions.len(),
        c.access.reads.len(),
        c.access.writes.len(),
        c.effects.len(),
    ] {
        if count > budget.helper_nodes {
            return Err(vec![zil_base::Diagnostic::limit(
                zil_base::Phase::Analyze,
                "dyn cleanup metadata node limit exceeded",
                None,
            )]);
        }
        budget.spend(count.saturating_add(1), WORK)?;
    }
    for position in c
        .positions
        .iter()
        .chain(&c.access.reads)
        .chain(&c.access.writes)
    {
        position_metadata(position, budget)?;
    }
    for e in &c.effects {
        match e {
            DynEffect::Call(position) => position_metadata(position, budget)?,
            DynEffect::Cleanup(ty) => crate::dyn_metadata::ty(ty, budget)?,
            _ => budget.spend(1, WORK)?,
        }
    }
    Ok(())
}
fn position_metadata(position: &StoragePosition, budget: &mut Budget) -> CompileResult<()> {
    if position.path.len() > budget.helper_nodes {
        return Err(vec![zil_base::Diagnostic::limit(
            zil_base::Phase::Analyze,
            "dyn cleanup position limit exceeded",
            None,
        )]);
    }
    budget.spend(
        position.path.len().saturating_mul(4).saturating_add(2),
        WORK,
    )
}

pub(super) fn bound<'a>(
    interface: &'a DynInterface,
    budget: &mut Budget,
) -> CompileResult<&'a DynCleanupContract> {
    let bound = interface
        .owning_cleanup
        .as_ref()
        .ok_or_else(|| bad("owned dyn cleanup contract is Unknown"))?;
    budget.spend(interface.retained_life.ports.len().saturating_mul(4), WORK)?;
    let position = |i| StoragePosition {
        base: StorageBase::Result,
        path: vec![ContractProjection::RetainedPort(i)],
    };
    let reads = (0..interface.retained_life.ports.len())
        .map(|i| position(i as u32))
        .collect::<Vec<_>>();
    let writes = interface
        .retained_life
        .ports
        .iter()
        .enumerate()
        .filter(|(_, p)| p.mode == StoredBorrowMode::ExclusiveAddress)
        .map(|(i, _)| position(i as u32))
        .collect::<Vec<_>>();
    budget.spend(bound.effects.len().saturating_add(1), WORK)?;
    if bound.effects.len() > 6
        || !bound.environment
        || !bound.positions.is_empty()
        || bound.access.reads != reads
        || bound.access.writes != writes
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
        return Err(bad(
            "owned dyn closed cleanup contract has unproved authority",
        ));
    }
    Ok(bound)
}

/// Cleanup qualification is separate from the carrier's retained targets.
/// Context captures keep their complete class/source obligations in the
/// environment ledger; a known cleanup upper never discharges those loans.
pub(super) fn capture_interface(
    module: &TypedModule,
    id: DynInterfaceId,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(module.dyn_interfaces.len(), WORK)?;
    let mut matches=module.dyn_interfaces.iter().filter(|i|i.id==id);
    let interface=matches.next().ok_or_else(||bad("owned capture interface is absent"))?;
    if matches.next().is_some(){return Err(bad("owned capture interface is ambiguous"));}
    bound(interface,budget)?;
    Ok(())
}

pub(super) fn owner_interface(
    module: &TypedModule,
    id: DynInterfaceId,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(module.dyn_interfaces.len(), WORK)?;
    let interface = module
        .dyn_interfaces
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| bad("owned dyn capture interface is absent"))?;
    bound(interface, budget)?;
    if interface.context.is_some()
        || !interface.retained_life.ports.is_empty()
        || interface.retained_life.binder_count != 0
    {
        return Err(bad("closed owned dyn capture retains external ports"));
    }
    Ok(())
}

pub(super) fn prove(
    module: &TypedModule,
    ty: &Type,
    bound: &DynCleanupContract,
    budget: &mut Budget,
) -> CompileResult<()> {
    shape(module, ty, bound, budget, 0)
}

fn shape(
    module: &TypedModule,
    ty: &Type,
    bound: &DynCleanupContract,
    budget: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    if depth > budget.partial_depth {
        return Err(bad("owned dyn cleanup depth exceeded"));
    }
    budget.spend(1, WORK)?;
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(());
    }
    match ty {
        Type::Managed(payload) => {
            budget.spend(module.resources.len(), WORK)?;
            for r in &module.resources {
                budget.type_work(&r.payload, false, WORK)?;
                budget.type_work(payload, false, WORK)?;
            }
            let mut matches = module.resources.iter().filter(|r| r.payload == **payload);
            let resource = matches
                .next()
                .ok_or_else(|| bad("owned dyn cleanup lacks exact resource definition"))?;
            if matches.next().is_some() {
                return Err(bad("owned dyn cleanup resource identity is ambiguous"));
            }
            match payload.as_ref() {
                Type::ByteBuffer
                    if resource.name == BYTE_BUFFER_RESOURCE_NAME
                        && resource.teardown.is_none() =>
                {
                    Ok(())
                }
                Type::OwnedStore { element } if resource.teardown.is_none() => {
                    crate::ownership_store::ordinary_owned(module, ty, budget)?;
                    shape(module, element, bound, budget, depth + 1)
                }
                Type::Record { name, fields }
                    if resource.name == *name && fields.iter().all(|(_, t)| t.is_copy_value()) =>
                {
                    // A descriptor only names this closed concrete payload. Its
                    // teardown cannot acquire any external borrowed authority.
                    shape(module, payload, bound, budget, depth + 1)?;
                    if let Some(entry) = resource.teardown {
                        let f = module
                            .functions
                            .get(entry.0 as usize)
                            .filter(|f| f.id == entry)
                            .ok_or_else(|| bad("owned dyn concrete teardown identity is absent"))?;
                        if f.parameters.len() != 1
                            || f.return_type != Type::Unit
                            || f.environment.is_some()
                            || f.foreign.is_some()
                            || f.call_kind != CallKind::Direct
                            || !f
                                .locals
                                .get(f.parameters[0].0 as usize)
                                .is_some_and(|l| l.ty == Type::Desc(payload.clone()))
                        {
                            return Err(bad(
                                "owned dyn teardown has wrong actual descriptor signature",
                            ));
                        }
                        function(module, entry, bound, budget, &mut BTreeSet::new(), 0)?;
                    }
                    Ok(())
                }
                _ => Err(bad("owned dyn cleanup lacks a proved synchronous resource")),
            }
        }
        Type::BorrowedStorage { storage, .. } => shape(module, storage, bound, budget, depth + 1),
        Type::Address { .. } | Type::Desc(_) => Ok(()),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                shape(module, field, bound, budget, depth + 1)?;
            }
            Ok(())
        }
        Type::Tuple(fields) => {
            for field in fields {
                shape(module, field, bound, budget, depth + 1)?;
            }
            Ok(())
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    shape(module, field, bound, budget, depth + 1)?;
                }
            }
            Ok(())
        }
        Type::Array { element, .. } => shape(module, element, bound, budget, depth + 1),
        _ => Err(bad("owned dyn cleanup retains unknown or borrowed state")),
    }
}

fn function(
    module: &TypedModule,
    id: FunctionId,
    bound: &DynCleanupContract,
    budget: &mut Budget,
    active: &mut BTreeSet<FunctionId>,
    depth: usize,
) -> CompileResult<()> {
    budget.spend(active.len().saturating_add(1), WORK)?;
    if depth > budget.helper_depth || !active.insert(id) {
        return Err(bad("owned dyn teardown call graph is unknown or cyclic"));
    }
    let f = module
        .functions
        .get(id.0 as usize)
        .filter(|f| f.id == id)
        .ok_or_else(|| bad("owned dyn teardown target is absent"))?;
    if f.call_kind != CallKind::Direct || f.environment.is_some() {
        return Err(bad(
            "owned dyn cleanup reaches non-synchronous callable code",
        ));
    }
    if f.foreign.is_some() {
        // Foreign code is conservatively allowed to perform every synchronous
        // observable effect. No missing foreign contract becomes empty.
        for effect in [
            DynEffect::Alloc,
            DynEffect::Ffi,
            DynEffect::Io,
            DynEffect::Block,
            DynEffect::Reenter,
            DynEffect::Panic,
        ] {
            allows(bound, effect)?;
        }
        active.remove(&id);
        return Ok(());
    }
    if f.call_kind != CallKind::Direct || f.environment.is_some() {
        return Err(bad("owned dyn teardown is not synchronous direct code"));
    }
    budget.spend(f.locals.len().saturating_add(f.values.len()), WORK)?;
    for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        match ty {
            Type::Desc(payload) => shape(module, payload, bound, budget, 0)?,
            _ => shape(module, ty, bound, budget, 0)?,
        }
    }
    for block in &f.blocks {
        budget.spend(block.instructions.len().saturating_add(1), WORK)?;
        for i in &block.instructions {
            match &i.kind {
                InstructionKind::Call { function: next, .. }
                | InstructionKind::ForeignCall { function: next, .. } => {
                    function(module, *next, bound, budget, active, depth + 1)?
                }
                InstructionKind::Unary {
                    op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                    ..
                }
                | InstructionKind::Binary { .. } => allows(bound, DynEffect::Panic)?,
                InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::ReadDesc { .. }
                | InstructionKind::WriteDesc { .. }
                | InstructionKind::MakeAggregate { .. }
                | InstructionKind::ExtractField { .. }
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. } => {}
                _ => {
                    return Err(bad(format!(
                        "owned dyn teardown has unproved effect or cleanup: {:?}",
                        i.kind
                    )));
                }
            }
        }
        if !matches!(
            block.terminator,
            Terminator::Return(_) | Terminator::Jump(_) | Terminator::Branch { .. }
        ) {
            return Err(bad("owned dyn teardown has unproved synchronous exit"));
        }
    }
    active.remove(&id);
    Ok(())
}
