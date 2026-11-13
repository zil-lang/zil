//! Pending invalidations of actual Copy local tag content, never tag authority.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pending {
    eligible: Arc<Vec<bool>>,
    locals: BTreeSet<usize>,
}
fn lookup(len: usize) -> usize {
    len.checked_ilog2().map_or(1, |n| n as usize + 2)
}
impl Pending {
    pub(super) fn new(eligible: Vec<bool>, budget: &mut Budget) -> Check<Self> {
        budget.spend(eligible.len().saturating_add(3))?;
        let mut locals = BTreeSet::new();
        for (local, &tracked) in eligible.iter().enumerate() {
            if tracked {
                budget.spend(lookup(locals.len()).saturating_add(1))?;
                locals.insert(local);
            }
        }
        Ok(Self {
            eligible: Arc::new(eligible),
            locals,
        })
    }
    pub(super) fn copy_work(&self) -> usize {
        self.locals.len().saturating_add(3)
    }
    pub(super) fn mark(&mut self, local: usize, budget: &mut Budget) -> Check<()> {
        budget.spend(2)?;
        let Some(&tracked) = self.eligible.get(local) else {
            return Err(bad(None, "pending Sum local identity absent"));
        };
        if tracked {
            budget.spend(lookup(self.locals.len()).saturating_add(1))?;
            self.locals.insert(local);
        }
        Ok(())
    }
    pub(super) fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<()> {
        // Both states belong to this one Flow and its immutable raw-Type mask.
        budget.spend(1)?;
        if !Arc::ptr_eq(&self.eligible, &other.eligible) {
            return Err(bad(None, "pending Sum function identity differs"));
        }
        budget.spend(other.locals.len())?;
        for &local in &other.locals {
            self.mark(local, budget)?;
        }
        Ok(())
    }
    pub(super) fn scheduled(
        &self,
        always: &[usize],
        budget: &mut Budget,
    ) -> Check<Vec<(usize, bool)>> {
        let len = self.locals.len().saturating_add(always.len());
        budget.spend(len.saturating_mul(3).saturating_add(2))?;
        let mut scheduled = Vec::with_capacity(len);
        let mut pending = self.locals.iter().copied().peekable();
        let mut fixed = always.iter().copied().peekable();
        while let (Some(&left), Some(&right)) = (pending.peek(), fixed.peek()) {
            budget.spend(3)?;
            if left < right {
                scheduled.push((pending.next().expect("actual pending local"), true));
            } else if right < left {
                scheduled.push((fixed.next().expect("actual always local"), false));
            } else {
                return Err(bad(
                    None,
                    "pending and unconditional Sum inventories overlap",
                ));
            }
        }
        scheduled.extend(pending.map(|local| (local, true)));
        scheduled.extend(fixed.map(|local| (local, false)));
        Ok(scheduled)
    }
    #[cfg(test)]
    pub(super) fn snapshot(&self, budget: &mut Budget) -> Check<Vec<usize>> {
        budget.spend(self.locals.len().saturating_add(1))?;
        Ok(self.locals.iter().copied().collect())
    }
    pub(super) fn clear(&mut self, local: usize, budget: &mut Budget) -> Check<()> {
        budget.spend(lookup(self.locals.len()).saturating_add(1))?;
        self.locals.remove(&local);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget(n: usize) -> Budget {
        Budget::new(&Limits {
            max_analysis_steps: n,
            ..Limits::default()
        })
    }
    #[test]
    fn pending_mask_snapshots_join_and_budget_failure_do_not_change_tag_content() {
        let mut b = budget(100_000);
        let mut first = Pending::new(vec![true, false, true], &mut b).unwrap();
        let alias = first.clone();
        assert_eq!(first.snapshot(&mut b).unwrap(), vec![0, 2]);
        assert_eq!(
            first.scheduled(&[1], &mut b).unwrap(),
            vec![(0, true), (1, false), (2, true)]
        );
        assert!(first.scheduled(&[0], &mut b).is_err());
        first.clear(0, &mut b).unwrap();
        first.clear(2, &mut b).unwrap();
        first.mark(1, &mut b).unwrap();
        assert!(first.snapshot(&mut b).unwrap().is_empty());
        first.mark(0, &mut b).unwrap();
        assert_eq!(alias.snapshot(&mut b).unwrap(), vec![0, 2]);
        first.merge(&alias, &mut b).unwrap();
        assert_eq!(first.snapshot(&mut b).unwrap(), vec![0, 2]);
        let unrelated = Pending::new(vec![true, false, true], &mut b).unwrap();
        assert!(first.merge(&unrelated, &mut b).is_err());
        assert!(first.mark(3, &mut b).is_err());
        first.clear(0, &mut b).unwrap();
        let before = first.clone();
        let mut needed = budget(100_000);
        first.mark(0, &mut needed).unwrap();
        let n = 100_000 - needed.remaining;
        first = before.clone();
        assert_eq!(
            first.mark(0, &mut budget(n - 1)).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(first, before);
        first.mark(0, &mut budget(n)).unwrap();
        assert_eq!(first.snapshot(&mut b).unwrap(), vec![0, 2]);
    }
}
