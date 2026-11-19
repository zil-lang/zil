//! Dominance comes only from the actual CFG. Numbering the immediate-dominator
//! tree gives constant-time queries on SSA uses without materializing a square
//! matrix. An unreachable block dominates only itself.
use super::*;

pub(super) struct Dominance {
    intervals: Vec<Option<(usize, usize)>>,
}

impl Dominance {
    pub(super) fn dominates(&self, ancestor: usize, block: usize) -> bool {
        if ancestor == block {
            return true;
        }
        match (self.intervals[ancestor], self.intervals[block]) {
            (Some((start, end)), Some((point, _))) => start <= point && point < end,
            _ => false,
        }
    }

    pub(super) fn build(
        entry: usize,
        successors: &[Vec<usize>],
        predecessors: &[Vec<usize>],
        b: &mut Budget,
    ) -> Check<Self> {
        let n = successors.len();
        if n == 1 {
            b.spend(1)?;
            return Ok(Self {
                intervals: Vec::new(),
            });
        }
        b.spend(n.saturating_mul(9).saturating_add(1))?;
        let mut visited = vec![false; n];
        let mut order = Vec::with_capacity(n);
        let mut dfs = Vec::with_capacity(n);
        visited[entry] = true;
        dfs.push((entry, 0usize));
        while let Some((block, next)) = dfs.last_mut() {
            b.spend(2)?;
            if let Some(&child) = successors[*block].get(*next) {
                *next += 1;
                if !visited[child] {
                    visited[child] = true;
                    dfs.push((child, 0));
                }
            } else {
                order.push(*block);
                dfs.pop();
            }
        }
        order.reverse();
        b.spend(order.len().saturating_mul(2))?;
        let mut rank = vec![usize::MAX; n];
        for (r, &block) in order.iter().enumerate() {
            rank[block] = r;
        }

        let mut parent = vec![None; n];
        parent[entry] = Some(entry);
        loop {
            let mut changed = false;
            for &block in &order[1..] {
                b.spend(predecessors[block].len().saturating_add(1))?;
                let mut incoming = predecessors[block]
                    .iter()
                    .copied()
                    .filter(|&p| parent[p].is_some());
                let Some(mut common) = incoming.next() else {
                    continue;
                };
                for mut other in incoming {
                    while common != other {
                        b.spend(2)?;
                        if rank[common] > rank[other] {
                            common = parent[common].unwrap();
                        } else {
                            other = parent[other].unwrap();
                        }
                    }
                }
                if parent[block] != Some(common) {
                    parent[block] = Some(common);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut children = vec![Vec::new(); n];
        for &node in &order[1..] {
            let siblings = &mut children[parent[node].ok_or_else(|| {
                reject(
                    "E-VERIFY-CFG",
                    None,
                    "reachable block has no immediate dominator",
                )
            })?];
            b.spend(
                1usize.saturating_add(if siblings.len() == siblings.capacity() {
                    siblings.len()
                } else {
                    0
                }),
            )?;
            siblings.push(node);
        }
        let mut intervals: Vec<Option<(usize, usize)>> = vec![None; n];
        let mut clock = 1;
        let mut stack = Vec::with_capacity(n);
        stack.push((entry, false));
        while let Some((node, exit)) = stack.pop() {
            b.spend(2)?;
            if exit {
                intervals[node].as_mut().unwrap().1 = clock;
            } else {
                intervals[node] = Some((clock, clock));
                clock += 1;
                b.spend(children[node].len().saturating_mul(2).saturating_add(1))?;
                stack.push((node, true));
                stack.extend(children[node].iter().map(|&child| (child, false)));
            }
        }
        Ok(Self { intervals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_three_block_graph_matches_reachability_with_one_removed_block() {
        let mut b = Budget::new(&Limits::default());
        for edges in 0..512 {
            let mut successors = vec![vec![]; 3];
            let mut predecessors = successors.clone();
            for from in 0..3 {
                for to in 0..3 {
                    if edges & (1 << (from * 3 + to)) != 0 {
                        successors[from].push(to);
                        predecessors[to].push(from);
                    }
                }
            }
            for entry in 0..3 {
                let actual = Dominance::build(entry, &successors, &predecessors, &mut b).unwrap();
                let reachable = |removed| {
                    let mut seen = [false; 3];
                    let mut queue = vec![entry];
                    while let Some(node) = queue.pop() {
                        if node == removed || seen[node] {
                            continue;
                        }
                        seen[node] = true;
                        queue.extend(&successors[node]);
                    }
                    seen
                };
                let all = reachable(3);
                for ancestor in 0..3 {
                    let avoiding = reachable(ancestor);
                    for block in 0..3 {
                        assert_eq!(
                            actual.dominates(ancestor, block),
                            ancestor == block || all[block] && !avoiding[block],
                            "edges={edges} entry={entry} ancestor={ancestor} block={block}"
                        );
                    }
                }
            }
        }
    }
}
