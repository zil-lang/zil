//! Private, exact may-initialization relation. Small relations keep the legacy
//! set; larger ones pack only observed destination words, never an ID-sized
//! matrix. No transitive edge, lifetime fact or Verify authority is inferred.
use super::*;
use std::collections::{btree_map, btree_set};

const PACK_AFTER: usize = 256;
type Edge = (LocalId, LocalId);
type Words = BTreeMap<(LocalId, u32), u64>;

#[derive(Clone, Debug)]
pub(super) enum Order {
    Small(BTreeSet<Edge>),
    Packed { count: usize, words: Words },
}
impl Default for Order {
    fn default() -> Self {
        Self::Small(BTreeSet::new())
    }
}
impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Small(a), Self::Small(b)) => a == b,
            (Self::Packed { words: a, .. }, Self::Packed { words: b, .. }) => a == b,
            _ => self.len() == other.len() && self.all().eq(other.all()),
        }
    }
}
impl Eq for Order {}

impl Order {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Small(s) => s.len(),
            Self::Packed { count, .. } => *count,
        }
    }
    pub(super) fn storage_cost(&self) -> usize {
        match self {
            Self::Small(s) => s.len(),
            Self::Packed { words, .. } => words.len().saturating_mul(2),
        }
    }
    fn pack(&mut self, budget: &mut Budget) -> CompileResult<()> {
        let Self::Small(edges) = self else {
            return Ok(());
        };
        // Pay source visits and all worst-case tree insertions before allocating.
        let depth = usize::BITS as usize - edges.len().leading_zeros() as usize;
        budget.spend(
            edges.len().saturating_mul(depth.saturating_add(3)),
            "resource order packing",
        )?;
        let count = edges.len();
        let mut words = Words::new();
        for &(a, b) in edges.iter() {
            *words.entry((a, b.0 / 64)).or_default() |= 1u64 << (b.0 % 64);
        }
        *self = Self::Packed { count, words };
        Ok(())
    }
    pub(super) fn insert(
        &mut self,
        edge: Edge,
        budget: &mut Budget,
        legacy_lookup: bool,
        context: &'static str,
    ) -> CompileResult<()> {
        match self {
            Self::Small(edges) => {
                if legacy_lookup {
                    budget.lookup(edges.len(), context)?;
                }
                edges.insert(edge);
                if edges.len() > PACK_AFTER {
                    self.pack(budget)?;
                }
            }
            Self::Packed { count, words } => {
                budget.lookup(words.len(), context)?;
                budget.spend(1, "resource order bit insertion")?;
                let bit = 1u64 << (edge.1.0 % 64);
                let word = words.entry((edge.0, edge.1.0 / 64)).or_default();
                *count += usize::from(*word & bit == 0);
                *word |= bit;
            }
        }
        Ok(())
    }
    pub(super) fn join(&mut self, source: &Self, budget: &mut Budget) -> CompileResult<bool> {
        let before = self.len();
        if let (Self::Small(to), Self::Small(from)) = (&mut *self, source) {
            budget.spend(
                to.len().saturating_add(from.len()),
                "resource initialization order join",
            )?;
            to.extend(from);
            if to.len() > PACK_AFTER {
                self.pack(budget)?;
            }
        } else {
            self.pack(budget)?;
            if let Self::Packed { words: from, .. } = source {
                budget.spend(from.len(), "resource order word join scan")?;
                let Self::Packed { count, words: to } = self else {
                    unreachable!()
                };
                for (&key, &bits) in from {
                    budget.lookup(to.len(), "resource order word join lookup")?;
                    let word = to.entry(key).or_default();
                    *count += (bits & !*word).count_ones() as usize;
                    *word |= bits;
                }
            } else {
                budget.spend(source.len(), "resource order sparse join scan")?;
                for edge in source.all() {
                    self.insert(edge, budget, true, "resource order sparse join insertion")?;
                }
            }
        }
        Ok(self.len() != before)
    }
    pub(super) fn forget(&mut self, local: LocalId, budget: &mut Budget) -> CompileResult<()> {
        match self {
            Self::Small(edges) => {
                budget.spend(edges.len(), "resource order removal")?;
                edges.retain(|&(a, b)| a != local && b != local);
            }
            Self::Packed { count, words } => {
                budget.spend(words.len().saturating_mul(2), "resource order word removal")?;
                let column = local.0 / 64;
                let mask = !(1u64 << (local.0 % 64));
                words.retain(|&(row, col), bits| {
                    let old = *bits;
                    if row == local {
                        *bits = 0;
                    } else if col == column {
                        *bits &= mask;
                    }
                    *count -= (old ^ *bits).count_ones() as usize;
                    *bits != 0
                });
            }
        }
        Ok(())
    }
    fn all(&self) -> Edges<'_> {
        match self {
            Self::Small(s) => Edges::Small(s.iter()),
            Self::Packed { words, .. } => Edges::Packed(words.range(..), None),
        }
    }
    pub(super) fn cleanup_edges(
        &self,
        local: LocalId,
        budget: &mut Budget,
    ) -> CompileResult<Edges<'_>> {
        match self {
            Self::Small(s) => Ok(Edges::Small(s.iter())),
            Self::Packed { words, .. } => {
                // A conservative prepaid bound for range traversal; every actual
                // successor is separately charged by the current cleanup proof.
                budget.lookup(words.len(), "resource order successor range")?;
                budget.spend(words.len(), "resource order successor words")?;
                Ok(Edges::Packed(
                    words.range((local, 0)..=(local, u32::MAX)),
                    None,
                ))
            }
        }
    }
}

pub(super) enum Edges<'a> {
    Small(btree_set::Iter<'a, Edge>),
    Packed(
        btree_map::Range<'a, (LocalId, u32), u64>,
        Option<(LocalId, u32, u64)>,
    ),
}
impl Iterator for Edges<'_> {
    type Item = Edge;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Small(s) => s.next().copied(),
            Self::Packed(words, current) => loop {
                if let Some((row, col, bits)) = current.as_mut()
                    && *bits != 0
                {
                    let bit = bits.trailing_zeros();
                    *bits &= *bits - 1;
                    return Some((*row, LocalId((*col << 6) + bit)));
                }
                let (&(row, col), &bits) = words.next()?;
                *current = Some((row, col, bits));
            },
        }
    }
}

#[cfg(test)]
mod tests;
