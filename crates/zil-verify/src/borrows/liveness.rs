//! Private liveness storage. Bit positions remain original SSA/local indices;
//! only storage copies, union and enumeration operate on actual u64 words.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Bits {
    width: usize,
    words: Vec<u64>,
}
impl Bits {
    pub(super) fn word_count(width: usize) -> usize {
        width / 64 + usize::from(!width.is_multiple_of(64))
    }
    // Callers prepay the actual word allocation/copy before construction.
    pub(super) fn new(width: usize) -> Self {
        Self {
            width,
            words: vec![0; Self::word_count(width)],
        }
    }
    pub(super) fn units(&self) -> usize {
        self.words.len()
    }
    pub(super) fn work(&self) -> usize {
        self.units() + 1
    }
    pub(super) fn get(&self, index: usize) -> Option<bool> {
        (index < self.width).then(|| self.words[index / 64] & (1u64 << (index % 64)) != 0)
    }
    pub(super) fn contains(&self, index: usize) -> Check<bool> {
        self.get(index).ok_or_else(|| {
            reject(
                "E-VERIFY-CFG",
                None,
                "liveness index exceeds actual SSA/local width",
            )
        })
    }
    fn assign(&mut self, index: usize, value: bool) -> Check<()> {
        if index >= self.width {
            return Err(reject(
                "E-VERIFY-CFG",
                None,
                "liveness definition exceeds actual SSA/local width",
            ));
        }
        let mask = 1u64 << (index % 64);
        let word = &mut self.words[index / 64];
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
        Ok(())
    }
    pub(super) fn join(&mut self, from: &Self, b: &mut Budget) -> Check<bool> {
        if self.width != from.width {
            return Err(reject(
                "E-VERIFY-CFG",
                None,
                "liveness rows have different actual widths",
            ));
        }
        b.spend(self.work())?;
        let mut changed = false;
        for (to, from) in self.words.iter_mut().zip(&from.words) {
            let merged = *to | *from;
            changed |= merged != *to;
            *to = merged;
        }
        Ok(changed)
    }
    pub(super) fn reverse(&mut self, instruction: &Instruction, values: usize) -> Check<()> {
        if let Some(value) = instruction.result {
            self.assign(value.0 as usize, false)?;
        }
        match instruction.kind {
            InstructionKind::Store { local, .. }
            | InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local) => {
                self.assign(values + local.0 as usize, false)?
            }
            InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocalMut { local }
            | InstructionKind::BorrowDynOwned { local, .. }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::CallClosureMut { callee: local, .. }
            | InstructionKind::CallClosureShared { callee: local, .. } => {
                self.assign(values + local.0 as usize, true)?
            }
            _ => {}
        }
        for operand in instruction_operands(&instruction.kind) {
            self.assign(operand.0 as usize, true)?;
        }
        Ok(())
    }
    pub(super) fn term_use(&mut self, terminator: &Terminator) -> Check<()> {
        match terminator {
            Terminator::Return(Some(value))
            | Terminator::Branch {
                condition: value, ..
            } => self.assign(value.0 as usize, true)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn retain(
        &mut self,
        schedule: Option<&crate::place_replacements::Schedule>,
        block: usize,
        position: usize,
        b: &mut Budget,
    ) -> Check<()> {
        if let Some(schedule) = schedule {
            let row = &schedule.retained[block][position];
            b.spend(row.len())?;
            for value in row {
                self.assign(value.0 as usize, true)?;
            }
        }
        Ok(())
    }
    pub(super) fn paid_ones(&self, b: &mut Budget) -> Check<Ones<'_>> {
        b.spend(self.work())?;
        Ok(Ones {
            words: self.words.iter().enumerate(),
            pending: 0,
            word: 0,
            width: self.width,
        })
    }
    #[cfg(test)]
    pub(super) fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(move |(word, &bits)| {
                let mut remaining = bits;
                std::iter::from_fn(move || {
                    if remaining == 0 {
                        return None;
                    }
                    let bit = remaining.trailing_zeros() as usize;
                    remaining &= remaining - 1;
                    Some(word * 64 + bit)
                })
            })
            .take_while(move |&index| index < self.width)
    }
}

// Word visits are prepaid on construction; each extracted bit is prepaid before
// trailing_zeros/clear, including a final local encountered by an SSA-only scan.
pub(super) struct Ones<'a> {
    words: std::iter::Enumerate<std::slice::Iter<'a, u64>>,
    pending: u64,
    word: usize,
    width: usize,
}
impl Ones<'_> {
    pub(super) fn next(&mut self, b: &mut Budget) -> Check<Option<usize>> {
        loop {
            if self.pending != 0 {
                b.spend(1)?;
                let bit = self.pending.trailing_zeros() as usize;
                self.pending &= self.pending - 1;
                let index = self.word * 64 + bit;
                return Ok((index < self.width).then_some(index));
            }
            let Some((word, &bits)) = self.words.next() else {
                return Ok(None);
            };
            self.word = word;
            self.pending = bits;
        }
    }
}

#[cfg(test)]
#[path = "liveness_tests.rs"]
mod tests;
