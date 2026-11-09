//! Word snapshots for large description histories, never a different live domain.
use super::*;
const WORD: usize = usize::BITS as usize;
// At most one machine word of bool bytes: packing does not reduce the payload
// allocation, but would add shifts/masks to reverse and every consumer read.
const SMALL: usize = std::mem::size_of::<usize>();
pub(super) enum History {
    Dense(Vec<Vec<Vec<bool>>>),
    Packed {
        len: usize,
        blocks: Vec<Vec<Vec<usize>>>,
    },
}
#[derive(Clone, Copy)]
pub(super) enum Row<'a> {
    Dense(&'a [bool]),
    Packed { len: usize, words: &'a [usize] },
}
impl History {
    pub(super) fn row(&self, block: usize, instruction: usize) -> Row<'_> {
        match self {
            Self::Dense(rows) => Row::Dense(&rows[block][instruction]),
            Self::Packed { len, blocks } => Row::Packed {
                len: *len,
                words: &blocks[block][instruction],
            },
        }
    }
}
impl Row<'_> {
    pub(super) fn len(self) -> usize {
        match self {
            Self::Dense(row) => row.len(),
            Self::Packed { len, .. } => len,
        }
    }
    pub(super) fn get(self, index: usize, b: &mut Budget) -> Check<bool> {
        match self {
            // Existing consumer loops already pay their ordinal visit. Keep
            // their unchanged direct bool read and diagnostics on the tiny path.
            Self::Dense(row) => Ok(row[index]),
            Self::Packed { len, words } => {
                // Logical bound, word index/read, shift and mask. Private rows
                // are constructed at exactly ceil(len/WORD), with zero tail.
                b.spend(4)?;
                if index >= len {
                    return Err(reject(
                        "E-VERIFY-RESOURCE",
                        None,
                        "description history slot absent",
                    ));
                }
                Ok(words[index / WORD] & (1usize << (index % WORD)) != 0)
            }
        }
    }
}
fn set(words: &mut [usize], len: usize, index: usize, value: bool, b: &mut Budget) -> Check<()> {
    // Logical bound, checked word coordinate, mask, read, bit operation, write;
    // all payment and bounds checks precede mutation of this private scratch.
    b.spend(6)?;
    if index >= len {
        return Err(reject(
            "E-VERIFY-RESOURCE",
            None,
            "description history slot absent",
        ));
    }
    let at = &mut words[index / WORD];
    let mask = 1usize << (index % WORD);
    if value {
        *at |= mask;
    } else {
        *at &= !mask;
    }
    Ok(())
}
fn reverse_words(
    slots: &Slots,
    live: &mut [usize],
    instruction: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?; // actual instruction dispatch, including no-operand instructions
    if let Some(value) = instruction.result {
        b.spend(1)?; // actual SSA -> description coordinate read
        if let Some(index) = slots.desc_value[value.0 as usize] {
            set(live, slots.desc_count, index, false, b)?;
        }
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            b.spend(1)?;
            if let Some(index) = slots.desc_local[local.0 as usize] {
                set(live, slots.desc_count, index, false, b)?;
            }
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => {
            b.spend(1)?;
            if let Some(index) = slots.desc_local[local.0 as usize] {
                set(live, slots.desc_count, index, true, b)?;
            }
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        b.spend(1)?; // operand visit and actual coordinate read
        if let Some(index) = slots.desc_value[operand.0 as usize] {
            set(live, slots.desc_count, index, true, b)?;
        }
    }
    Ok(())
}
pub(super) fn history(
    f: &TypedFunction,
    slots: &Slots,
    output: &[Vec<bool>],
    budget: &mut Budget,
) -> Check<History> {
    if slots.desc_count <= SMALL {
        return dense(f, slots, output, budget);
    }
    let len = slots.desc_count;
    let count = len.div_ceil(WORD);
    // Outer container capacity/writes and allocation/release headers.
    budget.spend(f.blocks.len().saturating_mul(2).saturating_add(2))?;
    let mut blocks = Vec::with_capacity(f.blocks.len());
    for (block_index, block) in f.blocks.iter().enumerate() {
        // One actual output-row index, zeroed scratch words and Vec headers.
        budget.spend(count.saturating_add(3))?;
        let out = &output[block_index];
        let mut live = vec![0usize; count];
        for (index, &active) in out.iter().enumerate() {
            budget.spend(1)?; // original bool exit row read, even when inactive
            if active {
                set(&mut live, len, index, true, budget)?;
            }
        }
        // History row capacity and allocation/release headers; each clone below
        // pays its word payload, own Vec headers and history-slot write.
        // No empty per-instruction Vecs.
        budget.spend(block.instructions.len().saturating_add(2))?;
        let mut rows = Vec::with_capacity(block.instructions.len());
        for instruction in block.instructions.iter().rev() {
            budget.spend(count.saturating_add(3))?;
            rows.push(live.clone());
            reverse_words(slots, &mut live, instruction, budget)?;
        }
        // Reverse the rows into original instruction order. One saved header read
        // and two header writes per pair, including all-zero payload rows.
        budget.spend((rows.len() / 2).saturating_mul(3))?;
        rows.reverse();
        blocks.push(rows);
    }
    Ok(History::Packed { len, blocks })
}
fn dense(
    f: &TypedFunction,
    slots: &Slots,
    output: &[Vec<bool>],
    budget: &mut Budget,
) -> Check<History> {
    let mut after = Vec::with_capacity(f.blocks.len());
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(
            block
                .instructions
                .len()
                .checked_mul(slots.desc_count)
                .ok_or_else(|| limit("resource liveness history overflow"))?,
        )?;
        let mut rows = vec![Vec::new(); block.instructions.len()];
        let mut live = output[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            rows[i] = live.clone();
            reverse(slots, &mut live, instruction);
        }
        after.push(rows);
    }
    Ok(History::Dense(after))
}

// One immutable actual function/selected graph for this resource-check request.
// Slots themselves are always rebuilt; only an exactly matching description
// coordinate inventory can reuse this permission-free after table.
pub(super) struct Binding<'a> {
    f: &'a TypedFunction,
    graph: &'a FunctionGraph,
    entry: Option<Entry>,
}
struct Entry {
    count: usize,
    local: Vec<Option<usize>>,
    value: Vec<Option<usize>>,
    history: History,
}
pub(super) enum After<'a> {
    Owned(History),
    Borrowed(&'a History),
}
impl After<'_> {
    pub(super) fn history(&self) -> &History {
        match self {
            Self::Owned(history) => history,
            Self::Borrowed(history) => history,
        }
    }
}
impl<'a> Binding<'a> {
    pub(super) fn new(f: &'a TypedFunction, graph: &'a FunctionGraph) -> Self {
        Self {
            f,
            graph,
            entry: None,
        }
    }
    pub(super) fn function(&self) -> &'a TypedFunction {
        self.f
    }
    pub(super) fn graph(&self) -> &'a FunctionGraph {
        self.graph
    }
    pub(super) fn after(&mut self, slots: &Slots, b: &mut Budget) -> Check<After<'_>> {
        b.spend(1)?; // actual small-domain dispatch; no hidden per-call work
        if slots.desc_count <= SMALL {
            return Ok(After::Owned(liveness(self.f, self.graph, slots, b)?));
        }
        b.spend(1)?; // entry presence
        let matches = if let Some(entry) = &self.entry {
            b.spend(3)?; // count and both complete vector lengths
            if entry.count != slots.desc_count
                || entry.local.len() != slots.desc_local.len()
                || entry.value.len() != slots.desc_value.len()
            {
                false
            } else {
                let mut matches = true;
                for (old, new) in entry
                    .local
                    .iter()
                    .zip(&slots.desc_local)
                    .chain(entry.value.iter().zip(&slots.desc_value))
                {
                    b.spend(3)?; // two Option discriminants and comparison
                    match (old, new) {
                        (Some(old), Some(new)) => {
                            b.spend(3)?; // two actual ordinal reads and comparison
                            matches &= old == new;
                        }
                        (None, None) => {}
                        _ => matches = false,
                    }
                }
                matches
            }
        } else {
            false
        };
        b.spend(1)?; // actual entry/history borrow, before any publication
        if !matches {
            // Full original solve/build remains at the original after position.
            // Publication and all key allocations follow complete success.
            let history = liveness(self.f, self.graph, slots, b)?;
            let words = std::mem::size_of::<Option<usize>>().div_ceil(std::mem::size_of::<usize>());
            b.spend(
                slots
                    .desc_local
                    .len()
                    .saturating_add(slots.desc_value.len())
                    .saturating_mul(words.saturating_mul(2))
                    .saturating_add(10),
            )?;
            // Copy each primitive key word, two Vec allocation/release headers,
            // record construction/publication and old-record release. History
            // is uniquely moved; its original build already funds its release.
            let entry = Entry {
                count: slots.desc_count,
                local: slots.desc_local.clone(),
                value: slots.desc_value.clone(),
                history,
            };
            self.entry = Some(entry);
        }
        Ok(After::Borrowed(&self.entry.as_ref().unwrap().history))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reuse_tests;
