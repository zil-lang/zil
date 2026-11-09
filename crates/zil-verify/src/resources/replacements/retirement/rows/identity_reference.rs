// Complete 182 eager paged merge and whole State merge/cost bodies.
impl Rows {
fn merge_eager_182(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        fn merge_rows(
            a: &mut [Sources],
            from: &[Sources],
            paged: bool,
            b: &mut Budget,
        ) -> Check<bool> {
            let mut changed = false;
            for (target, source) in a.iter_mut().zip(from) {
                // Newly inserted variable-width recipes may survive in an Arc
                // page. Prepay cleanup before the unchanged actual merge body.
                if paged {
                    source.cost(b)?;
                }
                changed |= target.merge(source, b)?;
            }
            Ok(changed)
        }
        let mut changed = false;
        match (&mut self.storage, &other.storage) {
            (Storage::Dense(a), Storage::Dense(from)) => changed |= merge_rows(a, from, false, b)?,
            (Storage::Dense(a), Storage::Pages { pages, .. }) => {
                for (target, source) in a.chunks_mut(PAGE).zip(pages) {
                    b.spend(2)?; // actual target chunk and source page records
                    changed |= merge_rows(target, source, false, b)?;
                }
            }
            (Storage::Pages { pages, .. }, Storage::Dense(from)) => {
                for (target, source) in pages.iter_mut().zip(from.chunks(PAGE)) {
                    b.spend(2)?;
                    changed |= merge_rows(unique_page(target, b)?, source, true, b)?;
                }
            }
            (Storage::Pages { pages, .. }, Storage::Pages { pages: from, .. }) => {
                for (target, source) in pages.iter_mut().zip(from) {
                    b.spend(2)?;
                    changed |= merge_rows(unique_page(target, b)?, source, true, b)?;
                }
            }
        }
        Ok(changed)
    }
}

fn eager_state_cost(s: &State, budget: &mut Budget) -> Check<()> {
    s.retirement.full_cost(budget)?;
    eager_other_state_cost(s, budget)
}

fn eager_other_state_cost(s: &State, budget: &mut Budget) -> Check<()> {
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

fn eager_state_merge(target: &mut State, source: &State, budget: &mut Budget) -> Check<bool> {
    eager_state_cost(source, budget)?;
    let mut changed = target.retirement.merge_eager_182(&source.retirement, budget)?;
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
