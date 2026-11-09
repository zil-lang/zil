//! Verifier-local bit coordinates for original SSA values and locals.
//! No analyzer structure, origin, loan or permission conclusion is imported.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WordRow {
    width: usize,
    words: Vec<u64>,
}
impl WordRow {
    fn count(width: usize) -> usize {
        width / 64 + usize::from(!width.is_multiple_of(64))
    }
    pub(super) fn work_for(width: usize) -> usize {
        Self::count(width) + 1
    }
    pub(super) fn empty(width: usize) -> Self {
        Self {
            width,
            words: vec![0; Self::count(width)],
        }
    }
    pub(super) fn work(&self) -> usize {
        self.words.len() + 1
    }
    pub(super) fn len(&self) -> usize {
        self.width
    }
    pub(super) fn is_live(&self, position: usize) -> Check<bool> {
        if position >= self.width {
            return Err(err("liveness use exceeds actual SSA/local width"));
        }
        Ok(self.words[position / 64] & (1u64 << (position % 64)) != 0)
    }
    pub(super) fn assign(&mut self, position: usize, live: bool) -> Check<()> {
        if position >= self.width {
            return Err(err("liveness definition exceeds actual SSA/local width"));
        }
        let bit = 1u64 << (position % 64);
        if live {
            self.words[position / 64] |= bit;
        } else {
            self.words[position / 64] &= !bit;
        }
        Ok(())
    }
    pub(super) fn union(&mut self, source: &Self, b: &mut Budget) -> Check<()> {
        if self.width != source.width {
            return Err(err("liveness rows have different actual widths"));
        }
        b.spend(self.work())?;
        for (left, right) in self.words.iter_mut().zip(&source.words) {
            *left |= *right;
        }
        Ok(())
    }
    pub(super) fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(|(word, &bits)| {
                let mut pending = bits;
                std::iter::from_fn(move || {
                    if pending == 0 {
                        return None;
                    }
                    let bit = pending.trailing_zeros() as usize;
                    pending &= pending - 1;
                    Some(word * 64 + bit)
                })
            })
            .take_while(move |&id| id < self.width)
    }
    #[cfg(test)]
    pub(super) fn from_bools(flags: &[bool]) -> Self {
        let mut row = Self::empty(flags.len());
        for (i, value) in flags.iter().enumerate() {
            row.assign(i, *value).unwrap();
        }
        row
    }
    #[cfg(test)]
    fn bools(&self) -> Vec<bool> {
        (0..self.width).map(|i| self.is_live(i).unwrap()).collect()
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
    fn graph(f: &TypedFunction) -> FunctionGraph {
        let mut successors = vec![Vec::new(); f.blocks.len()];
        let mut predecessors = successors.clone();
        for (id, block) in f.blocks.iter().enumerate() {
            successors[id] = match block.terminator {
                Terminator::Jump(to) => vec![to.0 as usize],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => vec![then_block.0 as usize, else_block.0 as usize],
                _ => vec![],
            };
            for &to in &successors[id] {
                predecessors[to].push(id);
            }
        }
        let mut reachable = vec![false; f.blocks.len()];
        let mut pending = vec![f.entry.0 as usize];
        while let Some(id) = pending.pop() {
            if !reachable[id] {
                reachable[id] = true;
                pending.extend(&successors[id]);
            }
        }
        FunctionGraph {
            successors,
            predecessors,
            reachable,
            package_views: packages::CaptureViews::default(),
            role_data_uses: vec![],
        }
    }
    fn compare(module: &TypedModule, f: &TypedFunction) -> Vec<WordRow> {
        let graph = graph(f);
        let expected = live_out_bool(module, f, &graph, &mut budget()).unwrap();
        let actual = live_out(module, f, &graph, &mut budget()).unwrap();
        assert_eq!(
            actual.iter().map(WordRow::bools).collect::<Vec<_>>(),
            expected,
            "full outputs"
        );
        let cached = WindowRows::reconstruct(module, f, &graph, &mut budget()).unwrap();
        for (bi, block) in f.blocks.iter().enumerate() {
            let rows = instruction_rows(module, f, bi, &actual[bi], &mut budget()).unwrap();
            if graph.reachable[bi] {
                assert_eq!(cached.blocks[bi], rows, "cached/noncached after");
            } else {
                assert!(cached.blocks[bi].is_empty());
            }
            let mut old = expected[bi].clone();
            let mut words = actual[bi].clone();
            term_live_bool(&block.terminator, &mut old);
            term_live(&block.terminator, &mut words).unwrap();
            assert_eq!(words.bools(), old, "terminal B{bi}");
            for (position, instruction) in block.instructions.iter().enumerate().rev() {
                assert_eq!(
                    rows[position].bools(),
                    old,
                    "complete after B{bi}:{position}"
                );
                reverse_bool(module, f, instruction, &mut old, &mut budget()).unwrap();
                reverse(module, f, instruction, &mut words, &mut budget()).unwrap();
                assert_eq!(words.bools(), old, "transfer/input B{bi}:{position}");
            }
        }
        actual
    }
    #[test]
    fn stored_words_match_boolean_boundaries_and_padding() {
        for width in [0, 1, 63, 64, 65, 127, 128, 129] {
            let mut old = vec![false; width];
            let mut row = WordRow::empty(width);
            for index in 0..width {
                old[index] = index % 3 != 0;
                row.assign(index, old[index]).unwrap();
                assert_eq!(row.bools(), old);
            }
            assert!(row.is_live(width).is_err());
            assert!(row.assign(width, true).is_err());
            let from = WordRow::from_bools(&(0..width).map(|i| i % 5 == 0).collect::<Vec<_>>());
            for (to, source) in old.iter_mut().zip(from.bools()) {
                *to |= source;
            }
            row.union(&from, &mut budget()).unwrap();
            assert_eq!(row.bools(), old);
            row.union(&from, &mut budget()).unwrap();
            assert_eq!(row.bools(), old);
            let before = row.clone();
            assert!(
                row.union(&WordRow::empty(width + 1), &mut budget())
                    .is_err()
            );
            assert_eq!(row, before);
            if !width.is_multiple_of(64) {
                assert_eq!(row.words.last().unwrap() >> (width % 64), 0);
            }
            for index in 0..width {
                row.assign(index, false).unwrap();
            }
            assert!(row.words.iter().all(|word| *word == 0));
        }
    }
    #[test]
    fn stored_word_cfg_and_snapshots_equal_original_verifier_boolean_solver() {
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
        // Static oracle only: preserve V's successor union, dirty queue and
        // subsequent processing of dead predecessors; 63 SSA bits plus a
        // local also tests the same physical word across both namespaces.
        for width in [1, 63, 64, 65, 127, 128, 129] {
            let mut f = module.functions[0].clone();
            f.values.resize(width, Type::I64);
            f.blocks = vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
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
            ];
            let outputs = compare(&module, &f);
            assert!(
                outputs[5].is_live(width - 1).unwrap(),
                "dirty queue still processes a dead predecessor"
            );
        }
    }
    #[test]
    fn stored_zero_width_rows_and_snapshot_containers_are_prepaid() {
        let module = fixture::module();
        let mut f = module.functions[0].clone();
        f.values.clear();
        f.locals.clear();
        f.parameters.clear();
        f.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }];
        let mut b = budget();
        let before = b.remaining;
        let outputs = live_out(&module, &f, &graph(&f), &mut b).unwrap();
        assert!(before - b.remaining >= 2, "matrix row records stay paid");
        let before = b.remaining;
        instruction_rows(&module, &f, 0, &outputs[0], &mut b).unwrap();
        assert!(before > b.remaining, "empty live copy stays paid");
        f.blocks[0].instructions.push(fixture::instruction(
            None,
            InstructionKind::MakeAggregate { fields: vec![] },
        ));
        let before = b.remaining;
        instruction_rows(&module, &f, 0, &outputs[0], &mut b).unwrap();
        assert!(
            before - b.remaining >= 4,
            "live, placeholder, snapshot and transfer stay paid"
        );
    }
}
