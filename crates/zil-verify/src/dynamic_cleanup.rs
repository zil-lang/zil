//! Verify reconstructs whole-owner destruction without trusting Types/Analyze.
use super::*;
use std::collections::{BTreeSet, VecDeque};

pub(super) fn metadata(
    _module: &TypedModule,
    claim: Option<&DynCleanupContract>,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(1)?;
    let Some(c) = claim else {
        return Ok(());
    };
    for list in [&c.positions, &c.access.reads, &c.access.writes] {
        if list.len() > budget.limits.max_nodes {
            return Err(limit("dyn owning cleanup position count exceeded"));
        }
        budget.spend(list.len().saturating_add(1))?;
        for p in list {
            position(p, budget)?;
        }
    }
    if c.effects.len() > budget.limits.max_nodes {
        return Err(limit("dyn owning cleanup effect count exceeded"));
    }
    budget.spend(c.effects.len().saturating_add(1))?;
    for e in &c.effects {
        match e {
            DynEffect::Cleanup(ty) => crate::dynamic_metadata::ty(ty, budget)?,
            DynEffect::Call(p) => position(p, budget)?,
            _ => budget.spend(1)?,
        }
    }
    Ok(())
}
fn position(p: &StoragePosition, budget: &mut Budget) -> Check<()> {
    if p.path.len() > budget.limits.max_nodes {
        return Err(limit("dyn owning cleanup projection count exceeded"));
    }
    budget.spend(p.path.len().saturating_mul(4).saturating_add(2))
}

/// A synchronous owning upper is checked in its actual interface domain.
/// Context ports are external access bounds, never additional owned payloads.
/// Concrete erasure still needs the independent provider/body cleanup proof.
pub(super) fn interface_bound<'a>(
    module: &TypedModule,
    interface: &'a DynInterface,
    budget: &mut Budget,
) -> Check<&'a DynCleanupContract> {
    metadata(module, interface.owning_cleanup.as_ref(), budget)?;
    if interface.context.is_some() {
        crate::dynamic_context::validate(module, interface, budget)?;
        crate::dynamic_ports::validate_cleanup(interface, budget)?;
        interface.owning_cleanup.as_ref().ok_or_else(||failure("owned Context cleanup contract is Unknown"))
    } else {
        // None is only the explicitly closed interface envelope. Its complete
        // directory must actually be empty before the closed bound can apply.
        crate::dynamic_context::validate(module, interface, budget)?;
        bound(interface.owning_cleanup.as_ref(), budget)
    }
}

pub(super) fn bound<'a>(
    claim: Option<&'a DynCleanupContract>,
    budget: &mut Budget,
) -> Check<&'a DynCleanupContract> {
    let claim = claim.ok_or_else(|| {
        reject(
            "E-VERIFY-DYN",
            None,
            "owned dyn cleanup contract is Unknown",
        )
    })?;
    budget.spend(claim.effects.len().saturating_add(1))?;
    if claim.effects.len() > 6
        || !claim.environment
        || !claim.positions.is_empty()
        || !claim.access.reads.is_empty()
        || !claim.access.writes.is_empty()
        || claim.effects.iter().any(|e| {
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
        return Err(reject(
            "E-VERIFY-DYN",
            None,
            "owned dyn cleanup bound contains unproved authority",
        ));
    }
    Ok(claim)
}

fn failure(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN", None, message)
}

pub(super) fn prove(module:&TypedModule,root:&Type,claim:&DynCleanupContract,budget:&mut Budget)->Check<()>{
    prove_impl(module,root,claim,false,budget).map(|_|())
}
/// Exact teardown roots discovered by this same complete concrete cleanup
/// proof. Work selection/effect joining does not turn them into permissions.
pub(super) fn prove_targets(module:&TypedModule,root:&Type,claim:&DynCleanupContract,budget:&mut Budget)->Check<Vec<FunctionId>>{
    prove_impl(module,root,claim,true,budget)
}
fn prove_impl(
    module: &TypedModule,
    root: &Type,
    claim: &DynCleanupContract,
    retain_targets:bool,
    budget: &mut Budget,
) -> Check<Vec<FunctionId>> {
    let mut pending = vec![(root, 0usize)];
    let mut entries = BTreeSet::new();
    while let Some((ty, depth)) = pending.pop() {
        if depth > budget.limits.max_call_depth {
            return Err(limit("owned dyn cleanup type depth exceeded"));
        }
        budget.spend(1)?;
        if ty.is_b0_scalar() || *ty == Type::Unit {
            continue;
        }
        match ty {
            Type::Managed(payload) => {
                budget.spend(module.resources.len())?;
                for resource in &module.resources {
                    check_type_shape(
                        &resource.payload,
                        &module.target,
                        &budget.limits.clone(),
                        budget,
                    )?;
                    check_type_shape(payload, &module.target, &budget.limits.clone(), budget)?;
                }
                let found = module
                    .resources
                    .iter()
                    .filter(|r| r.payload == **payload)
                    .collect::<Vec<_>>();
                let [r] = found.as_slice() else {
                    return Err(failure(
                        "owned dyn cleanup resource identity is absent or ambiguous",
                    ));
                };
                match payload.as_ref() {
                    Type::ByteBuffer
                        if r.name == BYTE_BUFFER_RESOURCE_NAME && r.teardown.is_none() => {}
                    Type::OwnedStore { element } if r.teardown.is_none() => {
                        crate::dynamic_store::check_carrier(module, ty, None, budget)?;
                        budget.spend(1)?;
                        pending.push((element, depth + 1));
                    }
                    Type::Record { name, .. } if r.name == *name => {
                        budget.spend(1)?;
                        pending.push((payload, depth + 1));
                        if let Some(id) = r.teardown {
                            let target = module
                                .functions
                                .get(id.0 as usize)
                                .filter(|f| f.id == id)
                                .ok_or_else(|| {
                                    failure("owned dyn concrete cleanup target is absent")
                                })?;
                            if target.foreign.is_some()
                                || target.environment.is_some()
                                || target.call_kind != CallKind::Direct
                                || target.return_type != Type::Unit
                                || target.parameters.len() != 1
                                || !target
                                    .locals
                                    .get(target.parameters[0].0 as usize)
                                    .is_some_and(|l| l.ty == Type::Desc(payload.clone()))
                            {
                                return Err(failure(
                                    "owned dyn cleanup target lacks its exact actual payload descriptor",
                                ));
                            }
                            budget.spend(entries.len().saturating_add(1))?;
                            entries.insert(id);
                        }
                    }
                    _ => {
                        return Err(failure(
                            "owned dyn cleanup resource is not proved synchronous",
                        ));
                    }
                }
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|t| (t, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| (t, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::Array { element, .. } => {
                budget.spend(1)?;
                pending.push((element, depth + 1));
            }
            _ => {
                return Err(failure(
                    "owned dyn cleanup contains borrowed or unknown state",
                ));
            }
        }
    }
    // A queue entry carries its ancestry, so mutual recursion cannot be hidden
    // by a global visited set. Every reached body supplies its own real effects.
    let targets=if retain_targets{budget.spend(entries.len())?;entries.iter().copied().collect()}else{Vec::new()};
    let mut code = VecDeque::new();
    for id in entries {
        budget.spend(1)?;
        code.push_back((id, Vec::<FunctionId>::new()));
    }
    while let Some((id, mut ancestors)) = code.pop_front() {
        budget.spend(ancestors.len().saturating_add(1))?;
        if ancestors.len() > budget.limits.max_call_depth || ancestors.contains(&id) {
            return Err(failure(
                "owned dyn cleanup has cyclic or unknown reachable code",
            ));
        }
        ancestors.push(id);
        let f = module
            .functions
            .get(id.0 as usize)
            .filter(|f| f.id == id)
            .ok_or_else(|| failure("owned dyn cleanup reachable function is absent"))?;
        let permits = |effect: &DynEffect| claim.effects.contains(effect);
        if f.call_kind != CallKind::Direct || f.environment.is_some() {
            return Err(failure(
                "owned dyn cleanup target has unknown callable or suspension semantics",
            ));
        }
        if f.foreign.is_some() {
            if ![
                DynEffect::Alloc,
                DynEffect::Ffi,
                DynEffect::Io,
                DynEffect::Block,
                DynEffect::Reenter,
                DynEffect::Panic,
            ]
            .iter()
            .all(permits)
            {
                return Err(failure(
                    "owned dyn foreign cleanup exceeds its synchronous effect bound",
                ));
            }
            continue;
        }
        if f.call_kind != CallKind::Direct || f.environment.is_some() {
            return Err(failure(
                "owned dyn cleanup reaches a non-synchronous callable",
            ));
        }
        budget.spend(f.values.len().saturating_add(f.locals.len()))?;
        for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            let value = match ty {
                Type::Desc(payload) => payload.as_ref(),
                _ => ty,
            };
            // Local cleanup code is deliberately closed: descriptors may point
            // only at finite Copy data, and it cannot create a new owner debt.
            if !closed_data(value, budget, 0)? {
                return Err(failure(
                    "owned dyn cleanup local has unknown borrowed or owning state",
                ));
            }
        }
        for block in &f.blocks {
            budget.spend(block.instructions.len().saturating_add(1))?;
            for instruction in &block.instructions {
                match &instruction.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::ForeignCall { function, .. } => {
                        budget.spend(ancestors.len().saturating_add(1))?;
                        code.push_back((*function, ancestors.clone()));
                    }
                    InstructionKind::Binary { .. }
                    | InstructionKind::Unary {
                        op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                        ..
                    } => {
                        if !permits(&DynEffect::Panic) {
                            return Err(failure(
                                "owned dyn arithmetic cleanup lacks a Panic bound",
                            ));
                        }
                    }
                    InstructionKind::ReadDesc { .. }
                    | InstructionKind::WriteDesc { .. }
                    | InstructionKind::Constant(_)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. } => {}
                    _ => {
                        return Err(failure(
                            "owned dyn cleanup has an unknown executable operation",
                        ));
                    }
                }
            }
            if !matches!(
                block.terminator,
                Terminator::Return(_) | Terminator::Jump(_) | Terminator::Branch { .. }
            ) {
                return Err(failure(
                    "owned dyn cleanup has unknown termination semantics",
                ));
            }
        }
    }
    Ok(targets)
}

fn closed_data(ty: &Type, budget: &mut Budget, depth: usize) -> Check<bool> {
    if depth > budget.limits.max_call_depth {
        return Err(limit("owned dyn cleanup local type depth exceeded"));
    }
    budget.spend(1)?;
    if ty.is_b0_scalar() || *ty == Type::Unit {
        return Ok(true);
    }
    match ty {
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                if !closed_data(field, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Tuple(fields) => {
            for field in fields {
                if !closed_data(field, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Array { element, .. } => closed_data(element, budget, depth + 1),
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if !closed_data(field, budget, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod interface_tests {
    use super::*;

    /// A qualified nominal Context is metadata, not a runtime provider. These
    /// tests exercise the upper only; actual erasure/debt needs other kernels.
    fn context() -> (TypedModule, DynInterface) {
        let (mut module,_) = crate::tests::baseline();
        let declaration=LifeDeclarationId{module:module.module_id,declaration:zil_base::NodeId(991),compiler:false};
        let leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        let life=StorageLifeShape{
            binder_count:1,
            slots:vec![StorageLifeSlot{path:vec![PlaceProjection::Field(0)],mode:StoredBorrowMode::SharedAddress,binder:0}],
            retained:vec![],
            nominal_arguments:vec![NominalLifeArgument{path:vec![],declaration,argument:0,binder:0}],
        };
        let ty=Type::BorrowedStorage{storage:Box::new(Type::Record{name:"cleanup.Context".into(),fields:vec![("pointer".into(),leaf.clone())]}),life:life.clone()};
        module.life_declarations.push(LifeNominalDeclaration{id:declaration,identity:LifeDeclarationIdentity{package:"cleanup".into(),module_path:vec![],declaration_path:vec!["Context".into()],generation_path:vec![]},kind:LifeNominalKind::Record,parameters:vec![LifeParameterKind::Lifetime]});
        module.life_instances.push(LifeNominalInstance{declaration,ty:ty.clone(),arguments:vec![LifeNominalActual::Lifetime(0)]});
        let interface=DynInterface{
            id:DynInterfaceId{module:module.module_id,index:0},
            context:Some(ty),
            application:DynTraitApplication{declaration,actuals:vec![]},inherited:vec![],associated_types:vec![],associated_constants:vec![],slots:vec![],
            retained_life:RetainedLifeShape{binder_count:1,ports:vec![RetainedBorrowPort{leaf,mode:StoredBorrowMode::SharedAddress,binder:0}],retained:vec![],nominal_arguments:life.nominal_arguments},
            owning_cleanup:Some(DynCleanupContract{environment:true,positions:vec![],access:DynCallAccess{reads:vec![StoragePosition{base:StorageBase::Result,path:vec![ContractProjection::RetainedPort(0)]}],writes:vec![]},effects:vec![]}),
        };
        (module,interface)
    }
    fn accepts(module:&TypedModule,interface:&DynInterface)->bool {
        interface_bound(module,interface,&mut Budget::new(&Limits::default())).is_ok()
    }
    #[test]
    fn context_cleanup_upper_keeps_exact_ports_without_granting_ownership() {
        let (mut module,interface)=context();
        module.dyn_interfaces.push(interface.clone());
        let limits=Limits::default();
        let (_,plan)=crate::tests::baseline();
        let wire=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
        let (decoded,_)=zil_ir::wire::decode_module_candidate(&wire,&limits).unwrap();
        for actual in [&module,&decoded] {
            let interface=&actual.dyn_interfaces[0];
            assert!(accepts(actual,interface));
            assert!(bound(interface.owning_cleanup.as_ref(),&mut Budget::new(&limits)).is_err(),"the closed bound still cannot grant external authority");
            let mut unknown=interface.clone();unknown.owning_cleanup=None;
            assert!(!accepts(actual,&unknown));
            let mut missing=interface.clone();missing.context=None;
            assert!(!accepts(actual,&missing),"a missing Context cannot turn its original domain empty");
            let mut forged=interface.clone();forged.retained_life.nominal_arguments[0].argument=1;
            assert!(!accepts(actual,&forged),"same N does not prove an original nominal argument");
            let mut absent=interface.clone();absent.owning_cleanup.as_mut().unwrap().access.reads[0].path=vec![ContractProjection::RetainedPort(1)];
            assert!(!accepts(actual,&absent));
            let mut parameter=interface.clone();parameter.owning_cleanup.as_mut().unwrap().access.reads[0].base=StorageBase::Parameter(0);
            assert!(!accepts(actual,&parameter));
            let mut owning=interface.clone();owning.owning_cleanup.as_mut().unwrap().positions=interface.owning_cleanup.as_ref().unwrap().access.reads.clone();
            assert!(!accepts(actual,&owning),"an external dependency is never an owned cleanup payload");
            let mut writing=interface.clone();writing.owning_cleanup.as_mut().unwrap().access.writes=interface.owning_cleanup.as_ref().unwrap().access.reads.clone();
            assert!(!accepts(actual,&writing),"a shared port supplies no write authority");
            for effect in [DynEffect::Suspend,DynEffect::Spawn,DynEffect::Call(StoragePosition{base:StorageBase::Result,path:vec![]}),DynEffect::Cleanup(Type::I32)] {
                let mut asynchronous=interface.clone();asynchronous.owning_cleanup.as_mut().unwrap().effects=vec![effect];
                assert!(!accepts(actual,&asynchronous));
            }
        }
    }
}
