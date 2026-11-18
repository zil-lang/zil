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

type Site = (BlockId, Option<u32>, Option<zil_base::Span>);

fn site(block: BlockId, position: usize, instruction: &Instruction) -> Site {
    use zil_base::Span;
    let Span { start, end, .. } = instruction.span;
    (
        block,
        u32::try_from(position).ok(),
        (start < end).then_some(instruction.span),
    )
}

fn origin(
    f: &TypedFunction,
    loan: usize,
    ids: &SlotLoanIds,
    b: &mut Budget,
) -> CompileResult<Option<Site>> {
    let mut address = None;
    if loan >= f.values.len() {
        for (&(instruction, _, _), &(id, _)) in &ids.physical {
            fee(b, 1)?;
            if id == loan {
                address = Some(instruction);
                break;
            }
        }
        if address.is_none() {
            for (&(instruction, _, _), &(id, _)) in &ids.dynamic {
                fee(b, 1)?;
                if id == loan {
                    address = Some(instruction);
                    break;
                }
            }
        }
        if address.is_none() {
            for (key, &(id, _)) in &ids.raw {
                fee(b, 1)?;
                if id == loan {
                    address = Some(match *key {
                        raw_references::WriteSite::Write(site)
                        | raw_references::WriteSite::Call { site, .. } => site,
                    });
                    break;
                }
            }
        }
        if address.is_none() {
            return Ok(None);
        }
    }
    for block in &f.blocks {
        for (position, instruction) in block.instructions.iter().enumerate() {
            fee(b, 1)?;
            let matches = match address {
                Some(address) => std::ptr::from_ref(instruction) as usize == address,
                None => instruction
                    .result
                    .is_some_and(|value| value.0 as usize == loan),
            };
            if matches {
                return Ok(Some(site(block.id, position, instruction)));
            }
        }
    }
    Ok(None)
}

fn later_use(
    module: &TypedModule,
    f: &TypedFunction,
    conflict: (BlockId, usize),
    instruction: &Instruction,
    loan: usize,
    live: &LiveRow,
    state: &State,
    values: &[Value],
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Option<Site>> {
    use std::collections::VecDeque;

    fee(b, live.work())?;
    let mut row = LiveRow::empty(live.len());
    let mut pending = VecDeque::new();
    let mut visited = BTreeSet::new();
    for index in live.ones() {
        fee(b, 1)?;
        let value = if index < values.len() {
            values.get(index)
        } else {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&LocalId((index - values.len()) as u32))
        };
        let Some(value) = value else { continue };
        let mut direct = Loans::new();
        active_value(value, state, &mut direct, b)?;
        if !ancestry(&direct, loans, b)?.contains(&loan) {
            continue;
        }
        // The future carrier must still contain the loan across this access.
        row.put(index, true)?;
        reverse(module, f, instruction, &mut row, b)?;
        if row.at(index)? {
            pending.push_back((conflict.0, conflict.1 + 1, index));
        }
    }
    while let Some((block, start, index)) = pending.pop_front() {
        ordered(b, visited.len(), 0)?;
        if !visited.insert((block, start, index)) {
            continue;
        }
        let Some(block) = f
            .blocks
            .get(block.0 as usize)
            .filter(|found| found.id == block)
        else {
            continue;
        };
        let mut survives = true;
        for (position, instruction) in block.instructions.iter().enumerate().skip(start) {
            // Applying the same transfer to clear and set bits distinguishes
            // a real use, a killing definition, and an unchanged carrier.
            row.put(index, false)?;
            reverse(module, f, instruction, &mut row, b)?;
            if row.at(index)? {
                return Ok(Some(site(block.id, position, instruction)));
            }
            row.put(index, true)?;
            reverse(module, f, instruction, &mut row, b)?;
            if !row.at(index)? {
                survives = false;
                break;
            }
        }
        if !survives {
            continue;
        }
        match block.terminator {
            Terminator::Return(Some(value))
            | Terminator::Branch {
                condition: value, ..
            } if value.0 as usize == index => {
                return Ok(Some((block.id, None, None)));
            }
            Terminator::Jump(target) => pending.push_back((target, 0, index)),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push_back((then_block, 0, index));
                pending.push_back((else_block, 0, index));
            }
            _ => {}
        }
    }
    Ok(None)
}

pub(super) fn explain_conflict(
    error: &mut zil_base::Diagnostic,
    module: &TypedModule,
    f: &TypedFunction,
    conflict: (BlockId, usize),
    instruction: &Instruction,
    loan: Option<usize>,
    live: &LiveRow,
    state: &State,
    values: &[Value],
    loans: &[Loan],
    ids: &SlotLoanIds,
    b: &mut Budget,
) {
    use zil_base::{DiagnosticIrLocation, DiagnosticRole};
    let current = site(conflict.0, conflict.1, instruction);
    error.span = current.2;
    let mut relate = |role, (block, position, span): Site, message| {
        error.relate(
            role,
            span,
            Some(DiagnosticIrLocation {
                function: f.id.0,
                block: Some(block.0),
                instruction: position,
            }),
            message,
        );
    };
    if let Some(loan) = loan {
        if let Ok(Some(created)) = origin(f, loan, ids, b) {
            relate(
                DiagnosticRole::BorrowCreated,
                created,
                "The borrow is created here.",
            );
        }
    }
    relate(
        DiagnosticRole::ConflictingAccess,
        current,
        "This access conflicts with the live borrow.",
    );
    if let Some(loan) = loan {
        if let Ok(Some(later)) = later_use(
            module,
            f,
            conflict,
            instruction,
            loan,
            live,
            state,
            values,
            loans,
            b,
        ) {
            relate(
                DiagnosticRole::LaterUse,
                later,
                "This use keeps the borrow live.",
            );
        }
    }
}

#[cfg(test)]
#[path = "permission_access_tests.rs"]
mod tests;
