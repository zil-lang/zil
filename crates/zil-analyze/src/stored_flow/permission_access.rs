//! Read conflict candidates for one instruction's immutable actual loan table.
use super::*;

pub(super) struct Access<'step> {
    all: &'step Loans,
    loans: &'step [Loan],
    reads: Option<Loans>,
}
impl<'step> Access<'step> {
    pub(super) fn new(
        all: &'step Loans,
        loans: &'step [Loan],
        b: &mut Budget,
    ) -> CompileResult<Self> {
        fee(b, 1)?;
        Ok(Self {
            all,
            loans,
            reads: None,
        })
    }
    fn read_candidates(&mut self, b: &mut Budget) -> CompileResult<&Loans> {
        fee(b, 1)?;
        if self.reads.is_none() {
            // Visit and inspect every current ID. Missing entries stay in the
            // original tail so ignore-before-error behavior is unchanged.
            fee(b, self.all.len().saturating_mul(2).saturating_add(1))?;
            let mut selected = Loans::new();
            for id in self.all {
                if self.loans.get(*id).is_none_or(|loan| loan.mutable) {
                    ordered(b, selected.len(), 0)?;
                    fee(b, 1)?;
                    selected.insert(*id);
                }
            }
            // A failed construction never publishes a partial candidate set.
            self.reads = Some(selected);
        }
        Ok(self.reads.as_ref().unwrap())
    }
    pub(super) fn check(
        &mut self,
        permission: &Permission,
        write: bool,
        apart: &mut BTreeSet<(Target, Target)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        permission_head(permission, write, b)?;
        let loans = self.loans;
        let ignore = ancestry(&permission.loans, loans, b)?;
        let active = if write {
            self.all
        } else {
            self.read_candidates(b)?
        };
        permission_tail(permission, write, active, loans, &ignore, apart, b)
    }
}

#[cfg(test)]
#[path = "permission_access_tests.rs"]
mod tests;
