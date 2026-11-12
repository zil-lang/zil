//! Declared dyn result uppers are checked against each actual returned row.
//! Calls substitute the declared upper through actual input permissions.
use super::*;
use zil_ir::{ContractProjection, LifeTypeProjection, StorageBinder, StorageLifeContract};
struct Upper {
    targets: Vec<Target>,
    mode: StoredBorrowMode,
}
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
                        heap_parents: Vec::new(),
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
                        heap_parents: Vec::new(),
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
                        heap_parents: Vec::new(),
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
fn place_address(
    f: &TypedFunction,
    place: &StoragePosition,
    mode: StoredBorrowMode,
    b: &mut Budget,
) -> CompileResult<Leaf> {
    fee(b, place.path.len().saturating_add(1))?;
    let StorageBase::Parameter(index) = place.base else {
        return Err(failure("address-of source is not an actual input head"));
    };
    let input = f
        .parameters
        .get(index as usize)
        .and_then(|id| f.locals.get(id.0 as usize))
        .ok_or_else(|| failure("address-of input head is absent"))?;
    let Type::Address { mutable, inner } = &input.ty else {
        return Err(failure("address-of source has no actual borrowed head"));
    };
    let requested = match mode {
        StoredBorrowMode::SharedAddress => false,
        StoredBorrowMode::ExclusiveAddress if *mutable => true,
        _ => return Err(failure("address-of source upgrades its actual head mode")),
    };
    let contract = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("address-of input has no complete signature relation"))?;
    fee(b, contract.bindings.len())?;
    let mut bindings = contract
        .bindings
        .iter()
        .filter(|row| row.position.base == place.base && row.position.path.is_empty());
    let binding = bindings
        .next()
        .ok_or_else(|| failure("address-of head lifetime is Unknown"))?;
    if bindings.next().is_some() {
        return Err(failure("address-of head lifetime is ambiguous"));
    }
    let mut node = inner.as_ref();
    let mut path = Vec::new();
    for step in &place.path {
        let ContractProjection::Place(PlaceProjection::Field(index)) = step else {
            return Err(failure(
                "address-of source crosses a non-inline field selector",
            ));
        };
        node = match node {
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.get(*index as usize).map(|(_, t)| t)
            }
            Type::Tuple(fields) => fields.get(*index as usize),
            _ => None,
        }
        .ok_or_else(|| failure("address-of source is not an actual inline field"))?;
        fee(b, path.len().saturating_add(1))?;
        path.push(PlaceProjection::Field(*index));
    }
    if !node.is_b0_scalar() && *node != Type::Unit {
        return Err(failure(
            "address-of aggregate endpoint needs its complete domain proof",
        ));
    }
    crate::dyn_metadata::ty(node, b)?;
    fee(
        b,
        place
            .path
            .len()
            .saturating_add(path.len())
            .saturating_add(3),
    )?;
    Ok(Leaf {
        position: place.clone(),
        ty: Type::Address {
            mutable: requested,
            inner: Box::new(node.clone()),
        },
        mode,
        binder: binding.binder,
        target: Some(Target {
            base: HoldRoot::Parameter(index),
            stored: vec![],
            retained_port: None,
            place: path,
            heap_element: None,
            heap_parents: vec![],
        }),
    })
}
fn owned_views(
    f: &TypedFunction,
    head: &StoragePosition,
    leaf: &Type,
    mode: StoredBorrowMode,
    b: &mut Budget,
) -> CompileResult<Vec<Leaf>> {
    crate::dyn_metadata::ty(leaf, b)?;
    fee(b, head.path.len().saturating_add(1))?;
    let StorageBase::Parameter(index) = head.base else {
        return Err(failure("owned region head is not an actual parameter"));
    };
    let input = f
        .parameters
        .get(index as usize)
        .and_then(|id| f.locals.get(id.0 as usize))
        .ok_or_else(|| failure("owned region has no actual input head"))?;
    let (
        Type::Address {
            mutable: input_mutable,
            inner,
        },
        Type::Address {
            mutable: result_mutable,
            inner: wanted,
        },
    ) = (&input.ty, leaf)
    else {
        return Err(failure(
            "owned region needs actual address and complete borrowed leaf",
        ));
    };
    if mode
        != if *result_mutable {
            StoredBorrowMode::ExclusiveAddress
        } else {
            StoredBorrowMode::SharedAddress
        }
        || *result_mutable && !input_mutable
        || !(wanted.is_b0_scalar() || wanted.as_ref() == &Type::Unit)
    {
        return Err(failure(
            "owned region changes mode or needs a complete child domain proof",
        ));
    }
    let life = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("owned region head relation is Unknown"))?;
    fee(b, life.bindings.len())?;
    let mut bindings = life
        .bindings
        .iter()
        .filter(|row| row.position.base == head.base && row.position.path.is_empty());
    let binding = bindings
        .next()
        .ok_or_else(|| failure("owned region lacks an actual head lifetime"))?;
    if bindings.next().is_some() {
        return Err(failure("owned region head lifetime is ambiguous"));
    }
    let mut node = inner.as_ref();
    let mut prefix = Vec::new();
    for step in &head.path {
        let ContractProjection::Place(PlaceProjection::Field(i)) = step else {
            return Err(failure("owned region head crosses a nonphysical selector"));
        };
        node = match node {
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.get(*i as usize).map(|(_, ty)| ty)
            }
            Type::Tuple(fields) => fields.get(*i as usize),
            _ => None,
        }
        .ok_or_else(|| failure("owned region head crosses an external or absent field"))?;
        fee(b, prefix.len().saturating_add(1))?;
        prefix.push(PlaceProjection::Field(*i));
    }
    let mut work = vec![(node, prefix)];
    let mut leaves = Vec::new();
    while let Some((node, path)) = work.pop() {
        fee(b, path.len().saturating_add(1))?;
        if path.len() > b.partial_depth.min(128) {
            return Err(failure("owned region directory exceeds depth"));
        }
        if node == wanted.as_ref() {
            crate::dyn_metadata::ty(leaf, b)?;
            fee(
                b,
                head.path.len().saturating_add(path.len()).saturating_add(3),
            )?;
            leaves.push(Leaf {
                position: head.clone(),
                ty: leaf.clone(),
                mode,
                binder: binding.binder,
                target: Some(Target {
                    base: HoldRoot::Parameter(index),
                    stored: vec![],
                    place: path.clone(),
                    retained_port: None,
                    heap_element: None,
                    heap_parents: vec![],
                }),
            });
        }
        match node {
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, (_, child)) in fields.iter().enumerate().rev() {
                    let mut selected = path.clone();
                    selected.push(PlaceProjection::Field(index as u32));
                    work.push((child, selected));
                }
            }
            Type::Tuple(fields) => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, child) in fields.iter().enumerate().rev() {
                    let mut selected = path.clone();
                    selected.push(PlaceProjection::Field(index as u32));
                    work.push((child, selected));
                }
            }
            // These are the boundary of owning reach, not loads of saved
            // pointers. An unrelated same-typed external pointee is excluded.
            Type::Address { .. } | Type::Desc(_) => {}
            ty if ty.is_b0_scalar() || *ty == Type::Unit => {}
            _ => {
                return Err(failure(
                    "owned region needs its actual owning-edge directory",
                ));
            }
        }
    }
    Ok(leaves)
}
fn contract(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<Option<BTreeMap<Vec<ContractProjection>, Upper>>> {
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
        for (ordinal, atom) in sources.iter().enumerate() {
            for previous in &sources[..ordinal] {
                crate::dyn_metadata::recipe(atom, b)?;
                crate::dyn_metadata::recipe(previous, b)?;
                if atom == previous {
                    return Err(failure("retained result repeats a source atom"));
                }
            }
            let derived;
            let input = match atom {
                StorageSource::Exact(source) => {
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
                    if selected.next().is_some() {
                        return Err(failure("retained source input leaf is ambiguous"));
                    }
                    input
                }
                StorageSource::PlaceAddress { place, mode } => {
                    derived = place_address(f, place, *mode, b)?;
                    &derived
                }
                StorageSource::OwnedView {
                    head,
                    leaf: ty,
                    mode,
                } => {
                    for input in owned_views(f, head, ty, *mode, b)? {
                        if input.mode != leaf.mode
                            || input.binder != leaf.binder
                            || input.ty != leaf.ty
                        {
                            return Err(failure(
                                "owned region changes complete leaf, mode or head class",
                            ));
                        }
                        let target = input.target.expect("actual owning region place");
                        fee(
                            b,
                            target_width(&target).saturating_mul(allowed.len().saturating_add(1)),
                        )?;
                        if !allowed.contains(&target) {
                            allowed.push(target);
                        }
                    }
                    continue;
                }
                StorageSource::Static { leaf: ty, mode } => {
                    crate::dyn_metadata::ty(ty, b)?;
                    if *mode != StoredBorrowMode::SharedAddress
                        || !matches!(ty,Type::Address{mutable:false,inner}
                        if matches!(inner.as_ref(),Type::Slice{element} if element.as_ref()==&Type::U8))
                    {
                        return Err(failure(
                            "static source has no supported complete immutable leaf",
                        ));
                    }
                    fee(b, 2)?;
                    derived = Leaf {
                        position: leaf.position.clone(),
                        ty: ty.clone(),
                        mode: *mode,
                        binder: leaf.binder,
                        target: Some(Target {
                            base: HoldRoot::Static,
                            stored: vec![],
                            retained_port: None,
                            place: vec![],
                            heap_element: None,
                            heap_parents: vec![],
                        }),
                    };
                    &derived
                }
                _ => {
                    return Err(failure("retained source atom needs its actual view proof"));
                }
            };
            if input.mode != leaf.mode || input.binder != leaf.binder || input.ty != leaf.ty {
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
        result.insert(
            leaf.position.path,
            Upper {
                targets: allowed,
                mode: leaf.mode,
            },
        );
    }
    Ok(Some(result))
}
pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    contract(module, f, b).map(|_| ())
}
/// Only an actual ordinary function may defer output-only class grounding.
/// The generic signature relation continues to reject unbound result classes.
pub(super) fn register_output_obligations(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.dyn_contexts || !f.parameters.is_empty() {
        return Ok(false);
    }
    let (result, path) = match &f.return_type {
        Type::OwnedDyn { .. } => (&f.return_type, vec![]),
        Type::ErrorUnion { error, result }
            if error.as_ref() == &crate::dyn_dispatch::create_error()
                && matches!(result.as_ref(), Type::OwnedDyn { .. }) =>
        {
            (
                result.as_ref(),
                vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))],
            )
        }
        _ => return Ok(false),
    };
    let Type::OwnedDyn { interface } = result else {
        unreachable!()
    };
    fee(b, module.dyn_interfaces.len())?;
    let interface = module
        .dyn_interfaces
        .iter()
        .find(|i| i.id == *interface)
        .ok_or_else(|| failure("pending output has no actual interface"))?;
    if interface.context.is_none() || interface.retained_life.binder_count == 0 {
        return Ok(false);
    }
    if f.call_kind != CallKind::Direct
        || f.environment.is_some()
        || f.foreign.is_some()
        || f.reference_return_bound.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || !f.protocol_contracts.is_empty()
        || !matches!(
            f.result_mode,
            PassingMode::Infer | PassingMode::Move | PassingMode::Give | PassingMode::Create
        )
    {
        return Err(failure(
            "output class deferral conflicts with another function contract",
        ));
    }
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("output class obligation has no complete signature"))?;
    let count = interface.retained_life.binder_count as usize;
    fee(
        b,
        count.saturating_mul(interface.retained_life.ports.len().saturating_add(3)),
    )?;
    if !c.bindings.is_empty() || !c.static_bindings.is_empty() || c.classes.len() != count {
        return Err(failure(
            "output class obligation omits or invents an actual occurrence",
        ));
    }
    let mut next_fixed = 0;
    let mut next_invocation = 0;
    let mut obligations = Vec::with_capacity(count);
    for (ordinal, row) in c.classes.iter().enumerate() {
        fee(b, path.len().saturating_add(1))?;
        if row.position.base != StorageBase::Result
            || row.position.path != path
            || row.position.class as usize != ordinal
        {
            return Err(failure(
                "pending class does not select its complete original output occurrence",
            ));
        }
        let (actual, limit, next) = match row.binder {
            StorageBinder::Fixed(i) => (i, c.fixed_binder_count, &mut next_fixed),
            StorageBinder::Invocation(i) => (i, c.invocation_binder_count, &mut next_invocation),
        };
        if actual >= limit || actual > *next {
            return Err(failure(
                "pending output class is noncanonical or outside its original namespace",
            ));
        }
        if actual == *next {
            *next += 1;
        }
        let mut ports = Vec::new();
        for (index, port) in interface.retained_life.ports.iter().enumerate() {
            if port.binder as usize != ordinal {
                continue;
            }
            crate::dyn_metadata::ty(&port.leaf, b)?;
            fee(b, path.len().saturating_mul(2).saturating_add(3))?;
            let mut selected = path
                .iter()
                .map(|p| {
                    let LifeTypeProjection::Place(p) = p else {
                        unreachable!()
                    };
                    ContractProjection::Place(*p)
                })
                .collect::<Vec<_>>();
            selected.push(ContractProjection::RetainedPort(index as u32));
            ports.push((
                StoragePosition {
                    base: StorageBase::Result,
                    path: selected,
                },
                port.leaf.clone(),
                port.mode,
            ));
        }
        obligations.push(DynOutputClass {
            binding: row.clone(),
            ports,
        });
    }
    if next_fixed != c.fixed_binder_count || next_invocation != c.invocation_binder_count {
        return Err(failure(
            "pending signature invents an unused binder namespace",
        ));
    }
    ordered(b, b.pending_dyn_outputs.len(), 0)?;
    b.pending_dyn_outputs.insert(f.id, obligations);
    Ok(true)
}

fn ground_output_obligations(
    f: &TypedFunction,
    value: &Value,
    facts: &mut Facts,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    if b.pending_dyn_outputs.is_empty() {
        return Ok(());
    }
    ordered(b, b.pending_dyn_outputs.len(), 0)?;
    // Detach the immutable directory while charging ancestry through Budget.
    let Some(obligations) = b.pending_dyn_outputs.remove(&f.id) else {
        return Ok(());
    };
    let checked = (|| {
        for class in &obligations {
            fee(
                b,
                class
                    .binding
                    .position
                    .path
                    .len()
                    .saturating_add(class.ports.len())
                    .saturating_add(1),
            )?;
            // Phantom classes retain their original occurrence and introduce
            // no data target. Each data-bearing occurrence needs a real row.
            for (position, leaf, mode) in &class.ports {
                crate::dyn_metadata::ty(leaf, b)?;
                ordered(b, value.dyn_ports.len(), position.path.len())?;
                let Some(actual) = value.dyn_ports.get(&position.path) else {
                    facts.dyn_return_complete = false;
                    continue;
                };
                if actual.targets.is_empty() {
                    continue;
                }
                if *mode != StoredBorrowMode::SharedAddress
                    || !matches!(leaf, Type::Address { mutable: false, .. })
                    || actual
                        .targets
                        .iter()
                        .any(|target| target.base != HoldRoot::Static)
                {
                    return Err(failure(
                        "output-only class retains a nonstatic actual target",
                    ));
                }
                for id in ancestry(&actual.loans, loans, b)? {
                    let loan = loans
                        .get(id)
                        .ok_or_else(|| failure("pending output ancestry exceeds actual arena"))?;
                    fee(b, loan.targets.len().saturating_add(1))?;
                    if loan.receiver_head
                        || loan
                            .targets
                            .iter()
                            .any(|target| target.base != HoldRoot::Static)
                    {
                        return Err(failure(
                            "output-only static class retains an activation parent",
                        ));
                    }
                }
            }
        }
        Ok(())
    })();
    b.pending_dyn_outputs.insert(f.id, obligations);
    checked
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    value: &Value,
    facts: &mut Facts,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    ground_output_obligations(f, value, facts, loans, b)?;
    let Some(upper) = contract(module, f, b)? else {
        return Ok(());
    };
    for (
        path,
        Upper {
            targets: sources, ..
        },
    ) in upper
    {
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
        let declares_static = sources.iter().any(|source| source.base == HoldRoot::Static);
        if actual
            .targets
            .iter()
            .any(|target| target.base == HoldRoot::Static)
        {
            if !declares_static {
                return Err(failure(
                    "actual program-static target is absent from its declared upper",
                ));
            }
            for id in ancestry(&actual.loans, loans, b)? {
                let parent = loans
                    .get(id)
                    .ok_or_else(|| failure("static result ancestry exceeds actual loan arena"))?;
                if parent.receiver_head {
                    return Err(failure(
                        "static result cannot erase an actual receiver head parent",
                    ));
                }
                for target in &parent.targets {
                    fee(
                        b,
                        target_width(target).saturating_mul(sources.len().saturating_add(1)),
                    )?;
                    if target.base != HoldRoot::Static && !sources.contains(target) {
                        return Err(failure(
                            "static result would erase an activation or undeclared parent",
                        ));
                    }
                }
            }
        }
        for target in &actual.targets {
            fee(
                b,
                target_width(target).saturating_mul(sources.len().saturating_add(1)),
            )?;
            if !(target.base == HoldRoot::Static && declares_static) && !sources.contains(target) {
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
// Stable sites are allocated from the actual validated output directory. A
// site allocates no permission until an independently complete Return summary
// is substituted through the caller's real pre-call inputs.
fn transports_existing(callee: &TypedFunction, source: &Target) -> bool {
    let HoldRoot::Parameter(index) = source.base else {
        return source.base == HoldRoot::Static;
    };
    let Some(local) = callee
        .parameters
        .get(index as usize)
        .and_then(|id| callee.locals.get(id.0 as usize))
    else {
        return false;
    };
    matches!(
        local.ty,
        Type::BorrowedStorage { .. }
            | Type::OwnedDyn { .. }
            | Type::Package { .. }
            | Type::Tuple(_)
    )
}
pub(super) fn sites(
    module: &TypedModule,
    instruction: &Instruction,
    sites: &mut BTreeMap<DynamicLoanKey, (usize, bool)>,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    fee(b, 1)?;
    let InstructionKind::Call { function, .. } = &instruction.kind else {
        return Ok(());
    };
    let callee = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| failure("dynamic retained call target absent"))?;
    if callee.retained_return_sources.is_none() {
        return Ok(());
    }
    let Some(upper) = contract(module, callee, b)? else {
        return Ok(());
    };
    for (path, row) in upper {
        for source in row.targets {
            if transports_existing(callee, &source) {
                continue;
            }
            fee(
                b,
                path.len()
                    .saturating_add(target_width(&source))
                    .saturating_add(3),
            )?;
            ordered(
                b,
                sites.len(),
                path.len().saturating_add(target_width(&source)),
            )?;
            let id = loans.len();
            loans.push(Loan::default());
            let mutable = row.mode == StoredBorrowMode::ExclusiveAddress;
            b.stored_exclusive_loans |= mutable;
            if sites
                .insert(
                    (
                        instruction as *const Instruction as usize,
                        path.clone(),
                        source,
                    ),
                    (id, mutable),
                )
                .is_some()
            {
                return Err(failure("dynamic retained child site repeated"));
            }
        }
    }
    Ok(())
}
fn retain(
    instruction: &Instruction,
    path: &[ContractProjection],
    source: &Target,
    permission: &Permission,
    sites: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Permission> {
    fee(b, path.len().saturating_add(target_width(source)))?;
    ordered(
        b,
        sites.dynamic.len(),
        path.len().saturating_add(target_width(source)),
    )?;
    let (id, mutable) = sites
        .dynamic
        .get(&(
            instruction as *const Instruction as usize,
            path.to_vec(),
            source.clone(),
        ))
        .ok_or_else(|| failure("dynamic borrowed result has no stable retained child site"))?;
    let loan = loans
        .get_mut(*id)
        .ok_or_else(|| failure("dynamic retained child exceeds actual loan arena"))?;
    let mut joined = Permission {
        targets: std::mem::take(&mut loan.targets),
        loans: std::mem::take(&mut loan.parents),
    };
    union(&mut joined, permission, b)?;
    joined.loans.remove(id);
    loan.targets = joined.targets;
    loan.parents = joined.loans;
    loan.mutable = *mutable;
    let mut out = Permission::default();
    union(&mut out, permission, b)?;
    out.loans.clear();
    fee(b, 1)?;
    out.loans.insert(*id);
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    instruction: &Instruction,
    callee: &TypedFunction,
    summary: &Facts,
    args: &[ValueId],
    values: &[Value],
    state: &State,
    out: &mut Value,
    sites: &SlotLoanIds,
    loans: &mut [Loan],
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
            let actual = substitute(source, args, values, state, b)?;
            let actual = if transports_existing(callee, source) {
                actual
            } else {
                retain(instruction, path, source, &actual, sites, loans, b)?
            };
            union(&mut permission, &actual, b)?;
        }
        fee(b, path.len())?;
        out.dyn_ports.insert(path.clone(), permission);
    }
    Ok(())
}
