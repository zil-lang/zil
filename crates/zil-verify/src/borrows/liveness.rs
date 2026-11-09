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
mod tests {
    use super::*;
    fn compare(words: &Bits, reference: &[bool]) {
        assert_eq!(words.width, reference.len());
        for (index, expected) in reference.iter().enumerate() {
            assert_eq!(words.get(index), Some(*expected));
        }
        assert_eq!(
            words.ones().collect::<Vec<_>>(),
            reference
                .iter()
                .enumerate()
                .filter_map(|(i, yes)| yes.then_some(i))
                .collect::<Vec<_>>()
        );
        assert_eq!(words.get(reference.len()), None);
        if !words.width.is_multiple_of(64) {
            assert_eq!(words.words.last().unwrap() >> (words.width % 64), 0);
        }
    }
    #[test]
    fn packed_liveness_matches_full_bool_at_word_and_local_boundaries() {
        let mut b = Budget::new(&Limits::default());
        for width in [0, 1, 63, 64, 65, 127, 128, 129] {
            let mut packed = Bits::new(width);
            let mut reference = vec![false; width];
            for (index, slot) in reference.iter_mut().enumerate() {
                *slot = index % 3 == 0;
                packed.assign(index, *slot).unwrap();
            }
            let mut other = Bits::new(width);
            let mut old_other = vec![false; width];
            for (index, slot) in old_other.iter_mut().enumerate() {
                *slot = index % 5 == 0;
                other.assign(index, *slot).unwrap();
            }
            assert_eq!(
                packed.join(&other, &mut b).unwrap(),
                super::super::bits_join(&mut reference, &old_other, &mut b).unwrap()
            );
            compare(&packed, &reference);
            for index in (0..width).step_by(7) {
                packed.assign(index, false).unwrap();
                reference[index] = false;
            }
            compare(&packed, &reference);
            let before = packed.clone();
            assert!(packed.assign(width, true).is_err());
            assert!(packed.contains(width).is_err());
            assert_eq!(packed, before);
        }
    }
    #[test]
    fn packed_reverse_matches_original_kill_gen_and_retained_order() {
        let mut b = Budget::new(&Limits::default());
        for width in [63, 64, 65, 127, 128, 129] {
            let values = if width > 64 { 63 } else { width / 2 };
            let local = LocalId((width - values - 1) as u32);
            let schedule = crate::place_replacements::Schedule {
                entries: vec![None],
                retained: vec![vec![vec![ValueId(1), ValueId((values - 1) as u32)]]],
            };
            for kind in [
                InstructionKind::Store {
                    local,
                    value: ValueId(0),
                },
                InstructionKind::StorageLive(local),
                InstructionKind::StorageDead(local),
                InstructionKind::Load(local),
                InstructionKind::TakeLocal { local },
                InstructionKind::BorrowLocalMut { local },
                InstructionKind::BorrowLocal { local },
                InstructionKind::BorrowDynOwned {
                    local,
                    mutable: true,
                },
                InstructionKind::CallClosureMut {
                    callee: local,
                    arguments: vec![ValueId(0)],
                },
                InstructionKind::CallClosureShared {
                    callee: local,
                    arguments: vec![ValueId(0)],
                },
            ] {
                let mut packed = Bits::new(width);
                let mut reference = vec![false; width];
                for (index, slot) in reference.iter_mut().enumerate() {
                    *slot = index % 3 == 0;
                    packed.assign(index, *slot).unwrap();
                }
                let instruction = Instruction {
                    result: Some(ValueId(1)),
                    kind,
                    span: Span::default(),
                };
                let terminator = Terminator::Return(Some(ValueId(2)));
                packed.term_use(&terminator).unwrap();
                super::super::term_use(&mut reference, &terminator);
                compare(&packed, &reference);
                packed.reverse(&instruction, values).unwrap();
                super::super::reverse(&mut reference, &instruction, values);
                compare(&packed, &reference);
                packed.retain(Some(&schedule), 0, 0, &mut b).unwrap();
                super::super::retain_replacement_values(
                    &mut reference,
                    Some(&schedule),
                    0,
                    0,
                    &mut b,
                )
                .unwrap();
                compare(&packed, &reference);
                assert!(
                    packed.contains(1).unwrap(),
                    "retained schedule runs after result kill"
                );
            }
        }
    }

    #[test]
    fn packed_cfg_inputs_outputs_and_instruction_rows_match_full_bool_with_backedges() {
        // Raw static-dataflow oracle, not a Source acceptance claim. Include a
        // diamond, a real backedge, a dead block and hidden retained SSA uses.
        for width in [63, 64, 65, 127, 128, 129] {
            let values = if width > 64 { 63 } else { width / 2 };
            let last_local = LocalId((width - values - 1) as u32);
            let instruction = |result, kind| Instruction {
                result,
                kind,
                span: Span::default(),
            };
            let blocks = [
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![
                        instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                        instruction(
                            Some(ValueId((values - 1) as u32)),
                            InstructionKind::Load(last_local),
                        ),
                    ],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![instruction(
                        Some(ValueId(1)),
                        InstructionKind::BorrowLocal { local: LocalId(0) },
                    )],
                    terminator: Terminator::Jump(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![instruction(
                        Some(ValueId(2)),
                        InstructionKind::TakeLocal { local: last_local },
                    )],
                    terminator: Terminator::Jump(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(3),
                    instructions: vec![instruction(
                        None,
                        InstructionKind::CallClosureMut {
                            callee: last_local,
                            arguments: vec![ValueId(0)],
                        },
                    )],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        then_block: BlockId(0),
                        else_block: BlockId(4),
                    },
                },
                BasicBlock {
                    id: BlockId(4),
                    instructions: vec![instruction(
                        Some(ValueId(3)),
                        InstructionKind::Load(last_local),
                    )],
                    terminator: Terminator::Return(Some(ValueId(3))),
                },
                BasicBlock {
                    id: BlockId(5),
                    instructions: vec![instruction(
                        Some(ValueId(4)),
                        InstructionKind::BorrowDynOwned {
                            local: last_local,
                            mutable: true,
                        },
                    )],
                    terminator: Terminator::Return(Some(ValueId(4))),
                },
            ];
            let successors = [vec![1, 2], vec![3], vec![3], vec![0, 4], vec![], vec![]];
            let reachable = [true, true, true, true, true, false];
            let mut schedule = crate::place_replacements::Schedule {
                entries: vec![None; blocks.len()],
                retained: blocks
                    .iter()
                    .map(|block| vec![vec![]; block.instructions.len()])
                    .collect(),
            };
            schedule.retained[3][0].push(ValueId((values - 1) as u32));
            let mut input = vec![Bits::new(width); blocks.len()];
            let mut output = input.clone();
            let mut old_input = vec![vec![false; width]; blocks.len()];
            let mut old_output = old_input.clone();
            let mut b = Budget::new(&Limits::default());
            let mut rounds = 0;
            loop {
                rounds += 1;
                let mut changed = false;
                for block in (0..blocks.len()).rev() {
                    if !reachable[block] {
                        continue;
                    }
                    let mut next = Bits::new(width);
                    let mut old = vec![false; width];
                    for &child in &successors[block] {
                        assert_eq!(
                            next.join(&input[child], &mut b).unwrap(),
                            super::super::bits_join(&mut old, &old_input[child], &mut b).unwrap()
                        );
                    }
                    output[block].clone_from(&next);
                    old_output[block].clone_from(&old);
                    next.term_use(&blocks[block].terminator).unwrap();
                    super::super::term_use(&mut old, &blocks[block].terminator);
                    for (position, i) in blocks[block].instructions.iter().enumerate().rev() {
                        next.reverse(i, values).unwrap();
                        super::super::reverse(&mut old, i, values);
                        next.retain(Some(&schedule), block, position, &mut b)
                            .unwrap();
                        super::super::retain_replacement_values(
                            &mut old,
                            Some(&schedule),
                            block,
                            position,
                            &mut b,
                        )
                        .unwrap();
                    }
                    compare(&next, &old);
                    compare(&output[block], &old_output[block]);
                    assert_eq!(next != input[block], old != old_input[block]);
                    if next != input[block] {
                        input[block] = next;
                        old_input[block] = old;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
                assert!(rounds < 32);
            }
            assert!(rounds > 1);
            for (block, data) in blocks.iter().enumerate() {
                if !reachable[block] {
                    continue;
                }
                let mut next = output[block].clone();
                let mut old = old_output[block].clone();
                next.term_use(&data.terminator).unwrap();
                super::super::term_use(&mut old, &data.terminator);
                for (position, i) in data.instructions.iter().enumerate().rev() {
                    compare(&next, &old); // after[position], before reverse
                    next.reverse(i, values).unwrap();
                    super::super::reverse(&mut old, i, values);
                    next.retain(Some(&schedule), block, position, &mut b)
                        .unwrap();
                    super::super::retain_replacement_values(
                        &mut old,
                        Some(&schedule),
                        block,
                        position,
                        &mut b,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "liveness_tests.rs"]
mod solver_tests;
