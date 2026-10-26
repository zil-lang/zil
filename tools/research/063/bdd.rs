//! Ordinary reduced ordered BDDs. Pre/post coordinates are interleaved.
//! This module is independent of TypedIR and ownership semantics.
use crate::budget::{MAX_CACHE, MAX_NODES, MAX_ROWS, R, Work};
use std::collections::{BTreeSet, HashMap};
pub type Id = u32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Mono,
    Cut,
    Wire,
    Fused,
}
impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Self::Mono => "mono",
            Self::Cut => "cut",
            Self::Wire => "wire",
            Self::Fused => "fused",
        }
    }
}
pub struct Plan {
    fused: bool,
    absent: BTreeSet<u16>,
    steps: Vec<Step>,
    final_retire: BTreeSet<u16>,
}
struct Step {
    factor: Id,
    retire: BTreeSet<u16>,
    substitution: Option<(u16, u16, bool)>,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    var: u16,
    low: Id,
    high: Id,
}
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub phase: &'static str,
    pub point: Option<usize>,
    pub equation: Option<usize>,
}

pub struct Bdd {
    nodes: Vec<Node>,
    free: Vec<Id>,
    pub created_nodes: usize,
    pub gc_count: usize,
    pub collected_nodes: usize,
    pub gc_work: usize,
    unique: HashMap<(u16, Id, Id), Id>,
    cache: HashMap<(u8, Id, Id), Id>,
    pub work: Work,
    pub bits: usize,
    pub image_calls: usize,
    pub early_eliminations: usize,
    pub max_boundary: usize,
    pub wire_substitutions: usize,
    pub max_factor_support: usize,
    pub context: Context,
    rank: Vec<usize>,
    canonical: Vec<usize>,
}
impl Bdd {
    pub fn new(bits: usize, limit: usize) -> Self {
        Self {
            nodes: vec![
                Node {
                    var: u16::MAX,
                    low: 0,
                    high: 0
                };
                2
            ],
            free: Vec::new(),
            created_nodes: 2,
            gc_count: 0,
            collected_nodes: 0,
            gc_work: 0,
            unique: HashMap::new(),
            cache: HashMap::new(),
            work: Work::new(limit),
            bits,
            image_calls: 0,
            early_eliminations: 0,
            max_boundary: 0,
            wire_substitutions: 0,
            max_factor_support: 0,
            context: Context {
                phase: "created",
                point: None,
                equation: None,
            },
            rank: (0..bits).collect(),
            canonical: (0..bits).collect(),
        }
    }
    /// `canonical_at_rank` is a permutation; all emitted rows retain the
    /// original semantic coordinates, including rejected-state witnesses.
    pub fn set_order(&mut self, canonical_at_rank: Vec<usize>) -> R<()> {
        self.work.charge(self.bits.saturating_mul(5))?;
        if self.nodes.len() != 2 || canonical_at_rank.len() != self.bits {
            return Err("InternalError: late or malformed variable order".into());
        }
        let mut seen = vec![false; self.bits];
        for &bit in &canonical_at_rank {
            if bit >= self.bits || seen[bit] {
                return Err("InternalError: non-permutation variable order".into());
            }
            seen[bit] = true;
        }
        for (rank, &bit) in canonical_at_rank.iter().enumerate() {
            self.rank[bit] = rank;
        }
        self.canonical = canonical_at_rank;
        Ok(())
    }
    fn coordinate(&self, bit: usize, post: bool) -> u16 {
        (self.rank[bit] * 2 + usize::from(post)) as u16
    }
    pub fn set_context(&mut self, phase: &'static str, point: Option<usize>) {
        self.context = Context {
            phase,
            point,
            equation: None,
        };
    }
    /// Diagnostic traversal only: its separately bounded work is not part of
    /// the candidate logical budget. Roots must be complete at a safe boundary.
    pub fn reachable_nodes(&self, roots: &[Id], work: &mut Work) -> R<usize> {
        work.charge(self.nodes.len().saturating_add(roots.len()))?;
        let mut seen = vec![false; self.nodes.len()];
        let mut todo = roots.to_vec();
        let mut count = 0;
        while let Some(id) = todo.pop() {
            work.charge(1)?;
            if id < 2 || seen[id as usize] {
                continue;
            }
            seen[id as usize] = true;
            count += 1;
            let node = self.nodes[id as usize];
            work.charge(2)?;
            todo.extend([node.low, node.high]);
        }
        Ok(count)
    }
    pub fn occupied_nodes(&self) -> usize {
        self.nodes.len() - self.free.len()
    }
    /// Mark/sweep only at explicit safe boundaries. IDs of live nodes are stable;
    /// every operation cache is cleared before any dead slot can be reused.
    pub fn collect(&mut self, roots: &[Id]) -> R<()> {
        // HashMap clear may scan allocated buckets, not merely live entries.
        // Twice the fixed usable capacity conservatively charges bucket/control work.
        let start = self.work.used;
        self.work.charge(
            self.nodes
                .len()
                .saturating_mul(3)
                .saturating_add(roots.len())
                .saturating_add(self.cache.capacity().saturating_mul(2))
                .saturating_add(self.unique.capacity().saturating_mul(2))
                .saturating_add(self.free.len()),
        )?;
        let mut marked = vec![false; self.nodes.len()];
        let mut todo = roots.to_vec();
        while let Some(id) = todo.pop() {
            self.work.charge(1)?;
            if id < 2 {
                continue;
            }
            let Some(node) = self.nodes.get(id as usize).copied() else {
                return Err("InternalError: GC root outside arena".into());
            };
            if node.var == u16::MAX {
                return Err("InternalError: GC root in free slot".into());
            }
            if marked[id as usize] {
                continue;
            }
            marked[id as usize] = true;
            self.work.charge(2)?;
            todo.extend([node.low, node.high]);
        }
        // No cache entry survives, even if its result currently happens to live.
        self.cache.clear();
        // Rebuild in NodeId order. `retain` leaves hash-layout-dependent tombstones,
        // so its reported usable capacity is not a deterministic work measure.
        // Clearing restores the fixed allocated capacity and removes stale keys.
        self.unique.clear();
        let old_free = self.free.len();
        self.free.clear();
        for (id, keep) in marked.into_iter().enumerate().skip(2) {
            if keep {
                self.work.charge(2)?;
                let node = self.nodes[id];
                self.unique
                    .insert((node.var, node.low, node.high), id as Id);
            } else {
                self.nodes[id] = Node {
                    var: u16::MAX,
                    low: 0,
                    high: 0,
                };
                self.free.push(id as Id);
            }
        }
        self.collected_nodes += self.free.len() - old_free;
        self.gc_count += 1;
        self.gc_work += self.work.used - start;
        Ok(())
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn mk(&mut self, var: u16, low: Id, high: Id) -> R<Id> {
        self.work.charge(1)?;
        if low == high {
            return Ok(low);
        }
        if usize::from(var) >= self.bits * 2
            || self.nodes[low as usize].var <= var
            || self.nodes[high as usize].var <= var
        {
            return Err("InternalError: BDD order".into());
        }
        if let Some(&id) = self.unique.get(&(var, low, high)) {
            return Ok(id);
        }
        if self.free.is_empty() && self.nodes.len() >= MAX_NODES {
            return Err("ResourceLimit: BDD nodes".into());
        }
        self.work.charge(3)?; // node + unique-table entry + eventual release
        let id = if let Some(id) = self.free.pop() {
            self.nodes[id as usize] = Node { var, low, high };
            id
        } else {
            let id = self.nodes.len() as Id;
            self.nodes.push(Node { var, low, high });
            id
        };
        self.created_nodes += 1;
        self.unique.insert((var, low, high), id);
        Ok(id)
    }
    pub fn cube(&mut self, values: &std::collections::BTreeMap<usize, Option<bool>>) -> R<Id> {
        self.work.charge(values.len())?;
        if values.len() != self.bits || values.keys().copied().ne(0..self.bits) {
            return Err("InternalError: cube coordinate coverage".into());
        }
        let mut root = 1;
        for rank in (0..self.bits).rev() {
            let bit = self.canonical[rank];
            if let Some(value) = values[&bit] {
                root = if value {
                    self.mk((rank * 2) as u16, 0, root)?
                } else {
                    self.mk((rank * 2) as u16, root, 0)?
                };
            }
        }
        Ok(root)
    }
    pub fn var(&mut self, bit: usize, post: bool) -> R<Id> {
        self.mk(self.coordinate(bit, post), 0, 1)
    }
    pub fn and(&mut self, a: Id, b: Id) -> R<Id> {
        self.apply(0, a, b)
    }
    pub fn or(&mut self, a: Id, b: Id) -> R<Id> {
        self.apply(1, a, b)
    }
    pub fn xor(&mut self, a: Id, b: Id) -> R<Id> {
        self.apply(2, a, b)
    }
    pub fn not(&mut self, a: Id) -> R<Id> {
        self.xor(a, 1)
    }
    pub fn equal(&mut self, a: Id, b: Id) -> R<Id> {
        let x = self.xor(a, b)?;
        self.not(x)
    }
    pub fn choose(&mut self, c: Id, yes: Id, no: Id) -> R<Id> {
        let a = self.and(c, yes)?;
        let nc = self.not(c)?;
        let b = self.and(nc, no)?;
        self.or(a, b)
    }
    fn apply(&mut self, op: u8, mut a: Id, mut b: Id) -> R<Id> {
        self.work.charge(1)?;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        match op {
            0 if a == 0 || a == b => return Ok(a),
            0 if a == 1 => return Ok(b),
            1 if a == 0 => return Ok(b),
            1 if a == 1 || a == b => return Ok(a),
            2 if a == 0 => return Ok(b),
            2 if a == b => return Ok(0),
            _ => {}
        }
        if let Some(&r) = self.cache.get(&(op, a, b)) {
            return Ok(r);
        }
        let na = self.nodes[a as usize];
        let nb = self.nodes[b as usize];
        let var = na.var.min(nb.var);
        if var == u16::MAX {
            return Err("InternalError: terminal apply".into());
        }
        let (al, ah) = if na.var == var {
            (na.low, na.high)
        } else {
            (a, a)
        };
        let (bl, bh) = if nb.var == var {
            (nb.low, nb.high)
        } else {
            (b, b)
        };
        let low = self.apply(op, al, bl)?;
        let high = self.apply(op, ah, bh)?;
        let r = self.mk(var, low, high)?;
        if self.cache.len() >= MAX_CACHE {
            self.work.charge(self.cache.len())?;
            self.cache.clear();
        }
        self.cache.insert((op, a, b), r);
        Ok(r)
    }
    fn support_nodes(&mut self, root: Id) -> R<(BTreeSet<u16>, usize)> {
        let mut seen = BTreeSet::new();
        let mut vars = BTreeSet::new();
        let mut q = vec![root];
        while let Some(x) = q.pop() {
            self.work.charge(1)?;
            if x < 2 || !seen.insert(x) {
                continue;
            }
            let n = self.nodes[x as usize];
            self.work.charge(5)?; // visited/variable entries, queue edges, release
            vars.insert(n.var);
            q.extend([n.low, n.high]);
        }
        Ok((vars, seen.len()))
    }
    pub fn support(&mut self, root: Id) -> R<BTreeSet<u16>> {
        Ok(self.support_nodes(root)?.0)
    }
    pub fn shape(&mut self, root: Id) -> R<(usize, usize)> {
        let (support, nodes) = self.support_nodes(root)?;
        Ok((support.len(), nodes))
    }
    pub fn exists(&mut self, root: Id, vars: &BTreeSet<u16>) -> R<Id> {
        fn go(b: &mut Bdd, x: Id, vars: &BTreeSet<u16>, memo: &mut HashMap<Id, Id>) -> R<Id> {
            b.work.charge(1)?;
            if x < 2 {
                return Ok(x);
            }
            if let Some(&v) = memo.get(&x) {
                return Ok(v);
            }
            let n = b.nodes[x as usize];
            let l = go(b, n.low, vars, memo)?;
            let h = go(b, n.high, vars, memo)?;
            let y = if vars.contains(&n.var) {
                b.or(l, h)?
            } else {
                b.mk(n.var, l, h)?
            };
            b.work.charge(2)?; // insertion and eventual temporary-map release
            memo.insert(x, y);
            Ok(y)
        }
        if vars.is_empty() {
            return Ok(root);
        }
        go(self, root, vars, &mut HashMap::new())
    }
    /// Relational product: existentially eliminate while constructing AND.
    /// This is the standard AndAbstract operation, not a new semantic rule.
    pub fn and_exists(&mut self, a: Id, b: Id, vars: &BTreeSet<u16>) -> R<Id> {
        fn go(
            bdd: &mut Bdd,
            mut a: Id,
            mut b: Id,
            vars: &BTreeSet<u16>,
            memo: &mut HashMap<(Id, Id), Id>,
        ) -> R<Id> {
            bdd.work.charge(1)?;
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            if a == 0 {
                return Ok(0);
            }
            if a == 1 && b == 1 {
                return Ok(1);
            }
            if let Some(&r) = memo.get(&(a, b)) {
                return Ok(r);
            }
            let na = bdd.nodes[a as usize];
            let nb = bdd.nodes[b as usize];
            let var = na.var.min(nb.var);
            if var == u16::MAX {
                return Err("InternalError: relational product terminal".into());
            }
            let (al, ah) = if na.var == var {
                (na.low, na.high)
            } else {
                (a, a)
            };
            let (bl, bh) = if nb.var == var {
                (nb.low, nb.high)
            } else {
                (b, b)
            };
            let low = go(bdd, al, bl, vars, memo)?;
            let high = go(bdd, ah, bh, vars, memo)?;
            let result = if vars.contains(&var) {
                bdd.or(low, high)?
            } else {
                bdd.mk(var, low, high)?
            };
            bdd.work.charge(2)?; // map insertion and eventual release
            memo.insert((a, b), result);
            Ok(result)
        }
        if vars.is_empty() {
            return self.and(a, b);
        }
        go(self, a, b, vars, &mut HashMap::new())
    }
    fn unprime(&mut self, root: Id) -> R<Id> {
        fn go(b: &mut Bdd, x: Id, m: &mut HashMap<Id, Id>) -> R<Id> {
            b.work.charge(1)?;
            if x < 2 {
                return Ok(x);
            }
            if let Some(&v) = m.get(&x) {
                return Ok(v);
            }
            let n = b.nodes[x as usize];
            let l = go(b, n.low, m)?;
            let h = go(b, n.high, m)?;
            let y = b.mk(n.var & !1, l, h)?;
            b.work.charge(2)?;
            m.insert(x, y);
            Ok(y)
        }
        go(self, root, &mut HashMap::new())
    }
    pub fn plan(&mut self, factors: &[(usize, Id)], method: Method) -> R<Plan> {
        self.work.charge(factors.len().saturating_mul(5))?;
        // Each equation may mention its own fresh post port and pre-state
        // coordinates only. Enforce the builder invariant at the generic API.
        let mut forward_supports = Vec::new();
        for (bit, factor) in factors {
            if *bit >= self.bits {
                return Err("Unsupported: output coordinate".into());
            }
            let support = self.support(*factor)?;
            self.max_factor_support = self.max_factor_support.max(support.len());
            self.work.charge(support.len())?;
            let output = self.coordinate(*bit, true);
            if support.iter().any(|v| v & 1 != 0 && *v != output) {
                return Err("Unsupported: equation references another post output".into());
            }
            forward_supports.push(support);
        }
        let written = factors
            .iter()
            .map(|(bit, _)| self.coordinate(*bit, false))
            .collect::<BTreeSet<_>>();
        if written.len() != factors.len() {
            return Err("InternalError: repeated output".into());
        }
        if method == Method::Mono {
            return Ok(Plan {
                fused: false,
                absent: BTreeSet::new(),
                steps: factors
                    .iter()
                    .map(|(_, factor)| Step {
                        factor: *factor,
                        retire: BTreeSet::new(),
                        substitution: None,
                    })
                    .collect(),
                final_retire: written,
            });
        }
        let mut last = std::collections::BTreeMap::new();
        let mut supports = Vec::new();
        for (i, support) in forward_supports.into_iter().rev().enumerate() {
            self.work.charge(support.len())?;
            for v in &support {
                if written.contains(v) {
                    last.insert(*v, i);
                }
            }
            supports.push(support);
        }
        let absent = written
            .iter()
            .filter(|v| !last.contains_key(v))
            .copied()
            .collect();
        let mut steps = Vec::new();
        let mut active = last.len();
        for (i, (bit, factor)) in factors.iter().rev().enumerate() {
            self.work.charge(last.len() + 1)?;
            let mut retire = last
                .iter()
                .filter(|(_, at)| **at == i)
                .map(|(v, _)| *v)
                .collect::<BTreeSet<_>>();
            self.max_boundary = self.max_boundary.max(active + i + 1);
            active -= retire.len();
            let output = self.coordinate(*bit, true);
            let mut substitution = None;
            if method == Method::Wire && supports[i].len() == 2 && supports[i].contains(&output) {
                let input = *supports[i]
                    .iter()
                    .find(|v| **v != output)
                    .expect("two variables");
                if input & 1 == 0 && retire.contains(&input) {
                    let mut truth = [false; 4];
                    for (a, slot) in truth.iter_mut().enumerate() {
                        let mut x = *factor;
                        while x >= 2 {
                            self.work.charge(1)?;
                            let n = self.nodes[x as usize];
                            let bit = if n.var == input {
                                a & 2 != 0
                            } else {
                                a & 1 != 0
                            };
                            x = if bit { n.high } else { n.low };
                        }
                        *slot = x == 1;
                    }
                    if truth == [true, false, false, true] || truth == [false, true, true, false] {
                        substitution = Some((input, output, truth[1]));
                        retire.remove(&input);
                    }
                }
            }
            steps.push(Step {
                factor: *factor,
                retire,
                substitution,
            });
        }
        Ok(Plan {
            fused: method == Method::Fused,
            absent,
            steps,
            final_retire: BTreeSet::new(),
        })
    }
    // Exact identity-end elimination: exists x. R(x,B) & (y = +/-x)
    // equals R(+/-y,B). y is this equation's unique, fresh output coordinate.
    fn substitute(&mut self, root: Id, old: u16, new: u16, flip: bool) -> R<Id> {
        fn go(
            b: &mut Bdd,
            x: Id,
            old: u16,
            new: u16,
            flip: bool,
            m: &mut HashMap<Id, Id>,
        ) -> R<Id> {
            b.work.charge(1)?;
            if x < 2 {
                return Ok(x);
            }
            if let Some(&v) = m.get(&x) {
                return Ok(v);
            }
            let n = b.nodes[x as usize];
            if n.var > old {
                return Ok(x);
            }
            let y = if n.var == old {
                let var = b.mk(new, 0, 1)?;
                if flip {
                    b.choose(var, n.low, n.high)?
                } else {
                    b.choose(var, n.high, n.low)?
                }
            } else {
                let l = go(b, n.low, old, new, flip, m)?;
                let h = go(b, n.high, old, new, flip, m)?;
                if b.nodes[l as usize].var > n.var && b.nodes[h as usize].var > n.var {
                    b.mk(n.var, l, h)?
                } else {
                    let var = b.mk(n.var, 0, 1)?;
                    b.choose(var, h, l)?
                }
            };
            b.work.charge(2)?;
            m.insert(x, y);
            Ok(y)
        }
        go(self, root, old, new, flip, &mut HashMap::new())
    }
    pub fn image(&mut self, input: Id, plan: &Plan) -> R<Id> {
        self.image_calls += 1;
        let mut state = self.exists(input, &plan.absent)?;
        for step in &plan.steps {
            self.work.charge(1)?;
            if let Some((old, new, flip)) = step.substitution {
                state = self.substitute(state, old, new, flip)?;
                self.wire_substitutions += 1;
            } else if plan.fused {
                state = self.and_exists(state, step.factor, &step.retire)?;
            } else {
                state = self.and(state, step.factor)?;
            }
            self.early_eliminations += step.retire.len();
            if !plan.fused {
                state = self.exists(state, &step.retire)?;
            }
        }
        state = self.exists(state, &plan.final_retire)?;
        self.unprime(state)
    }
    pub fn contains(&mut self, mut root: Id, words: &[u64]) -> R<bool> {
        if words.len() != self.bits.div_ceil(64) {
            return Err("Mismatch: joint row width".into());
        }
        if !self.bits.is_multiple_of(64)
            && words.last().is_some_and(|v| *v >> (self.bits % 64) != 0)
        {
            return Err("Mismatch: joint row high bits".into());
        }
        while root >= 2 {
            self.work.charge(1)?;
            let n = self.nodes[root as usize];
            if n.var & 1 != 0 {
                return Err("InternalError: post bit in result".into());
            }
            let bit = self.canonical[usize::from(n.var) / 2];
            root = if words[bit / 64] >> (bit % 64) & 1 == 0 {
                n.low
            } else {
                n.high
            };
        }
        Ok(root == 1)
    }
    /// Enumerate complete current-state rows, with an explicit bounded stack.
    /// No oracle row or candidate schema extractor is used to select the rows.
    pub fn rows(&mut self, root: Id, remaining: usize) -> R<crate::schema::Rows> {
        let count = self.count(root)?;
        if count > remaining.min(MAX_ROWS) as u128 {
            return Err("ResourceLimit: materialized total rows".into());
        }
        self.work.charge(self.bits.saturating_add(1))?;
        let mut row = vec![0u64; self.bits.div_ceil(64)];
        let mut stack = vec![(0usize, root, false)];
        let mut rows = crate::schema::Rows::new();
        while let Some((at, x, high)) = stack.pop() {
            self.work.charge(4)?; // pop, bit update, two bounded successor entries
            if at > 0 {
                let bit = self.canonical[at - 1];
                let mask = 1u64 << (bit % 64);
                if high {
                    row[bit / 64] |= mask;
                } else {
                    row[bit / 64] &= !mask;
                }
            }
            if x == 0 {
                continue;
            }
            if at == self.bits {
                if x != 1 {
                    return Err("InternalError: materialize nonterminal".into());
                }
                self.work
                    .charge(row.len().saturating_mul(2).saturating_add(1))?;
                rows.insert(row.clone());
                continue;
            }
            let n = self.nodes[x as usize];
            if x > 1 && (n.var & 1 != 0 || usize::from(n.var) / 2 < at) {
                return Err("InternalError: materialize coordinate order".into());
            }
            let (low, high) = if x > 1 && usize::from(n.var) / 2 == at {
                (n.low, n.high)
            } else {
                (x, x)
            };
            stack.push((at + 1, high, true));
            stack.push((at + 1, low, false));
        }
        if rows.len() as u128 != count {
            return Err("InternalError: materialize count".into());
        }
        Ok(rows)
    }
    pub fn count(&mut self, root: Id) -> R<u128> {
        fn shift(x: u128, n: usize) -> R<u128> {
            if x == 0 {
                return Ok(0);
            }
            if n >= 128 || x > u128::MAX >> n {
                return Err("ResourceLimit: exact cardinality".into());
            }
            Ok(x << n)
        }
        fn go(b: &mut Bdd, x: Id, level: usize, m: &mut HashMap<(Id, usize), u128>) -> R<u128> {
            b.work.charge(1)?;
            if x == 0 {
                return Ok(0);
            }
            if x == 1 {
                return shift(1, b.bits - level);
            }
            if let Some(&v) = m.get(&(x, level)) {
                return Ok(v);
            }
            let n = b.nodes[x as usize];
            if n.var & 1 != 0 {
                return Err("InternalError: count post bit".into());
            }
            let at = usize::from(n.var) / 2;
            let l = go(b, n.low, at + 1, m)?;
            let h = go(b, n.high, at + 1, m)?;
            let sum = l
                .checked_add(h)
                .ok_or("ResourceLimit: cardinality overflow")?;
            let v = shift(sum, at - level)?;
            b.work.charge(2)?;
            m.insert((x, level), v);
            Ok(v)
        }
        go(self, root, 0, &mut HashMap::new())
    }
}
