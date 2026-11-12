//! Closed erased owners still borrow their real position. Zero external ports
//! do not erase the receiver/head loan or a returned view's original parents.
use super::*;

fn closed(module: &TypedModule, id: DynInterfaceId, b: &mut Budget) -> Check<bool> {
    b.spend(module.dyn_interfaces.len().saturating_add(1))?;
    let item = module
        .dyn_interfaces
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| err("closed dyn receiver has no actual interface"))?;
    Ok(item.context.is_none()
        && item.retained_life.binder_count == 0
        && item.retained_life.ports.is_empty()
        && item.retained_life.retained.is_empty()
        && item.retained_life.nominal_arguments.is_empty())
}
fn owner_id(ty: &Type) -> Option<DynInterfaceId> {
    match ty {
        Type::OwnedDyn { interface } => Some(*interface),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::OwnedDyn { interface } => Some(*interface),
            _ => None,
        },
        _ => None,
    }
}
pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if wrapper(module, f, i, b)? {
        return Ok(true);
    }
    let id = match &i.kind {
        InstructionKind::MakeDynOwned { interface, .. }
        | InstructionKind::MakeDynBorrow { interface, .. }
        | InstructionKind::CallDynSlot { interface, .. } => Some(*interface),
        InstructionKind::BorrowDynOwned { local, .. } | InstructionKind::DropDynOwned { local } => {
            f.locals
                .get(local.0 as usize)
                .and_then(|local| owner_id(&local.ty))
        }
        InstructionKind::BorrowCapture { index, .. } => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|definition| definition.captures.get(*index as usize))
            .and_then(owner_id),
        _ => None,
    };
    id.map(|id| closed(module, id, b))
        .transpose()
        .map(|answer| answer.unwrap_or(false))
}
pub(super) fn head(
    f: &TypedFunction,
    local: LocalId,
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let declaration = f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("dyn owner local is absent"))?;
    if matches!(declaration.ty, Type::Address { .. }) {
        let actual = state
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| err("dyn owner address local has no actual payload"))?;
        permission_work(&actual.address, b)?;
        Ok(actual.address.clone())
    } else {
        b.spend(1)?;
        Ok(Permission {
            sources: vec![Source {
                retained_port: None,
                base: HoldRoot::Local(local),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            }],
            loans: Vec::new(),
        })
    }
}
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Payload>> {
    if !operation(module, f, i, b)? {
        return Ok(None);
    }
    let mut out = Payload::default();
    match &i.kind {
        InstructionKind::MakeDynBorrow { source, .. } => {
            join_permission(&mut out.address, &values[source.0 as usize].address, b)?;
        }
        InstructionKind::MakeDynOwned { .. } => {
            // Exact Self/Heap route and whole cleanup were independently
            // authenticated by the raw dyn inventory and instruction checker.
        }
        InstructionKind::BorrowDynOwned { local, .. } => {
            out.address = head(f, *local, state, b)?;
        }
        InstructionKind::BorrowCapture { index, .. } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::DropDynOwned { local } => {
            state.locals[local.0 as usize] = Payload::default();
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            ..
        } => {
            b.spend(module.dyn_interfaces.len())?;
            let selected = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .and_then(|item| item.slots.get(*slot as usize))
                .ok_or_else(|| err("closed dyn call slot is absent"))?;
            if matches!(selected.result, Type::Address { .. }) {
                let Some(DynResultSource::Borrow { sources, .. }) = &selected.contract.result
                else {
                    return Err(err("closed dyn borrowed result source is Unknown"));
                };
                b.spend(sources.len())?;
                if sources.as_slice()
                    != [StorageSource::Exact(StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: Vec::new(),
                    })]
                {
                    return Err(err(
                        "closed dyn view must retain its real receiver position",
                    ));
                }
                join_permission(&mut out.address, &values[callee.0 as usize].address, b)?;
            } else if selected.contract.result != Some(DynResultSource::Value) {
                return Err(err(
                    "closed dyn result lacks an independently checked value route",
                ));
            }
        }
        InstructionKind::DropErrorUnion { local } => {
            state.locals[local.0 as usize] = Payload::default();
        }
        _ if wrapper(module, f, i, b)? => {}
        _ => return Err(err("closed dyn operation has no actual transfer")),
    }
    Ok(Some(out))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn access(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    values: &[Payload],
    checks: &mut access_candidates::Access<'_>,
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    if !operation(module, f, i, b)? {
        return Ok(());
    }
    match &i.kind {
        InstructionKind::BorrowDynOwned { local, mutable } => {
            checks.check(&head(f, *local, state, b)?, *mutable, requirements, b)?
        }
        InstructionKind::DropDynOwned { local } => {
            checks.check(&head(f, *local, state, b)?, true, requirements, b)?
        }
        InstructionKind::MakeDynBorrow {
            source, mutable, ..
        } => checks.check(
            &values[source.0 as usize].address,
            *mutable,
            requirements,
            b,
        )?,
        InstructionKind::BorrowCapture { index, mutable } => {
            b.spend(1)?;
            let actual = Permission {
                sources: vec![Source {
                    retained_port: None,
                    base: HoldRoot::Capture(*index),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                }],
                loans: Vec::new(),
            };
            checks.check(&actual, *mutable, requirements, b)?;
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            ..
        } => {
            b.spend(module.dyn_interfaces.len())?;
            let selected = module
                .dyn_interfaces
                .iter()
                .find(|item| item.id == *interface)
                .and_then(|item| item.slots.get(*slot as usize))
                .ok_or_else(|| err("closed dyn access has no actual slot"))?;
            checks.check(
                &values[callee.0 as usize].address,
                selected.receiver.mutable,
                requirements,
                b,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Additional stored-lane values are tied to an authenticated closed table,
/// never admitted merely because an outer node owns something.
pub(super) fn value(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let actual = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::OwnedDyn { interface } | Type::Dyn { interface } = actual {
        return closed(module, *interface, b);
    }
    if let Type::ErrorUnion { error, result } = actual
        && let Type::OwnedDyn { interface } = result.as_ref()
    {
        super::super::dynamic_metadata::ty(error, b)?;
        let canonical = super::super::dynamic_dispatch::create_error();
        if **error != canonical {
            return Ok(false);
        }
        return closed(module, *interface, b);
    }
    b.spend(module.dyn_vtables.len())?;
    for table in &module.dyn_vtables {
        if table.owned_storage != Some(DynOwnedStorage::Heap)
            || !closed(module, table.interface, b)?
        {
            continue;
        }
        super::super::dynamic_metadata::ty(actual, b)?;
        super::super::dynamic_metadata::ty(&table.self_type, b)?;
        if *actual == table.self_type {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(super) fn related_resources(module: &TypedModule, b: &mut Budget) -> Check<bool> {
    if module.resources.is_empty() {
        return Ok(false);
    }
    let mut payloads = Vec::new();
    for table in &module.dyn_vtables {
        if table.owned_storage != Some(DynOwnedStorage::Heap)
            || !closed(module, table.interface, b)?
        {
            continue;
        }
        let mut pending = vec![(&table.self_type, 0usize)];
        while let Some((ty, depth)) = pending.pop() {
            b.spend(1)?;
            if depth > b.limits.max_call_depth {
                return Err(limit("closed dyn provider ownership graph exceeds depth"));
            }
            match ty {
                Type::Managed(payload) => {
                    b.spend(2)?;
                    payloads.push(payload.as_ref());
                    pending.push((payload, depth + 1));
                }
                Type::Package { fields, .. } | Type::Record { fields, .. } => {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|(_, t)| (t, depth + 1)));
                }
                Type::Tuple(fields) => {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|t| (t, depth + 1)));
                }
                Type::OwnedStore { element } | Type::Array { element, .. } => {
                    b.spend(1)?;
                    pending.push((element, depth + 1));
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
                _ => {}
            }
        }
    }
    if payloads.is_empty() {
        return Ok(false);
    }
    // Preserve the old directly captured scalar resource route in mixed code.
    for definition in &module.closures {
        if definition.storage_life.is_some() {
            b.spend(definition.captures.len())?;
            for ty in &definition.captures {
                if let Type::Managed(payload) = ty {
                    payloads.push(payload.as_ref());
                }
            }
        }
    }
    for definition in &module.resources {
        let mut found = false;
        for payload in &payloads {
            super::super::dynamic_metadata::ty(payload, b)?;
            super::super::dynamic_metadata::ty(&definition.payload, b)?;
            found |= **payload == definition.payload;
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A closed erased-owner allocation outcome remains affine. Observing its tag
/// or consuming one real arm never qualifies the owner as a pure Copy value.
pub(super) fn wrapper(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let ty = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("closed dyn outcome local is absent"))?
                .ty
        }
        InstructionKind::TakeVariantField { local, path }
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(_)]) =>
        {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("closed dyn outcome local is absent"))?
                .ty
        }
        InstructionKind::DropErrorUnion { local } => {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("closed dyn outcome local is absent"))?
                .ty
        }
        InstructionKind::ErrorTag { value }
        | InstructionKind::TakeErrorUnion { value, .. }
        | InstructionKind::ReadErrorUnion { value, .. } => vt(f, *value)?,
        InstructionKind::MakeErrorUnion { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("closed dyn outcome constructor lacks a result"))?,
        )?,
        _ => return Ok(false),
    };
    let Type::ErrorUnion { error, result } = ty else {
        return Ok(false);
    };
    let Type::OwnedDyn { interface } = result.as_ref() else {
        return Ok(false);
    };
    crate::dynamic_metadata::ty(error, b)?;
    if **error != crate::dynamic_dispatch::create_error() || !closed(module, *interface, b)? {
        return Ok(false);
    }
    let expected = match &i.kind {
        InstructionKind::PlaceTagLocal { .. } => Some(&Type::I32),
        InstructionKind::ErrorTag { .. } => Some(&Type::Bool),
        InstructionKind::TakeVariantField { path, .. } => {
            Some(if path == &[PlaceProjection::ErrorArm(true)] {
                error.as_ref()
            } else {
                result.as_ref()
            })
        }
        InstructionKind::TakeErrorUnion { is_error, .. }
        | InstructionKind::ReadErrorUnion { is_error, .. } => Some(if *is_error {
            error.as_ref()
        } else {
            result.as_ref()
        }),
        _ => None,
    };
    if let Some(expected) = expected {
        crate::dynamic_metadata::ty(expected, b)?;
        if i.result.map(|id| vt(f, id)).transpose()? != Some(expected) {
            return Err(err(
                "closed dyn outcome observation changes its actual selected type",
            ));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn boundary() -> (TypedModule, TypedFunction, DynInterfaceId, LocalId) {
        let (mut module, _) = crate::tests::baseline();
        let interface = DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        };
        let decl = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(0),
            compiler: false,
        };
        let receiver = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        };
        // These tests exercise the private transport after independent header
        // authentication; whole-module raw/Wire consumers exercise that gate.
        module.dyn_interfaces.push(DynInterface {
            id: interface,
            context: None,
            owning_cleanup: Some(DynCleanupContract {
                environment: true,
                positions: vec![],
                access: DynCallAccess {
                    reads: vec![],
                    writes: vec![],
                },
                effects: vec![],
            }),
            application: DynTraitApplication {
                declaration: decl,
                actuals: vec![],
            },
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
                    application: DynTraitApplication {
                        declaration: decl,
                        actuals: vec![],
                    },
                    ordinal: 0,
                },
                actuals: vec![],
                receiver: DynReceiver {
                    mutable: false,
                    mode: PassingMode::Borrow,
                },
                parameters: vec![],
                parameter_modes: vec![],
                result: Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
                storage_life: None,
                contract: DynContract {
                    requires: Some(vec![DynRequirement::Read(receiver.clone())]),
                    guarantees: Some(vec![]),
                    hold: Some(vec![]),
                    call: Some(DynCallAccess {
                        reads: vec![receiver.clone()],
                        writes: vec![],
                    }),
                    result: Some(DynResultSource::Borrow {
                        sources: (vec![receiver]).into_iter().map(zil_ir::StorageSource::Exact).collect(),
                        mutable: false,
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
                    effects: Some(vec![]),
                    resume: Some(DynResumeBound::Local),
                    threads: Some(DynThreadRequirements {
                        transfer: false,
                        share: false,
                    }),
                },
            }],
        });
        let mut f = module.functions[0].clone();
        let local = LocalId(f.locals.len() as u32);
        f.locals.push(LocalDecl {
            id: local,
            name: "actual_erased_owner".into(),
            ty: Type::OwnedDyn { interface },
            mutable: true,
            span: f.span,
        });
        f.values = vec![
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Dyn { interface }),
            },
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
        ];
        (module, f, interface, local)
    }
    #[test]
    fn borrowed_method_result_keeps_actual_head_and_parent_until_last_use() {
        let (module, f, interface, local) = boundary();
        let mut budget = Budget::new(&Limits::default());
        let mut state = Frame {
            locals: vec![Payload::default(); f.locals.len()],
            ..Frame::default()
        };
        let borrow = Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::BorrowDynOwned {
                local,
                mutable: false,
            },
            span: f.span,
        };
        let mut value = transfer(&module, &f, &borrow, &mut state, &[], &mut budget)
            .unwrap()
            .unwrap();
        assert_eq!(value.address.sources[0].base, HoldRoot::Local(local));
        value.address.loans = vec![17];
        let call = Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::CallDynSlot {
                callee: ValueId(0),
                interface,
                slot: 0,
                arguments: vec![],
            },
            span: f.span,
        };
        let result = transfer(&module, &f, &call, &mut state, &[value], &mut budget)
            .unwrap()
            .unwrap();
        assert_eq!(result.address.sources[0].base, HoldRoot::Local(local));
        assert_eq!(result.address.loans, [17]);
        let mut loans = vec![Loan::default(); 18];
        loans[17].sources = result.address.sources;
        let drop = Instruction {
            result: None,
            kind: InstructionKind::DropDynOwned { local },
            span: f.span,
        };
        let mut requirements = Vec::new();
        let mut live = access_candidates::Access::new(&[17], &loans, &mut budget).unwrap();
        assert!(
            access(
                &module,
                &f,
                &drop,
                &state,
                &[],
                &mut live,
                &mut requirements,
                &mut budget
            )
            .is_err()
        );
        let mut dead = access_candidates::Access::new(&[], &loans, &mut budget).unwrap();
        access(
            &module,
            &f,
            &drop,
            &state,
            &[],
            &mut dead,
            &mut requirements,
            &mut budget,
        )
        .unwrap();
    }
    #[test]
    fn context_is_never_reclassified_as_known_empty() {
        let (mut module, f, _, local) = boundary();
        module.dyn_interfaces[0].context = Some(Type::I32);
        let op = Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::BorrowDynOwned {
                local,
                mutable: false,
            },
            span: f.span,
        };
        assert!(!operation(&module, &f, &op, &mut Budget::new(&Limits::default())).unwrap());
    }
}
