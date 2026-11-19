//! Definite initialization for actual CFG locals. A block retains incoming bits
//! for unmodified locals. Its final write to a local generates or clears that
//! bit. At a reachable join every reachable predecessor must initialize it.
//! The entry has exactly the actual parameters even when an edge loops to it.
use super::*;
use std::collections::VecDeque;

#[derive(Clone)]
pub(super) struct Row(Vec<u64>);

impl Row {
    fn zero(width: usize) -> Self {
        Self(vec![0; width.div_ceil(64)])
    }

    fn top(width: usize) -> Self {
        let mut row = Self(vec![u64::MAX; width.div_ceil(64)]);
        if !width.is_multiple_of(64) {
            *row.0.last_mut().unwrap() = (1 << (width % 64)) - 1;
        }
        row
    }

    pub(super) fn has(&self, local: LocalId) -> bool {
        let id = local.0 as usize;
        self.0[id / 64] & (1 << (id % 64)) != 0
    }

    fn write(&mut self, local: LocalId, initialized: bool) {
        let id = local.0 as usize;
        let mask = 1 << (id % 64);
        if initialized {
            self.0[id / 64] |= mask;
        } else {
            self.0[id / 64] &= !mask;
        }
    }

    pub(super) fn check_claim(
        &self,
        claim: &[LocalId],
        block: BlockId,
        boundary: &str,
        span: Span,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.0.len().saturating_add(claim.len()).saturating_add(1))?;
        let mismatch = |expected: Option<LocalId>, actual: Option<LocalId>| {
            reject(
                "E-VERIFY-INITIALIZATION-CLAIM",
                Some(span),
                format!(
                    "candidate initialization set differs from independently checked CFG facts at block {} {boundary}: expected next initialized local {expected:?}, candidate supplies {actual:?}",
                    block.0
                ),
            )
        };
        let mut claimed = claim.iter().copied();
        for (word, &bits) in self.0.iter().enumerate() {
            let mut pending = bits;
            while pending != 0 {
                let local = word * 64 + pending.trailing_zeros() as usize;
                let actual = claimed.next();
                let expected = Some(LocalId(local as u32));
                if actual != expected {
                    return Err(mismatch(expected, actual));
                }
                pending &= pending - 1;
            }
        }
        if let Some(extra) = claimed.next() {
            return Err(mismatch(None, Some(extra)));
        }
        Ok(())
    }

    pub(super) fn apply(&mut self, kind: &InstructionKind, b: &mut Budget) -> Check<()> {
        writes(kind, b, |local, initialized| self.write(local, initialized))
    }
}

// Every local operand was bounds checked before CFG reconstruction. Projecting
// or modifying a field leaves the whole-local initialization bit unchanged.
// The separate ownership proof handles partial moves and cleanup of its leaves.
fn writes(
    kind: &InstructionKind,
    b: &mut Budget,
    mut write: impl FnMut(LocalId, bool),
) -> Check<()> {
    use InstructionKind::*;
    match kind {
        Store { local, .. } | BindPackageCapture { local, .. } => write(*local, true),
        UnpackVariant { local, fields, .. } | UnpackTuple { local, fields } => {
            write(*local, false);
            b.spend(fields.len())?;
            for &field in fields {
                write(field, true);
            }
        }
        TakeLocal { local }
        | DropManaged { local }
        | DropClosure { local }
        | DropErrorUnion { local }
        | DropEnum { local }
        | DropTuple { local }
        | DropPackage { local }
        | DropOwnedArray { local }
        | EndPackageCapture { local, .. }
        | StorageLive(local)
        | StorageDead(local) => write(*local, false),
        MakeDynBorrow { .. }
        | MakeDynOwned { .. }
        | BorrowDynOwned { .. }
        | DropDynOwned { .. }
        | CallDynSlot { .. }
        | LoadStoredBorrow { .. }
        | StoreStoredBorrow { .. }
        | OwnedStoreNew { .. }
        | OwnedStoreReserve { .. }
        | OwnedStorePush { .. }
        | OwnedStorePop { .. }
        | OwnedStoreLen { .. }
        | OwnedStoreCapacity { .. }
        | OwnedStoreBorrow { .. }
        | OwnedStoreBorrowMut { .. }
        | ByteBufferNew
        | ByteBufferReserve { .. }
        | ByteBufferPush { .. }
        | ByteBufferTruncate { .. }
        | ByteBufferLen { .. }
        | ByteBufferCapacity { .. }
        | ByteBufferBorrow { .. }
        | ScopeEnter { .. }
        | ScopeExit { .. }
        | RegisterDefer { .. }
        | MakeErrorUnion { .. }
        | ErrorTag { .. }
        | ReadErrorUnion { .. }
        | TakeErrorUnion { .. }
        | MakeManaged { .. }
        | MakePackage { .. }
        | MakeOwnedArray { .. }
        | ProjectOwnedArrayField { .. }
        | TakeOwnedArrayElement { .. }
        | DropOwnedArrayElement { .. }
        | StoreOwnedArrayElement { .. }
        | BorrowOwnedArrayElement { .. }
        | BorrowPackageField { .. }
        | ReadPackageField { .. }
        | DescribePackageField { .. }
        | TakePackageField { .. }
        | DropPackageField { .. }
        | StorePackageField { .. }
        | DescribeLocal { .. }
        | DescribeAddress { .. }
        | ReadDesc { .. }
        | WriteDesc { .. }
        | StaticUtf8 { .. }
        | Constant(..)
        | ConvertInteger { .. }
        | ConvertNumber { .. }
        | Load(..)
        | BorrowLocal { .. }
        | BorrowLocalMut { .. }
        | ReborrowAddress { .. }
        | ReadAddress { .. }
        | WriteAddress { .. }
        | AddressField { .. }
        | AddressIndex { .. }
        | AddressSlice { .. }
        | SliceLength { .. }
        | Unary { .. }
        | Binary { .. }
        | Call { .. }
        | SuspendCall { .. }
        | UnsafeEnter { .. }
        | UnsafeExit { .. }
        | ForeignCall { .. }
        | WithForeignCallback { .. }
        | WithScopedThread { .. }
        | WithSyncCell { .. }
        | SyncI32 { .. }
        | SyncI32CompareExchange { .. }
        | SyncInteger { .. }
        | SyncIntegerCompareExchange { .. }
        | MutexCreate { .. }
        | MutexLock { .. }
        | MutexValue { .. }
        | MutexUnlock { .. }
        | DropMutexGuard { .. }
        | TaskStart { .. }
        | TaskCreate { .. }
        | TaskCreateOwned { .. }
        | TaskYield
        | TaskValid { .. }
        | TaskPoll { .. }
        | TaskRequestCancel { .. }
        | TaskPollClose { .. }
        | TaskFinishDestroy { .. }
        | RegistrationStart { .. }
        | RegistrationValid { .. }
        | RegistrationClose { .. }
        | AsyncRegistrationStart { .. }
        | AsyncRegistrationValid { .. }
        | AsyncRegistrationRequestClose { .. }
        | AsyncRegistrationPollClose { .. }
        | AsyncRegistrationFinishDestroy { .. }
        | AsyncRegistrationFinishDestroyChecked { .. }
        | FunctionRef { .. }
        | ForeignFunctionRef { .. }
        | CallIndirect { .. }
        | MakeAggregate { .. }
        | MakeVariant { .. }
        | VariantTag { .. }
        | ReadVariant { .. }
        | MakeOwnedTuple { .. }
        | TakeTupleField { .. }
        | TakeVariantField { .. }
        | BorrowPlaceLocal { .. }
        | PlaceTagLocal { .. }
        | BorrowPlaceAddress { .. }
        | PlaceTagAddress { .. }
        | ReservePlaceLocal { .. }
        | ReservePlaceAddress { .. }
        | CommitPlaceReplace { .. }
        | AbortPlaceReservation { .. }
        | BorrowFieldLocal { .. }
        | VariantTagLocal { .. }
        | ReadVariantLocal { .. }
        | BorrowVariantLocal { .. }
        | VariantTagAddress { .. }
        | BorrowVariantAddress { .. }
        | ExtractIndex { .. }
        | UpdateIndex { .. }
        | ExtractField { .. }
        | UpdateField { .. }
        | MakeClosure { .. }
        | ReadCapture { .. }
        | DescribeCapture { .. }
        | TakeCapture { .. }
        | BorrowCapture { .. }
        | CallClosureShared { .. }
        | CallClosureBorrowed { .. }
        | CallClosureOnce { .. }
        | WriteCapture { .. }
        | CallClosureMut { .. }
        | CallClosure { .. }
        | Unsupported(..) => {}
    }
    Ok(())
}

#[derive(Clone)]
struct Transfer {
    written: Row,
    initialized: Row,
}

impl Transfer {
    fn write(&mut self, local: LocalId, initialized: bool) {
        self.written.write(local, true);
        self.initialized.write(local, initialized);
    }

    fn apply(&self, input: &Row, output: &mut Row) -> bool {
        let mut changed = false;
        for (i, before) in input.0.iter().enumerate() {
            let after = (before & !self.written.0[i]) | self.initialized.0[i];
            changed |= output.0[i] != after;
            output.0[i] = after;
        }
        changed
    }
}

pub(super) fn reconstruct(
    function: &TypedFunction,
    predecessors: &[Vec<usize>],
    successors: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> Check<(Vec<Row>, Vec<Row>)> {
    let width = function.locals.len();
    let words = width.div_ceil(64);
    let n = function.blocks.len();
    if n == 1 {
        b.spend(
            words
                .saturating_mul(2)
                .saturating_add(function.parameters.len())
                .saturating_add(function.blocks[0].instructions.len())
                .saturating_add(2),
        )?;
        let mut input = Row::zero(width);
        for &local in &function.parameters {
            input.write(local, true);
        }
        let mut output = input.clone();
        for inst in &function.blocks[0].instructions {
            output.apply(&inst.kind, b)?;
        }
        return Ok((vec![input], vec![output]));
    }
    // Reserve for both transfer masks, the two boundary rows, the queue and
    // temporary row. Charge storage before allocating a block-by-word table.
    b.spend(
        n.saturating_mul(words.saturating_mul(4).saturating_add(6))
            .saturating_add(words.saturating_mul(4))
            .saturating_add(function.parameters.len()),
    )?;
    let entry = function.entry.0 as usize;
    let empty = Row::zero(width);
    let top = Row::top(width);
    let mut parameters = empty.clone();
    for &local in &function.parameters {
        parameters.write(local, true);
    }
    let mut transfers = Vec::with_capacity(n);
    let mut inputs = Vec::with_capacity(n);
    let mut outputs = Vec::with_capacity(n);
    for (id, block) in function.blocks.iter().enumerate() {
        let input = if id == entry {
            &parameters
        } else if reachable[id] {
            &top
        } else {
            &empty
        };
        let mut transfer = Transfer {
            written: empty.clone(),
            initialized: empty.clone(),
        };
        b.spend(block.instructions.len().saturating_add(words))?;
        for inst in &block.instructions {
            writes(&inst.kind, b, |local, initialized| {
                transfer.write(local, initialized)
            })?;
        }
        let mut output = empty.clone();
        transfer.apply(input, &mut output);
        transfers.push(transfer);
        inputs.push(input.clone());
        outputs.push(output);
    }

    let mut queued: Vec<bool> = (0..n).map(|id| reachable[id] && id != entry).collect();
    let mut queue: VecDeque<_> = (0..n).filter(|&id| queued[id]).collect();
    while let Some(id) = queue.pop_front() {
        queued[id] = false;
        b.spend(
            words
                .saturating_mul(2)
                .saturating_add(predecessors[id].len())
                .saturating_add(1),
        )?;
        let input = &mut inputs[id];
        input.0.copy_from_slice(&top.0);
        for &parent in &predecessors[id] {
            if reachable[parent] {
                b.spend(words)?;
                for (word, &previous) in input.0.iter_mut().zip(&outputs[parent].0) {
                    *word &= previous;
                }
            }
        }
        if transfers[id].apply(input, &mut outputs[id]) {
            b.spend(successors[id].len())?;
            for &child in &successors[id] {
                if child != entry && !queued[child] {
                    queue.push_back(child);
                    queued[child] = true;
                }
            }
        }
    }
    Ok((inputs, outputs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definite_local_rows_match_enumerated_paths_in_every_three_block_graph() {
        let (module, _) = crate::tests::baseline();
        let mut f = module.functions.into_iter().next().unwrap();
        let first = f.locals[0].clone();
        f.locals = (0..70)
            .map(|n| LocalDecl {
                id: LocalId(n),
                ..first.clone()
            })
            .collect();
        f.parameters = vec![LocalId(0), LocalId(64)];
        let assign = |id| InstructionKind::Store {
            local: LocalId(id),
            value: ValueId(0),
        };
        let operations = [
            vec![
                InstructionKind::UnpackTuple {
                    local: LocalId(0),
                    fields: vec![LocalId(64), LocalId(2)],
                },
                InstructionKind::TakeLocal { local: LocalId(64) },
            ],
            vec![
                assign(0),
                InstructionKind::StorageDead(LocalId(2)),
                assign(69),
            ],
            vec![],
        ];
        f.blocks = operations
            .into_iter()
            .enumerate()
            .map(|(id, ops)| BasicBlock {
                id: BlockId(id as u32),
                terminator: Terminator::Return(None),
                instructions: ops
                    .into_iter()
                    .map(|kind| Instruction {
                        result: None,
                        kind,
                        span: f.span,
                    })
                    .collect(),
            })
            .collect();
        // The oracle enumerates concrete path states for locals 0, 2, 64, 69.
        // It directly applies the source operations, then intersects observations.
        let after = |block, state| match block {
            0 => (state & !0b0101u8) | 0b0010,
            1 => (state & !0b0010u8) | 0b1001,
            _ => state,
        };
        for edges in 0..512 {
            let mut pred = vec![vec![]; 3];
            let mut succ = pred.clone();
            for (from, dests) in succ.iter_mut().enumerate() {
                for (to, preds) in pred.iter_mut().enumerate() {
                    if edges & (1 << (from * 3 + to)) != 0 {
                        dests.push(to);
                        preds.push(from);
                    }
                }
            }
            for entry in 0..3 {
                f.entry = BlockId(entry as u32);
                let mut seen = [[false; 16]; 3];
                let mut queue = vec![(entry, 0b0101u8)];
                while let Some((block, incoming)) = queue.pop() {
                    let input = if block == entry { 0b0101 } else { incoming };
                    if seen[block][input as usize] {
                        continue;
                    }
                    seen[block][input as usize] = true;
                    for &child in &succ[block] {
                        queue.push((child, after(block, input)));
                    }
                }
                let reachable = seen.map(|row| row.into_iter().any(|x| x));
                let mut b = Budget::new(&Limits::default());
                let (inputs, outputs) = reconstruct(&f, &pred, &succ, &reachable, &mut b).unwrap();
                for id in 0..3 {
                    let expected = if reachable[id] {
                        (0u8..16)
                            .filter(|&i| seen[id][i as usize])
                            .fold(15, |a, i| a & i)
                    } else {
                        0
                    };
                    for (row, state) in
                        [(&inputs[id], expected), (&outputs[id], after(id, expected))]
                    {
                        let claim: Vec<_> = [0, 2, 64, 69]
                            .into_iter()
                            .enumerate()
                            .filter(|(bit, _)| state & (1 << bit) != 0)
                            .map(|(_, i)| LocalId(i))
                            .collect();
                        assert!(
                            row.check_claim(&claim, BlockId(id as u32), "boundary", f.span, &mut b)
                                .is_ok(),
                            "edges={edges} entry={entry} block={id} state={state}"
                        );
                    }
                }
            }
        }
    }
}
