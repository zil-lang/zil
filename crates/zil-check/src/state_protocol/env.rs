//! Lattice environments, entry capability scanner, and pure type/projection helpers
//! for forward CFG protocol-state verification.

use std::collections::{BTreeSet, HashMap};

use zil_ir::body::{
    AdtKind, ContractFieldPath, DynImpl, FieldIdx, Local, Mutability, OpaqueTy, Operand, Place,
    PlaceRoot, Projection, ProtocolStateSet, TyId, TyKind, TyTable,
};
use zil_semantics::{HandleProtocolSlot, StoreProtocolSummary};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DiscrTarget {
    Poll(Local),
    ErrorUnion(Local),
    HandleState(usize),
    ValidCheck { handle: usize, inverted: bool },
}

/// Branch-sensitive outcome of a `StorePush` or `StorePop` call returning an `ErrorUnion`.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum StoreOpOutcome {
    /// `StorePush(store, elem) -> ErrorUnion<(), elem>`:
    /// On `.err(elem)` (OOM), restores `pre_summaries` and retracts `newly_invalidated`.
    Push {
        pre_summaries: Vec<(usize, StoreProtocolSummary)>,
        newly_invalidated: Vec<(usize, Vec<FieldIdx>)>,
    },
    /// `StorePop(store) -> ErrorUnion<elem, ()>`:
    /// On `.err(())` (empty store), retracts `newly_invalidated` and proves `unique_store` empty.
    Pop {
        unique_store: Option<usize>,
        tracked_stores: Vec<usize>,
        newly_invalidated: Vec<(usize, Vec<FieldIdx>)>,
    },
}

pub(super) type EuTransitions = Vec<(usize, Option<ProtocolStateSet>, Option<ProtocolStateSet>)>;
pub(super) type ParamAliasSet = BTreeSet<(usize, Vec<FieldIdx>)>;
pub(super) type HandleCapSet = BTreeSet<(Vec<FieldIdx>, usize)>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum NestedCapTarget {
    /// Exact stripped resource/ADT type reachable at a field projection.
    Exact(TyId),
    /// Unknown or polymorphic target (`Param`, unknown `Dyn`, depth overflow).
    Wildcard,
}

pub(super) type NestedCapSet = BTreeSet<(Vec<FieldIdx>, NestedCapTarget)>;

/// Alias and storage-root provenance tracked per local.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct AliasEnv {
    /// May-alias map: local -> all `(param_idx, field_prefix)` it may alias on any reaching path (unioned on join).
    pub(super) param_alias: HashMap<Local, ParamAliasSet>,
    /// Must-alias map: local -> unique protocol handle index (intersected on join; fail-closed when ambiguous).
    pub(super) alias: HashMap<Local, usize>,
    /// May-alias map: local -> all protocol handle indices it may alias (unioned on join; conservatively widened on writes).
    pub(super) may_alias: HashMap<Local, BTreeSet<usize>>,
    /// May-map: local -> all root local indices whose storage/resources it may alias or borrow from (unioned on join).
    pub(super) local_roots: HashMap<Local, BTreeSet<usize>>,
    /// May-map: borrowed element handle slot index -> parent `OwnedStore` root local indices.
    pub(super) store_borrow_parents: HashMap<usize, BTreeSet<usize>>,
}

impl AliasEnv {
    pub(super) fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (k, other_set) in &other.param_alias {
            let entry = self.param_alias.entry(*k).or_default();
            for item in other_set {
                if entry.insert(item.clone()) {
                    changed = true;
                }
            }
        }
        for (k, other_set) in &other.may_alias {
            let entry = self.may_alias.entry(*k).or_default();
            for &h in other_set {
                if entry.insert(h) {
                    changed = true;
                }
            }
        }
        for (k, other_set) in &other.local_roots {
            let entry = self.local_roots.entry(*k).or_default();
            for &r in other_set {
                if entry.insert(r) {
                    changed = true;
                }
            }
        }
        for (k, other_set) in &other.store_borrow_parents {
            let entry = self.store_borrow_parents.entry(*k).or_default();
            for &s in other_set {
                if entry.insert(s) {
                    changed = true;
                }
            }
        }
        let prev_alias = self.alias.len();
        self.alias.retain(|k, v| other.alias.get(k) == Some(v));
        if self.alias.len() != prev_alias {
            changed = true;
        }
        changed
    }

    pub(super) fn clear_local(&mut self, local: Local) {
        self.alias.remove(&local);
        self.may_alias.remove(&local);
        self.local_roots.remove(&local);
        self.param_alias.remove(&local);
        self.store_borrow_parents.remove(&local.index());
    }
}

/// Reachable mutable capabilities carried inside closures, `dyn`/`OwnedDyn`, or aggregates.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct CapabilityEnv {
    /// May-map: local -> parameter projections reachable via captured `&mut`/`Desc`/`Managed`.
    pub(super) mut_cap_params: HashMap<Local, ParamAliasSet>,
    /// May-map: local -> `(field_path, handle_idx)` protocol handles reachable via nested `&mut`/`Desc`/`Managed`/`Resource`.
    pub(super) mut_cap_handles: HashMap<Local, HandleCapSet>,
}

impl CapabilityEnv {
    pub(super) fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (k, other_set) in &other.mut_cap_params {
            let entry = self.mut_cap_params.entry(*k).or_default();
            for item in other_set {
                if entry.insert(item.clone()) {
                    changed = true;
                }
            }
        }
        for (k, other_set) in &other.mut_cap_handles {
            let entry = self.mut_cap_handles.entry(*k).or_default();
            for item in other_set {
                if entry.insert(item.clone()) {
                    changed = true;
                }
            }
        }
        changed
    }

    pub(super) fn clear_local(&mut self, local: Local) {
        self.mut_cap_params.remove(&local);
        self.mut_cap_handles.remove(&local);
    }
}

/// Must-fact branch refinement maps (all intersected on CFG join so ambiguous paths fail closed).
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct RefinementEnv {
    /// Must-map: local holding copied `handle.state` field -> root handle index.
    pub(super) state_field_of: HashMap<Local, usize>,
    /// Must-map: exact enum variant for locals constructed via `AggregateKind::Adt`.
    pub(super) enum_variant: HashMap<Local, u32>,
    /// Must-map: exact ErrorUnion arm (`true = ok`, `false = err`).
    pub(super) error_arm_const: HashMap<Local, bool>,
    /// Must-map: Poll local -> `(handle_idx, pending_states, ready_states)`.
    pub(super) poll_of: HashMap<Local, (usize, ProtocolStateSet, ProtocolStateSet)>,
    /// Must-map: ErrorUnion local -> `(handle_idx, ok_states, err_states)` transitions.
    pub(super) eu_of: HashMap<Local, EuTransitions>,
    /// Must-map: `(eu_local, is_ok)` -> protocol handle slot of the payload carried in that arm.
    pub(super) eu_payload_slot: HashMap<(Local, bool), HandleProtocolSlot>,
    /// Must-map: `StorePush`/`StorePop` ErrorUnion local -> branch-sensitive store recovery facts.
    pub(super) store_op_of: HashMap<Local, StoreOpOutcome>,
    /// Must-map: discriminant/tag local -> target.
    pub(super) discr_of: HashMap<Local, DiscrTarget>,
}

impl RefinementEnv {
    pub(super) fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        let prev_sf = self.state_field_of.len();
        self.state_field_of
            .retain(|k, v| other.state_field_of.get(k) == Some(v));
        if self.state_field_of.len() != prev_sf {
            changed = true;
        }
        let prev_ev = self.enum_variant.len();
        self.enum_variant
            .retain(|k, v| other.enum_variant.get(k) == Some(v));
        if self.enum_variant.len() != prev_ev {
            changed = true;
        }
        let prev_ea = self.error_arm_const.len();
        self.error_arm_const
            .retain(|k, v| other.error_arm_const.get(k) == Some(v));
        if self.error_arm_const.len() != prev_ea {
            changed = true;
        }
        let prev_poll = self.poll_of.len();
        self.poll_of.retain(|k, v| other.poll_of.get(k) == Some(v));
        if self.poll_of.len() != prev_poll {
            changed = true;
        }
        let prev_eu = self.eu_of.len();
        self.eu_of.retain(|k, v| other.eu_of.get(k) == Some(v));
        if self.eu_of.len() != prev_eu {
            changed = true;
        }
        let prev_eps = self.eu_payload_slot.len();
        self.eu_payload_slot.retain(|k, v| {
            if let Some(ov) = other.eu_payload_slot.get(k) {
                if v.join(ov) {
                    changed = true;
                }
                true
            } else {
                false
            }
        });
        if self.eu_payload_slot.len() != prev_eps {
            changed = true;
        }
        let prev_so = self.store_op_of.len();
        self.store_op_of
            .retain(|k, v| other.store_op_of.get(k) == Some(v));
        if self.store_op_of.len() != prev_so {
            changed = true;
        }
        let prev_discr = self.discr_of.len();
        self.discr_of
            .retain(|k, v| other.discr_of.get(k) == Some(v));
        if self.discr_of.len() != prev_discr {
            changed = true;
        }
        changed
    }

    pub(super) fn clear_local(&mut self, local: Local) {
        self.state_field_of.remove(&local);
        self.enum_variant.remove(&local);
        self.error_arm_const.remove(&local);
        self.poll_of.remove(&local);
        self.eu_of.remove(&local);
        self.eu_payload_slot.remove(&(local, true));
        self.eu_payload_slot.remove(&(local, false));
        self.store_op_of.remove(&local);
        self.discr_of.remove(&local);
    }

    pub(super) fn invalidate_handle(&mut self, h_idx: usize) {
        self.poll_of.retain(|_, (h, _, _)| *h != h_idx);
        self.eu_of.retain(|_, entries| {
            entries.retain(|(h, _, _)| *h != h_idx);
            !entries.is_empty()
        });
        self.state_field_of.retain(|_, h| *h != h_idx);
        self.discr_of.retain(|_, t| {
            !matches!(
                t,
                DiscrTarget::HandleState(h) | DiscrTarget::ValidCheck { handle: h, .. }
                    if *h == h_idx
            )
        });
    }

    /// Invalidates any pending `StorePush`/`StorePop` branch recovery referencing `s_idx`
    /// once `s_idx` is subsequently mutated before the branch is inspected.
    pub(super) fn invalidate_store(&mut self, s_idx: usize) {
        self.store_op_of.retain(|_, outcome| match outcome {
            StoreOpOutcome::Push { pre_summaries, .. } => {
                !pre_summaries.iter().any(|(s, _)| *s == s_idx)
            }
            StoreOpOutcome::Pop { tracked_stores, .. } => !tracked_stores.contains(&s_idx),
        });
    }

    /// Ensures a later independent invalidation of `item` cannot be retracted by
    /// an earlier pending `StorePush`/`StorePop` `.err` branch.
    pub(super) fn on_param_invalidated(&mut self, item: &(usize, Vec<FieldIdx>)) {
        for outcome in self.store_op_of.values_mut() {
            match outcome {
                StoreOpOutcome::Push {
                    newly_invalidated, ..
                }
                | StoreOpOutcome::Pop {
                    newly_invalidated, ..
                } => {
                    newly_invalidated.retain(|existing| existing != item);
                }
            }
        }
    }
}

/// Complete forward-CFG abstract state for protocol-state contract verification.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ProtoState {
    /// Per-local protocol handle state, history `H`, rollback flag, and retired status.
    pub(super) slots: Vec<HandleProtocolSlot>,
    /// Per-local `OwnedStore<E>` element protocol summary (indexed by local index).
    pub(super) store_slots: Vec<StoreProtocolSummary>,
    /// Parameter projections `(param_idx, field_path)` invalidated on any path reaching here.
    pub(super) invalidated: ParamAliasSet,
    pub(super) aliases: AliasEnv,
    pub(super) caps: CapabilityEnv,
    pub(super) refinements: RefinementEnv,
}

impl ProtoState {
    pub(super) fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for i in 0..self.slots.len() {
            if self.slots[i].join(&other.slots[i]) {
                changed = true;
            }
        }
        for i in 0..self.store_slots.len() {
            if self.store_slots[i].join(&other.store_slots[i]) {
                changed = true;
            }
        }
        for item in &other.invalidated {
            if self.invalidated.insert(item.clone()) {
                changed = true;
            }
        }
        if self.aliases.join(&other.aliases) {
            changed = true;
        }
        if self.caps.join(&other.caps) {
            changed = true;
        }
        if self.refinements.join(&other.refinements) {
            changed = true;
        }
        changed
    }

    pub(super) fn clear_local(&mut self, local: Local) {
        self.aliases.clear_local(local);
        self.caps.clear_local(local);
        self.refinements.clear_local(local);
    }

    pub(super) fn insert_invalidation(&mut self, item: (usize, Vec<FieldIdx>)) {
        self.refinements.on_param_invalidated(&item);
        self.invalidated.insert(item);
    }

    /// Propagates an actual mutation performed on borrowed handle slot `h_idx`
    /// back into any parent `OwnedStore` summary it was borrowed from.
    pub(super) fn sync_store_borrow_write(&mut self, h_idx: usize) {
        if let Some(parents) = self.aliases.store_borrow_parents.get(&h_idx).cloned() {
            let slot = self.slots[h_idx];
            for s_idx in parents {
                if let Some(summary) = self.store_slots.get_mut(s_idx) {
                    summary.on_borrowed_mutation(slot);
                }
                self.refinements.invalidate_store(s_idx);
            }
        }
    }

    pub(super) fn set_handle_state(&mut self, h_idx: usize, new_state: ProtocolStateSet) {
        self.slots[h_idx].transition(new_state);
        self.sync_store_borrow_write(h_idx);
    }

    /// Conservative weak update for a handle that *may* alias the written handle.
    /// Accounts for both the non-aliasing branch (state unchanged) and the aliasing branch
    /// (transition to `new_state`, recording departures and potential rollback).
    pub(super) fn weak_update_handle_state(&mut self, h_idx: usize, new_state: ProtocolStateSet) {
        self.slots[h_idx].weak_transition(new_state);
        self.sync_store_borrow_write(h_idx);
    }

    pub(super) fn resolve_param_places(&self, place: &Place) -> ParamAliasSet {
        let mut out = ParamAliasSet::new();
        let PlaceRoot::Local(rl) = place.root else {
            return out;
        };
        let Some(prefixes) = self.aliases.param_alias.get(&rl) else {
            return out;
        };
        for (p_idx, prefix) in prefixes {
            let mut path = prefix.clone();
            for proj in &place.projections {
                match proj {
                    Projection::Deref => {}
                    Projection::Field(f) => path.push(f.field),
                    Projection::Index(_)
                    | Projection::TupleField(_)
                    | Projection::ErrorArm(_)
                    | Projection::ConstIndex(_) => break,
                }
            }
            out.insert((*p_idx, path));
        }
        out
    }

    pub(super) fn record_place_invalidation(&mut self, place: &Place) {
        let PlaceRoot::Local(rl) = place.root else {
            return;
        };
        for (p_idx, path) in self.resolve_param_places(place) {
            if !place.projections.is_empty() || rl.index() != p_idx {
                self.insert_invalidation((p_idx, path));
            }
        }
    }

    pub(super) fn resolve_handle(&self, op: &Operand) -> Option<usize> {
        match op {
            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => match p.root {
                PlaceRoot::Local(l) => self.aliases.alias.get(&l).copied(),
                PlaceRoot::Capture(_) => None,
            },
            Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => None,
        }
    }

    pub(super) fn invalidate_handle(&mut self, h_idx: usize) {
        self.refinements.invalidate_handle(h_idx);
    }
}

/// Scans entry parameter types across the complete type graph (`Desc`, `&mut`, `Managed`,
/// `Resource`, `Closure`, `Dyn`/`OwnedDyn`, `Opaque`, `Param`) to discover nested mutable
/// capabilities and aliasable resource handles at function entry.
pub(super) struct EntryCapScan<'a> {
    pub(super) tys: &'a TyTable,
    pub(super) dyn_impls: &'a [DynImpl],
    pub(super) param_idx: usize,
}

impl EntryCapScan<'_> {
    fn target_of(&self, mut ty: TyId) -> NestedCapTarget {
        for _ in 0..4 {
            match self.tys.get(ty) {
                Some(
                    TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
                ) => ty = *pointee,
                _ => break,
            }
        }
        match self.tys.get(ty) {
            Some(TyKind::Param(_) | TyKind::Dyn { .. } | TyKind::OwnedDyn { .. }) | None => {
                NestedCapTarget::Wildcard
            }
            Some(_) => NestedCapTarget::Exact(ty),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_container_member(
        &self,
        ty: TyId,
        prefix: &[FieldIdx],
        desc_only: bool,
        behind_indirection: bool,
        depth: u32,
        visiting: &mut Vec<TyId>,
        out_params: &mut ParamAliasSet,
        out_nested: &mut NestedCapSet,
    ) {
        let mut sub_params = BTreeSet::new();
        let mut sub_nested = NestedCapSet::new();
        let mut tmp_prefix = Vec::new();
        self.scan(
            ty,
            &mut tmp_prefix,
            desc_only,
            behind_indirection,
            depth + 1,
            visiting,
            &mut sub_params,
            &mut sub_nested,
        );
        if !sub_params.is_empty() {
            out_params.insert((self.param_idx, prefix.to_vec()));
        }
        for (_, target) in sub_nested {
            out_nested.insert((prefix.to_vec(), target));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_dyn_impls(
        &self,
        dyn_id: zil_ir::body::DynId,
        prefix: &[FieldIdx],
        desc_only: bool,
        behind_indirection: bool,
        depth: u32,
        visiting: &mut Vec<TyId>,
        out_params: &mut ParamAliasSet,
        out_nested: &mut NestedCapSet,
    ) {
        let mut found_any = false;
        for di in self.dyn_impls {
            if di.dyn_id == dyn_id {
                found_any = true;
                self.scan_container_member(
                    di.concrete_ty,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
        }
        // Fail closed when no concrete implementations are known in the module.
        if !found_any {
            out_params.insert((self.param_idx, prefix.to_vec()));
            out_nested.insert((prefix.to_vec(), NestedCapTarget::Wildcard));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn scan(
        &self,
        ty: TyId,
        prefix: &mut Vec<FieldIdx>,
        desc_only: bool,
        behind_indirection: bool,
        depth: u32,
        visiting: &mut Vec<TyId>,
        out_params: &mut ParamAliasSet,
        out_nested: &mut NestedCapSet,
    ) {
        if visiting.contains(&ty) {
            return;
        }
        if depth > 8 {
            out_params.insert((self.param_idx, prefix.clone()));
            out_nested.insert((prefix.clone(), NestedCapTarget::Wildcard));
            return;
        }
        let Some(kind) = self.tys.get(ty) else {
            out_params.insert((self.param_idx, prefix.clone()));
            out_nested.insert((prefix.clone(), NestedCapTarget::Wildcard));
            return;
        };
        visiting.push(ty);
        match kind {
            TyKind::Desc(inner) => {
                out_params.insert((self.param_idx, prefix.clone()));
                out_nested.insert((prefix.clone(), self.target_of(*inner)));
                // Also discover any nested resources/descriptors inside `*inner`.
                let mut scratch_params = BTreeSet::new();
                self.scan(
                    *inner,
                    prefix,
                    true,
                    true,
                    depth + 1,
                    visiting,
                    &mut scratch_params,
                    out_nested,
                );
            }
            TyKind::Ref {
                mutability: Mutability::Exclusive,
                pointee,
                ..
            } => {
                if !desc_only {
                    out_params.insert((self.param_idx, prefix.clone()));
                }
                // Even behind `&mut S`, nested `Desc`/`&Managed`/`Param`/`Dyn`/`Opaque`
                // fields can alias another shared/descriptor parameter.
                let mut scratch_params = BTreeSet::new();
                self.scan(
                    *pointee,
                    prefix,
                    true,
                    false,
                    depth + 1,
                    visiting,
                    &mut scratch_params,
                    out_nested,
                );
            }
            TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            } => {
                // Shared references cannot mutate through `&(&mut T)`, but may still
                // mutate through interior `Desc`, `Managed`, `Resource`, unconstrained
                // `Param`, `Dyn`/`OwnedDyn`, or `Opaque` payloads, and any nested
                // resource reachable through `&` can alias another shared/`Desc` parameter.
                self.scan(
                    *pointee,
                    prefix,
                    true,
                    true,
                    depth + 1,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
            TyKind::Managed(inner) => {
                // `Managed<R>` (even behind a shared `&Managed<R>`) supports auto-describe
                // (`Rvalue::Describe`) to obtain `Desc<R>`, granting mutation capability.
                out_params.insert((self.param_idx, prefix.clone()));
                if behind_indirection {
                    out_nested.insert((prefix.clone(), self.target_of(*inner)));
                }
                let mut scratch_params = BTreeSet::new();
                self.scan(
                    *inner,
                    prefix,
                    true,
                    behind_indirection,
                    depth + 1,
                    visiting,
                    &mut scratch_params,
                    out_nested,
                );
            }
            TyKind::Adt { adt, args } => {
                let Some(def) = self.tys.get_adt(*adt) else {
                    out_params.insert((self.param_idx, prefix.clone()));
                    out_nested.insert((prefix.clone(), NestedCapTarget::Wildcard));
                    visiting.pop();
                    return;
                };
                if def.kind == AdtKind::Resource {
                    // `Rvalue::Describe` accepts `AdtKind::Resource` directly (and behind `&`),
                    // granting a `Desc<Resource>` capability.
                    out_params.insert((self.param_idx, prefix.clone()));
                    if behind_indirection {
                        out_nested.insert((prefix.clone(), NestedCapTarget::Exact(ty)));
                    }
                    if def.variants.len() == 1 {
                        let mut scratch_params = BTreeSet::new();
                        for (fi, f) in def.variants[0].fields.iter().enumerate() {
                            let fty = self.tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                            let pushed = prefix.len() < ContractFieldPath::MAX_DEPTH;
                            if pushed {
                                prefix.push(FieldIdx(fi as u32));
                            }
                            self.scan(
                                fty,
                                prefix,
                                true,
                                behind_indirection,
                                depth + 1,
                                visiting,
                                &mut scratch_params,
                                out_nested,
                            );
                            if pushed {
                                prefix.pop();
                            }
                        }
                    }
                    visiting.pop();
                    return;
                }
                if def.variants.len() == 1 {
                    for (fi, f) in def.variants[0].fields.iter().enumerate() {
                        let fty = self.tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                        let pushed = prefix.len() < ContractFieldPath::MAX_DEPTH;
                        if pushed {
                            prefix.push(FieldIdx(fi as u32));
                        }
                        self.scan(
                            fty,
                            prefix,
                            desc_only,
                            behind_indirection,
                            depth + 1,
                            visiting,
                            out_params,
                            out_nested,
                        );
                        if pushed {
                            prefix.pop();
                        }
                    }
                } else {
                    for v in &def.variants {
                        for f in &v.fields {
                            let fty = self.tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                            self.scan_container_member(
                                fty,
                                prefix,
                                desc_only,
                                behind_indirection,
                                depth,
                                visiting,
                                out_params,
                                out_nested,
                            );
                        }
                    }
                }
            }
            TyKind::Tuple(elems) => {
                for &elem in elems {
                    self.scan_container_member(
                        elem,
                        prefix,
                        desc_only,
                        behind_indirection,
                        depth,
                        visiting,
                        out_params,
                        out_nested,
                    );
                }
            }
            TyKind::Array { element: elem, .. } | TyKind::Slice(elem) => {
                self.scan_container_member(
                    *elem,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
            TyKind::ErrorUnion { ok, err } => {
                self.scan_container_member(
                    *ok,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
                self.scan_container_member(
                    *err,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
            TyKind::Closure { def, args } => {
                let Some(cdef) = self.tys.closure(*def) else {
                    out_params.insert((self.param_idx, prefix.clone()));
                    out_nested.insert((prefix.clone(), NestedCapTarget::Wildcard));
                    visiting.pop();
                    return;
                };
                for &cty in &cdef.captures {
                    let cty = self.tys.subst_lookup(cty, *args).unwrap_or(cty);
                    self.scan_container_member(
                        cty,
                        prefix,
                        desc_only,
                        behind_indirection,
                        depth,
                        visiting,
                        out_params,
                        out_nested,
                    );
                }
            }
            TyKind::Dyn { def: dyn_id, .. } => {
                self.scan_dyn_impls(
                    *dyn_id,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
            TyKind::OwnedDyn {
                def: dyn_id,
                context,
                ..
            } => {
                if let Some(ctx_ty) = context {
                    self.scan_container_member(
                        *ctx_ty,
                        prefix,
                        desc_only,
                        behind_indirection,
                        depth,
                        visiting,
                        out_params,
                        out_nested,
                    );
                }
                self.scan_dyn_impls(
                    *dyn_id,
                    prefix,
                    desc_only,
                    behind_indirection,
                    depth,
                    visiting,
                    out_params,
                    out_nested,
                );
            }
            TyKind::Opaque(opaque) => match opaque {
                OpaqueTy::ForeignHandle | OpaqueTy::ForeignCallback { .. } => {}
                OpaqueTy::OwnedStore { element } => {
                    out_params.insert((self.param_idx, prefix.clone()));
                    self.scan_container_member(
                        *element,
                        prefix,
                        true,
                        behind_indirection,
                        depth,
                        visiting,
                        out_params,
                        out_nested,
                    );
                }
                OpaqueTy::ByteBuffer | OpaqueTy::SyncCell(_) | OpaqueTy::CWideString => {
                    // `ByteBuffer` and `SyncCell` admit interior mutation through shared
                    // references / atomics, and `CWideString` has opaque layout (fail-closed).
                    out_params.insert((self.param_idx, prefix.clone()));
                }
                OpaqueTy::Mutex { payload } | OpaqueTy::MutexGuard { payload, .. } => {
                    self.scan_container_member(
                        *payload, prefix, false, true, depth, visiting, out_params, out_nested,
                    );
                }
                OpaqueTy::Registration { environment }
                | OpaqueTy::AsyncRegistration { environment } => {
                    self.scan_container_member(
                        *environment,
                        prefix,
                        false,
                        true,
                        depth,
                        visiting,
                        out_params,
                        out_nested,
                    );
                }
                OpaqueTy::Task { result, context } => {
                    self.scan_container_member(
                        *result, prefix, false, true, depth, visiting, out_params, out_nested,
                    );
                    if let Some(ctx_ty) = context {
                        self.scan_container_member(
                            *ctx_ty, prefix, false, true, depth, visiting, out_params, out_nested,
                        );
                    }
                }
            },
            TyKind::Param(_) => {
                // Unconstrained generic type parameter may be instantiated with an
                // aggregate containing `Desc`, `Managed`, or `&mut`, even behind `&T`.
                out_params.insert((self.param_idx, prefix.clone()));
                out_nested.insert((prefix.clone(), NestedCapTarget::Wildcard));
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. } => {}
        }
        visiting.pop();
    }
}

pub(super) fn is_poll_ty(tys: &TyTable, ty: TyId) -> bool {
    let inner = match tys.get(ty) {
        Some(TyKind::ErrorUnion { ok, .. }) => *ok,
        _ => ty,
    };
    match tys.get(inner) {
        Some(TyKind::Adt { adt, .. }) => tys
            .get_adt(*adt)
            .is_some_and(|d| d.kind == AdtKind::Enum && d.variants.len() == 2),
        _ => false,
    }
}

pub(super) fn is_error_union_ty(tys: &TyTable, ty: TyId) -> bool {
    matches!(tys.get(ty), Some(TyKind::ErrorUnion { .. }))
}

pub(super) fn is_describable_ty(tys: &TyTable, ty: TyId) -> bool {
    match tys.get(ty) {
        Some(TyKind::Desc(_) | TyKind::Managed(_)) => true,
        Some(TyKind::Ref { pointee, .. }) => match tys.get(*pointee) {
            Some(TyKind::Desc(_) | TyKind::Managed(_)) => true,
            Some(TyKind::Adt { adt, .. }) => tys
                .get_adt(*adt)
                .is_some_and(|d| d.kind == AdtKind::Resource),
            _ => false,
        },
        Some(TyKind::Adt { adt, .. }) => tys
            .get_adt(*adt)
            .is_some_and(|d| d.kind == AdtKind::Resource),
        _ => false,
    }
}

pub(super) fn is_mut_ref_or_desc_ty(tys: &TyTable, ty: TyId) -> bool {
    matches!(
        tys.get(ty),
        Some(TyKind::Ref {
            mutability: Mutability::Exclusive,
            ..
        })
    ) || is_describable_ty(tys, ty)
}

pub(super) fn is_pure_scalar_ty(tys: &TyTable, ty: TyId) -> bool {
    matches!(
        tys.get(ty),
        Some(
            TyKind::Unit
                | TyKind::Bool
                | TyKind::Never
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::FnPtr { .. }
        )
    )
}

pub(super) fn strip_indirection(tys: &TyTable, mut ty: TyId) -> TyId {
    for _ in 0..4 {
        match tys.get(ty) {
            Some(
                TyKind::Ref { pointee, .. } | TyKind::Managed(pointee) | TyKind::Desc(pointee),
            ) => ty = *pointee,
            _ => break,
        }
    }
    ty
}

pub(super) fn extract_field_proj(place: &Place) -> Vec<FieldIdx> {
    place
        .projections
        .iter()
        .filter_map(|p| match p {
            Projection::Field(f) => Some(f.field),
            Projection::Deref
            | Projection::Index(_)
            | Projection::TupleField(_)
            | Projection::ErrorArm(_)
            | Projection::ConstIndex(_) => None,
        })
        .collect()
}

pub(super) fn filter_handle_caps(mch: &HandleCapSet, field_proj: &[FieldIdx]) -> HandleCapSet {
    if field_proj.is_empty() {
        return mch.clone();
    }
    let mut out = HandleCapSet::new();
    for (cap_path, h_idx) in mch {
        if cap_path.starts_with(field_proj) {
            out.insert((cap_path[field_proj.len()..].to_vec(), *h_idx));
        } else if field_proj.starts_with(cap_path) {
            out.insert((Vec::new(), *h_idx));
        }
    }
    out
}
