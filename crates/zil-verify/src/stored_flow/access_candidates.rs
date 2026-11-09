//! Conflict candidates reconstructed from this instruction's actual loan table.
use super::*;

pub(super) struct Access<'step> {
    all: &'step [usize],
    loans: &'step [Loan],
    reads: Option<Vec<usize>>,
}
impl<'step> Access<'step> {
    pub(super) fn new(all: &'step [usize], loans: &'step [Loan], b: &mut Budget) -> Check<Self> {
        b.spend(1)?;
        Ok(Self {
            all,
            loans,
            reads: None,
        })
    }
    fn read_candidates(&mut self, b: &mut Budget) -> Check<&[usize]> {
        b.spend(1)?;
        if self.reads.is_none() {
            // Preserve active order and duplicates, including missing IDs:
            // only a valid shared loan has no read-conflict tail effects.
            b.spend(self.all.len().saturating_mul(2).saturating_add(1))?;
            let mut selected = Vec::new();
            for id in self.all {
                if self.loans.get(*id).is_none_or(|loan| loan.exclusive) {
                    if selected.len() == selected.capacity() {
                        b.spend(selected.len().saturating_add(1))?;
                    }
                    b.spend(1)?;
                    selected.push(*id);
                }
            }
            // A budget error during growth or classification publishes nothing.
            self.reads = Some(selected);
        }
        Ok(self.reads.as_deref().unwrap())
    }
    pub(super) fn check(
        &mut self,
        permission: &Permission,
        write: bool,
        requirements: &mut Vec<(Source, Source)>,
        b: &mut Budget,
    ) -> Check<()> {
        access_head(permission, write, b)?;
        let loans = self.loans;
        let delegated = ancestry(&permission.loans, loans, b)?;
        let active = if write {
            self.all
        } else {
            self.read_candidates(b)?
        };
        access_tail(
            permission,
            write,
            active,
            loans,
            &delegated,
            requirements,
            b,
        )
    }
}

#[cfg(test)]
#[path = "access_candidates_tests.rs"]
mod tests;
