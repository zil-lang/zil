//! Actual Dyn Context permissions. This ledger has its own typed namespace;
//! ordinary receiver/head permissions remain in Payload.address.
use super::*;
type ContractPath = Vec<ContractProjection>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Port {
    pub permission: Permission,
    /// Only summaries retain symbolic non-target parents; caller transport
    /// restores their actual loans without adding their roots as targets.
    pub parent_sources: Vec<Source>,
    /// Actual head-child sites retain their role across an ordinary summary;
    /// these are additional parents, never port data targets.
    pub head_sources: Vec<Source>,
    /// Set only by a real input, a complete callee, or a complete constructor.
    /// In particular explicit unused provider rows are known empty.
    pub known: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Rows(pub Vec<(ContractPath, Port)>);

/// A borrowed view retains the exact actual provider/head pairing. Joining
/// CFG alternatives never forms a cross product of tables and owner positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct View {
    pub table: Option<DynVtableId>,
    pub head: Permission,
}
pub(super) fn views_work(views: &[View], b: &mut Budget) -> Check<()> {
    b.spend(views.len())?;
    for view in views {
        permission_work(&view.head, b)?;
    }
    Ok(())
}
fn view_insert(views: &mut Vec<View>, view: &View, b: &mut Budget) -> Check<bool> {
    views_work(views, b)?;
    permission_work(&view.head, b)?;
    if views.contains(view) {
        return Ok(false);
    }
    b.spend(views.len().saturating_add(1))?;
    views.push(view.clone());
    Ok(true)
}
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-SOURCE", None, message)
}
pub(super) fn work(rows: &Rows, b: &mut Budget) -> Check<()> {
    b.spend(rows.0.len())?;
    for (path, port) in &rows.0 {
        b.spend(path.len().saturating_add(1))?;
        permission_work(&port.permission, b)?;
        source_work(&port.parent_sources, b)?;
        source_work(&port.head_sources, b)?;
    }
    Ok(())
}
pub(super) fn find(
    rows: &Rows,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<Option<usize>> {
    b.spend(rows.0.len().saturating_mul(path.len().saturating_add(1)))?;
    Ok(rows.0.iter().position(|(p, _)| p == path))
}
pub(super) fn insert(
    rows: &mut Rows,
    path: &[ContractProjection],
    port: &Port,
    b: &mut Budget,
) -> Check<bool> {
    if let Some(at) = find(rows, path, b)? {
        let old = &mut rows.0[at].1;
        let mut changed = join_permission(&mut old.permission, &port.permission, b)?;
        changed |= add_sources(&mut old.parent_sources, &port.parent_sources, b)?;
        changed |= add_sources(&mut old.head_sources, &port.head_sources, b)?;
        changed |= port.known && !old.known;
        old.known |= port.known;
        Ok(changed)
    } else {
        b.spend(path.len().saturating_add(rows.0.len()).saturating_add(2))?;
        permission_work(&port.permission, b)?;
        rows.0.push((path.to_vec(), port.clone()));
        Ok(true)
    }
}
pub(super) fn join(to: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let mut changed = false;
    for view in &from.dyn_views {
        changed |= view_insert(&mut to.dyn_views, view, b)?;
    }
    for (path, port) in &from.dyn_ports.0 {
        changed |= insert(&mut to.dyn_ports, path, port, b)?;
    }
    Ok(changed)
}
pub(super) fn shape(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<Vec<(ContractPath, StoredBorrowMode)>> {
    let mut pending = vec![(ty, ContractPath::new())];
    let mut rows = Vec::new();
    while let Some((ty, path)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if path.len() > b.limits.max_call_depth {
            return Err(limit("Dyn carrier path exceeds selected depth"));
        }
        match ty {
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                let actual = crate::dynamic_ports::interface(module, *interface, b)?;
                b.spend(
                    actual
                        .retained_life
                        .ports
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (index, port) in actual.retained_life.ports.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::RetainedPort(index as u32));
                    rows.push((p, port.mode));
                }
            }
            Type::Address { inner, .. } | Type::BorrowedStorage { storage: inner, .. } => {
                pending.push((inner, path))
            }
            Type::Tuple(fields) => {
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, ty) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, p));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, (_, ty)) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, p));
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(path.len().saturating_add(2).saturating_mul(2))?;
                for (arm, child) in [(true, error), (false, result)] {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::ErrorArm(arm)));
                    pending.push((child, p));
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                    for (index, child) in fields.iter().enumerate() {
                        let mut p = path.clone();
                        p.push(ContractProjection::Place(PlaceProjection::Variant(
                            VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            },
                        )));
                        pending.push((child, p));
                    }
                }
            }
            Type::Closure(id) => {
                let definition = crate::closures::definition(module, *id, None)?;
                b.spend(
                    definition
                        .captures
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (index, child) in definition.captures.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Capture(index as u32));
                    pending.push((child, p));
                }
            }
            Type::Array { element: child, .. }
            | Type::Managed(child)
            | Type::Desc(child)
            | Type::OwnedStore { element: child }
            | Type::Mutex { payload: child }
            | Type::MutexGuard { payload: child, .. }
            | Type::Registration { environment: child }
            | Type::AsyncRegistration { environment: child }
            | Type::Opaque { hidden: child, .. } => {
                if crate::dynamic_ports::has_context(module, child, b)? {
                    return Err(unsupported(
                        "Context carrier requires a complete actual container/heap port ledger",
                        None,
                    ));
                }
            }
            // Retained Task pointers keep their separate namespace and parent
            // proof. An owning result cannot disappear as an empty Dyn row.
            Type::Task { result, .. } if crate::dynamic_ports::has_context(module, result, b)? => {
                return Err(unsupported(
                    "Task Context result requires its actual ownership and port transport",
                    None,
                ));
            }
            _ => {}
        }
        if pending.len().saturating_add(rows.len()) > b.limits.max_nodes {
            return Err(limit("Dyn carrier graph exceeds selected node count"));
        }
    }
    Ok(rows)
}
fn carrier_path(places: &[ContractProjection], index: u32, b: &mut Budget) -> Check<ContractPath> {
    b.spend(places.len().saturating_add(1))?;
    let mut p = places.to_vec();
    p.push(ContractProjection::RetainedPort(index));
    Ok(p)
}
pub(super) fn select(
    payload: &Payload,
    places: &[ContractProjection],
    index: u32,
    b: &mut Budget,
) -> Check<Permission> {
    let path = carrier_path(places, index, b)?;
    let Some(at) = find(&payload.dyn_ports, &path, b)? else {
        // Solver bottom. Every actual producer is checked for a complete
        // known row before any result or access certificate is published.
        return Ok(Permission::default());
    };
    let port = &payload.dyn_ports.0[at].1;
    if !port.known {
        return Ok(Permission::default());
    }
    permission_work(&port.permission, b)?;
    Ok(port.permission.clone())
}

fn empty_row(
    payload: &Payload,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<bool> {
    let path = carrier_path(carrier, port, b)?;
    let Some(at) = find(&payload.dyn_ports, &path, b)? else {
        return Ok(false);
    };
    let row = &payload.dyn_ports.0[at].1;
    permission_work(&row.permission, b)?;
    Ok(row.known && row.permission.sources.is_empty())
}

fn empty_at_head(
    head: &Permission,
    state: &Frame,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<bool> {
    permission_work(head, b)?;
    if head.sources.is_empty() {
        return Ok(false);
    }
    for source in &head.sources {
        let actual = holder(state, source)?;
        b.spend(source.inline.len().saturating_add(carrier.len()))?;
        let mut path = source
            .inline
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        path.extend(carrier);
        if !empty_row(actual, &path, port, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn owner_hole(local: LocalId) -> Source {
    Source {
        static_dependency: None,
        base: HoldRoot::Local(local),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: Vec::new(),
    }
}
pub(super) fn vacate_owner(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    state: &mut Frame,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let Type::OwnedDyn { interface } = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| bad("vacated owner local is absent"))?
        .ty
    else {
        return Ok(());
    };
    if crate::dynamic_ports::interface(module, *interface, b)?
        .context
        .is_some()
    {
        add_sources(&mut state.vacant, &[owner_hole(local)], b)?;
    }
    Ok(())
}

pub(super) fn moved_owner(state: &Frame, local: LocalId, b: &mut Budget) -> Check<bool> {
    b.spend(state.vacant.len().saturating_add(1))?;
    Ok(state
        .locals
        .get(local.0 as usize)
        .is_some_and(|p| p.dyn_ports.0.is_empty())
        && state.vacant.contains(&owner_hole(local)))
}

/// Empty access is vacuous only for a complete actual known-empty row. Solver
/// bottom, missing storage, or a missing producer binding is never that proof.
#[allow(clippy::too_many_arguments)]
fn empty_actual(
    module: &TypedModule,
    f: &TypedFunction,
    argument: ValueId,
    payload: &Payload,
    state: &Frame,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<bool> {
    if carrier.is_empty() && !payload.dyn_views.is_empty() {
        b.spend(payload.dyn_views.len())?;
        for view in &payload.dyn_views {
            if let Some(table) = view.table {
                b.spend(module.dyn_vtables.len())?;
                let table = module
                    .dyn_vtables
                    .iter()
                    .find(|row| row.id == table)
                    .ok_or_else(|| bad("known-empty view table is absent"))?;
                let binding = table
                    .ports
                    .get(port as usize)
                    .filter(|row| row.port == port)
                    .ok_or_else(|| bad("known-empty view port binding is absent"))?;
                if !binding.sources.is_empty() {
                    return Ok(false);
                }
                // A concrete view still requires a real current receiver.
                permission_work(&view.head, b)?;
                if view.head.sources.is_empty() {
                    return Ok(false);
                }
            } else if !empty_at_head(&view.head, state, carrier, port, b)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let current = matches!(vt(f, argument)?, Type::Address { inner, .. } if !matches!(inner.as_ref(), Type::Dyn { .. }));
    if current {
        empty_at_head(&payload.address, state, carrier, port, b)
    } else {
        empty_row(payload, carrier, port, b)
    }
}
pub(super) fn seed(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. })) {
        view_insert(
            &mut out.dyn_views,
            &View {
                table: None,
                head: out.address.clone(),
            },
            b,
        )?;
    }
    for (path, mode) in shape(module, ty, b)? {
        let Some((ContractProjection::RetainedPort(index), prefix)) = path.split_last() else {
            return Err(bad("Dyn input lacks a typed terminal port"));
        };
        b.spend(path.len().saturating_add(4))?;
        let source = Source {
            static_dependency: None,
            retained_port: Some(PortOrigin::Dyn {
                carrier: prefix.to_vec(),
                port: *index,
            }),
            base,
            leaf: Vec::new(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let id = loans.len();
        let exclusive = mode == StoredBorrowMode::ExclusiveAddress;
        loans.push(Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive,
        });
        b.stored_exclusive_loans |= exclusive;
        insert(
            &mut out.dyn_ports,
            &path,
            &Port {
                parent_sources: Vec::new(),
                head_sources: Vec::new(),
                permission: Permission {
                    sources: vec![source],
                    loans: vec![id],
                },
                known: true,
            },
            b,
        )?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    module: &TypedModule,
    f: &TypedFunction,
    callee: &TypedFunction,
    instruction: &Instruction,
    summary: &Summary,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    out: &mut Payload,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    if summary.complete && !summary.dyn_ports.0.is_empty() {
        dyn_returns::validate(module, callee, &summary.dyn_ports, b)?;
    }
    for (path, source) in &summary.dyn_ports.0 {
        let mut permission = Permission::default();
        let published;
        let originals = if summary.complete {
            published = dyn_returns::published_sources(module, callee, path, b)?;
            &published
        } else {
            &source.permission.sources
        };
        source_work(&source.head_sources, b)?;
        let mut heads = source.head_sources.clone();
        for original in originals {
            if summary.complete {
                add_sources(
                    &mut heads,
                    &dyn_returns::published_heads(module, callee, path, original, b)?,
                    b,
                )?;
            }
        }
        let mut retained_heads = Vec::new();
        for head in &heads {
            let permission = instantiate(module, f, head, args, values, state, b)?;
            access_head(&permission, false, b)?;
            dyn_returns::initialized(state, &permission, b)?;
            b.spend(width(head).saturating_add(path.len()).saturating_add(2))?;
            let retained = dyn_leases::retain(
                sites,
                loans,
                instruction,
                dyn_leases::Destination::Result(path.clone()),
                dyn_leases::Component::Head(head.clone()),
                mode_at(module, &callee.return_type, path, b)?,
                &permission,
                b,
            )?;
            add_loans(&mut retained_heads, &retained.loans, b)?;
        }
        for original in originals {
            let mut actual = instantiate(module, f, original, args, values, state, b)?;
            dyn_returns::initialized(state, &actual, b)?;
            // Head children are prepared before data children. The latter
            // retain both the original data parents and these distinct head
            // sites without ever publishing the heads as pointee targets.
            add_loans(&mut actual.loans, &retained_heads, b)?;
            if dyn_leases::borrowed_input(callee, original)? {
                b.spend(path.len().saturating_add(width(original)).saturating_add(2))?;
                actual = dyn_leases::retain(
                    sites,
                    loans,
                    instruction,
                    dyn_leases::Destination::Result(path.clone()),
                    dyn_leases::Component::Summary(original.clone()),
                    mode_at(module, &callee.return_type, path, b)?,
                    &actual,
                    b,
                )?;
            }
            join_permission(&mut permission, &actual, b)?;
        }
        add_loans(&mut permission.loans, &retained_heads, b)?;
        for parent in &source.parent_sources {
            let actual = instantiate(module, f, parent, args, values, state, b)?;
            add_loans(&mut permission.loans, &actual.loans, b)?;
        }
        insert(
            &mut out.dyn_ports,
            path,
            &Port {
                parent_sources: Vec::new(),
                head_sources: Vec::new(),
                permission,
                known: source.known && summary.complete,
            },
            b,
        )?;
    }
    Ok(())
}

fn mode_at(
    module: &TypedModule,
    ty: &Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<StoredBorrowMode> {
    let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
        return Err(bad("retained lease has no typed terminal Dyn port"));
    };
    let (Type::Dyn { interface } | Type::OwnedDyn { interface }) =
        carrier_type(module, ty, carrier, b)?
    else {
        return Err(bad("retained lease destination has no actual Dyn carrier"));
    };
    crate::dynamic_ports::interface(module, *interface, b)?
        .retained_life
        .ports
        .get(*port as usize)
        .map(|p| p.mode)
        .ok_or_else(|| bad("retained lease destination port is absent"))
}

fn old_port(
    source: &Source,
    parameter: u32,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<bool> {
    source_work(std::slice::from_ref(source), b)?;
    let Some(PortOrigin::Dyn { carrier, port }) = &source.retained_port else {
        return Ok(false);
    };
    let expected = carrier_path(carrier, *port, b)?;
    b.spend(path.len().saturating_add(expected.len()))?;
    Ok(source.base == HoldRoot::Parameter(parameter)
        && source.leaf.is_empty()
        && source.inline.is_empty()
        && source.heap_element.is_none()
        && expected == path)
}

pub(super) struct PreparedAfter(Vec<(Permission, Rows)>);
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_after(
    module: &TypedModule,
    f: &TypedFunction,
    callee: &TypedFunction,
    instruction: &Instruction,
    summary: &Summary,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<PreparedAfter> {
    if !b.dyn_contexts {
        return Ok(PreparedAfter(Vec::new()));
    }
    let mut prepared = Vec::new();
    b.spend(summary.dyn_after.len())?;
    for (parameter, rows) in &summary.dyn_after {
        let argument = *args
            .get(*parameter as usize)
            .ok_or_else(|| bad("helper after receiver operand is absent"))?;
        let receiver = values
            .get(argument.0 as usize)
            .ok_or_else(|| bad("helper after receiver payload is absent"))?;
        let mut next = Rows::default();
        for (path, source) in &rows.0 {
            if !source.head_sources.is_empty() {
                return Err(unsupported(
                    "new bounded head in receiver after-state requires an atomic head-site transition proof",
                    None,
                ));
            }
            let mut permission = Permission::default();
            for original in &source.permission.sources {
                let mut actual = instantiate(module, f, original, args, values, state, b)?;
                if !old_port(original, *parameter, path, b)?
                    && dyn_leases::borrowed_input(callee, original)?
                {
                    let ty = &callee.locals[callee.parameters[*parameter as usize].0 as usize].ty;
                    b.spend(path.len().saturating_add(width(original)).saturating_add(2))?;
                    actual = dyn_leases::retain(
                        sites,
                        loans,
                        instruction,
                        dyn_leases::Destination::After(StoragePosition {
                            base: StorageBase::Parameter(*parameter),
                            path: path.clone(),
                        }),
                        dyn_leases::Component::Summary(original.clone()),
                        mode_at(module, ty, path, b)?,
                        &actual,
                        b,
                    )?;
                }
                join_permission(&mut permission, &actual, b)?;
            }
            for parent in &source.parent_sources {
                let actual = instantiate(module, f, parent, args, values, state, b)?;
                add_loans(&mut permission.loans, &actual.loans, b)?;
            }
            insert(
                &mut next,
                path,
                &Port {
                    parent_sources: Vec::new(),
                    head_sources: Vec::new(),
                    permission,
                    known: source.known && summary.complete,
                },
                b,
            )?;
        }
        permission_work(&receiver.address, b)?;
        prepared.push((receiver.address.clone(), next));
    }
    Ok(PreparedAfter(prepared))
}
pub(super) fn publish_after(
    state: &mut Frame,
    prepared: PreparedAfter,
    b: &mut Budget,
) -> Check<()> {
    for (head, rows) in prepared.0 {
        publish_owner(&head, state, &rows, b)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn select_actual(
    module: &TypedModule,
    f: &TypedFunction,
    argument: ValueId,
    payload: &Payload,
    state: &Frame,
    places: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<Permission> {
    let actual_type = carrier_type(module, vt(f, argument)?, places, b)?;
    let (Type::Dyn { interface } | Type::OwnedDyn { interface }) = actual_type else {
        return Err(bad("Dyn source selector has a non-Dyn actual endpoint"));
    };
    let item = crate::dynamic_ports::interface(module, *interface, b)?;
    if item.retained_life.ports.get(port as usize).is_none() {
        return Err(bad("actual Dyn endpoint port is absent"));
    }
    if places.is_empty() && !payload.dyn_views.is_empty() {
        let mut out = Permission::default();
        for view in &payload.dyn_views {
            join_permission(&mut out, &view_port(module, view, port, state, b)?, b)?;
        }
        return Ok(out);
    }
    // Addresses of owner storage observe its current value. Extracted pointer
    // SSA values keep their original permission independently of that storage.
    let current = matches!(vt(f, argument)?, Type::Address { inner, .. } if !matches!(inner.as_ref(), Type::Dyn { .. }));
    if !current {
        return select(payload, places, port, b);
    }
    let mut out = Permission::default();
    for origin in &payload.address.sources {
        if origin.retained_port.is_some()
            || !origin.leaf.is_empty()
            || origin.heap_element.is_some()
        {
            return Err(bad(
                "owner storage address has no complete actual Dyn carrier route",
            ));
        }
        let holder = match origin.base {
            HoldRoot::Local(local) => state.locals.get(local.0 as usize),
            HoldRoot::Parameter(index) => state.external.get(index as usize),
            HoldRoot::Capture(index) => state.captures.get(index as usize),
            HoldRoot::Static => None,
        }
        .ok_or_else(|| bad("actual Dyn owner storage is absent"))?;
        b.spend(origin.inline.len().saturating_add(places.len()))?;
        let mut path = origin
            .inline
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        path.extend(places);
        join_permission(&mut out, &select(holder, &path, port, b)?, b)?;
    }
    // The head is a separate receiver/view obligation. Publishing it as a
    // permanent data parent would pin an owner to its own temporary borrow.
    Ok(out)
}
fn wrapped(rows: &Rows, prefix: ContractProjection, b: &mut Budget) -> Check<Rows> {
    let mut out = Rows::default();
    for (path, port) in &rows.0 {
        b.spend(path.len().saturating_add(1))?;
        let mut p = vec![prefix];
        p.extend(path);
        insert(&mut out, &p, port, b)?;
    }
    Ok(out)
}
fn projected(rows: &Rows, prefix: &[ContractProjection], b: &mut Budget) -> Check<Rows> {
    let mut out = Rows::default();
    for (path, port) in &rows.0 {
        b.spend(prefix.len().saturating_add(path.len()).saturating_add(1))?;
        if let Some(tail) = path.strip_prefix(prefix) {
            insert(&mut out, tail, port, b)?;
        }
    }
    Ok(out)
}
fn remove(rows: &mut Rows, prefix: &[ContractProjection], b: &mut Budget) -> Check<()> {
    b.spend(rows.0.len().saturating_mul(prefix.len().saturating_add(1)))?;
    rows.0.retain(|(path, _)| !path.starts_with(prefix));
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Option<Payload>> {
    if !b.dyn_contexts {
        return Ok(None);
    }
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| bad("Dyn operation operand is absent"))
    };
    let mut out = Payload::default();
    match &instruction.kind {
        InstructionKind::MakeDynOwned {
            interface,
            vtable,
            source,
            ..
        }
        | InstructionKind::MakeDynBorrow {
            interface,
            vtable,
            source,
            ..
        } => {
            b.spend(module.dyn_vtables.len())?;
            let table = module
                .dyn_vtables
                .iter()
                .find(|table| table.id == *vtable && table.interface == *interface)
                .ok_or_else(|| bad("Dyn construction has no exact actual table"))?;
            crate::dynamic_ports::validate_table(module, table, b)?;
            let actual = get(*source)?;
            let borrowed = matches!(instruction.kind, InstructionKind::MakeDynBorrow { .. });
            if borrowed {
                join_permission(&mut out.address, &actual.address, b)?;
                view_insert(
                    &mut out.dyn_views,
                    &View {
                        table: Some(*vtable),
                        head: actual.address.clone(),
                    },
                    b,
                )?;
            }
            for binding in &table.ports {
                let mut permission = Permission::default();
                let mut known = true;
                for position in &binding.sources {
                    let path = crate::dynamic_ports::source_places(position, b)?;
                    let selected = if borrowed {
                        super::select(state, &actual.address, &path, b)?
                    } else if let Some(p) = leaf(actual, &path, b)? {
                        permission_work(p, b)?;
                        p.clone()
                    } else {
                        Permission::default()
                    };
                    known &= !selected.sources.is_empty();
                    join_permission(&mut permission, &selected, b)?;
                }
                let path = if borrowed {
                    vec![ContractProjection::RetainedPort(binding.port)]
                } else {
                    vec![
                        ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                        ContractProjection::RetainedPort(binding.port),
                    ]
                };
                insert(
                    &mut out.dyn_ports,
                    &path,
                    &Port {
                        parent_sources: Vec::new(),
                        head_sources: Vec::new(),
                        permission,
                        known,
                    },
                    b,
                )?;
            }
        }
        InstructionKind::BorrowDynOwned { local, .. } => {
            let actual = &state.locals[local.0 as usize];
            if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
                join_permission(&mut out.address, &actual.address, b)?;
            } else {
                b.spend(1)?;
                out.address.sources.push(Source {
                    static_dependency: None,
                    retained_port: None,
                    base: HoldRoot::Local(*local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                });
            }
            join(&mut out, actual, b)?;
            view_insert(
                &mut out.dyn_views,
                &View {
                    table: None,
                    head: out.address.clone(),
                },
                b,
            )?;
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            arguments,
        } => {
            let definition = crate::dynamic_ports::interface(module, *interface, b)?;
            let slot = definition
                .slots
                .get(*slot as usize)
                .ok_or_else(|| bad("actual dynamic method slot is absent"))?;
            if definition.context.is_none() {
                return Ok(None);
            }
            let mut args = Vec::with_capacity(arguments.len().saturating_add(1));
            b.spend(arguments.len().saturating_add(1))?;
            args.push(*callee);
            args.extend(arguments);
            match &slot.contract.result {
                Some(DynResultSource::Value) => {}
                Some(DynResultSource::Borrow { sources, .. }) => {
                    for atom in sources {
                        let source = crate::exact_storage_source(atom)?;
                        join_permission(
                            &mut out.address,
                            &slot_source(module, f, source, &args, values, state, b)?,
                            b,
                        )?;
                    }
                }
                _ => return Err(bad("dynamic method result source is Unknown")),
            }
            if matches!(slot.result, Type::Address { .. })
                && slot
                    .storage_life
                    .as_ref()
                    .and_then(|life| crate::signature_life::outer(life, StorageBase::Result))
                    == Some(StorageBinder::Invocation(0))
            {
                add_loans(&mut out.address.loans, &get(*callee)?.address.loans, b)?;
            }
            // Result sources and all after rows read the same call-before
            // state. Post-state rows are prepared before any owner mutation.
            let guarantees = slot
                .contract
                .guarantees
                .as_ref()
                .ok_or_else(|| bad("dynamic method after sources are Unknown"))?;
            let mut prepared = Vec::new();
            b.spend(guarantees.len())?;
            for guarantee in guarantees {
                let DynGuarantee::StorageSources(binding) = guarantee else {
                    return Err(bad(
                        "dynamic method guarantee lacks actual source integration",
                    ));
                };
                let [ContractProjection::RetainedPort(port)] = binding.position.path.as_slice()
                else {
                    return Err(bad(
                        "dynamic method after destination is not a direct receiver port",
                    ));
                };
                if binding.position.base != StorageBase::Parameter(0) {
                    return Err(bad("dynamic method after destination is not its receiver"));
                }
                let mut permission = Permission::default();
                for atom in &binding.sources {
                    let source = crate::exact_storage_source(atom)?;
                    let mut actual = slot_source(module, f, source, &args, values, state, b)?;
                    if source != &binding.position {
                        b.spend(
                            binding
                                .position
                                .path
                                .len()
                                .saturating_add(source.path.len())
                                .saturating_add(2),
                        )?;
                        actual = dyn_leases::retain(
                            sites,
                            loans,
                            instruction,
                            dyn_leases::Destination::After(binding.position.clone()),
                            dyn_leases::Component::Slot(source.clone()),
                            definition.retained_life.ports[*port as usize].mode,
                            &actual,
                            b,
                        )?;
                    }
                    join_permission(&mut permission, &actual, b)?;
                }
                prepared.push((*port, permission));
            }
            publish(module, get(*callee)?, state, &prepared, b)?;
        }
        InstructionKind::PlaceTagLocal { .. } | InstructionKind::ErrorTag { .. }
            if wrapper(module, f, instruction, b)? => {}
        InstructionKind::TakeVariantField { local, path }
            if wrapper(module, f, instruction, b)? =>
        {
            b.spend(path.len())?;
            let prefix = path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            out.dyn_ports = projected(&state.locals[local.0 as usize].dyn_ports, &prefix, b)?;
            remove(&mut state.locals[local.0 as usize].dyn_ports, &prefix, b)?;
        }
        InstructionKind::DropErrorUnion { local } if wrapper(module, f, instruction, b)? => {
            state.locals[local.0 as usize].dyn_ports = Rows::default();
        }
        InstructionKind::DropDynOwned { local } => {
            state.locals[local.0 as usize].dyn_ports = Rows::default();
            vacate_owner(module, f, *local, state, b)?;
        }
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } => {
            for (index, value) in fields.iter().enumerate() {
                let rows = wrapped(
                    &get(*value)?.dyn_ports,
                    ContractProjection::Place(PlaceProjection::Field(index as u32)),
                    b,
                )?;
                for (p, port) in rows.0 {
                    insert(&mut out.dyn_ports, &p, &port, b)?;
                }
            }
        }
        InstructionKind::MakeErrorUnion { value, is_error } => {
            out.dyn_ports = wrapped(
                &get(*value)?.dyn_ports,
                ContractProjection::Place(PlaceProjection::ErrorArm(*is_error)),
                b,
            )?;
            let result = vt(
                f,
                instruction
                    .result
                    .ok_or_else(|| bad("outcome constructor result is absent"))?,
            )?;
            if outcome(module, result, b)? {
                let inactive = ContractProjection::Place(PlaceProjection::ErrorArm(!*is_error));
                for (path, _) in shape(module, result, b)? {
                    if path.first() == Some(&inactive) {
                        insert(
                            &mut out.dyn_ports,
                            &path,
                            &Port {
                                permission: Permission::default(),
                                parent_sources: vec![],
                                head_sources: Vec::new(),
                                known: true,
                            },
                            b,
                        )?;
                    }
                }
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error }
        | InstructionKind::TakeErrorUnion { value, is_error } => {
            out.dyn_ports = projected(
                &get(*value)?.dyn_ports,
                &[ContractProjection::Place(PlaceProjection::ErrorArm(
                    *is_error,
                ))],
                b,
            )?;
        }
        InstructionKind::ExtractField { aggregate, index } => {
            out.dyn_ports = projected(
                &get(*aggregate)?.dyn_ports,
                &[ContractProjection::Place(PlaceProjection::Field(*index))],
                b,
            )?;
        }
        InstructionKind::TakePackageField { local, index }
        | InstructionKind::ReadPackageField { local, index } => {
            let prefix = [ContractProjection::Place(PlaceProjection::Field(*index))];
            out.dyn_ports = projected(&state.locals[local.0 as usize].dyn_ports, &prefix, b)?;
            if matches!(instruction.kind, InstructionKind::TakePackageField { .. }) {
                remove(&mut state.locals[local.0 as usize].dyn_ports, &prefix, b)?;
            }
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            let prefix = ContractProjection::Place(PlaceProjection::Field(*index));
            remove(&mut state.locals[local.0 as usize].dyn_ports, &[prefix], b)?;
            let rows = wrapped(&get(*value)?.dyn_ports, prefix, b)?;
            for (p, port) in rows.0 {
                insert(&mut state.locals[local.0 as usize].dyn_ports, &p, &port, b)?;
            }
        }
        InstructionKind::DropPackageField { local, index } => {
            remove(
                &mut state.locals[local.0 as usize].dyn_ports,
                &[ContractProjection::Place(PlaceProjection::Field(*index))],
                b,
            )?;
        }
        InstructionKind::ReborrowAddress { address, .. } => {
            join(&mut out, get(*address)?, b)?;
        }
        _ => return Ok(None),
    }
    Ok(Some(out))
}

pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let id = match &instruction.kind {
        InstructionKind::MakeDynOwned { interface, .. }
        | InstructionKind::MakeDynBorrow { interface, .. }
        | InstructionKind::CallDynSlot { interface, .. } => Some(*interface),
        InstructionKind::BorrowDynOwned { local, .. } | InstructionKind::DropDynOwned { local } => {
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("dynamic owner local is absent"))?
                .ty;
            let ty = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            if let Type::OwnedDyn { interface } = ty {
                Some(*interface)
            } else {
                None
            }
        }
        _ => None,
    };
    Ok(id
        .map(|id| crate::dynamic_ports::interface(module, id, b).map(|d| d.context.is_some()))
        .transpose()?
        .unwrap_or(false))
}
fn holder<'a>(state: &'a Frame, origin: &Source) -> Check<&'a Payload> {
    if origin.retained_port.is_some() || !origin.leaf.is_empty() || origin.heap_element.is_some() {
        return Err(bad("Dyn head lacks an actual storage position"));
    }
    match origin.base {
        HoldRoot::Local(id) => state.locals.get(id.0 as usize),
        HoldRoot::Parameter(p) => state.external.get(p as usize),
        HoldRoot::Capture(p) => state.captures.get(p as usize),
        HoldRoot::Static => None,
    }
    .ok_or_else(|| bad("Dyn actual head storage is absent"))
}
fn view_port(
    module: &TypedModule,
    view: &View,
    port: u32,
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    if let Some(id) = view.table {
        b.spend(module.dyn_vtables.len())?;
        let table = module
            .dyn_vtables
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| bad("actual borrowed provider table is absent"))?;
        let binding = table
            .ports
            .get(port as usize)
            .filter(|row| row.port == port)
            .ok_or_else(|| bad("actual borrowed provider port is absent"))?;
        let mut out = Permission::default();
        for source in &binding.sources {
            let path = crate::dynamic_ports::source_places(source, b)?;
            join_permission(&mut out, &super::select(state, &view.head, &path, b)?, b)?;
        }
        return Ok(out);
    }
    let mut out = Permission::default();
    for origin in &view.head.sources {
        let actual = holder(state, origin)?;
        b.spend(origin.inline.len())?;
        let path = origin
            .inline
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        join_permission(&mut out, &select(actual, &path, port, b)?, b)?;
    }
    Ok(out)
}
pub(super) fn slot_source(
    module: &TypedModule,
    f: &TypedFunction,
    source: &StoragePosition,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let StorageBase::Parameter(position) = source.base else {
        return Err(bad("dynamic source is not an actual call operand"));
    };
    let argument = *args
        .get(position as usize)
        .ok_or_else(|| bad("dynamic source parameter is absent"))?;
    let payload = values
        .get(argument.0 as usize)
        .ok_or_else(|| bad("dynamic source operand is absent"))?;
    if let Some((ContractProjection::RetainedPort(port), carrier)) = source.path.split_last() {
        return select_actual(module, f, argument, payload, state, carrier, *port, b);
    }
    let path = source
        .path
        .iter()
        .map(|step| match step {
            ContractProjection::Place(p) => Ok(*p),
            _ => Err(bad(
                "dynamic source is not a typed physical or opaque port endpoint",
            )),
        })
        .collect::<Check<Vec<_>>>()?;
    b.spend(path.len())?;
    if path.is_empty() {
        permission_work(&payload.address, b)?;
        return Ok(payload.address.clone());
    }
    if matches!(vt(f, argument)?, Type::Address { .. }) {
        super::select(state, &payload.address, &path, b)
    } else {
        Ok(leaf(payload, &path, b)?.cloned().unwrap_or_default())
    }
}
fn publish_owner(head: &Permission, state: &mut Frame, rows: &Rows, b: &mut Budget) -> Check<()> {
    let unique = head.sources.len() == 1;
    for origin in &head.sources {
        holder(state, origin)?;
        let actual = match origin.base {
            HoldRoot::Local(id) => &mut state.locals[id.0 as usize],
            HoldRoot::Parameter(p) => &mut state.external[p as usize],
            HoldRoot::Capture(p) => &mut state.captures[p as usize],
            HoldRoot::Static => return Err(bad("helper after receiver targets static storage")),
        };
        for (path, row) in &rows.0 {
            b.spend(origin.inline.len().saturating_add(path.len()))?;
            let mut complete = origin
                .inline
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect::<Vec<_>>();
            complete.extend(path);
            if unique {
                remove(&mut actual.dyn_ports, &complete, b)?;
            }
            insert(&mut actual.dyn_ports, &complete, row, b)?;
        }
    }
    Ok(())
}
fn publish(
    module: &TypedModule,
    receiver: &Payload,
    state: &mut Frame,
    prepared: &[(u32, Permission)],
    b: &mut Budget,
) -> Check<()> {
    if receiver.dyn_views.is_empty() {
        return Err(bad("dynamic mutable receiver has no actual head route"));
    }
    // Multiple possible heads cannot all be strongly updated. Keep all old
    // alternatives and parents unless a single actual storage position is known.
    let unique = receiver.dyn_views.len() == 1 && receiver.dyn_views[0].head.sources.len() == 1;
    for view in &receiver.dyn_views {
        for origin in &view.head.sources {
            holder(state, origin)?;
            let actual = match origin.base {
                HoldRoot::Local(id) => &mut state.locals[id.0 as usize],
                HoldRoot::Parameter(p) => &mut state.external[p as usize],
                HoldRoot::Capture(p) => &mut state.captures[p as usize],
                HoldRoot::Static => {
                    return Err(bad("dynamic owner mutation targets static storage"));
                }
            };
            for (port, permission) in prepared {
                if let Some(id) = view.table {
                    b.spend(module.dyn_vtables.len())?;
                    let table = module
                        .dyn_vtables
                        .iter()
                        .find(|t| t.id == id)
                        .ok_or_else(|| bad("dynamic borrowed provider table is absent"))?;
                    let binding = table
                        .ports
                        .get(*port as usize)
                        .ok_or_else(|| bad("dynamic after port is absent"))?;
                    for source in &binding.sources {
                        let path = crate::dynamic_ports::source_places(source, b)?;
                        b.spend(origin.inline.len().saturating_add(path.len()))?;
                        let mut complete = origin.inline.clone();
                        complete.extend(path);
                        if unique {
                            actual.leaves.retain(|(path, _)| path != &complete, b)?;
                        }
                        add_leaf(actual, &complete, permission, b)?;
                    }
                } else {
                    b.spend(origin.inline.len().saturating_add(1))?;
                    let mut path = origin
                        .inline
                        .iter()
                        .copied()
                        .map(ContractProjection::Place)
                        .collect::<Vec<_>>();
                    path.push(ContractProjection::RetainedPort(*port));
                    if unique {
                        remove(&mut actual.dyn_ports, &path, b)?;
                    }
                    insert(
                        &mut actual.dyn_ports,
                        &path,
                        &Port {
                            parent_sources: Vec::new(),
                            head_sources: Vec::new(),
                            permission: permission.clone(),
                            known: true,
                        },
                        b,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn carrier_type<'a>(
    module: &'a TypedModule,
    ty: &'a Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    let mut ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    b.spend(path.len())?;
    for step in path {
        ty = match step {
            ContractProjection::Place(p) => crate::dynamic_ports::physical_field(ty, *p)?,
            ContractProjection::Capture(index) => {
                let Type::Closure(id) = ty else {
                    return Err(bad(
                        "Dyn carrier Capture selector has no actual environment",
                    ));
                };
                crate::closures::definition(module, *id, None)?
                    .captures
                    .get(*index as usize)
                    .ok_or_else(|| bad("Dyn carrier Capture field is absent"))?
            }
            _ => {
                return Err(bad(
                    "Dyn carrier path cannot reinterpret a metadata class or another opaque target",
                ));
            }
        };
    }
    if let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    Ok(ty)
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
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| bad("dynamic access operand is absent"))
    };
    match &i.kind {
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            arguments,
        } => {
            let item = crate::dynamic_ports::interface(module, *interface, b)?;
            let slot = item
                .slots
                .get(*slot as usize)
                .ok_or_else(|| bad("dynamic access slot is absent"))?;
            b.spend(arguments.len().saturating_add(1))?;
            let mut args = vec![*callee];
            args.extend(arguments);
            let access = slot
                .contract
                .requires
                .as_ref()
                .ok_or_else(|| bad("dynamic caller requirements are Unknown"))?;
            for requirement in access {
                let (position, write) = match requirement {
                    DynRequirement::Read(position) => (position, false),
                    DynRequirement::Write(position) => (position, true),
                    _ => {
                        return Err(bad(
                            "dynamic requirement has no independent actual permission check",
                        ));
                    }
                };
                let actual = slot_source(module, f, position, &args, values, state, b)?;
                if actual.sources.is_empty()
                    && let Some((ContractProjection::RetainedPort(port), carrier)) =
                        position.path.split_last()
                    && let StorageBase::Parameter(parameter) = position.base
                {
                    let argument = *args
                        .get(parameter as usize)
                        .ok_or_else(|| bad("empty port actual argument is absent"))?;
                    if empty_actual(
                        module,
                        f,
                        argument,
                        get(argument)?,
                        state,
                        carrier,
                        *port,
                        b,
                    )? {
                        continue;
                    }
                }
                checks.check(&actual, write, requirements, b)?;
            }
        }
        InstructionKind::BorrowDynOwned { local, mutable } => {
            if moved_owner(state, *local, b)? {
                return Err(bad("borrow reads an actually moved erased owner"));
            }
            checks.check(
                &dyn_closed::head(f, *local, state, b)?,
                *mutable,
                requirements,
                b,
            )?;
        }
        InstructionKind::DropDynOwned { local } => {
            checks.check(
                &dyn_closed::head(f, *local, state, b)?,
                true,
                requirements,
                b,
            )?;
            // Drop is conditional on the actual affine initialized flag.
            // An executed TakeLocal vacates this exact slot. At a join a live
            // alternative still has its complete rows, so it cannot be skipped.
            if moved_owner(state, *local, b)? {
                return Ok(());
            }
            let Type::OwnedDyn { interface } = f.locals[local.0 as usize].ty else {
                return Err(bad("actual dynamic cleanup local has no owner type"));
            };
            let item = crate::dynamic_ports::interface(module, interface, b)?;
            let cleanup = item
                .owning_cleanup
                .as_ref()
                .ok_or_else(|| bad("dynamic whole cleanup access is Unknown"))?;
            for (write, positions) in [
                (false, &cleanup.access.reads),
                (true, &cleanup.access.writes),
            ] {
                for position in positions {
                    let [ContractProjection::RetainedPort(port)] = position.path.as_slice() else {
                        return Err(bad("dynamic cleanup has no typed direct port route"));
                    };
                    let payload = &state.locals[local.0 as usize];
                    let actual = select(payload, &[], *port, b)?;
                    if actual.sources.is_empty() && empty_row(payload, &[], *port, b)? {
                        continue;
                    }
                    checks.check(&actual, write, requirements, b)?;
                }
            }
        }
        InstructionKind::MakeDynBorrow {
            source, mutable, ..
        } => checks.check(&get(*source)?.address, *mutable, requirements, b)?,
        InstructionKind::MakeDynOwned { vtable, source, .. } => {
            b.spend(module.dyn_vtables.len())?;
            let table = module
                .dyn_vtables
                .iter()
                .find(|table| table.id == *vtable)
                .ok_or_else(|| bad("dynamic construction table is absent"))?;
            let iface = crate::dynamic_ports::interface(module, table.interface, b)?;
            for row in &table.ports {
                let mode = iface.retained_life.ports[row.port as usize].mode;
                for position in &row.sources {
                    let path = crate::dynamic_ports::source_places(position, b)?;
                    let actual = leaf(get(*source)?, &path, b)?.ok_or_else(|| {
                        bad("dynamic owner construction lacks an actual source leaf")
                    })?;
                    dyn_returns::initialized(state, actual, b)?;
                    checks.check(
                        actual,
                        mode == StoredBorrowMode::ExclusiveAddress,
                        requirements,
                        b,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn outcome(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let Type::ErrorUnion { error, result } = ty else {
        return Ok(false);
    };
    let Type::OwnedDyn { interface } = result.as_ref() else {
        return Ok(false);
    };
    crate::dynamic_metadata::ty(error, b)?;
    if **error != crate::dynamic_dispatch::create_error() {
        return Ok(false);
    }
    Ok(crate::dynamic_ports::interface(module, *interface, b)?
        .context
        .is_some())
}
pub(super) fn value(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let actual = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::Dyn { interface } | Type::OwnedDyn { interface } = actual {
        return Ok(crate::dynamic_ports::interface(module, *interface, b)?
            .context
            .is_some());
    }
    outcome(module, actual, b)
}
pub(super) fn wrapper(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.dyn_contexts {
        return Ok(false);
    }
    let ty = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            &f.locals[local.0 as usize].ty
        }
        InstructionKind::TakeVariantField { local, path }
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(_)]) =>
        {
            &f.locals[local.0 as usize].ty
        }
        InstructionKind::DropErrorUnion { local } => &f.locals[local.0 as usize].ty,
        InstructionKind::ErrorTag { value }
        | InstructionKind::TakeErrorUnion { value, .. }
        | InstructionKind::ReadErrorUnion { value, .. } => vt(f, *value)?,
        InstructionKind::MakeErrorUnion { .. } => vt(
            f,
            i.result
                .ok_or_else(|| bad("dynamic outcome constructor lacks a result"))?,
        )?,
        _ => return Ok(false),
    };
    if !outcome(module, ty, b)? {
        return Ok(false);
    }
    let Type::ErrorUnion { error, result } = ty else {
        unreachable!()
    };
    match &i.kind {
        InstructionKind::PlaceTagLocal { .. } => {
            if i.result.map(|v| vt(f, v)).transpose()? != Some(&Type::I32) {
                return Err(bad("dynamic outcome tag has the wrong full result type"));
            }
        }
        InstructionKind::TakeVariantField { path, .. } => {
            let expected = if path == &[PlaceProjection::ErrorArm(true)] {
                error.as_ref()
            } else {
                result.as_ref()
            };
            if i.result.map(|v| vt(f, v)).transpose()? != Some(expected) {
                return Err(bad("dynamic outcome take changes its real selected arm"));
            }
        }
        _ => {}
    }
    Ok(true)
}

#[cfg(test)]
mod empty_tests {
    use super::*;
    #[test]
    fn untransported_context_wrappers_are_unknown_instead_of_empty_source_envelopes() {
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../tests/fixtures/dyn-context/context-read.typed.wire"),
            &Limits::default(),
        )
        .unwrap();
        let interface = module
            .dyn_interfaces
            .iter()
            .find(|i| i.context.is_some())
            .unwrap()
            .id;
        let owner = Type::OwnedDyn { interface };
        for ty in [
            Type::Managed(Box::new(owner.clone())),
            Type::Desc(Box::new(owner.clone())),
            Type::OwnedStore {
                element: Box::new(owner.clone()),
            },
            Type::Array {
                element: Box::new(owner.clone()),
                length: 2,
            },
            Type::Task {
                result: Box::new(owner.clone()),
                retained_life: RetainedLifeShape {
                    binder_count: 0,
                    ports: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            },
            Type::Opaque {
                identity: zil_base::NodeId(99),
                hidden: Box::new(owner),
            },
        ] {
            let mut b = Budget::new(&Limits::default());
            b.dyn_contexts = true;
            let failure = shape(&module, &ty, &mut b).unwrap_err();
            assert_eq!(failure.category, zil_base::ResultCategory::Unsupported);
        }
    }

    #[test]
    fn conditional_cleanup_never_confuses_a_joined_live_row_with_a_moved_slot() {
        let mut b = Budget::new(&Limits::default());
        let mut state = Frame {
            locals: vec![Payload::default()],
            vacant: vec![owner_hole(LocalId(0))],
            ..Frame::default()
        };
        assert!(moved_owner(&state, LocalId(0), &mut b).unwrap());
        insert(
            &mut state.locals[0].dyn_ports,
            &[ContractProjection::RetainedPort(0)],
            &Port {
                permission: Permission::default(),
                parent_sources: vec![],
                head_sources: Vec::new(),
                known: true,
            },
            &mut b,
        )
        .unwrap();
        assert!(!moved_owner(&state, LocalId(0), &mut b).unwrap());
        state.locals[0].dyn_ports.0[0].1.known = false;
        assert!(!moved_owner(&state, LocalId(0), &mut b).unwrap());
        state.locals[0].dyn_ports.0.clear();
        state.vacant = vec![owner_hole(LocalId(1))];
        assert!(!moved_owner(&state, LocalId(0), &mut b).unwrap());
    }
    #[test]
    fn empty_access_requires_every_actual_owner_row_to_be_explicitly_known() {
        let mut b = Budget::new(&Limits::default());
        let source = |local| Source {
            static_dependency: None,
            base: HoldRoot::Local(LocalId(local)),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: Vec::new(),
        };
        let mut state = Frame {
            locals: vec![Payload::default(); 2],
            ..Frame::default()
        };
        for actual in &mut state.locals {
            insert(
                &mut actual.dyn_ports,
                &[ContractProjection::RetainedPort(0)],
                &Port {
                    permission: Permission::default(),
                    parent_sources: vec![],
                    head_sources: Vec::new(),
                    known: true,
                },
                &mut b,
            )
            .unwrap();
        }
        let head = Permission {
            sources: vec![source(0), source(1)],
            loans: vec![],
        };
        assert!(empty_at_head(&head, &state, &[], 0, &mut b).unwrap());
        state.locals[1].dyn_ports.0[0].1.known = false;
        assert!(!empty_at_head(&head, &state, &[], 0, &mut b).unwrap());
        state.locals[1].dyn_ports.0[0].1.known = true;
        state.locals[1].dyn_ports.0[0]
            .1
            .permission
            .sources
            .push(source(0));
        assert!(!empty_at_head(&head, &state, &[], 0, &mut b).unwrap());
        state.locals[1].dyn_ports.0.clear();
        assert!(!empty_at_head(&head, &state, &[], 0, &mut b).unwrap());
        assert!(!empty_at_head(&Permission::default(), &state, &[], 0, &mut b).unwrap());
    }
}
