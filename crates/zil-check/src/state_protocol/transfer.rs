//! Instruction transfer functions (`Replace` and `Assign`) for protocol-state verification.

use std::collections::BTreeSet;

use zil_ir::body::{
    AggregateKind, ContractFieldPath, FieldIdx, Local, Operand, Place, PlaceRoot, Projection,
    ProtocolStateSet, Rvalue, TyKind,
};
use zil_semantics::HandleProtocolSlot;

use super::ProtocolVerifier;
use super::env::{
    DiscrTarget, HandleCapSet, ParamAliasSet, ProtoState, extract_field_proj, filter_handle_caps,
    is_describable_ty, is_mut_ref_or_desc_ty, is_pure_scalar_ty,
};

impl ProtocolVerifier<'_> {
    /// Transfers protocol state, history `H`, retirement obligation, and nested mutable
    /// capabilities across `InstructionKind::Replace`.
    pub(super) fn step_replace(
        &self,
        st: &mut ProtoState,
        place: &Place,
        value: &Operand,
        old: Local,
    ) {
        let old = &old;
        // `Replace` moves the previous instance at `place` into `old` and
        // installs the incoming instance `value` at `place`:
        // - Old and new instances never share history `H`;
        // - Old state, history `H`, and retirement obligation move completely to `old`;
        // - `place` inherits `value`'s own state/history/obligation and clears old `retired`.
        st.record_place_invalidation(place);

        // Capture genuinely nested mutable capabilities carried by `value`,
        // plus direct ref/desc capabilities when `value` is stored into an aggregate field.
        let (
            nested_cap_params,
            nested_cap_handles,
            direct_cap_params,
            direct_cap_handles,
            src_self_handles,
        ) = if let Operand::Copy(src) | Operand::Move(src) = value
            && let PlaceRoot::Local(sl) = src.root
        {
            let fp = extract_field_proj(src);
            let nested_p = st.caps.mut_cap_params.get(&sl).cloned().unwrap_or_default();
            let nested_h = st
                .caps
                .mut_cap_handles
                .get(&sl)
                .map(|m| filter_handle_caps(m, &fp))
                .unwrap_or_default();
            let mut direct_p = ParamAliasSet::new();
            let mut direct_h = HandleCapSet::new();
            let mut self_h = BTreeSet::new();
            if src.projections.is_empty() {
                if let Some(&h) = st.aliases.alias.get(&sl) {
                    self_h.insert(h);
                }
                if let Some(ma) = st.aliases.may_alias.get(&sl) {
                    self_h.extend(ma.iter().copied());
                }
                if self
                    .func
                    .body
                    .locals
                    .get(sl.index())
                    .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
                {
                    if let Some(pa) = st.aliases.param_alias.get(&sl) {
                        direct_p.extend(pa.iter().cloned());
                    }
                    if let Some(ma) = st.aliases.may_alias.get(&sl) {
                        for &h in ma {
                            direct_h.insert((Vec::new(), h));
                        }
                    }
                }
            }
            (nested_p, nested_h, direct_p, direct_h, self_h)
        } else {
            (
                ParamAliasSet::new(),
                HandleCapSet::new(),
                ParamAliasSet::new(),
                HandleCapSet::new(),
                BTreeSet::new(),
            )
        };

        let mut transferred_old_slot: Option<HandleProtocolSlot> = None;
        let mut is_field_replace = false;

        if let Some(h_idx) = self.place_targets_state_field(place, st) {
            // Replacing only the `.state` enum field is an in-place state transition
            // on the same handle instance; `old` receives the previous state value.
            let prev_state = st.slots[h_idx].states;
            let vc = self.local_info[h_idx].map_or(3, |i| i.variant_count);
            let new_state = match value {
                Operand::Copy(src) | Operand::Move(src) if src.projections.is_empty() => {
                    if let PlaceRoot::Local(sl) = src.root
                        && let Some(&v) = st.refinements.enum_variant.get(&sl)
                    {
                        ProtocolStateSet::single(v)
                    } else {
                        ProtocolStateSet::all_variants(vc)
                    }
                }
                Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => {
                    ProtocolStateSet::all_variants(vc)
                }
            };
            st.set_handle_state(h_idx, new_state);
            st.invalidate_handle(h_idx);
            if let PlaceRoot::Local(rl) = place.root
                && let Some(targets) = st.aliases.may_alias.get(&rl).cloned()
            {
                for other_h in targets {
                    if other_h != h_idx {
                        st.weak_update_handle_state(other_h, new_state);
                        st.invalidate_handle(other_h);
                    }
                }
            }
            if self
                .local_info
                .get(old.index())
                .is_some_and(Option::is_some)
            {
                transferred_old_slot = Some(HandleProtocolSlot::fresh(prev_state));
            }
        } else if place.projections.is_empty()
            && let PlaceRoot::Local(rl) = place.root
            && let Some(Some(info)) = self.local_info.get(rl.index())
        {
            // Direct whole-local protocol handle replacement (`replace(rl, value) -> old`).
            let ri = rl.index();
            let incoming = self.resolve_incoming_slot(st, value, info.variant_count);
            let prev_targets: Vec<usize> = st
                .aliases
                .may_alias
                .get(&rl)
                .map(|s| s.iter().copied().collect::<Vec<usize>>())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| vec![st.aliases.alias.get(&rl).copied().unwrap_or(ri)]);
            let primary_prev = st.aliases.alias.get(&rl).copied().unwrap_or(ri);
            let mut acc_old = st.slots[primary_prev];
            for &other_h in &prev_targets {
                if other_h != primary_prev {
                    let snap = st.slots[other_h].replace_weak_target(incoming);
                    acc_old.join(&snap);
                    st.sync_store_borrow_write(other_h);
                    st.invalidate_handle(other_h);
                }
            }
            st.slots[ri] = incoming;
            st.aliases.alias.insert(rl, ri);
            let mut single = BTreeSet::new();
            single.insert(ri);
            st.aliases.may_alias.insert(rl, single.clone());
            st.aliases.local_roots.insert(rl, single);
            st.invalidate_handle(ri);
            if primary_prev != ri {
                st.invalidate_handle(primary_prev);
            }
            transferred_old_slot = Some(acc_old);
        } else if !place.projections.is_empty()
            && place
                .projections
                .iter()
                .all(|p| matches!(p, Projection::Deref))
            && let PlaceRoot::Local(rl) = place.root
        {
            // Indirect whole-handle replacement (`*rl = value`, moving previous into `old`).
            if let Some(&h_idx) = st.aliases.alias.get(&rl)
                && let Some(Some(info)) = self.local_info.get(h_idx)
            {
                let incoming = self.resolve_incoming_slot(st, value, info.variant_count);
                let mut acc_old = st.slots[h_idx].replace_strong(incoming);
                st.sync_store_borrow_write(h_idx);
                st.invalidate_handle(h_idx);
                if let Some(targets) = st.aliases.may_alias.get(&rl).cloned() {
                    for other_h in targets {
                        if other_h != h_idx
                            && self.local_info.get(other_h).is_some_and(Option::is_some)
                        {
                            let snap = st.slots[other_h].replace_weak_target(incoming);
                            acc_old.join(&snap);
                            st.sync_store_borrow_write(other_h);
                            st.invalidate_handle(other_h);
                        }
                    }
                }
                transferred_old_slot = Some(acc_old);
            } else if let Some(targets) = st.aliases.may_alias.get(&rl).cloned() {
                // Ambiguous merged reference: weak-replace every candidate target
                // and join all candidate previous slots into `old`.
                let mut acc_old: Option<HandleProtocolSlot> = None;
                for h_idx in targets {
                    if let Some(Some(info)) = self.local_info.get(h_idx) {
                        let incoming = self.resolve_incoming_slot(st, value, info.variant_count);
                        let snap = st.slots[h_idx].replace_weak_target(incoming);
                        st.sync_store_borrow_write(h_idx);
                        st.invalidate_handle(h_idx);
                        match &mut acc_old {
                            None => acc_old = Some(snap),
                            Some(acc) => {
                                acc.join(&snap);
                            }
                        }
                    }
                }
                transferred_old_slot = acc_old;
            }
        } else if let PlaceRoot::Local(rl) = place.root {
            // Replacing a field inside a parent aggregate (e.g. `holder.payload = ...`).
            is_field_replace = true;
            if let Some(targets) = st.aliases.may_alias.get(&rl).cloned() {
                let last_field = place.projections.last().and_then(|p| match p {
                    Projection::Field(f) => Some(f.field),
                    Projection::Deref
                    | Projection::Index(_)
                    | Projection::TupleField(_)
                    | Projection::ErrorArm(_)
                    | Projection::ConstIndex(_) => None,
                });
                for h_idx in targets {
                    if let Some(Some(info)) = self.local_info.get(h_idx) {
                        let touches_state = match (info.state_field, last_field) {
                            (None, _) => true,
                            (Some((_, sf)), Some(lf)) => sf == lf,
                            (Some(_), None) => true,
                        };
                        if touches_state {
                            let all = ProtocolStateSet::all_variants(info.variant_count);
                            st.set_handle_state(h_idx, all);
                            st.invalidate_handle(h_idx);
                        }
                    }
                }
            }
        }

        if let PlaceRoot::Local(rl) = place.root {
            let mut add_params = nested_cap_params;
            let mut add_handles = if is_field_replace {
                nested_cap_handles
            } else {
                // On whole-handle Replace, the incoming handle's own identity is
                // already transferred into `st.slots[h_idx]`; keep only genuinely
                // nested capabilities (non-empty path or distinct handle).
                nested_cap_handles
                    .into_iter()
                    .filter(|(sub_path, h)| !(sub_path.is_empty() && src_self_handles.contains(h)))
                    .collect()
            };
            if is_field_replace {
                add_params.extend(direct_cap_params);
                add_handles.extend(direct_cap_handles);
            }
            if !add_params.is_empty() {
                st.caps
                    .mut_cap_params
                    .entry(rl)
                    .or_default()
                    .extend(add_params);
            }
            if !add_handles.is_empty() {
                let dest_proj = extract_field_proj(place);
                let entry = st.caps.mut_cap_handles.entry(rl).or_default();
                for (sub_path, h_idx) in add_handles {
                    let mut full_path = Vec::with_capacity(
                        (dest_proj.len() + sub_path.len()).min(ContractFieldPath::MAX_DEPTH),
                    );
                    for &f in dest_proj.iter().chain(sub_path.iter()) {
                        if full_path.len() < ContractFieldPath::MAX_DEPTH {
                            full_path.push(f);
                        }
                    }
                    entry.insert((full_path, h_idx));
                }
            }
        }

        // Clear stale alias/value facts on `old`, then bind `old` to its own
        // independent local slot holding the transferred previous resource.
        st.clear_local(*old);
        let oi = old.index();
        let mut own_root = BTreeSet::new();
        own_root.insert(oi);
        st.aliases.local_roots.insert(*old, own_root.clone());
        if let Some(Some(old_info)) = self.local_info.get(oi) {
            st.slots[oi] = transferred_old_slot
                .unwrap_or_else(|| HandleProtocolSlot::unknown(old_info.variant_count));
            st.aliases.alias.insert(*old, oi);
            st.aliases.may_alias.insert(*old, own_root);
            st.invalidate_handle(oi);
        }
    }

    /// Transfers protocol state and alias/capability facts across `InstructionKind::Assign`.
    pub(super) fn step_assign(&self, st: &mut ProtoState, dest: &Place, value: &Rvalue) {
        // Always record parameter-projection invalidations on any write.
        st.record_place_invalidation(dest);

        // Case A: Writing directly to a protocol handle's `state` field (`h.state = ...`).
        if let Some(h_idx) = self.place_targets_state_field(dest, st) {
            let vc = self.local_info[h_idx].map_or(3, |i| i.variant_count);
            let new_state = if let Rvalue::Aggregate {
                kind: AggregateKind::Adt { variant, .. },
                ..
            } = value
            {
                ProtocolStateSet::single(variant.0)
            } else if let Rvalue::Use(Operand::Copy(src) | Operand::Move(src)) = value
                && src.projections.is_empty()
                && let PlaceRoot::Local(sl) = src.root
                && let Some(&v) = st.refinements.enum_variant.get(&sl)
            {
                ProtocolStateSet::single(v)
            } else {
                ProtocolStateSet::all_variants(vc)
            };
            st.set_handle_state(h_idx, new_state);
            st.invalidate_handle(h_idx);
            if let PlaceRoot::Local(rl) = dest.root
                && let Some(targets) = st.aliases.may_alias.get(&rl).cloned()
            {
                for other_h in targets {
                    if other_h != h_idx {
                        st.weak_update_handle_state(other_h, new_state);
                        st.invalidate_handle(other_h);
                    }
                }
            }
            return;
        }

        // Case A1b: Writing through an AMBIGUOUS merged reference (`r.state = ...` or `*r = ...`).
        if !dest.projections.is_empty()
            && let PlaceRoot::Local(rl) = dest.root
            && !st.aliases.alias.contains_key(&rl)
            && let Some(targets) = st.aliases.may_alias.get(&rl).cloned()
        {
            let is_deref_only = dest
                .projections
                .iter()
                .all(|p| matches!(p, Projection::Deref));
            for h_idx in targets {
                if let Some(Some(info)) = self.local_info.get(h_idx) {
                    if is_deref_only && !info.is_direct_enum {
                        let incoming = if let Some((_, sf)) = info.state_field
                            && let Rvalue::Aggregate { operands, .. } = value
                        {
                            let field_state = operands
                                .get(sf.0 as usize)
                                .and_then(|op| match op {
                                    Operand::Copy(p) | Operand::Move(p)
                                        if p.projections.is_empty() =>
                                    {
                                        if let PlaceRoot::Local(sl) = p.root {
                                            st.refinements
                                                .enum_variant
                                                .get(&sl)
                                                .map(|&v| ProtocolStateSet::single(v))
                                        } else {
                                            None
                                        }
                                    }
                                    Operand::Copy(_)
                                    | Operand::Move(_)
                                    | Operand::Const(_)
                                    | Operand::Pooled(_) => None,
                                })
                                .unwrap_or_else(|| {
                                    ProtocolStateSet::all_variants(info.variant_count)
                                });
                            HandleProtocolSlot::fresh(field_state)
                        } else if let Rvalue::Use(op) = value {
                            self.resolve_incoming_slot(st, op, info.variant_count)
                        } else {
                            HandleProtocolSlot::unknown(info.variant_count)
                        };
                        let _ = st.slots[h_idx].replace_weak_target(incoming);
                        st.sync_store_borrow_write(h_idx);
                        st.invalidate_handle(h_idx);
                    } else {
                        let touches_state = match info.state_field {
                            None => true,
                            Some((_, sf)) => {
                                is_deref_only
                                    || dest
                                        .projections
                                        .iter()
                                        .any(|p| matches!(p, Projection::Field(f) if f.field == sf))
                            }
                        };
                        if touches_state {
                            let all = ProtocolStateSet::all_variants(info.variant_count);
                            st.set_handle_state(h_idx, all);
                            st.invalidate_handle(h_idx);
                        }
                    }
                }
            }
            return;
        }

        // Case A2: Writing through a reference to a protocol handle (`*h = value`).
        if !dest.projections.is_empty()
            && dest
                .projections
                .iter()
                .all(|p| matches!(p, Projection::Deref))
            && let PlaceRoot::Local(rl) = dest.root
            && let Some(&h_idx) = st.aliases.alias.get(&rl)
            && let Some(Some(info)) = self.local_info.get(h_idx)
        {
            if !info.is_direct_enum {
                // Whole struct/package/resource/built-in-opaque handle overwrite
                // through `&mut`: treat as instance replacement (isolated history,
                // transferred obligations, cleared stale retired).
                let incoming = if let Some((_, sf)) = info.state_field
                    && let Rvalue::Aggregate { operands, .. } = value
                {
                    let field_state = operands
                        .get(sf.0 as usize)
                        .and_then(|op| match op {
                            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                                if let PlaceRoot::Local(sl) = p.root {
                                    st.refinements
                                        .enum_variant
                                        .get(&sl)
                                        .map(|&v| ProtocolStateSet::single(v))
                                } else {
                                    None
                                }
                            }
                            Operand::Copy(_)
                            | Operand::Move(_)
                            | Operand::Const(_)
                            | Operand::Pooled(_) => None,
                        })
                        .unwrap_or_else(|| ProtocolStateSet::all_variants(info.variant_count));
                    HandleProtocolSlot::fresh(field_state)
                } else if let Rvalue::Use(op) = value {
                    self.resolve_incoming_slot(st, op, info.variant_count)
                } else {
                    HandleProtocolSlot::unknown(info.variant_count)
                };
                let _ = st.slots[h_idx].replace_strong(incoming);
                st.sync_store_borrow_write(h_idx);
                st.invalidate_handle(h_idx);
                if let Some(targets) = st.aliases.may_alias.get(&rl).cloned() {
                    for other_h in targets {
                        if other_h != h_idx
                            && self.local_info.get(other_h).is_some_and(Option::is_some)
                        {
                            let _ = st.slots[other_h].replace_weak_target(incoming);
                            st.sync_store_borrow_write(other_h);
                            st.invalidate_handle(other_h);
                        }
                    }
                }
            } else {
                // Direct enum state handle (`*e = Variant`): in-place state transition.
                let new_state = if let Rvalue::Aggregate {
                    kind: AggregateKind::Adt { variant, .. },
                    ..
                } = value
                {
                    ProtocolStateSet::single(variant.0)
                } else if let Rvalue::Use(op) = value {
                    st.resolve_handle(op).map_or_else(
                        || ProtocolStateSet::all_variants(info.variant_count),
                        |src_h| st.slots[src_h].states,
                    )
                } else {
                    ProtocolStateSet::all_variants(info.variant_count)
                };
                st.set_handle_state(h_idx, new_state);
                st.invalidate_handle(h_idx);
                if let Some(targets) = st.aliases.may_alias.get(&rl).cloned() {
                    for other_h in targets {
                        if other_h != h_idx {
                            st.weak_update_handle_state(other_h, new_state);
                            st.invalidate_handle(other_h);
                        }
                    }
                }
            }
            return;
        }

        if dest.projections.is_empty()
            && let PlaceRoot::Local(dl) = dest.root
        {
            self.step_assign_local(st, dl, value);
        }
    }

    /// Whole-local assignment (`dl = value`, Case B).
    fn step_assign_local(&self, st: &mut ProtoState, dl: Local, value: &Rvalue) {
        let di = dl.index();
        let dest_ty = self.func.body.locals[di].ty;
        st.clear_local(dl);
        let mut own_root = BTreeSet::new();
        own_root.insert(di);
        st.aliases.local_roots.insert(dl, own_root);
        if di < self.param_count {
            let mut s = ParamAliasSet::new();
            s.insert((di, Vec::new()));
            st.aliases.param_alias.insert(dl, s);
        }
        if !matches!(
            self.tys.get(dest_ty),
            Some(TyKind::Ref { .. } | TyKind::Desc(_))
        ) {
            st.aliases.alias.insert(dl, di);
        }
        if self.local_info.get(di).is_some_and(Option::is_some) {
            st.aliases.alias.insert(dl, di);
            let mut s = BTreeSet::new();
            s.insert(di);
            st.aliases.may_alias.insert(dl, s);
            st.invalidate_handle(di);
        }

        match value {
            Rvalue::Use(incoming_op @ (Operand::Copy(src) | Operand::Move(src))) => {
                let dest_is_pure_scalar = is_pure_scalar_ty(self.tys, dest_ty);
                let dest_is_indirect = matches!(
                    self.tys.get(dest_ty),
                    Some(TyKind::Ref { .. } | TyKind::Desc(_) | TyKind::Managed(_))
                );
                let is_copy_value =
                    matches!(value, Rvalue::Use(Operand::Copy(_))) && !dest_is_indirect;
                let field_proj = extract_field_proj(src);
                let mut matched_handle_caps = HandleCapSet::new();
                if !dest_is_pure_scalar && let PlaceRoot::Local(sl) = src.root {
                    if let Some(mcp) = st.caps.mut_cap_params.get(&sl) {
                        let filtered: ParamAliasSet = if field_proj.is_empty() {
                            mcp.clone()
                        } else {
                            mcp.iter()
                                .filter(|(_, path)| {
                                    path.starts_with(&field_proj) || field_proj.starts_with(path)
                                })
                                .cloned()
                                .collect()
                        };
                        if !filtered.is_empty() {
                            st.caps.mut_cap_params.insert(dl, filtered);
                        }
                    }
                    if let Some(mch) = st.caps.mut_cap_handles.get(&sl) {
                        matched_handle_caps = filter_handle_caps(mch, &field_proj);
                        if !matched_handle_caps.is_empty() {
                            st.caps
                                .mut_cap_handles
                                .insert(dl, matched_handle_caps.clone());
                        }
                    }
                    if (dest_is_indirect
                        || !is_copy_value
                        || st.caps.mut_cap_params.contains_key(&dl)
                        || st.caps.mut_cap_handles.contains_key(&dl))
                        && let Some(sr) = st.aliases.local_roots.get(&sl).cloned()
                    {
                        st.aliases.local_roots.insert(dl, sr);
                    }
                }
                if dest_is_indirect {
                    let pa = st.resolve_param_places(src);
                    if !pa.is_empty() {
                        st.aliases.param_alias.insert(dl, pa);
                    }
                }
                if let Some(h_idx) = self.place_targets_state_field(src, st) {
                    st.refinements.state_field_of.insert(dl, h_idx);
                } else if let PlaceRoot::Local(sl) = src.root
                    && let Some((Projection::ErrorArm(is_ok), prefix)) =
                        src.projections.split_last()
                    && prefix.iter().all(|p| matches!(p, Projection::Deref))
                    && self.local_info.get(di).is_some_and(Option::is_some)
                {
                    let slot = st
                        .refinements
                        .eu_payload_slot
                        .get(&(sl, *is_ok))
                        .copied()
                        .unwrap_or_else(|| {
                            HandleProtocolSlot::unknown(
                                self.local_info[di].map_or(3, |i| i.variant_count),
                            )
                        });
                    st.slots[di] = slot;
                } else if src.projections.is_empty()
                    && let PlaceRoot::Local(sl) = src.root
                {
                    if let Some(ma) = st.aliases.may_alias.get(&sl).cloned()
                        && !is_copy_value
                    {
                        st.aliases.may_alias.insert(dl, ma);
                    }
                    if let Some(&h) = st.aliases.alias.get(&sl) {
                        let reassign_owning_param = di < self.param_count
                            && self.local_info.get(di).is_some_and(Option::is_some)
                            && !matches!(
                                self.tys.get(dest_ty),
                                Some(TyKind::Ref { .. } | TyKind::Desc(_))
                            );
                        if (is_copy_value || reassign_owning_param)
                            && self.local_info.get(di).is_some_and(Option::is_some)
                        {
                            // By-value Copy or reassignment of an owning parameter slot
                            // installs the incoming handle's own state/history and clears stale retired.
                            let incoming = if let Some(Some(info)) = self.local_info.get(di) {
                                self.resolve_incoming_slot(st, incoming_op, info.variant_count)
                            } else {
                                st.slots[h]
                            };
                            st.slots[di] = incoming;
                            st.aliases.alias.insert(dl, di);
                            let mut s = BTreeSet::new();
                            s.insert(di);
                            st.aliases.may_alias.insert(dl, s);
                        } else {
                            // Move of an owning handle or copy of &/&mut/Desc preserves handle identity.
                            st.aliases.alias.insert(dl, h);
                        }
                        if self.local_store_info.get(di).is_some_and(Option::is_some)
                            && self.local_store_info.get(h).is_some_and(Option::is_some)
                        {
                            st.store_slots[di] = st.store_slots[h];
                        }
                    } else if matches!(
                        self.tys.get(dest_ty),
                        Some(TyKind::Ref { .. } | TyKind::Desc(_))
                    ) {
                        // Source reference had ambiguous alias; destination must also be ambiguous!
                        st.aliases.alias.remove(&dl);
                    }
                    if let Some(&sf) = st.refinements.state_field_of.get(&sl) {
                        st.refinements.state_field_of.insert(dl, sf);
                    }
                    if let Some(&ev) = st.refinements.enum_variant.get(&sl) {
                        st.refinements.enum_variant.insert(dl, ev);
                    }
                    if let Some(&ea) = st.refinements.error_arm_const.get(&sl) {
                        st.refinements.error_arm_const.insert(dl, ea);
                    }
                    if let Some(&po) = st.refinements.poll_of.get(&sl) {
                        st.refinements.poll_of.insert(dl, po);
                    }
                    if let Some(eu) = st.refinements.eu_of.get(&sl).cloned() {
                        st.refinements.eu_of.insert(dl, eu);
                    }
                    if let Some(&slot) = st.refinements.eu_payload_slot.get(&(sl, true)) {
                        st.refinements.eu_payload_slot.insert((dl, true), slot);
                    }
                    if let Some(&slot) = st.refinements.eu_payload_slot.get(&(sl, false)) {
                        st.refinements.eu_payload_slot.insert((dl, false), slot);
                    }
                    if let Some(so) = st.refinements.store_op_of.get(&sl).cloned() {
                        st.refinements.store_op_of.insert(dl, so);
                    }
                }
                // Propagate nested resource handle identities into `may_alias` when
                // projecting an indirect protocol handle field (e.g. `ctx_borrow.handle`).
                if !is_copy_value
                    && self.local_info.get(di).is_some_and(Option::is_some)
                    && !matched_handle_caps.is_empty()
                {
                    let entry = st.aliases.may_alias.entry(dl).or_default();
                    for (rem_path, h_idx) in &matched_handle_caps {
                        if rem_path.is_empty() {
                            entry.insert(*h_idx);
                        }
                    }
                }
            }
            Rvalue::Ref { place: src, .. } | Rvalue::Describe { place: src } => {
                let pa = st.resolve_param_places(src);
                if !pa.is_empty() {
                    st.aliases.param_alias.insert(dl, pa);
                }
                if let PlaceRoot::Local(sl) = src.root
                    && let Some((Projection::ErrorArm(is_ok), prefix)) =
                        src.projections.split_last()
                    && prefix.iter().all(|p| matches!(p, Projection::Deref))
                    && let Some(Some(info)) = self.local_info.get(di)
                {
                    let slot = st
                        .refinements
                        .eu_payload_slot
                        .get(&(sl, *is_ok))
                        .copied()
                        .unwrap_or_else(|| HandleProtocolSlot::unknown(info.variant_count));
                    st.slots[di] = slot;
                    st.aliases.alias.insert(dl, di);
                    let mut s = BTreeSet::new();
                    s.insert(di);
                    st.aliases.may_alias.insert(dl, s);
                } else if src
                    .projections
                    .iter()
                    .all(|p| matches!(p, Projection::Deref))
                    && let PlaceRoot::Local(sl) = src.root
                {
                    if let Some(&h) = st.aliases.alias.get(&sl) {
                        st.aliases.alias.insert(dl, h);
                    } else {
                        st.aliases.alias.remove(&dl);
                    }
                    if let Some(ma) = st.aliases.may_alias.get(&sl).cloned() {
                        st.aliases.may_alias.insert(dl, ma);
                    }
                }
                if let PlaceRoot::Local(sl) = src.root {
                    if let Some(sr) = st.aliases.local_roots.get(&sl).cloned() {
                        st.aliases.local_roots.insert(dl, sr);
                    }
                    let field_proj = extract_field_proj(src);
                    if let Some(mcp) = st.caps.mut_cap_params.get(&sl) {
                        let filtered: ParamAliasSet = if field_proj.is_empty() {
                            mcp.clone()
                        } else {
                            mcp.iter()
                                .filter(|(_, path)| {
                                    path.starts_with(&field_proj) || field_proj.starts_with(path)
                                })
                                .cloned()
                                .collect()
                        };
                        if !filtered.is_empty() {
                            st.caps.mut_cap_params.insert(dl, filtered);
                        }
                    }
                    if let Some(mch) = st.caps.mut_cap_handles.get(&sl) {
                        let filtered = filter_handle_caps(mch, &field_proj);
                        if !filtered.is_empty() {
                            if self.local_info.get(di).is_some_and(Option::is_some) {
                                let entry = st.aliases.may_alias.entry(dl).or_default();
                                for (rem_path, h_idx) in &filtered {
                                    if rem_path.is_empty() {
                                        entry.insert(*h_idx);
                                    }
                                }
                            }
                            st.caps.mut_cap_handles.insert(dl, filtered);
                        }
                    }
                }
            }
            Rvalue::Discriminant(src) => {
                if let Some(h_idx) = self.place_targets_state_field(src, st) {
                    st.refinements
                        .discr_of
                        .insert(dl, DiscrTarget::HandleState(h_idx));
                } else if src.projections.is_empty()
                    && let PlaceRoot::Local(sl) = src.root
                {
                    if let Some(&h_idx) = st.refinements.state_field_of.get(&sl) {
                        st.refinements
                            .discr_of
                            .insert(dl, DiscrTarget::HandleState(h_idx));
                    } else if st.refinements.poll_of.contains_key(&sl) {
                        st.refinements.discr_of.insert(dl, DiscrTarget::Poll(sl));
                    } else if let Some(&h_idx) = st.aliases.alias.get(&sl)
                        && self
                            .local_info
                            .get(h_idx)
                            .and_then(|o| *o)
                            .is_some_and(|i| i.state_field.is_none())
                    {
                        st.refinements
                            .discr_of
                            .insert(dl, DiscrTarget::HandleState(h_idx));
                    } else {
                        st.refinements.discr_of.insert(dl, DiscrTarget::Poll(sl));
                    }
                }
            }
            Rvalue::ErrorArm(src) => {
                if src.projections.is_empty()
                    && let PlaceRoot::Local(sl) = src.root
                {
                    st.refinements
                        .discr_of
                        .insert(dl, DiscrTarget::ErrorUnion(sl));
                }
            }
            Rvalue::ErrorUnion {
                ok, value: operand, ..
            } => {
                st.refinements.error_arm_const.insert(dl, *ok);
                if let Operand::Copy(p) | Operand::Move(p) = operand
                    && let PlaceRoot::Local(sl) = p.root
                {
                    if p.projections.is_empty()
                        && let Some(Some(info)) = self.local_info.get(sl.index())
                    {
                        let slot = self.resolve_incoming_slot(st, operand, info.variant_count);
                        st.refinements.eu_payload_slot.insert((dl, *ok), slot);
                    }
                    if let Some(mcp) = st.caps.mut_cap_params.get(&sl).cloned() {
                        st.caps.mut_cap_params.insert(dl, mcp);
                    }
                    if let Some(mch) = st.caps.mut_cap_handles.get(&sl) {
                        let flattened: HandleCapSet =
                            mch.iter().map(|(_, h_idx)| (Vec::new(), *h_idx)).collect();
                        if !flattened.is_empty() {
                            st.caps.mut_cap_handles.insert(dl, flattened);
                        }
                    }
                    if self
                        .func
                        .body
                        .locals
                        .get(sl.index())
                        .is_some_and(|ld| !is_pure_scalar_ty(self.tys, ld.ty))
                        && let Some(sr) = st.aliases.local_roots.get(&sl).cloned()
                    {
                        st.aliases.local_roots.entry(dl).or_default().extend(sr);
                    }
                }
            }
            Rvalue::Aggregate { kind, operands } => {
                // Track mutable capabilities captured inside closures or wrapped in aggregates.
                let is_single_variant_adt = matches!(
                    kind,
                    AggregateKind::Adt { adt, .. }
                        if self.tys.get_adt(*adt).is_some_and(|d| d.variants.len() == 1)
                );
                let mut cap_params = ParamAliasSet::new();
                let mut cap_handles = HandleCapSet::new();
                let mut agg_roots = BTreeSet::new();
                agg_roots.insert(di);
                for (fi, op) in operands.iter().enumerate() {
                    if let Operand::Copy(p) | Operand::Move(p) = op
                        && let PlaceRoot::Local(sl) = p.root
                    {
                        let field_proj = extract_field_proj(p);
                        if let Some(mcp) = st.caps.mut_cap_params.get(&sl) {
                            cap_params.extend(mcp.iter().cloned());
                        }
                        if let Some(mch) = st.caps.mut_cap_handles.get(&sl) {
                            for (sub_path, h_idx) in filter_handle_caps(mch, &field_proj) {
                                if is_single_variant_adt {
                                    let mut new_path = Vec::with_capacity(
                                        (sub_path.len() + 1).min(ContractFieldPath::MAX_DEPTH),
                                    );
                                    new_path.push(FieldIdx(fi as u32));
                                    for f in sub_path {
                                        if new_path.len() < ContractFieldPath::MAX_DEPTH {
                                            new_path.push(f);
                                        }
                                    }
                                    cap_handles.insert((new_path, h_idx));
                                } else {
                                    cap_handles.insert((Vec::new(), h_idx));
                                }
                            }
                        }
                        if p.projections.is_empty()
                            && self
                                .func
                                .body
                                .locals
                                .get(sl.index())
                                .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
                        {
                            if let Some(pa) = st.aliases.param_alias.get(&sl) {
                                cap_params.extend(pa.iter().cloned());
                            }
                            if let Some(ma) = st.aliases.may_alias.get(&sl) {
                                for &h_idx in ma {
                                    if is_single_variant_adt {
                                        cap_handles.insert((vec![FieldIdx(fi as u32)], h_idx));
                                    } else {
                                        cap_handles.insert((Vec::new(), h_idx));
                                    }
                                }
                            }
                        }
                        if self
                            .func
                            .body
                            .locals
                            .get(sl.index())
                            .is_some_and(|ld| !is_pure_scalar_ty(self.tys, ld.ty))
                            && let Some(sr) = st.aliases.local_roots.get(&sl)
                        {
                            agg_roots.extend(sr.iter().copied());
                        }
                    }
                }
                st.aliases.local_roots.insert(dl, agg_roots);
                if !cap_params.is_empty() {
                    st.caps.mut_cap_params.insert(dl, cap_params);
                }
                if !cap_handles.is_empty() {
                    st.caps.mut_cap_handles.insert(dl, cap_handles);
                }
                if let AggregateKind::Adt { variant, .. } = kind {
                    st.refinements.enum_variant.insert(dl, variant.0);
                    if let Some(Some(info)) = self.local_info.get(di) {
                        if let Some((_, sf)) = info.state_field {
                            let field_state = operands
                                .get(sf.0 as usize)
                                .and_then(|op| match op {
                                    Operand::Copy(p) | Operand::Move(p)
                                        if p.projections.is_empty() =>
                                    {
                                        if let PlaceRoot::Local(sl) = p.root {
                                            st.refinements
                                                .enum_variant
                                                .get(&sl)
                                                .map(|&v| ProtocolStateSet::single(v))
                                        } else {
                                            None
                                        }
                                    }
                                    Operand::Copy(_)
                                    | Operand::Move(_)
                                    | Operand::Const(_)
                                    | Operand::Pooled(_) => None,
                                })
                                .unwrap_or_else(|| {
                                    ProtocolStateSet::all_variants(info.variant_count)
                                });
                            st.slots[di] = HandleProtocolSlot::fresh(field_state);
                        } else {
                            st.slots[di] =
                                HandleProtocolSlot::fresh(ProtocolStateSet::single(variant.0));
                        }
                    }
                }
            }
            Rvalue::Convert { operand, to } => {
                if let Operand::Copy(p) | Operand::Move(p) = operand
                    && let PlaceRoot::Local(sl) = p.root
                {
                    let field_proj = extract_field_proj(p);
                    let mut cap_params =
                        st.caps.mut_cap_params.get(&sl).cloned().unwrap_or_default();
                    let mut cap_handles = st
                        .caps
                        .mut_cap_handles
                        .get(&sl)
                        .map(|mch| filter_handle_caps(mch, &field_proj))
                        .unwrap_or_default();
                    if !is_pure_scalar_ty(self.tys, *to)
                        && let Some(sr) = st.aliases.local_roots.get(&sl).cloned()
                    {
                        st.aliases.local_roots.insert(dl, sr);
                    }
                    let target_is_dyn = matches!(
                        self.tys.get(*to),
                        Some(TyKind::Dyn { .. } | TyKind::OwnedDyn { .. })
                    ) || matches!(
                        self.tys.get(*to),
                        Some(TyKind::Ref { pointee, .. })
                            if matches!(
                                self.tys.get(*pointee),
                                Some(TyKind::Dyn { .. } | TyKind::OwnedDyn { .. })
                            )
                    );
                    if p.projections.is_empty() {
                        let src_is_describable = self
                            .func
                            .body
                            .locals
                            .get(sl.index())
                            .is_some_and(|ld| is_describable_ty(self.tys, ld.ty));
                        if matches!(
                            self.tys.get(*to),
                            Some(TyKind::Ref { .. } | TyKind::Desc(_))
                        ) {
                            if let Some(pa) = st.aliases.param_alias.get(&sl).cloned() {
                                if src_is_describable {
                                    cap_params.extend(pa.iter().cloned());
                                }
                                st.aliases.param_alias.insert(dl, pa);
                            }
                            if let Some(ma) = st.aliases.may_alias.get(&sl).cloned() {
                                if src_is_describable {
                                    for &h_idx in &ma {
                                        cap_handles.insert((Vec::new(), h_idx));
                                    }
                                }
                                st.aliases.may_alias.insert(dl, ma);
                            }
                            if let Some(&h) = st.aliases.alias.get(&sl) {
                                st.aliases.alias.insert(dl, h);
                            } else {
                                st.aliases.alias.remove(&dl);
                            }
                        } else if matches!(self.tys.get(*to), Some(TyKind::OwnedDyn { .. }))
                            && self
                                .func
                                .body
                                .locals
                                .get(sl.index())
                                .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
                        {
                            if let Some(pa) = st.aliases.param_alias.get(&sl) {
                                cap_params.extend(pa.iter().cloned());
                            }
                            if let Some(ma) = st.aliases.may_alias.get(&sl) {
                                for &h_idx in ma {
                                    cap_handles.insert((Vec::new(), h_idx));
                                }
                            }
                        }
                    }
                    if target_is_dyn {
                        cap_handles = cap_handles
                            .into_iter()
                            .map(|(_, h_idx)| (Vec::new(), h_idx))
                            .collect();
                    }
                    if self.local_info.get(di).is_some_and(Option::is_some)
                        && matches!(
                            self.tys.get(*to),
                            Some(TyKind::Ref { .. } | TyKind::Desc(_))
                        )
                    {
                        let entry = st.aliases.may_alias.entry(dl).or_default();
                        for (rem_path, h_idx) in &cap_handles {
                            if rem_path.is_empty() {
                                entry.insert(*h_idx);
                            }
                        }
                    }
                    if !cap_params.is_empty() {
                        st.caps.mut_cap_params.insert(dl, cap_params);
                    }
                    if !cap_handles.is_empty() {
                        st.caps.mut_cap_handles.insert(dl, cap_handles);
                    }
                }
            }
            Rvalue::Unary {
                op: zil_ir::body::UnOp::Not,
                operand,
            } => {
                if let Operand::Copy(p) | Operand::Move(p) = operand
                    && p.projections.is_empty()
                    && let PlaceRoot::Local(sl) = p.root
                    && let Some(DiscrTarget::ValidCheck { handle, inverted }) =
                        st.refinements.discr_of.get(&sl).copied()
                {
                    st.refinements.discr_of.insert(
                        dl,
                        DiscrTarget::ValidCheck {
                            handle,
                            inverted: !inverted,
                        },
                    );
                }
            }
            Rvalue::Use(_)
            | Rvalue::SliceRef { .. }
            | Rvalue::Unary { .. }
            | Rvalue::Binary { .. }
            | Rvalue::Len(_)
            | Rvalue::FnRef(_)
            | Rvalue::ForeignFnRef(_)
            | Rvalue::StaticUtf(_) => {}
        }
    }
}
