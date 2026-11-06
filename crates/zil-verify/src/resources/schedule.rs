//! Kernel-private CFG scheduling. Ranks change when a ready state is visited,
//! never the transfer, merge, successor set or eventual fixed point.
use super::*;
use std::{cmp::Reverse, collections::BinaryHeap};

pub(crate) enum Worklist {
    Legacy(VecDeque<usize>),
    Ranked {
        ranks: Vec<usize>,
        ready: BinaryHeap<Reverse<(usize, usize)>>,
    },
}

impl Worklist {
    pub(crate) fn new(
        graph: &FunctionGraph,
        entry: usize,
        ranked: bool,
        budget: &mut Budget,
    ) -> Check<Self> {
        if !ranked {
            return Ok(Self::Legacy(VecDeque::from([entry])));
        }
        let n = graph.successors.len();
        // Prepay the row-length scan before counting edges, and then every
        // rank/visited/order/DFS slot and edge before allocation or traversal.
        budget.spend(n.saturating_add(1))?;
        let edges = graph
            .successors
            .iter()
            .fold(0usize, |sum, row| sum.saturating_add(row.len()));
        budget.spend(
            n.saturating_mul(9)
                .saturating_add(edges.saturating_mul(4))
                .saturating_add(1),
        )?;
        let mut seen = vec![false; n];
        let mut ranks = vec![usize::MAX; n];
        let mut order = Vec::with_capacity(n);
        let mut stack = Vec::with_capacity(n.saturating_add(edges).saturating_add(1));
        stack.push((entry, false));
        while let Some((block, finish)) = stack.pop() {
            if finish {
                order.push(block);
                continue;
            }
            if seen[block] {
                continue;
            }
            seen[block] = true;
            stack.push((block, true));
            for &next in graph.successors[block].iter().rev() {
                if !seen[next] {
                    stack.push((next, false));
                }
            }
        }
        for (rank, block) in order.into_iter().rev().enumerate() {
            ranks[block] = rank;
        }
        let mut work = Self::Ranked {
            ranks,
            ready: BinaryHeap::with_capacity(n),
        };
        work.push(entry, budget)?;
        Ok(work)
    }

    pub(crate) fn push(&mut self, block: usize, budget: &mut Budget) -> Check<()> {
        match self {
            Self::Legacy(ready) => ready.push_back(block),
            Self::Ranked { ranks, ready } => {
                budget.spend(
                    ready.len().saturating_add(1).checked_ilog2().unwrap_or(0) as usize + 2,
                )?;
                ready.push(Reverse((ranks[block], block)));
            }
        }
        Ok(())
    }

    pub(crate) fn pop(&mut self, budget: &mut Budget) -> Check<Option<usize>> {
        Ok(match self {
            Self::Legacy(ready) => ready.pop_front(),
            Self::Ranked { ready, .. } => {
                budget.spend(
                    ready.len().saturating_add(1).checked_ilog2().unwrap_or(0) as usize + 2,
                )?;
                ready.pop().map(|Reverse((_, block))| block)
            }
        })
    }
}
