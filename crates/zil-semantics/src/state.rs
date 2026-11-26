//! The abstract state domain of the place/initialization/variant family.
//!
//! Order: `a ⊑ b` means `a` is at least as precise as `b`; every
//! precondition that holds in `b` also holds in `a`. Join is the least upper
//! bound. A certificate invariant is accepted at a block iff the state
//! flowing in along every edge is `⊑` it.

use crate::paths::{MovePaths, PathId};
use zil_ir::body::{InitClaim, Local, Place};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum BitWords {
    Inline { words: [u64; 2], len: u8 },
    Heap(Vec<u64>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BitSet {
    words: BitWords,
}

pub struct BitSetIter<'a> {
    words: &'a [u64],
    word_idx: usize,
    cur: u64,
}

impl Iterator for BitSetIter<'_> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.cur == 0 {
            if self.word_idx >= self.words.len() {
                return None;
            }
            self.cur = self.words[self.word_idx];
            self.word_idx += 1;
        }
        let tz = self.cur.trailing_zeros() as usize;
        self.cur &= self.cur - 1;
        Some((self.word_idx - 1) * 64 + tz)
    }
}

impl BitSet {
    #[inline]
    pub fn new(len: usize) -> Self {
        let word_count = len.div_ceil(64);
        let words = if word_count <= 2 {
            BitWords::Inline {
                words: [0; 2],
                len: word_count as u8,
            }
        } else {
            BitWords::Heap(vec![0; word_count])
        };
        BitSet { words }
    }

    #[inline]
    pub fn new_ones(len: usize) -> Self {
        let mut s = Self::new(len);
        let full_words = len / 64;
        let rem_bits = len % 64;
        let slice = s.words_mut();
        for w in &mut slice[..full_words] {
            *w = u64::MAX;
        }
        if rem_bits != 0 {
            slice[full_words] = (1u64 << rem_bits) - 1;
        }
        s
    }

    #[inline]
    fn words(&self) -> &[u64] {
        match &self.words {
            BitWords::Inline { words, len } => &words[..*len as usize],
            BitWords::Heap(v) => v.as_slice(),
        }
    }

    #[inline]
    fn words_mut(&mut self) -> &mut [u64] {
        match &mut self.words {
            BitWords::Inline { words, len } => &mut words[..*len as usize],
            BitWords::Heap(v) => v.as_mut_slice(),
        }
    }

    #[inline]
    pub fn contains(&self, i: usize) -> bool {
        self.words()
            .get(i / 64)
            .is_some_and(|w| w & (1u64 << (i % 64)) != 0)
    }

    #[inline]
    pub fn insert(&mut self, i: usize) {
        self.words_mut()[i / 64] |= 1u64 << (i % 64);
    }

    #[inline]
    pub fn remove(&mut self, i: usize) {
        self.words_mut()[i / 64] &= !(1u64 << (i % 64));
    }

    #[inline]
    pub fn clear_all(&mut self) {
        for w in self.words_mut() {
            *w = 0;
        }
    }

    #[inline]
    pub fn copy_from(&mut self, other: &BitSet) {
        let dst = self.words_mut();
        let src = other.words();
        if dst.len() == src.len() {
            dst.copy_from_slice(src);
        } else {
            *self = other.clone();
        }
    }

    #[inline]
    pub fn union_with(&mut self, other: &BitSet) {
        for (a, b) in self.words_mut().iter_mut().zip(other.words()) {
            *a |= *b;
        }
    }

    #[inline]
    pub fn union_with_changed(&mut self, other: &BitSet) -> bool {
        let mut changed = false;
        for (a, b) in self.words_mut().iter_mut().zip(other.words()) {
            let next = *a | *b;
            if next != *a {
                *a = next;
                changed = true;
            }
        }
        changed
    }

    /// Sets `self = (live_out & !kill) | uses` and returns whether `self` changed.
    #[inline]
    pub fn assign_gen_kill_changed(
        &mut self,
        live_out: &BitSet,
        kill: &BitSet,
        uses: &BitSet,
    ) -> bool {
        let mut changed = false;
        let dst = self.words_mut();
        let out_w = live_out.words();
        let kill_w = kill.words();
        let uses_w = uses.words();
        for i in 0..dst.len() {
            let next = (out_w[i] & !kill_w[i]) | uses_w[i];
            if next != dst[i] {
                dst[i] = next;
                changed = true;
            }
        }
        changed
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.words().iter().all(|w| *w == 0)
    }

    #[inline]
    pub fn first_set(&self) -> Option<usize> {
        for (i, &w) in self.words().iter().enumerate() {
            if w != 0 {
                return Some(i * 64 + w.trailing_zeros() as usize);
            }
        }
        None
    }

    #[inline]
    pub fn iter(&self) -> BitSetIter<'_> {
        BitSetIter {
            words: self.words(),
            word_idx: 0,
            cur: 0,
        }
    }

    #[inline]
    pub fn is_subset(&self, other: &BitSet) -> bool {
        self.words()
            .iter()
            .zip(other.words())
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
        InitState {
            maybe_init: BitSet::new(paths.len()),
            maybe_uninit: BitSet::new_ones(paths.len()),
        }
    }
    /// The weakest state: every path may be either.
    pub fn top(paths: &MovePaths) -> Self {
        InitState {
            maybe_init: BitSet::new_ones(paths.len()),
            maybe_uninit: BitSet::new_ones(paths.len()),
        }
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
    #[inline]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    #[inline]
    pub fn insert(&mut self, key: K, value: V) {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(k, _)| k == &key) {
            *existing = value;
            return;
        }
        self.entries.push((key, value));
    }
    #[inline]
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries.retain(|(k, v)| keep(k, v));
    }
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
    /// Intersection: keep facts both sides agree on.
    #[inline]
    pub fn meet_with(&mut self, other: &Self) {
        self.meet_with_changed(other);
    }
    /// Intersection returning `true` iff any fact was removed.
    #[inline]
    pub fn meet_with_changed(&mut self, other: &Self) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        let before = self.entries.len();
        self.entries.retain(|(k, v)| other.get(k) == Some(v));
        self.entries.len() != before
    }
    /// `self` has every fact of `other` (so `self ⊑ other`).
    #[inline]
    pub fn covers(&self, other: &Self) -> bool {
        other.entries.iter().all(|(k, v)| self.get(k) == Some(v))
    }
}

/// Loans each local may carry (a may-fact; index 0 is `EXTERNAL`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Carries {
    sets: Vec<BitSet>,
    /// Bit `l` is set iff `!sets[l].is_empty()`.
    non_empty: BitSet,
    /// Sparse per-field loan breakdown for aggregate/closure values, sorted by
    /// local index; absence falls back conservatively to `sets[local]`.
    fields: Vec<(u32, Vec<BitSet>)>,
    loans: usize,
}

impl Carries {
    pub fn empty(locals: usize, loans: usize) -> Self {
        Carries {
            sets: vec![BitSet::new(loans); locals],
            non_empty: BitSet::new(locals),
            fields: Vec::new(),
            loans,
        }
    }
    #[inline]
    pub fn loans(&self) -> usize {
        self.loans
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.non_empty.is_empty()
    }
    #[inline]
    pub fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }
    #[inline]
    pub fn all_non_empty_in(&self, live: &BitSet) -> bool {
        self.non_empty.is_subset(live)
    }
    #[inline]
    pub fn non_empty_locals(&self) -> BitSetIter<'_> {
        self.non_empty.iter()
    }
    #[inline]
    pub fn field_locals(&self) -> impl Iterator<Item = usize> + '_ {
        self.fields.iter().map(|(l, _)| *l as usize)
    }
    #[inline]
    pub fn of(&self, local: Local) -> &BitSet {
        &self.sets[local.index()]
    }
    #[inline]
    pub fn get(&self, local: Local) -> Option<&BitSet> {
        self.sets.get(local.index())
    }
    #[inline]
    pub fn fields_of(&self, local: Local) -> Option<&[BitSet]> {
        if self.fields.is_empty() {
            return None;
        }
        let key = local.index() as u32;
        self.fields
            .binary_search_by_key(&key, |(l, _)| *l)
            .ok()
            .map(|pos| self.fields[pos].1.as_slice())
    }
    fn remove_fields(&mut self, local: Local) {
        if self.fields.is_empty() {
            return;
        }
        let key = local.index() as u32;
        if let Ok(pos) = self.fields.binary_search_by_key(&key, |(l, _)| *l) {
            self.fields.remove(pos);
        }
    }
    pub fn set(&mut self, local: Local, loans: BitSet) {
        let idx = local.index();
        if loans.is_empty() {
            self.non_empty.remove(idx);
        } else {
            self.non_empty.insert(idx);
        }
        self.sets[idx] = loans;
        self.remove_fields(local);
    }
    pub fn set_fields(&mut self, local: Local, field_loans: Vec<BitSet>) {
        let key = local.index() as u32;
        match self.fields.binary_search_by_key(&key, |(l, _)| *l) {
            Ok(pos) => self.fields[pos].1 = field_loans,
            Err(pos) => self.fields.insert(pos, (key, field_loans)),
        }
    }
    /// Replaces field `idx`'s carried loans and recomputes the local's union.
    /// Returns `false` if per-field tracking is unavailable for `local` or `idx`.
    pub fn set_field(&mut self, local: Local, idx: usize, loans: BitSet) -> bool {
        let key = local.index() as u32;
        let Ok(pos) = self.fields.binary_search_by_key(&key, |(l, _)| *l) else {
            return false;
        };
        let fields = &mut self.fields[pos].1;
        let Some(slot) = fields.get_mut(idx) else {
            return false;
        };
        *slot = loans;
        let mut union = BitSet::new(self.loans);
        for f in fields.iter() {
            union.union_with(f);
        }
        let lidx = local.index();
        if union.is_empty() {
            self.non_empty.remove(lidx);
        } else {
            self.non_empty.insert(lidx);
        }
        self.sets[lidx] = union;
        true
    }
    /// Unions `loans` into field `idx` and into the local's overall set.
    /// Returns `false` if per-field tracking is unavailable for `local` or `idx`.
    pub fn add_field(&mut self, local: Local, idx: usize, loans: &BitSet) -> bool {
        let key = local.index() as u32;
        let Ok(pos) = self.fields.binary_search_by_key(&key, |(l, _)| *l) else {
            return false;
        };
        let fields = &mut self.fields[pos].1;
        let Some(slot) = fields.get_mut(idx) else {
            return false;
        };
        slot.union_with(loans);
        let lidx = local.index();
        self.sets[lidx].union_with(loans);
        if !loans.is_empty() {
            self.non_empty.insert(lidx);
        }
        true
    }
    pub fn add(&mut self, local: Local, loans: &BitSet) {
        let idx = local.index();
        self.sets[idx].union_with(loans);
        if !loans.is_empty() {
            self.non_empty.insert(idx);
        }
        self.remove_fields(local);
    }
    pub fn insert_loan(&mut self, local: Local, loan: usize) {
        let idx = local.index();
        self.sets[idx].insert(loan);
        self.non_empty.insert(idx);
        self.remove_fields(local);
    }
    #[inline]
    pub fn clear(&mut self, local: Local) {
        let idx = local.index();
        if self.non_empty.contains(idx) {
            self.sets[idx].clear_all();
            self.non_empty.remove(idx);
        }
        self.remove_fields(local);
    }
    #[inline]
    pub fn locals(&self) -> usize {
        self.sets.len()
    }
    /// Every loan carried by any local.
    pub fn all(&self) -> BitSet {
        let mut out = BitSet::new(self.loans);
        for l in self.non_empty.iter() {
            out.union_with(&self.sets[l]);
        }
        out
    }
    fn join_with_changed(&mut self, other: &Carries) -> bool {
        let mut changed = false;
        for l in other.non_empty.iter() {
            if self.sets[l].union_with_changed(&other.sets[l]) {
                self.non_empty.insert(l);
                changed = true;
            }
        }
        if !self.fields.is_empty() {
            let before = self.fields.len();
            self.fields.retain_mut(|(l, fa)| {
                let Ok(pos) = other.fields.binary_search_by_key(l, |(ol, _)| *ol) else {
                    return false;
                };
                let fb = &other.fields[pos].1;
                if fa.len() != fb.len() {
                    return false;
                }
                for (x, y) in fa.iter_mut().zip(fb) {
                    if x.union_with_changed(y) {
                        changed = true;
                    }
                }
                true
            });
            if self.fields.len() != before {
                changed = true;
            }
        }
        changed
    }
    fn leq(&self, other: &Carries) -> bool {
        self.sets.len() == other.sets.len()
            && self.non_empty.is_subset(&other.non_empty)
            && self
                .non_empty
                .iter()
                .all(|l| self.sets[l].is_subset(&other.sets[l]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct State {
    pub init: InitState,
    /// Known active variant (Enum) or arm (error union; 1 = success).
    pub variants: MustFacts<Place, u32>,
    /// `local` holds the unchanged tag of `place`.
    pub tags: MustFacts<Local, Place>,
    /// The declared drop flag `local` equals "`place` is fully initialized".
    pub flags: MustFacts<Local, Place>,
    /// Loans each local may keep alive.
    pub carries: Carries,
}

impl State {
    /// Base for decoding claims: every init claim weakest, no must-facts,
    /// no carried loans (claims add them; edges must be covered by them).
    pub fn claims_base(paths: &MovePaths, locals: usize, loans: usize) -> Self {
        State {
            init: InitState::top(paths),
            variants: MustFacts::default(),
            tags: MustFacts::default(),
            flags: MustFacts::default(),
            carries: Carries::empty(locals, loans),
        }
    }
    pub fn join_with(&mut self, other: &State) {
        self.join_with_changed(other);
    }
    /// In-place join returning `true` iff `self` changed.
    pub fn join_with_changed(&mut self, other: &State) -> bool {
        let mut changed = false;
        changed |= self
            .init
            .maybe_init
            .union_with_changed(&other.init.maybe_init);
        changed |= self
            .init
            .maybe_uninit
            .union_with_changed(&other.init.maybe_uninit);
        changed |= self.variants.meet_with_changed(&other.variants);
        changed |= self.tags.meet_with_changed(&other.tags);
        changed |= self.flags.meet_with_changed(&other.flags);
        changed |= self.carries.join_with_changed(&other.carries);
        changed
    }
    /// `self ⊑ other`.
    pub fn leq(&self, other: &State) -> bool {
        self.init.maybe_init.is_subset(&other.init.maybe_init)
            && self.init.maybe_uninit.is_subset(&other.init.maybe_uninit)
            && self.variants.covers(&other.variants)
            && self.tags.covers(&other.tags)
            && self.flags.covers(&other.flags)
            && self.carries.leq(&other.carries)
    }
}
