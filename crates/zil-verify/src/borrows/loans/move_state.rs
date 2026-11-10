//! Private move-state storage: every bit retains its actual SSA ordinal.
//! Tiny rows keep the original bool operations; wide rows pay for real words.
use super::*;

// One u64 holds eight original bool payload bytes; do not widen tiny states.
const SMALL: usize = std::mem::size_of::<u64>();
const WORD: usize = u64::BITS as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Dense(Vec<bool>),
    Words { width: usize, words: Vec<u64> },
}

fn absent() -> zil_base::Diagnostic {
    reject(
        "E-VERIFY-MOVE",
        None,
        "move state has an inconsistent SSA inventory",
    )
}

impl Row {
    fn header() -> usize {
        std::mem::size_of::<Self>().div_ceil(std::mem::size_of::<usize>())
    }
    fn dense_header_delta() -> usize {
        Self::header()
            .saturating_sub(std::mem::size_of::<Vec<bool>>().div_ceil(std::mem::size_of::<usize>()))
    }
    fn units(width: usize) -> usize {
        width.div_ceil(WORD)
    }
    fn zero(width: usize) -> Self {
        if width <= SMALL {
            Self::Dense(vec![false; width])
        } else {
            Self::Words {
                width,
                words: vec![0; Self::units(width)],
            }
        }
    }
    pub(super) fn tables(
        count: usize,
        width: usize,
        b: &mut Budget,
    ) -> Check<(Vec<Self>, Vec<Self>)> {
        // Retain the old logical-overflow rejection before selecting storage.
        count
            .checked_mul(width)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| limit("borrow state size overflow"))?;
        if width <= SMALL {
            reserve(b, &[count, width, 2])?;
            // Preserve the old bool payload work, then fund the actual larger
            // Row slot, construction selection and its eventual enum drop.
            let added = Self::dense_header_delta()
                .checked_add(2)
                .and_then(|n| n.checked_mul(count))
                .and_then(|n| n.checked_mul(2))
                .ok_or_else(|| limit("borrow state size overflow"))?;
            b.spend(added)?;
        } else {
            // Each row's zeroed words, logical width, slot write and allocation /
            // eventual release headers; both outer containers are also prepaid.
            let outer = std::mem::size_of::<Vec<Self>>().div_ceil(std::mem::size_of::<usize>());
            let cost = Self::units(width)
                .checked_add(Self::header().saturating_add(4))
                .and_then(|n| n.checked_mul(count))
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(outer.saturating_add(2).saturating_mul(2)))
                .ok_or_else(|| limit("borrow state size overflow"))?;
            b.spend(cost)?;
        }
        Ok((
            (0..count).map(|_| Self::zero(width)).collect(),
            (0..count).map(|_| Self::zero(width)).collect(),
        ))
    }
    pub(super) fn new(width: usize, b: &mut Budget) -> Check<Self> {
        if width <= SMALL {
            b.spend(
                width
                    .saturating_add(Self::dense_header_delta())
                    .saturating_add(2),
            )?;
            Ok(Self::Dense(vec![false; width]))
        } else {
            let count = Self::units(width);
            b.spend(count.saturating_add(Self::header()).saturating_add(4))?;
            Ok(Self::Words {
                width,
                words: vec![0; count],
            })
        }
    }
    pub(super) fn copy(&self, b: &mut Budget) -> Check<Self> {
        // One enum selection and its eventual drop, beyond payload/header work.
        b.spend(2)?;
        Ok(match self {
            Self::Dense(row) => {
                b.spend(
                    row.len()
                        .saturating_add(Self::dense_header_delta().saturating_mul(2)),
                )?;
                Self::Dense(row.clone())
            }
            // Read/write every actual word; prepay the new row's width and
            // allocation/release headers before cloning its payload.
            Self::Words { width, words } => {
                b.spend(
                    words
                        .len()
                        .saturating_add(Self::header())
                        .saturating_mul(2)
                        .saturating_add(2),
                )?;
                Self::Words {
                    width: *width,
                    words: words.clone(),
                }
            }
        })
    }
    pub(super) fn copy_from(&mut self, source: &Self, b: &mut Budget) -> Check<()> {
        b.spend(2)?; // both actual enum selections, including the dense path
        match (self, source) {
            (Self::Dense(to), Self::Dense(from)) => to.clone_from(from),
            (
                Self::Words {
                    width: a,
                    words: to,
                },
                Self::Words {
                    width: z,
                    words: from,
                },
            ) => {
                b.spend(to.len().saturating_mul(2).saturating_add(4))?;
                if *a != *z || to.len() != from.len() {
                    return Err(absent());
                }
                to.clone_from_slice(from);
            }
            _ => {
                return Err(absent());
            }
        }
        Ok(())
    }
    pub(super) fn same(&self, source: &Self, b: &mut Budget) -> Check<bool> {
        b.spend(2)?;
        Ok(match (self, source) {
            (Self::Dense(to), Self::Dense(from)) => to == from,
            (
                Self::Words {
                    width: a,
                    words: to,
                },
                Self::Words {
                    width: z,
                    words: from,
                },
            ) => {
                b.spend(to.len().saturating_mul(3).saturating_add(4))?;
                if *a != *z || to.len() != from.len() {
                    return Err(absent());
                }
                to == from
            }
            _ => {
                return Err(absent());
            }
        })
    }
    pub(super) fn get(&self, index: usize, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        match self {
            Self::Dense(row) => row.get(index).copied().ok_or_else(absent),
            Self::Words { width, words } => {
                // Logical bound, word coordinate, mask, word read and bit test.
                b.spend(5)?;
                if index >= *width {
                    return Err(absent());
                }
                Ok(words[index / WORD] & (1u64 << (index % WORD)) != 0)
            }
        }
    }
    pub(super) fn set(&mut self, index: usize, value: bool, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        match self {
            Self::Dense(row) => *row.get_mut(index).ok_or_else(absent)? = value,
            Self::Words { width, words } => {
                // All payment and bounds checks precede the read/modify/write.
                b.spend(7)?;
                if index >= *width {
                    return Err(absent());
                }
                let mask = 1u64 << (index % WORD);
                let word = &mut words[index / WORD];
                if value {
                    *word |= mask;
                } else {
                    *word &= !mask;
                }
            }
        }
        Ok(())
    }
    pub(super) fn join(&mut self, source: &Self, b: &mut Budget) -> Check<bool> {
        b.spend(2)?;
        let mut changed = false;
        match (self, source) {
            (Self::Dense(to), Self::Dense(from)) => {
                b.spend(to.len())?;
                for (a, z) in to.iter_mut().zip(from) {
                    if *z && !*a {
                        *a = true;
                        changed = true;
                    }
                }
            }
            (
                Self::Words {
                    width: a,
                    words: to,
                },
                Self::Words {
                    width: z,
                    words: from,
                },
            ) => {
                // Complete width/shape, then two reads, OR, comparison and write
                // for each actual word. No partial publication on unpaid work.
                b.spend(to.len().saturating_mul(5).saturating_add(4))?;
                if *a != *z || to.len() != from.len() {
                    return Err(absent());
                }
                for (a, z) in to.iter_mut().zip(from) {
                    let merged = *a | *z;
                    changed |= merged != *a;
                    *a = merged;
                }
            }
            _ => {
                return Err(absent());
            }
        }
        Ok(changed)
    }
    #[cfg(test)]
    fn logical(&self) -> Vec<bool> {
        match self {
            Self::Dense(row) => row.clone(),
            Self::Words { width, words } => (0..*width)
                .map(|i| words[i / WORD] & (1u64 << (i % WORD)) != 0)
                .collect(),
        }
    }
}

#[cfg(test)]
type Snapshot = (Vec<Vec<bool>>, Vec<Vec<bool>>);
#[cfg(test)]
thread_local! {
    static TRACE: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) };
}
#[cfg(test)]
pub(super) fn record(inputs: &[Row], outputs: &[Row]) {
    TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push((
                inputs.iter().map(Row::logical).collect(),
                outputs.iter().map(Row::logical).collect(),
            ));
        }
    });
}

#[cfg(test)]
mod tests;
