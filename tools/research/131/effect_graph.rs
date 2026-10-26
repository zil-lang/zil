#![forbid(unsafe_code)]
//! Research-only incremental Boolean dependency closure; not a ZIL safety kernel.
//! Edges point from a callee's effect fact to the dependent caller's fact.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Edge(usize, usize, bool),
    Seed(usize, bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidNode(usize),
    Budget,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub work: usize,
    pub dirty: usize,
    pub changed: usize,
    pub incoming_scans: usize,
    pub propagation_scans: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    out: Vec<BTreeSet<usize>>,
    incoming: Vec<BTreeSet<usize>>,
    seeds: Vec<bool>,
    active: Vec<bool>,
}
struct Meter {
    used: usize,
    limit: usize,
}
impl Meter {
    fn spend(&mut self, n: usize) -> Result<(), Error> {
        let next = self.used.checked_add(n).ok_or(Error::Budget)?;
        if next > self.limit {
            return Err(Error::Budget);
        }
        self.used = next;
        Ok(())
    }
}
impl Graph {
    /// Cold construction is separate from update accounting and scans all input.
    /// Normal Rust allocation failure may abort; update Budget/InvalidNode errors
    /// (not allocator aborts) guarantee exact rollback. No production quota claim.
    pub fn new(n: usize, edges: &[(usize, usize)], seeds: &[usize]) -> Result<Self, Error> {
        for v in edges.iter().flat_map(|&(a, b)| [a, b]).chain(seeds.iter().copied()) {
            if v >= n {
                return Err(Error::InvalidNode(v));
            }
        }
        let mut g = Self {
            out: vec![BTreeSet::new(); n],
            incoming: vec![BTreeSet::new(); n],
            seeds: vec![false; n],
            active: vec![false; n],
        };
        for &(a, b) in edges {
            g.out[a].insert(b);
            g.incoming[b].insert(a);
        }
        let mut q = VecDeque::new();
        for &s in seeds {
            g.seeds[s] = true;
            if !g.active[s] {
                g.active[s] = true;
                q.push_back(s);
            }
        }
        while let Some(v) = q.pop_front() {
            for &w in &g.out[v] {
                if !g.active[w] {
                    g.active[w] = true;
                    q.push_back(w);
                }
            }
        }
        Ok(g)
    }
    /// O(1) borrow. Copying/serializing the complete result remains O(V).
    pub fn active(&self) -> &[bool] {
        &self.active
    }
    /// Last edit of a given edge/seed wins. All fallible work precedes publish.
    /// Budget counts explicit visits, not CPU cycles or BTree comparison costs.
    pub fn apply(&mut self, edits: &[Edit], limit: usize) -> Result<Stats, Error> {
        let mut meter = Meter { used: 0, limit };
        let mut raw_edges = BTreeMap::new();
        let mut raw_seeds = BTreeMap::new();
        for &edit in edits {
            meter.spend(1)?;
            match edit {
                Edit::Edge(a, b, present) => {
                    for v in [a, b] {
                        if v >= self.active.len() { return Err(Error::InvalidNode(v)); }
                    }
                    raw_edges.insert((a, b), present);
                }
                Edit::Seed(v, present) => {
                    if v >= self.active.len() { return Err(Error::InvalidNode(v)); }
                    raw_seeds.insert(v, present);
                }
            }
        }
        let mut edges = BTreeMap::new();
        let mut seeds = BTreeMap::new();
        let mut added_out: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        let mut added_in: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        let mut dirty = BTreeSet::new();
        let mut q = VecDeque::new();
        for ((a, b), present) in raw_edges {
            meter.spend(1)?;
            if self.out[a].contains(&b) == present { continue; }
            edges.insert((a, b), present);
            if present {
                added_out.entry(a).or_default().insert(b);
                added_in.entry(b).or_default().insert(a);
            }
            if dirty.insert(b) { q.push_back(b); }
        }
        for (v, present) in raw_seeds {
            meter.spend(1)?;
            if self.seeds[v] == present { continue; }
            seeds.insert(v, present);
            if dirty.insert(v) { q.push_back(v); }
        }
        // The union of old/new edges is deliberately conservative for deletion.
        while let Some(v) = q.pop_front() {
            meter.spend(1)?;
            for w in self.out[v].iter().copied().chain(
                added_out.get(&v).into_iter().flat_map(|s| s.iter().copied())
            ) {
                meter.spend(1)?;
                if dirty.insert(w) { q.push_back(w); }
            }
        }
        let mut live = BTreeSet::new();
        let mut stats = Stats { dirty: dirty.len(), ..Stats::default() };
        for &v in &dirty {
            meter.spend(1)?;
            let mut supported = *seeds.get(&v).unwrap_or(&self.seeds[v]);
            for u in self.incoming[v].iter().copied().chain(
                added_in.get(&v).into_iter().flat_map(|s| s.iter().copied())
            ) {
                meter.spend(1)?;
                stats.incoming_scans += 1;
                if edges.get(&(u, v)) != Some(&false) && !dirty.contains(&u) && self.active[u] {
                    supported = true;
                }
            }
            if supported && live.insert(v) { q.push_back(v); }
        }
        // NO old true fact inside dirty is used as a seed: seedless cycles retract.
        while let Some(v) = q.pop_front() {
            meter.spend(1)?;
            for w in self.out[v].iter().copied().chain(
                added_out.get(&v).into_iter().flat_map(|s| s.iter().copied())
            ) {
                meter.spend(1)?;
                stats.propagation_scans += 1;
                if edges.get(&(v, w)) == Some(&false) { continue; }
                debug_assert!(dirty.contains(&w));
                if live.insert(w) { q.push_back(w); }
            }
        }
        // Precharge the complete publication. There is no returned error below.
        meter.spend(edges.len().checked_mul(2).ok_or(Error::Budget)?)?;
        meter.spend(seeds.len())?;
        meter.spend(dirty.len())?;
        for ((a, b), present) in edges {
            if present {
                self.out[a].insert(b);
                self.incoming[b].insert(a);
            } else {
                self.out[a].remove(&b);
                self.incoming[b].remove(&a);
            }
        }
        for (v, present) in seeds { self.seeds[v] = present; }
        for v in dirty {
            let next = live.contains(&v);
            stats.changed += usize::from(next != self.active[v]);
            self.active[v] = next;
        }
        stats.work = meter.used;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Deliberately different oracle: repeated whole-edge equations, not a queue,
    // and no use of incoming adjacency, dirty cones or cached active state.
    fn oracle(g: &Graph) -> Vec<bool> {
        let mut result = g.seeds.clone();
        loop {
            let old = result.clone();
            for (u, successors) in g.out.iter().enumerate() {
                for &v in successors { result[v] |= old[u]; }
            }
            if old == result { return result; }
        }
    }
    fn reference_update(base: &Graph, edits: &[Edit]) -> Graph {
        let mut next = base.clone();
        for &edit in edits {
            match edit {
                Edit::Edge(a, b, yes) => {
                    if yes { next.out[a].insert(b); next.incoming[b].insert(a); }
                    else { next.out[a].remove(&b); next.incoming[b].remove(&a); }
                }
                Edit::Seed(v, yes) => next.seeds[v] = yes,
            }
        }
        next.active = oracle(&next);
        next
    }
    fn check(g: &Graph) {
        assert_eq!(g.active, oracle(g));
        for (u, successors) in g.out.iter().enumerate() {
            for v in 0..g.active.len() {
                assert_eq!(successors.contains(&v), g.incoming[v].contains(&u));
            }
        }
    }
    #[test]
    fn recursive_seed_deletion_cannot_self_support() {
        let mut g = Graph::new(3, &[(0, 1), (1, 0), (1, 2), (2, 2)], &[0]).unwrap();
        assert_eq!(g.active(), [true, true, true]);
        let s = g.apply(&[Edit::Seed(0, false)], 1000).unwrap();
        assert_eq!(s.changed, 3);
        assert_eq!(g.active(), [false, false, false]);
        check(&g);
    }
    #[test]
    fn external_support_and_alternate_paths_survive() {
        let mut g = Graph::new(4, &[(0, 1), (0, 2), (1, 3), (2, 3), (3, 3)], &[0]).unwrap();
        g.apply(&[Edit::Edge(1, 3, false)], 1000).unwrap();
        assert!(g.active[3]);
        g.apply(&[Edit::Edge(2, 3, false)], 1000).unwrap();
        assert!(!g.active[3]);
        check(&g);
    }
    #[test]
    fn last_write_wins_and_noop_has_no_dirty_graph_scan() {
        let mut g = Graph::new(1000, &[(0, 1)], &[0]).unwrap();
        let before = g.clone();
        let s = g.apply(&[Edit::Edge(0, 1, false), Edit::Edge(0, 1, true),
                          Edit::Seed(0, false), Edit::Seed(0, true)], 100).unwrap();
        assert_eq!(g, before);
        assert_eq!(s.dirty, 0);
        assert_eq!(s.work, 6);
        assert_eq!(g.apply(&[], 0).unwrap().work, 0);
    }
    #[test]
    fn every_budget_prefix_rolls_back_and_n_minus_one_fails() {
        let original = Graph::new(5, &[(0, 1), (1, 2), (2, 1), (3, 4)], &[0]).unwrap();
        let edits = [Edit::Seed(0, false), Edit::Edge(4, 2, true), Edit::Seed(3, true)];
        let mut success = original.clone();
        let n = success.apply(&edits, usize::MAX).unwrap().work;
        for budget in 0..n {
            let mut failed = original.clone();
            assert_eq!(failed.apply(&edits, budget).unwrap_err(), Error::Budget);
            assert_eq!(failed, original);
        }
        let mut exact = original.clone();
        assert_eq!(exact.apply(&edits, n).unwrap().work, n);
        assert_eq!(exact, success);
        check(&exact);
        let mut invalid = original.clone();
        assert_eq!(invalid.apply(&[Edit::Seed(1, true), Edit::Edge(0, 99, true)],
                                 usize::MAX).unwrap_err(), Error::InvalidNode(99));
        assert_eq!(invalid, original);
    }
    #[test]
    fn exhaustive_three_node_graphs_seeds_and_single_updates() {
        let mut cases = 0;
        for mask in 0usize..512 {
            let edges: Vec<_> = (0..9).filter(|i| mask & (1 << i) != 0)
                .map(|i| (i / 3, i % 3)).collect();
            for seeds in 0usize..8 {
                let roots: Vec<_> = (0..3).filter(|v| seeds & (1 << v) != 0).collect();
                let base = Graph::new(3, &edges, &roots).unwrap();
                check(&base);
                for k in 0..12 {
                    let edit = if k < 9 { Edit::Edge(k / 3, k % 3, mask & (1 << k) == 0) }
                        else { Edit::Seed(k - 9, seeds & (1 << (k - 9)) == 0) };
                    let mut g = base.clone();
                    g.apply(&[edit], 1000).unwrap();
                    assert_eq!(g, reference_update(&base, &[edit]));
                    check(&g);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 49152);
        println!("exhaustive_updates={cases}");
    }
    fn random(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }
    #[test]
    fn mixed_batches_match_whole_graph_fixed_point() {
        let mut rng = 0x1315eed_u64;
        let mut g = Graph::new(48, &[], &[]).unwrap();
        for _ in 0..2000 {
            let mut edits = Vec::new();
            for _ in 0..(1 + random(&mut rng) % 7) {
                let a = (random(&mut rng) % 48) as usize;
                let b = (random(&mut rng) % 48) as usize;
                let yes = random(&mut rng) & 1 == 0;
                edits.push(if random(&mut rng) % 4 == 0 { Edit::Seed(a, yes) }
                           else { Edit::Edge(a, b, yes) });
            }
            let expected = reference_update(&g, &edits);
            g.apply(&edits, 100_000).unwrap();
            assert_eq!(g, expected);
            check(&g);
        }
    }
    #[test]
    fn local_edit_and_whole_cone_growth_are_both_reported() {
        for n in [128, 256, 512, 1024] {
            let edges: Vec<_> = (1..n).map(|i| (i - 1, i)).collect();
            let base = Graph::new(n, &edges, &[0]).unwrap();
            let mut local = base.clone();
            let a = local.apply(&[Edit::Edge(n - 2, n - 1, false)], usize::MAX).unwrap();
            let mut root = base.clone();
            let b = root.apply(&[Edit::Seed(0, false)], usize::MAX).unwrap();
            assert_eq!(a.dirty, 1);
            assert_eq!(b.dirty, n);
            check(&local);
            check(&root);
            println!("growth n={n} local_work={} local_dirty={} root_work={} root_dirty={}",
                     a.work, a.dirty, b.work, b.dirty);
        }
    }
    #[test]
    fn empty_graph_and_invalid_construction_are_explicit() {
        let mut empty = Graph::new(0, &[], &[]).unwrap();
        assert_eq!(empty.apply(&[], 0).unwrap().work, 0);
        assert_eq!(Graph::new(1, &[(0, 1)], &[]), Err(Error::InvalidNode(1)));
        assert_eq!(empty.apply(&[Edit::Seed(0, true)], 1).unwrap_err(), Error::InvalidNode(0));
        assert!(empty.active().is_empty());
    }
}
