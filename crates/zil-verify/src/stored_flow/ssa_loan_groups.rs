//! Current-window equivalence of direct SSA loan contributions only.
//! Permissions, source targets, parent loans and frame contents remain dynamic.
use super::*;

struct Members<'values> {
    loans: &'values [usize],
    values: Vec<usize>,
}
#[derive(Debug)]
pub(super) struct Sources<'values> {
    _values: &'values [Payload],
    ordinary: Vec<usize>,
    groups: Vec<WordRow>,
}
fn push<T>(into: &mut Vec<T>, value: T, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    if into.len() == into.capacity() {
        b.spend(into.len().saturating_add(1))?;
    }
    b.spend(1)?;
    into.push(value);
    Ok(())
}
// A conservative current-inventory bound; this schedules an internal lookup,
// never a language/permission qualification. Overflow chooses the old scan.
fn query_bound(groups: usize, ordinary: usize, words: usize) -> usize {
    let selected = groups.saturating_add(ordinary);
    groups
        .saturating_mul(words.saturating_mul(2).saturating_add(5))
        .saturating_add(ordinary.saturating_mul(3))
        .saturating_add(selected.saturating_mul(selected).saturating_mul(4))
        .saturating_add(selected.saturating_mul(8))
        .saturating_add(5)
}
impl<'values> Sources<'values> {
    pub(super) fn collect(values: &'values [Payload], b: &mut Budget) -> Check<Option<Self>> {
        // Both temporary containers and their eventual disposal.
        b.spend(4)?;
        let mut members: Vec<Members<'values>> = Vec::new();
        let mut ordinary = Vec::new();
        let words = WordRow::work_for(values.len());
        b.spend(16)?;
        if query_bound(0, 0, words) >= values.len() {
            return Ok(None);
        }
        for (id, payload) in values.iter().enumerate() {
            b.spend(3)?;
            if !payload.leaves.is_empty() || !payload.task_ports.is_empty()
                || b.nested_store_windows && !payload.store_description.loans.is_empty() {
                push(&mut ordinary, id, b)?;
                b.spend(16)?;
                if query_bound(members.len(), ordinary.len(), words) >= values.len() {
                    return Ok(None);
                }
                continue;
            }
            if payload.address.loans.is_empty() {
                // The unchanged collector reads no other SSA fields here.
                continue;
            }
            let key = payload.address.loans.as_slice();
            let mut found = None;
            for (at, group) in members.iter().enumerate() {
                b.spend(
                    group
                        .loans
                        .len()
                        .saturating_add(key.len())
                        .saturating_add(1),
                )?;
                if group.loans == key {
                    found = Some(at);
                    break;
                }
            }
            let at = if let Some(at) = found {
                at
            } else {
                // Borrow the complete original sequence; no arena validation,
                // sorting, duplicate removal or loan metadata is cached.
                b.spend(3)?;
                let at = members.len();
                push(
                    &mut members,
                    Members {
                        loans: key,
                        values: Vec::new(),
                    },
                    b,
                )?;
                b.spend(16)?;
                if query_bound(members.len(), ordinary.len(), words) >= values.len() {
                    return Ok(None);
                }
                at
            };
            push(&mut members[at].values, id, b)?;
        }
        // Paid source record, mask container/writes and every mask's disposal.
        b.spend(members.len().saturating_mul(2).saturating_add(2))?;
        let mut groups = Vec::with_capacity(members.len());
        for group in members {
            b.spend(words.saturating_add(1))?;
            let mut row = WordRow::empty(values.len());
            for id in group.values {
                b.spend(1)?;
                row.assign(id, true)?;
            }
            groups.push(row);
        }
        Ok(Some(Self {
            _values: values,
            ordinary,
            groups,
        }))
    }
    pub(super) fn select(
        &self,
        live: &WordRow,
        definition: Option<ValueId>,
        b: &mut Budget,
    ) -> Check<Option<live_loan_ids::SortedLoanIds>> {
        b.spend(1)?;
        if live.len() < self._values.len() {
            // The legacy full scan also accepts a shorter logical domain;
            // its indexed fee would subtract the SSA width, so keep that lane.
            return Ok(None);
        }
        b.spend(
            self.ordinary
                .len()
                .saturating_add(self.groups.len())
                .saturating_add(1),
        )?;
        let mut selected = live_loan_ids::SortedLoanIds::default();
        for &id in &self.ordinary {
            b.spend(2)?;
            if live.is_live(id)? && definition.is_none_or(|v| v.0 as usize != id) {
                selected.add(&[id], b)?;
            }
        }
        for group in &self.groups {
            if let Some(id) = group.first_common(live, definition, b)? {
                selected.add(&[id], b)?;
            }
        }
        Ok(Some(selected))
    }
}

#[cfg(test)]
#[path = "ssa_loan_groups_tests.rs"]
mod tests;
