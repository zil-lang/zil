//! Function result uppers are compared with actual SSA/store facts. They never
//! seed a result permission or turn an absent source row into an empty one.
use super::*;
mod atoms;
pub(super) mod pending;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-SOURCE", None, message)
}
fn static_target(source: &Source) -> bool {
    source.base == HoldRoot::Static
        && source.retained_port.is_none()
        && source.leaf.is_empty()
        && source.heap_element.is_none()
        && source.heap_parents.is_empty()
}

pub(super) enum Publication<'a> {
    Original(Option<&'a Source>),
    Static(std::slice::Iter<'a, Source>),
}
impl<'a> Iterator for Publication<'a> {
    type Item = &'a Source;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Original(source) => source.take(),
            Self::Static(sources) => sources.find(|source| static_target(source)),
        }
    }
}
/// A Static upper describes a category, never a new literal root. Its actual
/// conditional source nodes must survive until the whole-module least-root
/// proof completes. In particular a callable Result -> Context -> callable
/// cycle cannot become grounded by discarding the Result dependency here.
pub(super) fn publication<'a>(
    upper: &'a Source,
    actual: &'a dyn_ports::Port,
    b: &mut Budget,
) -> Check<Publication<'a>> {
    if static_target(upper) && upper.static_dependency.is_none() {
        source_work(&actual.permission.sources, b)?;
        if !actual.permission.sources.iter().any(static_target) {
            return Err(bad(
                "Static publication has no actual typed static provenance",
            ));
        }
        Ok(Publication::Static(actual.permission.sources.iter()))
    } else {
        // The established Exact route allocates no additional vector and
        // retains its complete actual-argument substitution below.
        Ok(Publication::Original(Some(upper)))
    }
}

/// A typed address cannot be retained as a view of a vacated actual slot.
/// Complete source paths are compared, so stored pointees stay separate from
/// the physical pointer slots that may have been extracted or replaced.
pub(super) fn initialized(state: &Frame, permission: &Permission, b: &mut Budget) -> Check<()> {
    for source in &permission.sources {
        for vacant in &state.vacant {
            if overlap(source, vacant, b)? {
                return Err(bad("retained view selects a vacant actual storage region"));
            }
        }
    }
    Ok(())
}
pub(super) fn complete(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    b: &mut Budget,
) -> Check<bool> {
    for (path, _) in dyn_ports::shape(module, ty, b)? {
        if dyn_ports::find(&payload.dyn_ports, &path, b)?
            .is_none_or(|at| !payload.dyn_ports.0[at].1.known)
        {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    payload: &Payload,
    summary: &mut Summary,
    loans: &[Loan],
    sites: &dyn_leases::Sites,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    for (path, _) in dyn_ports::shape(module, &f.return_type, b)? {
        let Some(at) = dyn_ports::find(&payload.dyn_ports, &path, b)? else {
            summary.complete = false;
            continue;
        };
        let original = &payload.dyn_ports.0[at].1;
        permission_work(&original.permission, b)?;
        source_work(&original.parent_sources, b)?;
        let mut row = original.clone();
        summary.complete &= row.known;
        // Loan indices are function-local. Only real source positions cross a
        // summary edge; instantiate rebuilds every actual caller parent set.
        add_sources(
            &mut row.parent_sources,
            &parents(&row.permission, loans, b)?,
            b,
        )?;
        add_sources(
            &mut row.head_sources,
            &sites.head_sources(&row.permission, loans, b)?,
            b,
        )?;
        row.permission.loans.clear();
        dyn_ports::insert(&mut summary.dyn_ports, &path, &row, b)?;
    }
    Ok(())
}
fn parameter(f: &TypedFunction, position: u32) -> Check<&Type> {
    f.parameters
        .get(position as usize)
        .and_then(|id| f.locals.get(id.0 as usize))
        .map(|l| &l.ty)
        .ok_or_else(|| bad("source upper parameter has no actual full type"))
}
fn path_places(path: &[ContractProjection], b: &mut Budget) -> Check<Path> {
    b.spend(path.len())?;
    path.iter()
        .map(|step| match step {
            ContractProjection::Place(p) => Ok(*p),
            _ => Err(bad(
                "physical source upper contains an opaque or capture projection",
            )),
        })
        .collect()
}
/// Return the actual full dependency leaf and its canonical symbolic source.
/// Address of holder storage and the pointer read from a holder are distinct.
fn input(
    module: &TypedModule,
    f: &TypedFunction,
    position: &StoragePosition,
    b: &mut Budget,
) -> Check<(Type, Source)> {
    let StorageBase::Parameter(index) = position.base else {
        return Err(bad(
            "ordinary result upper must select a real input parameter",
        ));
    };
    b.spend(position.path.len().saturating_add(1))?;
    let root = parameter(f, index)?;
    if position.path.is_empty() && matches!(root, Type::Address { .. }) {
        dynamic_metadata::ty(root, b)?;
        return Ok((
            root.clone(),
            Source {
                static_dependency: None,
                base: HoldRoot::Parameter(index),
                retained_port: None,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            },
        ));
    }
    if let Some((ContractProjection::RetainedPort(port), carrier)) = position.path.split_last() {
        let endpoint = dyn_ports::carrier_type(module, root, carrier, b)?;
        let (Type::Dyn { interface } | Type::OwnedDyn { interface }) = endpoint else {
            return Err(bad("source upper opaque selector has a non-Dyn endpoint"));
        };
        let port_type = &crate::dynamic_ports::interface(module, *interface, b)?
            .retained_life
            .ports
            .get(*port as usize)
            .ok_or_else(|| bad("source upper Dyn port is absent"))?
            .leaf;
        dynamic_metadata::ty(port_type, b)?;
        return Ok((
            port_type.clone(),
            Source {
                static_dependency: None,
                base: HoldRoot::Parameter(index),
                retained_port: Some(PortOrigin::Dyn {
                    carrier: carrier.to_vec(),
                    port: *port,
                }),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            },
        ));
    }
    let path = path_places(&position.path, b)?;
    let storage = if let Type::Address { inner, .. } = root {
        inner.as_ref()
    } else {
        root
    };
    let endpoint = crate::dynamic_ports::physical_path(storage, &path, b)?;
    dynamic_metadata::ty(endpoint, b)?;
    let source = Source {
        static_dependency: None,
        base: HoldRoot::Parameter(index),
        retained_port: None,
        leaf: path,
        inline: Vec::new(),
        heap_parents: Vec::new(),
        heap_element: None,
    };
    if matches!(endpoint, Type::Address { .. } | Type::Desc(_)) {
        Ok((endpoint.clone(), source))
    } else {
        Err(bad(
            "result upper does not select an actual borrowed input leaf",
        ))
    }
}

/// Canonical public call transport uses the independently checked upper.
/// Actual Return rows remain separate and are still required to prove it.
fn source_claim(f: &TypedFunction) -> Check<(&[StorageSourceBinding], bool)> {
    if let Type::Closure(id) = f.return_type {
        let claim = f
            .callable_return_contract
            .as_ref()
            .filter(|claim| claim.closure == id)
            .and_then(|claim| claim.capture_sources.as_deref())
            .ok_or_else(|| bad("returned Context environment sources are Unknown"))?;
        if f.retained_return_sources
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
        {
            return Err(bad(
                "Context environment has conflicting result source families",
            ));
        }
        Ok((claim, true))
    } else {
        f.retained_return_sources
            .as_deref()
            .map(|claim| (claim, false))
            .ok_or_else(|| bad("retained result source upper is Unknown"))
    }
}
fn destination_path(
    row: &StorageSourceBinding,
    environment: bool,
    b: &mut Budget,
) -> Check<Vec<ContractProjection>> {
    b.spend(row.position.path.len().saturating_add(1))?;
    if environment {
        let StorageBase::Capture(index) = row.position.base else {
            return Err(bad(
                "Context environment destination has no actual Capture position",
            ));
        };
        let mut path = vec![ContractProjection::Capture(index)];
        path.extend(&row.position.path);
        Ok(path)
    } else if row.position.base == StorageBase::Result {
        Ok(row.position.path.clone())
    } else {
        Err(bad("retained result destination is not a Result position"))
    }
}
fn declared_row<'a>(
    claim: &'a [StorageSourceBinding],
    environment: bool,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<Option<&'a StorageSourceBinding>> {
    for row in claim {
        if environment
            && !matches!(
                row.position.path.last(),
                Some(ContractProjection::RetainedPort(_))
            )
        {
            continue;
        }
        if destination_path(row, environment, b)? == path {
            return Ok(Some(row));
        }
    }
    Ok(None)
}
pub(super) fn published_sources(
    module: &TypedModule,
    f: &TypedFunction,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<Vec<Source>> {
    let (claim, environment) = source_claim(f)?;
    b.spend(claim.len().saturating_mul(path.len().saturating_add(1)))?;
    let mut result = Vec::new();
    if let Some(row) = declared_row(claim, environment, path, b)? {
        for atom in &row.sources {
            let (_, _, source) = if matches!(atom, StorageSource::ReborrowedLeaf { .. }) {
                atoms::resolve_output(module, f, atom, result_leaf(module, f, path, b)?, b)?
            } else {
                atoms::resolve(module, f, atom, b)?
            };
            add_sources(&mut result, source.sources(), b)?;
        }
    }
    Ok(result)
}
/// A head is an additional real parent obligation, not a new data target.
/// These positions are resolved only after the actual Return containment proof.
pub(super) fn published_heads(
    module: &TypedModule,
    f: &TypedFunction,
    path: &[ContractProjection],
    data: &Source,
    b: &mut Budget,
) -> Check<Vec<Source>> {
    if !b.dyn_reborrow_sources {
        return Ok(Vec::new());
    }
    let mut heads = Vec::new();
    let (claim, environment) = source_claim(f)?;
    b.spend(claim.len().saturating_mul(path.len().saturating_add(1)))?;
    if let Some(row) = declared_row(claim, environment, path, b)? {
        for atom in &row.sources {
            if !matches!(atom, StorageSource::ReborrowedLeaf { .. }) {
                continue;
            }
            let (_, _, selected) =
                atoms::resolve_output(module, f, atom, result_leaf(module, f, path, b)?, b)?;
            source_work(selected.sources(), b)?;
            b.spend(
                selected
                    .sources()
                    .len()
                    .saturating_mul(width(data).saturating_add(1)),
            )?;
            if selected.sources().contains(data)
                && let Some(head) = selected.parent_head()
            {
                add_sources(&mut heads, std::slice::from_ref(head), b)?;
            }
        }
    }
    Ok(heads)
}
fn result_leaf<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
        return Err(bad("result reborrow has no typed retained endpoint"));
    };
    let (Type::Dyn { interface } | Type::OwnedDyn { interface }) =
        dyn_ports::carrier_type(module, &f.return_type, carrier, b)?
    else {
        return Err(bad("result reborrow has no actual Dyn carrier"));
    };
    crate::dynamic_ports::interface(module, *interface, b)?
        .retained_life
        .ports
        .get(*port as usize)
        .map(|port| &port.leaf)
        .ok_or_else(|| bad("result reborrow selects an absent actual port"))
}
pub(super) fn validate(
    module: &TypedModule,
    f: &TypedFunction,
    rows: &dyn_ports::Rows,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    // A closure's explicit Static address result has its own whole-module
    // actual Return/ancestry proof. It is not a Dyn retained-port proposal.
    if f.environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .is_some_and(stored_callables::static_result)
    {
        return Ok(());
    }
    let expected = dyn_ports::shape(module, &f.return_type, b)?;
    let claim = f.retained_return_sources.as_ref();
    if expected.is_empty() {
        // Closure capture_sources has a separate independent proof. A missing
        // new family contract must not reject an older borrowed closure.
        if claim.is_some_and(|claim| !claim.is_empty()) {
            return Err(bad("source proposal names no actual result Dyn port"));
        }
        pending::discharge(f, rows, b)?;
        b.pending_dyn_returns.remove(&f.id);
        return Ok(());
    }
    let (claim, environment) = source_claim(f)?;
    b.spend(claim.len().saturating_add(expected.len()))?;
    let mut seen = Vec::new();
    for row in claim {
        if environment
            && !matches!(
                row.position.path.last(),
                Some(ContractProjection::RetainedPort(_))
            )
        {
            continue;
        }
        b.spend(
            row.position
                .path
                .len()
                .saturating_mul(seen.len().saturating_add(1))
                .saturating_add(1),
        )?;
        let destination = destination_path(row, environment, b)?;
        if !expected.iter().any(|(path, _)| *path == destination) || seen.contains(&destination) {
            return Err(bad(
                "result source upper has a duplicate or non-result-port destination",
            ));
        }
        seen.push(destination);
    }
    for (path, _) in expected {
        let Some(at) = dyn_ports::find(rows, &path, b)? else {
            return Err(bad("actual Return is missing a result dependency row"));
        };
        let actual = &rows.0[at].1;
        if !actual.known {
            return Err(bad("actual Return result port remains Unknown"));
        }
        let declared = declared_row(claim, environment, &path, b)?;
        if !claim.is_empty() && declared.is_none() {
            return Err(bad("source upper omits a real result port"));
        }
        let Some((ContractProjection::RetainedPort(port), carrier)) = path.split_last() else {
            return Err(bad("actual result dependency has no typed terminal port"));
        };
        let (Type::Dyn { interface } | Type::OwnedDyn { interface }) =
            dyn_ports::carrier_type(module, &f.return_type, carrier, b)?
        else {
            return Err(bad("result port has a non-Dyn actual endpoint"));
        };
        let output = &crate::dynamic_ports::interface(module, *interface, b)?
            .retained_life
            .ports[*port as usize];
        let (class_path, class) = class_key(module, &f.return_type, carrier, output.binder, 0, b)?;
        let output_binder = class_binding(f, StorageBase::Result, &class_path, class, b)?;
        for parent in actual.parent_sources.iter().chain(&actual.head_sources) {
            if !matches!(parent.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                return Err(bad("returned Dyn retains an activation-local parent loan"));
            }
        }
        let mut upper = Vec::new();
        if let Some(declared) = declared {
            for atom in &declared.sources {
                let (leaf, binder, symbolic) =
                    atoms::resolve_output(module, f, atom, &output.leaf, b)?;
                if matches!(atom, StorageSource::Static { .. })
                    && !actual.permission.sources.iter().any(static_target)
                {
                    return Err(bad(
                        "Static upper has no actual typed static target at this result occurrence",
                    ));
                }
                dynamic_metadata::ty(&output.leaf, b)?;
                if binder.is_some_and(|binder| binder != output_binder) {
                    return Err(bad(
                        "result source upper contains a different actual lifetime class",
                    ));
                }
                if leaf != output.leaf || atoms::mode(&leaf)? != output.mode {
                    return Err(bad(
                        "source upper full borrowed leaf differs from result port",
                    ));
                }
                source_work(symbolic.sources(), b)?;
                for source in symbolic.into_sources() {
                    b.spend(upper.len().saturating_mul(width(&source).saturating_add(1)))?;
                    if upper.contains(&source) {
                        return Err(bad("result source upper repeats an actual input selector"));
                    }
                    upper.push(source);
                }
            }
        }
        for source in &actual.permission.sources {
            source_work(std::slice::from_ref(source), b)?;
            if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                return Err(bad("returned Dyn retains activation or capture storage"));
            }
            b.spend(upper.len().saturating_mul(width(source).saturating_add(1)))?;
            let covered = if static_target(source) {
                upper.iter().any(static_target)
            } else {
                upper.contains(source)
            };
            if !covered {
                return Err(bad(
                    "actual Return target exceeds its declared source upper",
                ));
            }
        }
    }
    pending::discharge(f, rows, b)?;
    b.pending_dyn_returns.remove(&f.id);
    Ok(())
}

/// Resolve a Dyn data class through the real enclosing carrier metadata. This
/// never reuses the Task selector interpretation or a coincident numeric class.
pub(super) fn source_relation(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: u32,
    carrier: &[ContractProjection],
    port: u32,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let root = parameter_type(f, parameter)?;
    let endpoint = dyn_ports::carrier_type(module, root, carrier, b)?;
    let (Type::Dyn { interface } | Type::OwnedDyn { interface }) = endpoint else {
        return Err(bad("Dyn class source has a non-Dyn endpoint"));
    };
    let class = crate::dynamic_ports::interface(module, *interface, b)?
        .retained_life
        .ports
        .get(port as usize)
        .ok_or_else(|| bad("Dyn class source port is absent"))?
        .binder;
    let (path, class) = class_key(module, root, carrier, class, 0, b)?;
    class_binding(f, StorageBase::Parameter(parameter), &path, class, b)
}
fn class_binding(
    f: &TypedFunction,
    base: StorageBase,
    path: &[LifeTypeProjection],
    class: u32,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let life = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("Dyn dependency has no complete signature class relation"))?;
    b.spend(
        life.classes
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    life.classes
        .iter()
        .find(|row| {
            row.position.base == base && row.position.path == path && row.position.class == class
        })
        .map(|row| row.binder)
        .ok_or_else(|| bad("Dyn dependency omits its full actual enclosing class binding"))
}

fn parameter_type(f: &TypedFunction, index: u32) -> Check<&Type> {
    parameter(f, index)
}
pub(super) fn class_key(
    module: &TypedModule,
    root: &Type,
    carrier: &[ContractProjection],
    class: u32,
    depth: usize,
    b: &mut Budget,
) -> Check<(Vec<LifeTypeProjection>, u32)> {
    b.spend(carrier.len().saturating_add(1))?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("Dyn source class path exceeds selected depth"));
    }
    let (root, mut prefix) = if let Type::Address { inner, .. } = root {
        (inner.as_ref(), vec![LifeTypeProjection::Pointee])
    } else {
        (root, vec![])
    };
    if let Type::Closure(id) = root {
        let Some((ContractProjection::Capture(index), tail)) = carrier.split_first() else {
            return Err(bad(
                "closure Dyn source class omits the actual capture path",
            ));
        };
        let definition = crate::closures::definition(module, *id, None)?;
        let child = definition
            .captures
            .get(*index as usize)
            .ok_or_else(|| bad("Dyn class source capture is absent"))?;
        let (path, child_class) = class_key(module, child, tail, class, depth + 1, b)?;
        let life = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("captured Dyn class map is Unknown"))?;
        b.spend(
            life.classes
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let binding = life
            .classes
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Capture(*index)
                    && row.position.path == path
                    && row.position.class == child_class
            })
            .ok_or_else(|| bad("captured Dyn has no complete environment class map"))?;
        let StorageBinder::Fixed(class) = binding.binder else {
            return Err(bad(
                "captured Dyn persistent class was narrowed to an invocation",
            ));
        };
        return Ok((prefix, class));
    }
    let path = carrier
        .iter()
        .map(|step| match step {
            ContractProjection::Place(p) => Ok(LifeTypeProjection::Place(*p)),
            _ => Err(bad(
                "Dyn class carrier path has no complete typed metadata route",
            )),
        })
        .collect::<Check<Vec<_>>>()?;
    if let Type::BorrowedStorage { life, .. } = root {
        b.spend(
            life.retained
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let row = life
            .retained
            .iter()
            .find(|row| row.path == path && row.class == class)
            .ok_or_else(|| bad("Dyn carrier class omits its actual enclosing fixed map"))?;
        return Ok((prefix, row.binder));
    }
    prefix.extend(path);
    Ok((prefix, class))
}

pub(super) fn after(
    module: &TypedModule,
    f: &TypedFunction,
    state: &Frame,
    summary: &mut Summary,
    loans: &[Loan],
    sites: &dyn_leases::Sites,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    for (position, local) in f.parameters.iter().enumerate() {
        let ty = &f.locals[local.0 as usize].ty;
        if !matches!(ty, Type::Address { mutable: true, .. }) {
            continue;
        }
        let expected = dyn_ports::shape(module, ty, b)?;
        if expected.is_empty() {
            continue;
        }
        let actual = state
            .external
            .get(position)
            .ok_or_else(|| bad("helper actual external Dyn carrier is absent"))?;
        let mut rows = dyn_ports::Rows::default();
        for (path, _) in expected {
            let Some(at) = dyn_ports::find(&actual.dyn_ports, &path, b)? else {
                summary.complete = false;
                continue;
            };
            let original = &actual.dyn_ports.0[at].1;
            permission_work(&original.permission, b)?;
            source_work(&original.parent_sources, b)?;
            let mut row = original.clone();
            summary.complete &= row.known;
            add_sources(
                &mut row.parent_sources,
                &parents(&row.permission, loans, b)?,
                b,
            )?;
            for parent in &row.parent_sources {
                if !matches!(parent.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                    return Err(bad(
                        "helper after-state retains an activation-local parent loan",
                    ));
                }
            }
            add_sources(
                &mut row.head_sources,
                &sites.head_sources(&row.permission, loans, b)?,
                b,
            )?;
            row.permission.loans.clear();
            for source in &row.permission.sources {
                if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                    return Err(bad(
                        "helper after Dyn port retains activation-local storage",
                    ));
                }
            }
            dyn_ports::insert(&mut rows, &path, &row, b)?;
        }
        b.spend(summary.dyn_after.len())?;
        if let Some((_, old)) = summary
            .dyn_after
            .iter_mut()
            .find(|(p, _)| *p == position as u32)
        {
            for (path, row) in rows.0 {
                dyn_ports::insert(old, &path, &row, b)?;
            }
        } else {
            summary.dyn_after.push((position as u32, rows));
        }
    }
    Ok(())
}

/// Preserve actual non-target parents across an ordinary call boundary. The
/// source graph of each ancestor was reconstructed by this kernel's SSA pass.
pub(super) fn check_parent_escape(
    f: &TypedFunction,
    sources: &[Source],
    b: &mut Budget,
) -> Check<()> {
    source_work(sources, b)?;
    for source in sources {
        if matches!(source.base, HoldRoot::Local(_))
            || f.environment.is_none() && matches!(source.base, HoldRoot::Capture(_))
        {
            return Err(bad(
                "borrowed result retains an activation-local parent loan",
            ));
        }
    }
    Ok(())
}

pub(super) fn parents(
    permission: &Permission,
    loans: &[Loan],
    b: &mut Budget,
) -> Check<Vec<Source>> {
    let mut result = Vec::new();
    for id in ancestry(&permission.loans, loans, b)? {
        let actual = loans
            .get(id)
            .ok_or_else(|| bad("returned parent lies outside the actual loan arena"))?;
        add_sources(&mut result, &actual.sources, b)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
