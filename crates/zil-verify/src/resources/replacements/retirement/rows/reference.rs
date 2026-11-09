// Complete flat 54b State and operations, with original Source recipes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OldState {
    debt: Vec<u8>,
    fresh: Vec<u8>,
    // Independent SSA Package field provenance. These bytes do not add debt:
    // a complete Package still moves as one value, with local partial fields
    // checked by the existing responsibility slots.
    package_fresh: field_fresh::Fields,
    // Actual local/SSA Managed leaves of new Product functions, independently
    // rebuilt here. Empty on the historical flat path; never a public proof.
    product_fresh: Vec<u8>,
    desc: Vec<BTreeSet<Root>>,
    retirement: Vec<replacements::Sources>,
    // An edge a -> b means b may have been initialized later while a remained live.
    newer: Vec<BTreeSet<usize>>,
}

fn old_state_cost(s: &OldState, budget: &mut Budget) -> Check<()> {
    if !s.retirement.is_empty() {
        budget.spend(s.retirement.len())?;
        for sources in &s.retirement {
            sources.cost(budget)?;
        }
    }
    budget.spend(
        s.debt
            .len()
            .saturating_mul(2)
            .saturating_add(s.product_fresh.len())
            .saturating_add(s.desc.len())
            .saturating_add(s.newer.len()),
    )?;
    s.package_fresh.cost(budget)?;
    for row in &s.desc {
        budget.spend(row.len())?;
    }
    for row in &s.newer {
        budget.spend(row.len())?;
    }
    Ok(())
}

fn old_clone_state(s: &OldState, budget: &mut Budget) -> Check<OldState> {
    old_state_cost(s, budget)?;
    Ok(s.clone())
}

fn old_merge(target: &mut OldState, source: &OldState, budget: &mut Budget) -> Check<bool> {
    old_state_cost(source, budget)?;
    let mut changed = false;
    for (a, b) in target.retirement.iter_mut().zip(&source.retirement) {
        changed |= a.merge(b, budget)?;
    }
    for (a, b) in target
        .debt
        .iter_mut()
        .zip(&source.debt)
        .chain(target.fresh.iter_mut().zip(&source.fresh))
        .chain(target.product_fresh.iter_mut().zip(&source.product_fresh))
    {
        let next = *a | *b;
        changed |= next != *a;
        *a = next;
    }
    changed |= target.package_fresh.merge(&source.package_fresh, budget)?;
    for (a, b) in target.desc.iter_mut().zip(&source.desc) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    for (a, b) in target.newer.iter_mut().zip(&source.newer) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    Ok(changed)
}
