//! Producer-only deep leaf origins. These paths carry freshness, never an
//! additional owning debt or a permission to operate through a deep place.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Carrier {
    Value(ValueId),
    Local(LocalId),
}
pub(super) type Leaves = BTreeMap<Vec<u32>, BTreeSet<Birth>>;
pub(super) type Origins = BTreeMap<Carrier, Leaves>;

pub(crate) fn nested(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, field)| matches!(field, Type::Package { .. })))
}

pub(crate) fn admit(ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    // Raw preflight has bounded depth already. Pay before any new identity
    // collection, Copy classification or repeated complete-type traversal.
    budget.type_work(ty, true, "ordinary Product admission")?;
    fn fields(ty: &Type, budget: &mut Budget) -> CompileResult<()> {
        let Type::Package {
            name,
            fields: entries,
        } = ty
        else {
            return Err(error("Product child must be a Package"));
        };
        budget.product(
            entries.len(),
            entries.len().saturating_add(1),
            1,
            "Product field identity index",
        )?;
        let mut names = BTreeSet::new();
        let mut owned = false;
        if name.is_empty() {
            return Err(error("Product nominal identity is empty"));
        }
        for (name, ty) in entries {
            budget.product(
                name.len(),
                entries.len().saturating_add(1),
                1,
                "Product field name comparison",
            )?;
            if name.is_empty() || !names.insert(name) {
                return Err(error("Product field identity is empty or duplicated"));
            }
            match ty {
                Type::Managed(payload)
                    if **payload == Type::ByteBuffer
                        || (matches!(payload.as_ref(), Type::Record { .. })
                            && payload.is_copy_value()) =>
                {
                    owned = true
                }
                Type::Package { .. } => {
                    fields(ty, budget)?;
                    owned = true;
                }
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(error(
                        "ordinary Product has an unsupported owning or borrowed child",
                    ));
                }
            }
        }
        if !owned {
            return Err(error("Product has no transitive Managed leaf"));
        }
        Ok(())
    }
    fields(ty, budget)
}

fn visit(
    ty: &Type,
    path: &mut Vec<u32>,
    budget: &mut Budget,
    action: &mut impl FnMut(&[u32], &mut Budget) -> CompileResult<()>,
) -> CompileResult<()> {
    let Type::Package { fields, .. } = ty else {
        return Err(error("Product leaf traversal requires a complete Package"));
    };
    budget.spend(fields.len(), "Product subtree fields")?;
    for (index, (_, child)) in fields.iter().enumerate() {
        if matches!(child, Type::Managed(_) | Type::Package { .. }) {
            budget.spend(path.len().saturating_add(1), "Product path growth")?;
            path.push(index as u32);
            if matches!(child, Type::Managed(_)) {
                action(path, budget)?;
            } else {
                visit(child, path, budget, action)?;
            }
            path.pop();
        }
    }
    Ok(())
}

pub(super) fn add(
    origins: &mut Origins,
    carrier: Carrier,
    index: u32,
    child: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.lookup(origins.len(), "Product carrier insertion")?;
    budget.spend(2, "Product carrier and prefix allocation")?;
    let leaves = origins.entry(carrier).or_default();
    visit(child, &mut vec![index], budget, &mut |path, budget| {
        budget.product(
            path.len(),
            leaves.len().saturating_add(1).ilog2() as usize + 2,
            1,
            "Product path insertion",
        )?;
        leaves.insert(path.to_vec(), BTreeSet::from([Birth::Incoming]));
        Ok(())
    })
}

pub(super) fn copy_cost(origins: &Origins, budget: &mut Budget) -> CompileResult<()> {
    if origins.is_empty() {
        return Ok(());
    }
    budget.spend(origins.len(), "Product carrier snapshot")?;
    for leaves in origins.values() {
        budget.spend(leaves.len(), "Product leaf snapshot")?;
        for (path, birth) in leaves {
            budget.spend(
                path.len().saturating_add(birth.len()),
                "Product origin snapshot",
            )?;
        }
    }
    Ok(())
}

pub(super) fn join(to: &mut Origins, from: &Origins, budget: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (carrier, leaves) in from {
        budget.lookup(to.len(), "Product carrier join")?;
        let target = to
            .get_mut(carrier)
            .ok_or_else(|| error("Product carrier join shape differs"))?;
        for (path, birth) in leaves {
            budget.product(
                path.len(),
                target.len().saturating_add(1).ilog2() as usize + 2,
                1,
                "Product leaf join lookup",
            )?;
            let current = target
                .get_mut(path)
                .ok_or_else(|| error("Product leaf join shape differs"))?;
            budget.spend(
                current.len().saturating_add(birth.len()),
                "Product leaf origin union",
            )?;
            let before = current.len();
            current.extend(birth);
            changed |= before != current.len();
        }
    }
    Ok(changed)
}

fn birth(
    state: &State,
    carrier: Carrier,
    path: &[u32],
    budget: &mut Budget,
) -> CompileResult<BTreeSet<Birth>> {
    if let [index] = path {
        return match carrier {
            Carrier::Value(value) => package_birth(state, value, *index, budget),
            Carrier::Local(local) => Ok(copied(
                state,
                Place::PackageField {
                    local,
                    index: *index,
                },
                budget,
            )?
            .birth),
        };
    }
    budget.lookup(state.product_births.len(), "Product origin carrier lookup")?;
    let leaves = state
        .product_births
        .get(&carrier)
        .ok_or_else(|| error("missing Product origin carrier"))?;
    budget.product(
        path.len(),
        leaves.len().saturating_add(1).ilog2() as usize + 2,
        1,
        "Product origin path lookup",
    )?;
    let value = leaves
        .get(path)
        .ok_or_else(|| error("missing Product origin leaf"))?;
    budget.spend(value.len(), "Product origin set copy")?;
    Ok(value.clone())
}

fn set(
    state: &mut State,
    carrier: Carrier,
    path: &[u32],
    value: BTreeSet<Birth>,
    budget: &mut Budget,
) -> CompileResult<()> {
    if let [index] = path {
        return match carrier {
            Carrier::Value(result) => set_package_birth(state, result, *index, value, budget),
            Carrier::Local(local) => {
                let place = Place::PackageField {
                    local,
                    index: *index,
                };
                let mut fact = copied(state, place, budget)?;
                fact.birth = value;
                replace(state, place, fact, budget)
            }
        };
    }
    budget.lookup(state.product_births.len(), "Product origin carrier update")?;
    let leaves = state
        .product_births
        .get_mut(&carrier)
        .ok_or_else(|| error("missing Product destination carrier"))?;
    budget.product(
        path.len(),
        leaves.len().saturating_add(1).ilog2() as usize + 2,
        1,
        "Product origin destination lookup",
    )?;
    let target = leaves
        .get_mut(path)
        .ok_or_else(|| error("missing Product destination leaf"))?;
    budget.spend(target.len(), "Product origin replacement")?;
    *target = value;
    Ok(())
}

pub(super) fn transfer(
    state: &mut State,
    source: (Carrier, &[u32]),
    target: (Carrier, &[u32]),
    child: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    // Both complete actual types were compared by the instruction transfer.
    // Enumerate each occurrence; a shared type recipe never shares state.
    visit(child, &mut Vec::new(), budget, &mut |relative, budget| {
        budget.spend(
            source
                .1
                .len()
                .saturating_add(target.1.len())
                .saturating_add(relative.len().saturating_mul(2)),
            "Product source and target paths",
        )?;
        let mut from = source.1.to_vec();
        from.extend_from_slice(relative);
        let mut to = target.1.to_vec();
        to.extend_from_slice(relative);
        let value = birth(state, source.0, &from, budget)?;
        set(state, target.0, &to, value, budget)
    })
}
