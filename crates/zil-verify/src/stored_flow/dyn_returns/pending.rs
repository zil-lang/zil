//! Output-only classes retain their exact identity while actual Return facts
//! remain pending. Neither a source upper nor a missing row proves grounding.
use super::*;

pub(super) fn register(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> Check<Vec<SignatureLifeClass>> {
    if !b.dyn_contexts || f.environment.is_some() || f.call_kind != CallKind::Direct {
        return Ok(Vec::new());
    }
    let Some(life) = &f.storage_life else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for row in &life.classes {
        if row.position.base != StorageBase::Result
            || !matches!(row.binder, StorageBinder::Fixed(_))
        {
            continue;
        }
        b.spend(life.classes.len().saturating_add(life.bindings.len()))?;
        if life.classes.iter().any(|input| {
            matches!(input.position.base, StorageBase::Parameter(_)) && input.binder == row.binder
        }) || life.bindings.iter().any(|input| {
            matches!(input.position.base, StorageBase::Parameter(_)) && input.binder == row.binder
        }) {
            continue;
        }
        let Some((carrier, interface)) = carrier(module, &f.return_type, &row.position, b)? else {
            continue;
        };
        let mut ports = Vec::new();
        for (port, leaf) in interface.retained_life.ports.iter().enumerate() {
            if leaf.binder != row.position.class {
                continue;
            }
            dynamic_metadata::ty(&leaf.leaf, b)?;
            b.spend(carrier.len().saturating_add(3))?;
            let mut path = carrier.clone();
            path.push(ContractProjection::RetainedPort(port as u32));
            ports.push((path, leaf.leaf.clone(), leaf.mode));
        }
        b.spend(
            b.dyn_static_classes
                .len()
                .saturating_mul(row.position.path.len().saturating_add(1)),
        )?;
        if let Some(old) = b
            .dyn_static_classes
            .iter()
            .find(|old| old.function == f.id && old.class == row.position)
        {
            if old.binder != row.binder || old.ports != ports {
                return Err(bad(
                    "pending Dyn class changed its actual result occurrence",
                ));
            }
        } else {
            if b.dyn_static_classes.len() >= b.limits.max_nodes {
                return Err(limit(
                    "pending Dyn result classes exceed selected node bound",
                ));
            }
            b.spend(
                row.position
                    .path
                    .len()
                    .saturating_add(ports.len())
                    .saturating_add(2),
            )?;
            b.dyn_static_classes.push(DynStaticObligation {
                function: f.id,
                class: row.position.clone(),
                binder: row.binder,
                ports,
                proven: false,
            });
        }
        b.spend(row.position.path.len().saturating_add(1))?;
        result.push(row.position.clone());
    }
    Ok(result)
}

/// A result's actual local class is selected before its enclosing signature
/// map is consulted. Only the complete actual carrier supplies its port list.
fn carrier<'a>(
    module: &'a TypedModule,
    ty: &Type,
    key: &SignatureLifeClass,
    b: &mut Budget,
) -> Check<Option<(Vec<ContractProjection>, &'a DynInterface)>> {
    b.spend(key.path.len().saturating_add(1))?;
    let mut current = ty;
    let mut physical = Vec::new();
    for step in &key.path {
        let LifeTypeProjection::Place(place) = step else {
            return Ok(None);
        };
        current = crate::dynamic_ports::physical_field(current, *place)?;
        b.spend(1)?;
        physical.push(ContractProjection::Place(*place));
    }
    let Type::OwnedDyn { interface } = current else {
        return Ok(None);
    };
    let interface = crate::dynamic_ports::interface(module, *interface, b)?;
    if interface.context.is_none() || key.class >= interface.retained_life.binder_count {
        return Ok(None);
    }
    Ok(Some((physical, interface)))
}

pub(super) fn discharge(f: &TypedFunction, rows: &dyn_ports::Rows, b: &mut Budget) -> Check<()> {
    b.spend(b.dyn_static_classes.len())?;
    for index in 0..b.dyn_static_classes.len() {
        if b.dyn_static_classes[index].function != f.id {
            continue;
        }
        // Move the private record out while charging/inspecting its complete
        // Types. Both vector shifts are paid first; restore even on rejection.
        b.spend(
            b.dyn_static_classes
                .len()
                .saturating_mul(2)
                .saturating_add(1),
        )?;
        let mut obligation = b.dyn_static_classes.remove(index);
        let proof = prove(&obligation, rows, b);
        obligation.proven = proof.is_ok();
        b.dyn_static_classes.insert(index, obligation);
        proof?;
    }
    Ok(())
}
fn prove(obligation: &DynStaticObligation, rows: &dyn_ports::Rows, b: &mut Budget) -> Check<()> {
    b.spend(obligation.ports.len())?;
    for (path, leaf, _) in &obligation.ports {
        dynamic_metadata::ty(leaf, b)?;
        let at = dyn_ports::find(rows, path, b)?
            .ok_or_else(|| bad("pending result class has no actual Return port row"))?;
        let actual = &rows.0[at].1;
        if !actual.known {
            return Err(bad(
                "pending result class has Unknown actual Return provenance",
            ));
        }
        for source in &actual.permission.sources {
            source_work(std::slice::from_ref(source), b)?;
            if !static_target(source) {
                return Err(bad(
                    "output-only persistent class retains a non-static actual target",
                ));
            }
        }
        for parent in &actual.parent_sources {
            source_work(std::slice::from_ref(parent), b)?;
            if !matches!(parent.base, HoldRoot::Static | HoldRoot::Parameter(_)) {
                return Err(bad(
                    "pending static result retains an activation-local parent",
                ));
            }
        }
        // A present known-empty row proves NoTarget, never Static. A nonempty
        // row is Static only after every actual target passed this check.
    }
    // A phantom class has no data ports by its complete actual directory;
    // retain its original class identity without manufacturing a target root.
    Ok(())
}
