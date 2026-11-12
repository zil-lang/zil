//! Analyzer-local storage for the original SSA/local liveness coordinates.
//! This is immutable-use data, not an origin, permission or loan conclusion.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiveRow {
    width: usize,
    data: Vec<u64>,
}
impl LiveRow {
    pub(super) fn words_for(width: usize) -> usize {
        width / 64 + usize::from(!width.is_multiple_of(64))
    }
    pub(super) fn empty(width: usize) -> Self {
        Self {
            width,
            data: vec![0; Self::words_for(width)],
        }
    }
    pub(super) fn len(&self) -> usize {
        self.width
    }
    // Include a row/container record even when the logical width is zero.
    pub(super) fn work(&self) -> usize {
        self.data.len().saturating_add(1)
    }
    pub(super) fn work_for(width: usize) -> usize {
        Self::words_for(width).saturating_add(1)
    }
    pub(super) fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.data
            .iter()
            .enumerate()
            .flat_map(|(word, &bits)| {
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
            .take_while(move |&index| index < self.len())
    }
    #[cfg(test)]
    pub(super) fn at(&self, index: usize) -> CompileResult<bool> {
        if index >= self.width {
            return Err(failure("liveness use exceeds actual SSA/local width"));
        }
        Ok(self.data[index / 64] & (1u64 << (index % 64)) != 0)
    }
    pub(super) fn put(&mut self, index: usize, live: bool) -> CompileResult<()> {
        if index >= self.width {
            return Err(failure(
                "liveness definition exceeds actual SSA/local width",
            ));
        }
        let mask = 1u64 << (index % 64);
        let slot = &mut self.data[index / 64];
        if live {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
        Ok(())
    }
    pub(super) fn include(&mut self, from: &Self, b: &mut Budget) -> CompileResult<bool> {
        fee(b, self.work())?;
        if self.width != from.width {
            return Err(failure("liveness rows have different logical widths"));
        }
        let mut changed = false;
        for (left, right) in self.data.iter_mut().zip(&from.data) {
            let union = *left | *right;
            changed |= union != *left;
            *left = union;
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(super) fn from_flags(flags: &[bool]) -> Self {
        let mut row = Self::empty(flags.len());
        for (index, yes) in flags.iter().enumerate() {
            row.put(index, *yes).unwrap();
        }
        row
    }
    #[cfg(test)]
    pub(super) fn flags(&self) -> Vec<bool> {
        (0..self.width)
            .map(|index| self.at(index).unwrap())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }
    fn budget() -> Budget {
        Budget::new(&Limits::default())
    }
    fn graph(f: &TypedFunction) -> crate::CfgFacts {
        let mut predecessors = vec![Vec::new(); f.blocks.len()];
        let successors = |t: &Terminator| -> Vec<usize> {
            match t {
                Terminator::Jump(to) => vec![to.0 as usize],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block.0 as usize, else_block.0 as usize],
                _ => vec![],
            }
        };
        for (index, block) in f.blocks.iter().enumerate() {
            for to in successors(&block.terminator) {
                predecessors[to].push(index);
            }
        }
        let mut reachable = vec![false; f.blocks.len()];
        let mut pending = vec![f.entry.0 as usize];
        while let Some(index) = pending.pop() {
            if !reachable[index] {
                reachable[index] = true;
                pending.extend(successors(&f.blocks[index].terminator));
            }
        }
        (predecessors.into(), reachable.into())
    }
    fn compare(module: &TypedModule, f: &TypedFunction) -> Vec<LiveRow> {
        let (predecessors, reachable) = graph(f);
        let expected = liveness_bool(module, f, &predecessors, &reachable, &mut budget()).unwrap();
        let actual = liveness(module, f, &predecessors, &reachable, &mut budget()).unwrap();
        assert_eq!(
            actual.iter().map(LiveRow::flags).collect::<Vec<_>>(),
            expected
        );
        for (bi, block) in f.blocks.iter().enumerate() {
            let mut old = expected[bi].clone();
            term_live_bool(&block.terminator, &mut old);
            let after = instruction_live(module, f, bi, &actual[bi], &mut budget()).unwrap();
            let mut words = actual[bi].clone();
            term_live(&block.terminator, &mut words).unwrap();
            for (position, instruction) in block.instructions.iter().enumerate().rev() {
                assert_eq!(
                    after[position].flags(),
                    old,
                    "complete after B{bi}:{position}"
                );
                reverse_bool(module, f, instruction, &mut old, &mut budget()).unwrap();
                reverse(module, f, instruction, &mut words, &mut budget()).unwrap();
                assert_eq!(words.flags(), old, "transfer/input B{bi}:{position}");
            }
        }
        actual
    }
    #[test]
    fn bounded_words_match_boolean_rows_at_boundaries() {
        for width in [0, 1, 63, 64, 65, 127, 128, 129] {
            let mut expected = vec![false; width];
            let mut row = LiveRow::empty(width);
            for index in 0..width {
                let live = index % 3 != 0;
                expected[index] = live;
                row.put(index, live).unwrap();
                assert_eq!(row.flags(), expected);
            }
            assert!(row.at(width).is_err());
            assert!(row.put(width, true).is_err());
            let other = LiveRow::from_flags(&(0..width).map(|i| i % 5 == 0).collect::<Vec<_>>());
            let expected_changed = expected.iter().zip(other.flags()).any(|(a, b)| b && !a);
            for (a, b) in expected.iter_mut().zip(other.flags()) {
                *a |= b;
            }
            assert_eq!(
                row.include(&other, &mut budget()).unwrap(),
                expected_changed
            );
            assert_eq!(row.flags(), expected);
            assert!(!row.include(&other, &mut budget()).unwrap());
            let before = row.clone();
            assert!(
                row.include(&LiveRow::empty(width + 1), &mut budget())
                    .is_err()
            );
            assert_eq!(row, before);
            if width % 64 != 0 {
                assert_eq!(row.data.last().unwrap() >> (width % 64), 0);
            }
            for index in 0..width {
                row.put(index, false).unwrap();
            }
            assert!(row.data.iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn stored_words_match_original_boolean_cfg_and_conditional_transfers() {
        for module in [
            fixture::module(),
            fixture::exclusive_returns(false),
            fixture::exclusive_returns(true),
            fixture::mutable_holder(),
            fixture::shared_two_slots(),
            fixture::wide_shared(65),
            fixture::captured_fixed(CallableKind::FnMut, false),
            fixture::captured_owned_fixed(false),
        ] {
            for f in &module.functions {
                compare(&module, f);
            }
        }
        let module = fixture::module();
        // Static dataflow oracle: diamond, backedge and a dead predecessor.
        // 63 SSA bits and one local share the same physical word.
        for width in [1, 63, 64, 65, 127, 128, 129] {
            let mut f = module.functions[0].clone();
            f.values.resize(width, Type::I64);
            let branch = Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            };
            f.blocks = vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: branch,
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![fixture::instruction(
                        Some(0),
                        InstructionKind::Load(LocalId(0)),
                    )],
                    terminator: Terminator::Jump(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![fixture::instruction(
                        None,
                        InstructionKind::StorageDead(LocalId(0)),
                    )],
                    terminator: Terminator::Jump(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(3),
                    instructions: vec![fixture::instruction(
                        None,
                        InstructionKind::TakeLocal { local: LocalId(0) },
                    )],
                    terminator: Terminator::Branch {
                        condition: ValueId((width - 1) as u32),
                        then_block: BlockId(1),
                        else_block: BlockId(4),
                    },
                },
                BasicBlock {
                    id: BlockId(4),
                    instructions: vec![],
                    terminator: Terminator::Return(Some(ValueId((width - 1) as u32))),
                },
                BasicBlock {
                    id: BlockId(5),
                    instructions: vec![],
                    terminator: Terminator::Jump(BlockId(4)),
                },
            ]
            .into();
            let outputs = compare(&module, &f);
            assert!(
                outputs[5].at(width - 1).unwrap(),
                "dead predecessor still receives live successor input"
            );
        }
    }
    #[test]
    fn zero_width_rows_and_snapshot_containers_still_consume_work() {
        let module = fixture::module();
        let mut f = module.functions[0].clone();
        f.values.clear();
        f.locals.clear();
        f.parameters.clear();
        f.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }]
        .into();
        // First test a truly empty CFG row.
        let mut b = budget();
        let before = b.remaining();
        let rows = liveness(&module, &f, &[vec![]], &[true], &mut b).unwrap();
        assert!(
            before - b.remaining() >= 2,
            "two outer matrix row records are prepaid"
        );
        let before = b.remaining();
        instruction_live(&module, &f, 0, &rows[0], &mut b).unwrap();
        assert!(before > b.remaining(), "zero-width live copy is prepaid");
        // An instruction without any operands also incurs its snapshot record.
        f.blocks[0].instructions.push(fixture::instruction(
            None,
            InstructionKind::MakeAggregate { fields: vec![] },
        ));
        let before = b.remaining();
        instruction_live(&module, &f, 0, &rows[0], &mut b).unwrap();
        assert!(
            before - b.remaining() >= 4,
            "live, placeholder, snapshot and transfer are prepaid"
        );
    }
}
