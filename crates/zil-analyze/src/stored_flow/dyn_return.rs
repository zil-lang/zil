//! Declared dyn result uppers are checked against each actual returned row.
//! Calls substitute the declared upper through actual input permissions.
use super::*;
use zil_ir::{ContractProjection, LifeTypeProjection, StorageBinder, StorageLifeContract};
struct Leaf {
    position: StoragePosition,
    ty: Type,
    mode: StoredBorrowMode,
    binder: StorageBinder,
    target: Option<Target>,
}
fn class(
    c: &StorageLifeContract,
    base: StorageBase,
    path: &[LifeTypeProjection],
    ordinal: u32,
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    fee(
        b,
        c.classes.len().saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut rows = c.classes.iter().filter(|r| {
        r.position.base == base && r.position.path == path && r.position.class == ordinal
    });
    let row = rows
        .next()
        .ok_or_else(|| failure("dyn result source lacks a complete class occurrence"))?;
    if rows.next().is_some() {
        return Err(failure("dyn result source class occurrence is ambiguous"));
    }
    Ok(row.binder)
}
#[allow(clippy::too_many_arguments)]
fn leaves(
    module: &TypedModule,
    f: &TypedFunction,
    ty: &Type,
    base: StorageBase,
    cp: &mut Vec<LifeTypeProjection>,
    path: &mut Vec<ContractProjection>,
    output: bool,
    rows: &mut Vec<Leaf>,
    found: &mut bool,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    if cp.len() > b.partial_depth.min(128) {
        return Err(failure("dyn result source directory exceeds depth"));
    }
    let c = f.storage_life.as_ref();
    match ty {
        Type::OwnedDyn { interface } | Type::Dyn { interface } => {
            fee(b, module.dyn_interfaces.len())?;
            let iface = module
                .dyn_interfaces
                .iter()
                .find(|i| i.id == *interface)
                .ok_or_else(|| failure("dyn result interface absent"))?;
            if iface.context.is_none() {
                return Ok(());
            }
            *found = true;
            for (ordinal, port) in iface.retained_life.ports.iter().enumerate() {
                let binder = class(
                    c.ok_or_else(|| failure("dyn result lacks complete signature classes"))?,
                    base,
                    cp,
                    port.binder,
                    b,
                )?;
                crate::dyn_metadata::ty(&port.leaf, b)?;
                fee(b, path.len().saturating_mul(3).saturating_add(3))?;
                let mut selected = path.clone();
                selected.push(ContractProjection::RetainedPort(ordinal as u32));
                let target = if let StorageBase::Parameter(index) = base {
                    let mut stored = Vec::new();
                    for p in path.iter() {
                        let ContractProjection::Place(p) = p else {
                            return Err(failure(
                                "dyn input capture source needs its environment transport",
                            ));
                        };
                        stored.push(*p);
                    }
                    Some(Target {
                        base: HoldRoot::Parameter(index),
                        stored,
                        retained_port: Some(PortOrigin::Dyn {
                            carrier: path.clone(),
                            port: ordinal as u32,
                        }),
                        place: vec![],
                        heap_element: None,
                    })
                } else {
                    None
                };
                rows.push(Leaf {
                    position: StoragePosition {
                        base,
                        path: selected,
                    },
                    ty: port.leaf.clone(),
                    mode: port.mode,
                    binder,
                    target,
                });
            }
        }
        Type::BorrowedStorage { storage, life } if !output => {
            for slot in &life.slots {
                let binder = class(
                    c.ok_or_else(|| failure("dyn saved source lacks complete signature classes"))?,
                    base,
                    cp,
                    slot.binder,
                    b,
                )?;
                fee(
                    b,
                    slot.path.len().saturating_add(path.len()).saturating_mul(3),
                )?;
                let mut leaf = storage.as_ref();
                for p in &slot.path {
                    if let Type::BorrowedStorage { storage, .. } = leaf {
                        leaf = storage;
                    }
                    leaf = match (leaf, p) {
                        (
                            Type::Record { fields, .. } | Type::Package { fields, .. },
                            PlaceProjection::Field(i),
                        ) => fields.get(*i as usize).map(|(_, t)| t),
                        (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
                        (Type::Array { element, length }, PlaceProjection::Field(i))
                            if *i < *length =>
                        {
                            Some(element.as_ref())
                        }
                        (Type::Enum { variants, .. }, PlaceProjection::Variant(v)) => variants
                            .get(v.variant as usize)
                            .and_then(|(_, f)| f.get(v.index as usize)),
                        (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                            Some(if *e { error.as_ref() } else { result.as_ref() })
                        }
                        _ => None,
                    }
                    .ok_or_else(|| failure("dyn result source is not an actual stored leaf"))?;
                }
                crate::dyn_metadata::ty(leaf, b)?;
                let mut selected = path.clone();
                selected.extend(slot.path.iter().copied().map(ContractProjection::Place));
                let StorageBase::Parameter(index) = base else {
                    return Err(failure(
                        "dyn result capture requires explicit environment binding",
                    ));
                };
                let stored = selected
                    .iter()
                    .map(|p| match p {
                        ContractProjection::Place(p) => Ok(*p),
                        _ => Err(failure("dyn saved source crosses a nonphysical selector")),
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                rows.push(Leaf {
                    position: StoragePosition {
                        base,
                        path: selected,
                    },
                    ty: leaf.clone(),
                    mode: slot.mode,
                    binder,
                    target: Some(Target {
                        base: HoldRoot::Parameter(index),
                        stored,
                        retained_port: None,
                        place: vec![],
                        heap_element: None,
                    }),
                });
            }
        }
        Type::Address { inner, mutable } => {
            if matches!(
                inner.as_ref(),
                Type::OwnedDyn { .. } | Type::Dyn { .. } | Type::BorrowedStorage { .. }
            ) {
                cp.push(LifeTypeProjection::Pointee);
                leaves(module, f, inner, base, cp, path, output, rows, found, b)?;
                cp.pop();
            } else if !output {
                let c = c.ok_or_else(|| failure("dyn input address lacks a signature relation"))?;
                fee(
                    b,
                    c.bindings
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                let mut bind = c
                    .bindings
                    .iter()
                    .filter(|r| r.position.base == base && r.position.path == *path);
                let row = bind.next().ok_or_else(|| {
                    failure("dyn input address has no actual declared outer lifetime")
                })?;
                if bind.next().is_some() {
                    return Err(failure("dyn input outer lifetime is ambiguous"));
                }
                let StorageBase::Parameter(index) = base else {
                    return Err(failure("dyn captured input requires environment binding"));
                };
                crate::dyn_metadata::ty(ty, b)?;
                fee(b, path.len().saturating_mul(2))?;
                if !path.is_empty() {
                    return Err(failure(
                        "nested inline reference source needs complete aggregate field transport",
                    ));
                }
                rows.push(Leaf {
                    position: StoragePosition {
                        base,
                        path: path.clone(),
                    },
                    ty: ty.clone(),
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    binder: row.binder,
                    target: Some(Target {
                        base: HoldRoot::Parameter(index),
                        stored: vec![],
                        retained_port: None,
                        place: vec![],
                        heap_element: None,
                    }),
                });
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                let p = PlaceProjection::ErrorArm(arm);
                cp.push(LifeTypeProjection::Place(p));
                path.push(ContractProjection::Place(p));
                leaves(module, f, child, base, cp, path, output, rows, found, b)?;
                path.pop();
                cp.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, child)) in fields.iter().enumerate() {
                let p = PlaceProjection::Field(i as u32);
                cp.push(LifeTypeProjection::Place(p));
                path.push(ContractProjection::Place(p));
                leaves(module, f, child, base, cp, path, output, rows, found, b)?;
                path.pop();
                cp.pop();
            }
        }
        Type::Tuple(fields) => {
            for (i, child) in fields.iter().enumerate() {
                let p = PlaceProjection::Field(i as u32);
                cp.push(LifeTypeProjection::Place(p));
                path.push(ContractProjection::Place(p));
                leaves(module, f, child, base, cp, path, output, rows, found, b)?;
                path.pop();
                cp.pop();
            }
        }
        _ => {}
    }
    Ok(())
}
fn contract(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<Option<BTreeMap<Vec<ContractProjection>, Vec<Target>>>> {
    if !b.dyn_contexts {
        if f.retained_return_sources.is_some() {
            return Err(failure(
                "retained source claim has no actual dyn Context consumer",
            ));
        }
        return Ok(None);
    }
    // First inspect the real result constructor directory, including known
    // zero-port Contexts. A missing class contract can never manufacture rows.
    let mut directories = Vec::new();
    dyn_address::directory(module, &f.return_type, &mut vec![], &mut directories, b)?;
    if directories.is_empty() {
        if f.retained_return_sources.is_some() {
            return Err(failure(
                "retained source claim has no supported Context result",
            ));
        }
        return Ok(None);
    }
    let written = f
        .retained_return_sources
        .as_ref()
        .ok_or_else(|| failure("dyn Context result source contract is Unknown"))?;
    let mut outputs = Vec::new();
    let mut found = false;
    leaves(
        module,
        f,
        &f.return_type,
        StorageBase::Result,
        &mut vec![],
        &mut vec![],
        true,
        &mut outputs,
        &mut found,
        b,
    )?;
    let mut inputs = Vec::new();
    for (index, local) in f.parameters.iter().enumerate() {
        leaves(
            module,
            f,
            &f.locals[local.0 as usize].ty,
            StorageBase::Parameter(index as u32),
            &mut vec![],
            &mut vec![],
            false,
            &mut inputs,
            &mut false,
            b,
        )?;
    }
    fee(
        b,
        written
            .len()
            .saturating_mul(outputs.len().saturating_add(1)),
    )?;
    if written
        .iter()
        .any(|row| !outputs.iter().any(|leaf| leaf.position == row.position))
    {
        return Err(failure(
            "retained source claim selects no actual result port",
        ));
    }
    let mut result = BTreeMap::new();
    for leaf in outputs {
        let rows = written
            .iter()
            .filter(|row| row.position == leaf.position)
            .collect::<Vec<_>>();
        let sources = if written.is_empty() {
            &[][..]
        } else {
            if rows.len() != 1 {
                return Err(failure(
                    "retained result omits or repeats an explicit complete port row",
                ));
            }
            rows[0].sources.as_slice()
        };
        let mut allowed = Vec::new();
        for source in sources {
            fee(
                b,
                inputs
                    .len()
                    .saturating_mul(source.path.len().saturating_add(1)),
            )?;
            let mut selected = inputs.iter().filter(|input| input.position == *source);
            let input = selected.next().ok_or_else(|| {
                failure("retained result source is not an actual typed input leaf")
            })?;
            if selected.next().is_some()
                || input.mode != leaf.mode
                || input.binder != leaf.binder
                || input.ty != leaf.ty
            {
                return Err(failure(
                    "retained result source changes full leaf, mode or actual signature class",
                ));
            }
            let target = input.target.as_ref().ok_or_else(|| {
                failure("retained result source has no actual external input origin")
            })?;
            fee(
                b,
                target_width(target)
                    .saturating_add(allowed.len())
                    .saturating_add(1),
            )?;
            if !allowed.contains(target) {
                allowed.push(target.clone());
            }
        }
        fee(b, leaf.position.path.len())?;
        result.insert(leaf.position.path, allowed);
    }
    Ok(Some(result))
}
pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    contract(module, f, b).map(|_| ())
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    value: &Value,
    facts: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(upper) = contract(module, f, b)? else {
        return Ok(());
    };
    for (path, sources) in upper {
        ordered(b, value.dyn_ports.len(), path.len())?;
        let Some(actual) = value.dyn_ports.get(&path) else {
            // This solver iteration has not established a producer yet.
            // Keep it Unknown; a public upper cannot bootstrap a real row.
            facts.dyn_return_complete = false;
            continue;
        };
        fee(
            b,
            actual
                .targets
                .len()
                .saturating_mul(sources.len().saturating_add(1)),
        )?;
        for target in &actual.targets {
            fee(
                b,
                target_width(target).saturating_mul(sources.len().saturating_add(1)),
            )?;
            if !sources.contains(target) {
                return Err(failure(
                    "actual returned dyn target exceeds its declared source upper",
                ));
            }
        }
        ordered(b, facts.returned_dyn_ports.len(), path.len())?;
        fee(b, path.len())?;
        let returned = facts.returned_dyn_ports.entry(path.clone()).or_default();
        for target in &actual.targets {
            fee(
                b,
                target_width(target)
                    .saturating_mul(returned.len().saturating_add(1))
                    .saturating_add(1),
            )?;
            if !returned.contains(target) {
                returned.push(target.clone());
            }
        }
        ordered(b, facts.declared_dyn_return_upper.len(), path.len())?;
        facts.declared_dyn_return_upper.insert(path, sources);
    }
    Ok(())
}
pub(super) fn call(
    callee: &TypedFunction,
    summary: &Facts,
    args: &[ValueId],
    values: &[Value],
    state: &State,
    out: &mut Value,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts || callee.retained_return_sources.is_none() {
        return Ok(());
    }
    if !summary.complete {
        out.creation_complete = Some(false);
        return Ok(());
    }
    for (path, sources) in &summary.declared_dyn_return_upper {
        let mut permission = Permission::default();
        for source in sources {
            union(
                &mut permission,
                &substitute(source, args, values, state, b)?,
                b,
            )?;
        }
        fee(b, path.len())?;
        out.dyn_ports.insert(path.clone(), permission);
    }
    Ok(())
}
