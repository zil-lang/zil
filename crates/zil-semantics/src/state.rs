//! The abstract state domain of the place/initialization/variant family.
//!
//! Order: `a ⊑ b` means `a` is at least as precise as `b`; every
//! precondition that holds in `b` also holds in `a`. Join is the least upper
//! bound. A certificate invariant is accepted at a block iff the state
//! flowing in along every edge is `⊑` it.

use crate::paths::{MovePaths, PathId};
use zil_ir::body::{InitClaim, Local, Place};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn new(len: usize) -> Self {
        BitSet {
            words: vec![0; len.div_ceil(64)],
        }
    }
    pub fn contains(&self, i: usize) -> bool {
        self.words
            .get(i / 64)
            .is_some_and(|w| w & (1 << (i % 64)) != 0)
    }
    pub fn insert(&mut self, i: usize) {
        self.words[i / 64] |= 1 << (i % 64);
    }
    pub fn remove(&mut self, i: usize) {
        self.words[i / 64] &= !(1 << (i % 64));
    }
    pub fn union_with(&mut self, other: &BitSet) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }
    pub fn is_subset(&self, other: &BitSet) -> bool {
        self.words
            .iter()
            .zip(&other.words)
            .all(|(a, b)| a & !b == 0)
    }
}

/// Per-path initialization: a path is definitely initialized iff it is in
/// `maybe_init` and not in `maybe_uninit`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InitState {
    pub maybe_init: BitSet,
    pub maybe_uninit: BitSet,
}

impl InitState {
    pub fn all_uninit(paths: &MovePaths) -> Self {
        let mut s = InitState {
            maybe_init: BitSet::new(paths.len()),
            maybe_uninit: BitSet::new(paths.len()),
        };
        for i in 0..paths.len() {
            s.maybe_uninit.insert(i);
        }
        s
    }
    /// The weakest state: every path may be either.
    pub fn top(paths: &MovePaths) -> Self {
        let mut s = Self::all_uninit(paths);
        for i in 0..paths.len() {
            s.maybe_init.insert(i);
        }
        s
    }
    pub fn claim(&self, path: PathId) -> InitClaim {
        match (
            self.maybe_init.contains(path.index()),
            self.maybe_uninit.contains(path.index()),
        ) {
            (true, false) => InitClaim::Init,
            (false, true) => InitClaim::Uninit,
            // (false, false) cannot be produced by transfers; read it as the
            // weakest claim so a forged state never gains precision.
            (true, true) | (false, false) => InitClaim::Maybe,
        }
    }
    pub fn set(&mut self, path: PathId, claim: InitClaim) {
        let i = path.index();
        match claim {
            InitClaim::Init => {
                self.maybe_init.insert(i);
                self.maybe_uninit.remove(i);
            }
            InitClaim::Uninit => {
                self.maybe_init.remove(i);
                self.maybe_uninit.insert(i);
            }
            InitClaim::Maybe => {
                self.maybe_init.insert(i);
                self.maybe_uninit.insert(i);
            }
        }
    }
    pub fn set_subtree(&mut self, paths: &MovePaths, path: PathId, claim: InitClaim) {
        for p in paths.subtree(path) {
            self.set(p, claim);
        }
    }
    pub fn is_definitely_init(&self, path: PathId) -> bool {
        self.claim(path) == InitClaim::Init
    }
    pub fn subtree_definitely_init(&self, paths: &MovePaths, path: PathId) -> bool {
        paths.subtree(path).all(|p| self.is_definitely_init(p))
    }
    pub fn subtree_definitely_uninit(&self, paths: &MovePaths, path: PathId) -> bool {
        paths
            .subtree(path)
            .all(|p| self.claim(p) == InitClaim::Uninit)
    }
}

/// Facts that must hold on every path into a point.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MustFacts<K: Eq, V: Eq> {
    entries: Vec<(K, V)>,
}

impl<K: Eq + Clone, V: Eq + Clone> Default for MustFacts<K, V> {
    fn default() -> Self {
        MustFacts {
            entries: Vec::new(),
        }
    }
}

impl<K: Eq + Clone, V: Eq + Clone> MustFacts<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn insert(&mut self, key: K, value: V) {
        self.entries.retain(|(k, _)| *k != key);
        self.entries.push((key, value));
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries.retain(|(k, v)| keep(k, v));
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
    /// Intersection: keep facts both sides agree on.
    pub fn meet_with(&mut self, other: &Self) {
        self.entries.retain(|(k, v)| other.get(k) == Some(v));
    }
    /// `self` has every fact of `other` (so `self ⊑ other`).
    pub fn covers(&self, other: &Self) -> bool {
        other.entries.iter().all(|(k, v)| self.get(k) == Some(v))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct State {
    pub init: InitState,
    /// Known active variant (Enum) or arm (error union; 1 = success).
    pub variants: MustFacts<Place, u32>,
    /// `local` holds the unchanged tag of `place`.
    pub tags: MustFacts<Local, Place>,
}

impl State {
    pub fn top(paths: &MovePaths) -> Self {
        State {
            init: InitState::top(paths),
            variants: MustFacts::default(),
            tags: MustFacts::default(),
        }
    }
    pub fn join_with(&mut self, other: &State) {
        self.init.maybe_init.union_with(&other.init.maybe_init);
        self.init.maybe_uninit.union_with(&other.init.maybe_uninit);
        self.variants.meet_with(&other.variants);
        self.tags.meet_with(&other.tags);
    }
    /// `self ⊑ other`.
    pub fn leq(&self, other: &State) -> bool {
        self.init.maybe_init.is_subset(&other.init.maybe_init)
            && self.init.maybe_uninit.is_subset(&other.init.maybe_uninit)
            && self.variants.covers(&other.variants)
            && self.tags.covers(&other.tags)
    }
}
