//! Shared protocol-handle classification and state/history transfer rules.
//!
//! Used across the compiler pipeline (`zil-solve` / `zil-check`) so protocol
//! handle identity, history `H`, rollback detection, and `Replace` transitions
//! share one authoritative semantic definition rather than duplicating rules in
//! the verifier.

use zil_ir::body::{AdtId, AdtKind, FieldIdx, OpaqueTy, ProtocolStateSet, TyId, TyKind, TyTable};

/// Protocol-handle metadata shared by analysis and verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtoHandleInfo {
    /// Number of finite protocol states (`3` for built-in handles, `1..=64` for user enums).
    pub variant_count: usize,
    /// `Some((adt, field_idx))` when the handle is a single-variant ADT carrying
    /// an enum-typed `state` field; `None` for built-in opaque handles or direct enums.
    pub state_field: Option<(AdtId, FieldIdx)>,
    /// `true` iff the handle type itself is a direct finite-state enum (`*e = Variant`
    /// is an in-place state transition rather than a whole-instance resource replacement).
    pub is_direct_enum: bool,
}

/// Classifies whether `ty` (through up to 4 layers of `&`, `&mut`, `Managed`,
/// or `Desc`) is a built-in or user-defined finite-state protocol handle.
pub fn proto_handle_info(tys: &TyTable, mut ty: TyId) -> Option<ProtoHandleInfo> {
    for _ in 0..4 {
        match tys.get(ty) {
            Some(
                TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
            ) => ty = *pointee,
            Some(TyKind::Opaque(
                OpaqueTy::Task { .. }
                | OpaqueTy::Registration { .. }
                | OpaqueTy::AsyncRegistration { .. },
            )) => {
                return Some(ProtoHandleInfo {
                    variant_count: 3,
                    state_field: None,
                    is_direct_enum: false,
                });
            }
            Some(TyKind::Adt { adt, .. }) => {
                let def = tys.get_adt(*adt)?;
                if def.kind == AdtKind::Enum && !def.variants.is_empty() && def.variants.len() <= 64
                {
                    return Some(ProtoHandleInfo {
                        variant_count: def.variants.len(),
                        state_field: None,
                        is_direct_enum: true,
                    });
                }
                if def.variants.len() == 1
                    && let Some((fi, f)) = def.variants[0]
                        .fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| f.name == "state")
                    && let Some(TyKind::Adt { adt: state_adt, .. }) = tys.get(f.ty)
                    && let Some(state_def) = tys.get_adt(*state_adt)
                    && state_def.kind == AdtKind::Enum
                    && !state_def.variants.is_empty()
                    && state_def.variants.len() <= 64
                {
                    return Some(ProtoHandleInfo {
                        variant_count: state_def.variants.len(),
                        state_field: Some((*adt, FieldIdx(fi as u32))),
                        is_direct_enum: false,
                    });
                }
                return None;
            }
            Some(
                TyKind::Unit
                | TyKind::Bool
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::Never
                | TyKind::Tuple(_)
                | TyKind::Array { .. }
                | TyKind::Slice(_)
                | TyKind::ErrorUnion { .. }
                | TyKind::FnPtr { .. }
                | TyKind::Closure { .. }
                | TyKind::Dyn { .. }
                | TyKind::OwnedDyn { .. }
                | TyKind::Opaque(_)
                | TyKind::Param(_),
            )
            | None => return None,
        }
    }
    None
}

/// Complete protocol state, history `H`, and retirement obligation for one
/// tracked handle instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandleProtocolSlot {
    /// Possible current states of this handle instance.
    pub states: ProtocolStateSet,
    /// Origin state(s) when this handle instance began being tracked.
    pub origin: ProtocolStateSet,
    /// History `H`: union of all protocol states visited by this instance.
    pub history: ProtocolStateSet,
    /// States this instance has departed from on some reaching path.
    pub departed_from: ProtocolStateSet,
    /// True if any reaching path re-entered a state this instance previously departed from.
    pub rolled_back: bool,
    /// True iff this handle instance is proved retired on all reaching paths.
    pub retired: bool,
}

impl HandleProtocolSlot {
    /// Empty slot for non-protocol locals.
    pub const fn empty() -> Self {
        Self {
            states: ProtocolStateSet(0),
            origin: ProtocolStateSet(0),
            history: ProtocolStateSet(0),
            departed_from: ProtocolStateSet(0),
            rolled_back: false,
            retired: false,
        }
    }

    /// Freshly initialized handle instance starting at `states`.
    /// Carries no prior history, no rollback, and is not retired.
    pub const fn fresh(states: ProtocolStateSet) -> Self {
        Self {
            states,
            origin: states,
            history: states,
            departed_from: ProtocolStateSet(0),
            rolled_back: false,
            retired: false,
        }
    }

    /// Conservatively initialized handle instance whose state is unknown across
    /// all `variant_count` states and which is not proved retired.
    pub const fn unknown(variant_count: usize) -> Self {
        Self::fresh(ProtocolStateSet::all_variants(variant_count))
    }

    /// Lattice join across CFG merge edges.
    /// States/origin/history/departures union; `rolled_back` ORs; `retired` ANDs.
    pub fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        let merged_states = self.states.union(other.states);
        if merged_states != self.states {
            self.states = merged_states;
            changed = true;
        }
        let merged_origin = self.origin.union(other.origin);
        if merged_origin != self.origin {
            self.origin = merged_origin;
            changed = true;
        }
        let merged_history = self.history.union(other.history);
        if merged_history != self.history {
            self.history = merged_history;
            changed = true;
        }
        let merged_dep = self.departed_from.union(other.departed_from);
        if merged_dep != self.departed_from {
            self.departed_from = merged_dep;
            changed = true;
        }
        let rb = self.rolled_back || other.rolled_back;
        if rb != self.rolled_back {
            self.rolled_back = rb;
            changed = true;
        }
        let ret = self.retired && other.retired;
        if ret != self.retired {
            self.retired = ret;
            changed = true;
        }
        changed
    }

    /// Strong in-place state transition on the SAME handle instance.
    /// Records departures, detects path-correlated rollback `H`, and extends history.
    pub fn transition(&mut self, new_states: ProtocolStateSet) {
        let departed = self.states.subtract(new_states);
        self.departed_from = self.departed_from.union(departed);
        if !new_states.intersect(self.departed_from).is_empty() {
            self.rolled_back = true;
        }
        self.states = new_states;
        self.history = self.history.union(new_states);
    }

    /// Conservative weak in-place state transition on a handle that *may* alias
    /// the written handle instance.
    pub fn weak_transition(&mut self, new_states: ProtocolStateSet) {
        let aliased_departed = self.states.subtract(new_states);
        self.departed_from = self.departed_from.union(aliased_departed);
        if !new_states.intersect(self.departed_from).is_empty() {
            self.rolled_back = true;
        }
        self.states = self.states.union(new_states);
        self.history = self.history.union(new_states);
    }

    /// Strong `Replace` of a whole handle slot (`place <- incoming`, `old <- previous`):
    /// - Old and new instances NEVER share history `H`;
    /// - The complete old state, history `H`, and `retired` obligation move into the returned `old`;
    /// - The slot inherits `incoming`'s own state, history `H`, and `retired` status
    ///   (clearing any stale `retired` from the previous instance).
    pub fn replace_strong(&mut self, incoming: Self) -> Self {
        let old = *self;
        *self = incoming;
        old
    }

    /// Weak `Replace` on one candidate target in a `may_alias` set:
    /// - Returns the previous slot snapshot so the caller can join all candidate old values into `old`;
    /// - Updates this slot as `unchanged ⊔ incoming` without conflating old departures with
    ///   incoming states as an in-place rollback, while clearing `retired` if `incoming` is not retired.
    pub fn replace_weak_target(&mut self, incoming: Self) -> Self {
        let old_snapshot = *self;
        self.join(&incoming);
        old_snapshot
    }

    /// True if this handle instance exhibits a hidden history rollback `H`
    /// (either path-correlated re-entry into a departed state, or visiting a
    /// state outside `origin ∪ current_states`).
    pub const fn has_hidden_rollback(&self) -> bool {
        self.rolled_back || !self.history.is_subset_of(self.origin.union(self.states))
    }

    /// Conservatively widens state & marks rollback after an unknown mutable call.
    pub fn widen_unknown_mutation(&mut self, variant_count: usize) {
        let all = ProtocolStateSet::all_variants(variant_count);
        self.transition(all);
        self.rolled_back = true;
    }
}

/// Classifies whether `ty` (through up to 4 layers of `&`, `&mut`, `Managed`,
/// or `Desc`) is an `OwnedStore<E>` whose element type `E` is a finite-state
/// protocol handle, returning `E`'s [`ProtoHandleInfo`].
pub fn store_element_proto_info(tys: &TyTable, mut ty: TyId) -> Option<ProtoHandleInfo> {
    for _ in 0..4 {
        match tys.get(ty) {
            Some(
                TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
            ) => ty = *pointee,
            Some(TyKind::Opaque(OpaqueTy::OwnedStore { element })) => {
                return proto_handle_info(tys, *element);
            }
            Some(_) | None => return None,
        }
    }
    None
}

/// Abstract protocol-state summary for elements stored inside an `OwnedStore<E>`.
///
/// Distinguishes a statically empty store (`may_be_non_empty == false`, e.g. right
/// after `StoreNew`) from a potentially populated store, and fails closed when
/// popping from an empty store so an empty bitmask can never vacuously satisfy a
/// protocol precondition or `Closed` check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreProtocolSummary {
    /// Joined protocol state, history `H`, and retirement status across stored elements.
    pub elements: HandleProtocolSlot,
    /// `false` iff the store is proved empty on all reaching paths.
    pub may_be_non_empty: bool,
}

impl StoreProtocolSummary {
    /// Freshly constructed empty store (`StoreNew`).
    pub const fn empty_store() -> Self {
        Self {
            elements: HandleProtocolSlot::empty(),
            may_be_non_empty: false,
        }
    }

    /// Conservatively initialized store (e.g. entry parameter) whose elements may
    /// be in any of `variant_count` protocol states.
    pub const fn unknown_store(variant_count: usize) -> Self {
        Self {
            elements: HandleProtocolSlot::unknown(variant_count),
            may_be_non_empty: true,
        }
    }

    /// Lattice join across CFG merge edges.
    pub fn join(&mut self, other: &Self) -> bool {
        if !other.may_be_non_empty {
            return false;
        }
        if !self.may_be_non_empty {
            *self = *other;
            return true;
        }
        self.elements.join(&other.elements)
    }

    /// Applies a successful `StorePush(store, incoming)` transition.
    pub fn on_push_success(&mut self, incoming: HandleProtocolSlot) {
        if !self.may_be_non_empty {
            self.elements = incoming;
            self.may_be_non_empty = true;
        } else {
            self.elements.join(&incoming);
        }
    }

    /// Returns the protocol slot for an element extracted via `StorePop.ok` or
    /// borrowed via `StoreBorrow`. Fails closed to `unknown(variant_count)` when
    /// the store has no known non-empty state so `ProtocolStateSet(0)` can never
    /// vacuously satisfy a precondition.
    pub const fn element_slot(&self, variant_count: usize) -> HandleProtocolSlot {
        if self.may_be_non_empty && !self.elements.states.is_empty() {
            self.elements
        } else {
            HandleProtocolSlot::unknown(variant_count)
        }
    }

    /// Incorporates an actual mutation performed through an exclusive `StoreBorrow`.
    pub fn on_borrowed_mutation(&mut self, updated: HandleProtocolSlot) {
        self.on_push_success(updated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: ProtocolStateSet = ProtocolStateSet::single(0);
    const OPEN: ProtocolStateSet = ProtocolStateSet::single(1);
    const CLOSED: ProtocolStateSet = ProtocolStateSet::single(2);

    #[test]
    fn replace_strong_isolates_history_transfers_old_and_clears_retired() {
        // Old handle started Open, transitioned to Closed, and was retired.
        let mut slot = HandleProtocolSlot::fresh(OPEN);
        slot.transition(CLOSED);
        slot.retired = true;
        assert!(!slot.has_hidden_rollback());

        // Incoming handle started Init and monotonically advanced to Open (not retired).
        let mut incoming = HandleProtocolSlot::fresh(INIT);
        incoming.transition(OPEN);
        assert!(!incoming.has_hidden_rollback());

        let old = slot.replace_strong(incoming);

        // Old resource receives full old state, history H, departure, and retired flag.
        assert_eq!(old.states, CLOSED);
        assert_eq!(old.origin, OPEN);
        assert_eq!(old.history, OPEN.union(CLOSED));
        assert_eq!(old.departed_from, OPEN);
        assert!(!old.rolled_back);
        assert!(old.retired);

        // New resource in slot inherits only its own state/history and clears old retired.
        assert_eq!(slot.states, OPEN);
        assert_eq!(slot.origin, INIT);
        assert_eq!(slot.history, INIT.union(OPEN));
        assert_eq!(slot.departed_from, INIT);
        assert!(!slot.rolled_back);
        assert!(!slot.retired);
        assert!(!slot.has_hidden_rollback());
    }

    #[test]
    fn replace_strong_preserves_incoming_rollback_and_old_rollback_independently() {
        // Old handle had an internal rollback Open -> Closed -> Open.
        let mut slot = HandleProtocolSlot::fresh(OPEN);
        slot.transition(CLOSED);
        slot.transition(OPEN);
        assert!(slot.has_hidden_rollback());

        // Clean incoming Open handle replaces it.
        let clean = HandleProtocolSlot::fresh(OPEN);
        let old = slot.replace_strong(clean);
        assert!(old.has_hidden_rollback());
        assert!(!slot.has_hidden_rollback());

        // Now replace clean slot with an incoming handle that itself rolled back.
        let mut dirty = HandleProtocolSlot::fresh(OPEN);
        dirty.transition(CLOSED);
        dirty.transition(OPEN);
        let old2 = slot.replace_strong(dirty);
        assert!(!old2.has_hidden_rollback());
        assert!(slot.has_hidden_rollback());
    }

    #[test]
    fn replace_weak_target_clears_retired_without_spurious_rollback() {
        let mut slot = HandleProtocolSlot::fresh(OPEN);
        slot.transition(CLOSED);
        slot.retired = true;

        let incoming = HandleProtocolSlot::fresh(OPEN);
        let snap = slot.replace_weak_target(incoming);

        assert_eq!(snap.states, CLOSED);
        assert!(snap.retired);
        assert_eq!(slot.states, OPEN.union(CLOSED));
        assert!(
            !slot.retired,
            "weak replace with unretired incoming must clear retired"
        );
        assert!(
            !slot.rolled_back,
            "weak replace must not conflate old departures with incoming states as rollback"
        );
    }

    #[test]
    fn store_summary_distinguishes_empty_from_populated_and_fails_closed() {
        let mut s = StoreProtocolSummary::empty_store();
        assert!(!s.may_be_non_empty);
        // Popping from a proved-empty store must NEVER return ProtocolStateSet(0)
        // which would vacuously satisfy `is_subset_of(CLOSED)`.
        let empty_pop = s.element_slot(3);
        assert_eq!(empty_pop.states, ProtocolStateSet::all_variants(3));
        assert!(!empty_pop.retired);

        // After a successful push of an Open handle, element_slot is exact Open.
        s.on_push_success(HandleProtocolSlot::fresh(OPEN));
        assert!(s.may_be_non_empty);
        assert_eq!(s.element_slot(3).states, OPEN);

        // Joining with an empty branch keeps Open; joining with a Closed push widens to Open|Closed.
        let empty_branch = StoreProtocolSummary::empty_store();
        assert!(!s.join(&empty_branch));
        assert_eq!(s.element_slot(3).states, OPEN);

        let mut other = StoreProtocolSummary::empty_store();
        other.on_push_success(HandleProtocolSlot::fresh(CLOSED));
        assert!(s.join(&other));
        assert_eq!(s.element_slot(3).states, OPEN.union(CLOSED));
    }

    #[test]
    fn store_summary_records_borrowed_mutation_without_spurious_rollback() {
        let mut s = StoreProtocolSummary::empty_store();
        s.on_push_success(HandleProtocolSlot::fresh(OPEN));
        let mut borrowed = s.element_slot(3);
        borrowed.transition(CLOSED);
        s.on_borrowed_mutation(borrowed);
        assert_eq!(s.element_slot(3).states, OPEN.union(CLOSED));
        assert!(!s.element_slot(3).has_hidden_rollback());
    }
}
