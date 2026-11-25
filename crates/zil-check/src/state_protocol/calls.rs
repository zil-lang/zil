//! Call-terminator transfer functions, effect application, and direct-callee
//! protocol-state contract verification.

use std::collections::{BTreeSet, HashMap, VecDeque};

use zil_ir::body::{
    BlockId, Callee, ContractFieldPath, FieldIdx, FnId, IntrinsicOp, Local, Mutability, Operand,
    Place, PlaceRoot, ProtocolStateSet, StateContractClause, TyKind,
};
use zil_semantics::{HandleProtocolSlot, StoreProtocolSummary};

use super::env::{
    DiscrTarget, HandleCapSet, ParamAliasSet, ProtoState, StoreOpOutcome, extract_field_proj,
    filter_handle_caps, is_mut_ref_or_desc_ty, is_pure_scalar_ty,
};
use super::{ProtocolVerifier, enqueue};
use crate::CheckError;

impl ProtocolVerifier<'_> {
    fn collect_store_invalidation_targets(&self, st: &ProtoState, op: &Operand) -> ParamAliasSet {
        let mut out = ParamAliasSet::new();
        if let Operand::Copy(p) | Operand::Move(p) = op {
            out.extend(st.resolve_param_places(p));
            if let PlaceRoot::Local(al) = p.root
                && let Some(mcp) = st.caps.mut_cap_params.get(&al)
            {
                out.extend(mcp.iter().cloned());
            }
        }
        out
    }

    fn apply_local_effects(
        &self,
        st: &mut ProtoState,
        local: Local,
        include_direct_mut_or_desc: bool,
        excluded_handles: &[usize],
    ) {
        if let Some(mcp) = st.caps.mut_cap_params.get(&local).cloned() {
            for item in mcp {
                st.insert_invalidation(item);
            }
        }
        if let Some(mch) = st.caps.mut_cap_handles.get(&local).cloned() {
            for (_, h_idx) in mch {
                if !excluded_handles.contains(&h_idx)
                    && let Some(Some(info)) = self.local_info.get(h_idx)
                {
                    st.slots[h_idx].widen_unknown_mutation(info.variant_count);
                    st.sync_store_borrow_write(h_idx);
                    st.invalidate_handle(h_idx);
                }
            }
        }
        if include_direct_mut_or_desc
            && self
                .func
                .body
                .locals
                .get(local.index())
                .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
        {
            if let Some(pa_set) = st.aliases.param_alias.get(&local).cloned() {
                for item in pa_set {
                    st.insert_invalidation(item);
                }
            }
            if let Some(ma_set) = st.aliases.may_alias.get(&local).cloned() {
                for h_idx in ma_set {
                    if !excluded_handles.contains(&h_idx)
                        && let Some(Some(info)) = self.local_info.get(h_idx)
                    {
                        st.slots[h_idx].widen_unknown_mutation(info.variant_count);
                        st.sync_store_borrow_write(h_idx);
                        st.invalidate_handle(h_idx);
                    }
                }
            }
            let mut store_targets = BTreeSet::new();
            if let Some(&s_idx) = st.aliases.alias.get(&local)
                && self
                    .local_store_info
                    .get(s_idx)
                    .is_some_and(Option::is_some)
            {
                store_targets.insert(s_idx);
            }
            if let Some(roots) = st.aliases.local_roots.get(&local) {
                for &r in roots {
                    if self.local_store_info.get(r).is_some_and(Option::is_some) {
                        store_targets.insert(r);
                    }
                }
            }
            for s_idx in store_targets {
                if let Some(Some(elem_info)) = self.local_store_info.get(s_idx) {
                    st.store_slots[s_idx] =
                        StoreProtocolSummary::unknown_store(elem_info.variant_count);
                    st.refinements.invalidate_store(s_idx);
                }
            }
        }
    }

    fn apply_place_effects(
        &self,
        st: &mut ProtoState,
        place: &Place,
        include_direct_mut_or_desc: bool,
        excluded_handles: &[usize],
    ) {
        if let PlaceRoot::Local(al) = place.root {
            if place.projections.is_empty() {
                self.apply_local_effects(st, al, include_direct_mut_or_desc, excluded_handles);
            } else {
                self.apply_local_effects(st, al, true, excluded_handles);
                if include_direct_mut_or_desc {
                    st.record_place_invalidation(place);
                }
            }
        }
    }

    fn apply_operand_effects(
        &self,
        st: &mut ProtoState,
        op: &Operand,
        include_direct_mut_or_desc: bool,
        excluded_handles: &[usize],
    ) {
        if let Operand::Copy(p) | Operand::Move(p) = op {
            self.apply_place_effects(st, p, include_direct_mut_or_desc, excluded_handles);
        }
    }

    fn apply_all_args(&self, st: &mut ProtoState, args: &[Operand], excluded_handles: &[usize]) {
        for arg in args {
            self.apply_operand_effects(st, arg, true, excluded_handles);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn step_call(
        &self,
        mut st: ProtoState,
        callee: &Callee,
        args: &[Operand],
        dest: &Place,
        target: BlockId,
        cancel: Option<BlockId>,
        in_states: &mut [Option<ProtoState>],
        queue: &mut VecDeque<usize>,
    ) -> Result<(), CheckError> {
        let cancel = &cancel;
        if let Some(cancel_target) = cancel {
            enqueue(*cancel_target, st.clone(), in_states, queue);
        }
        st.record_place_invalidation(dest);
        let dest_local = if dest.projections.is_empty() {
            match dest.root {
                PlaceRoot::Local(l) => Some(l),
                PlaceRoot::Capture(_) => None,
            }
        } else {
            None
        };
        if let Some(dl) = dest_local {
            let di = dl.index();
            st.caps.mut_cap_params.remove(&dl);
            st.caps.mut_cap_handles.remove(&dl);
            st.aliases.store_borrow_parents.remove(&di);
            st.refinements.eu_payload_slot.remove(&(dl, true));
            st.refinements.eu_payload_slot.remove(&(dl, false));
            st.refinements.store_op_of.remove(&dl);
            let mut dest_roots = BTreeSet::new();
            dest_roots.insert(di);
            if !is_pure_scalar_ty(self.tys, self.func.body.locals[di].ty) {
                for arg in args {
                    if let Operand::Copy(p) | Operand::Move(p) = arg
                        && let PlaceRoot::Local(al) = p.root
                        && self
                            .func
                            .body
                            .locals
                            .get(al.index())
                            .is_some_and(|ld| !is_pure_scalar_ty(self.tys, ld.ty))
                        && let Some(sr) = st.aliases.local_roots.get(&al)
                    {
                        dest_roots.extend(sr.iter().copied());
                    }
                }
            }
            st.aliases.local_roots.insert(dl, dest_roots);
        }

        match callee {
            Callee::Intrinsic(IntrinsicOp::ManagedCreate) => {
                if let Some(dl) = dest_local {
                    let di = dl.index();
                    let mut cap_params = ParamAliasSet::new();
                    let mut cap_handles = HandleCapSet::new();
                    for (fi, arg) in args.iter().enumerate() {
                        if let Operand::Copy(p) | Operand::Move(p) = arg
                            && let PlaceRoot::Local(al) = p.root
                        {
                            let field_proj = extract_field_proj(p);
                            if let Some(mcp) = st.caps.mut_cap_params.get(&al) {
                                cap_params.extend(mcp.iter().cloned());
                            }
                            if let Some(mch) = st.caps.mut_cap_handles.get(&al) {
                                for (sub_path, h_idx) in filter_handle_caps(mch, &field_proj) {
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
                                }
                            }
                            if p.projections.is_empty()
                                && self
                                    .func
                                    .body
                                    .locals
                                    .get(al.index())
                                    .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
                            {
                                if let Some(pa) = st.aliases.param_alias.get(&al) {
                                    cap_params.extend(pa.iter().cloned());
                                }
                                if let Some(ma) = st.aliases.may_alias.get(&al) {
                                    for &h_idx in ma {
                                        cap_handles.insert((vec![FieldIdx(fi as u32)], h_idx));
                                    }
                                }
                            }
                        }
                    }
                    if !cap_params.is_empty() {
                        st.caps.mut_cap_params.insert(dl, cap_params);
                    }
                    if !cap_handles.is_empty() {
                        st.caps.mut_cap_handles.insert(dl, cap_handles);
                    }
                    if let Some(Some(info)) = self.local_info.get(di) {
                        st.aliases.alias.insert(dl, di);
                        let mut s = BTreeSet::new();
                        s.insert(di);
                        st.aliases.may_alias.insert(dl, s);
                        st.invalidate_handle(di);
                        if let Some((_, sf)) = info.state_field {
                            let field_state = args
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
                        }
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::OwnedDynCreate) => {
                if let Some(dl) = dest_local {
                    let mut cap_params = ParamAliasSet::new();
                    let mut cap_handles = HandleCapSet::new();
                    for arg in args {
                        if let Operand::Copy(p) | Operand::Move(p) = arg
                            && let PlaceRoot::Local(al) = p.root
                        {
                            let field_proj = extract_field_proj(p);
                            if let Some(mcp) = st.caps.mut_cap_params.get(&al) {
                                cap_params.extend(mcp.iter().cloned());
                            }
                            if let Some(mch) = st.caps.mut_cap_handles.get(&al) {
                                for (_, h_idx) in filter_handle_caps(mch, &field_proj) {
                                    cap_handles.insert((Vec::new(), h_idx));
                                }
                            }
                            if p.projections.is_empty()
                                && self
                                    .func
                                    .body
                                    .locals
                                    .get(al.index())
                                    .is_some_and(|ld| is_mut_ref_or_desc_ty(self.tys, ld.ty))
                            {
                                if let Some(pa) = st.aliases.param_alias.get(&al) {
                                    cap_params.extend(pa.iter().cloned());
                                }
                                if let Some(ma) = st.aliases.may_alias.get(&al) {
                                    for &h_idx in ma {
                                        cap_handles.insert((Vec::new(), h_idx));
                                    }
                                }
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
            Callee::Intrinsic(IntrinsicOp::StoreNew) => {
                if let Some(dl) = dest_local {
                    let di = dl.index();
                    if self.local_store_info.get(di).is_some_and(Option::is_some) {
                        st.store_slots[di] = StoreProtocolSummary::empty_store();
                        st.aliases.alias.insert(dl, di);
                        st.refinements.invalidate_store(di);
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::StoreReserve) => {
                if let Some(store_arg) = args.first() {
                    for item in self.collect_store_invalidation_targets(&st, store_arg) {
                        st.insert_invalidation(item);
                    }
                    let (_, may_stores) = self.resolve_store_targets(&st, store_arg);
                    for s_idx in may_stores {
                        st.refinements.invalidate_store(s_idx);
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::StorePush) => {
                if let Some(store_arg) = args.first() {
                    let (_, may_stores) = self.resolve_store_targets(&st, store_arg);
                    let inv_targets = self.collect_store_invalidation_targets(&st, store_arg);
                    let newly_invalidated: Vec<(usize, Vec<FieldIdx>)> = inv_targets
                        .iter()
                        .filter(|item| !st.invalidated.contains(*item))
                        .cloned()
                        .collect();
                    let pre_summaries: Vec<(usize, StoreProtocolSummary)> = may_stores
                        .iter()
                        .map(|&s_idx| (s_idx, st.store_slots[s_idx]))
                        .collect();
                    let elem_info = may_stores
                        .iter()
                        .next()
                        .and_then(|&s| self.local_store_info[s])
                        .or_else(|| {
                            args.get(1).and_then(|ea| match ea {
                                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                                    match p.root {
                                        PlaceRoot::Local(el) => {
                                            self.local_info.get(el.index()).and_then(|o| *o)
                                        }
                                        PlaceRoot::Capture(_) => None,
                                    }
                                }
                                Operand::Copy(_)
                                | Operand::Move(_)
                                | Operand::Const(_)
                                | Operand::Pooled(_) => None,
                            })
                        });
                    let incoming_slot = elem_info.zip(args.get(1)).map(|(info, elem_arg)| {
                        self.resolve_incoming_slot(&st, elem_arg, info.variant_count)
                    });

                    // Conservative post-push state (applies on `.ok`; retracted on `.err`).
                    for item in inv_targets {
                        st.insert_invalidation(item);
                    }
                    for &s_idx in &may_stores {
                        st.refinements.invalidate_store(s_idx);
                        if let Some(slot) = incoming_slot {
                            st.store_slots[s_idx].on_push_success(slot);
                        }
                    }

                    if let Some(dl) = dest_local {
                        if let Some(slot) = incoming_slot {
                            st.refinements.eu_payload_slot.insert((dl, false), slot);
                        }
                        st.refinements.store_op_of.insert(
                            dl,
                            StoreOpOutcome::Push {
                                pre_summaries,
                                newly_invalidated,
                            },
                        );
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::StorePop) => {
                if let Some(store_arg) = args.first() {
                    let (must_store, may_stores) = self.resolve_store_targets(&st, store_arg);
                    let inv_targets = self.collect_store_invalidation_targets(&st, store_arg);
                    let newly_invalidated: Vec<(usize, Vec<FieldIdx>)> = inv_targets
                        .iter()
                        .filter(|item| !st.invalidated.contains(*item))
                        .cloned()
                        .collect();
                    let elem_info = may_stores
                        .iter()
                        .next()
                        .and_then(|&s| self.local_store_info[s])
                        .or_else(|| {
                            dest_local.and_then(|dl| {
                                let dty = self.func.body.locals[dl.index()].ty;
                                match self.tys.get(dty) {
                                    Some(TyKind::ErrorUnion { ok, .. }) => {
                                        zil_semantics::proto_handle_info(self.tys, *ok)
                                    }
                                    _ => None,
                                }
                            })
                        });
                    let popped_slot = elem_info.map(|info| {
                        let mut iter = may_stores.iter().copied();
                        if let Some(first) = iter.next() {
                            let mut acc = st.store_slots[first].element_slot(info.variant_count);
                            for other in iter {
                                acc.join(&st.store_slots[other].element_slot(info.variant_count));
                            }
                            acc
                        } else {
                            HandleProtocolSlot::unknown(info.variant_count)
                        }
                    });

                    // Conservative post-pop state (applies on `.ok`; retracted on `.err`).
                    for item in inv_targets {
                        st.insert_invalidation(item);
                    }
                    for &s_idx in &may_stores {
                        st.refinements.invalidate_store(s_idx);
                    }

                    if let Some(dl) = dest_local {
                        if let Some(slot) = popped_slot {
                            st.refinements.eu_payload_slot.insert((dl, true), slot);
                        }
                        let unique_store = if may_stores.len() == 1 {
                            must_store
                        } else {
                            None
                        };
                        let tracked_stores: Vec<usize> = may_stores.iter().copied().collect();
                        st.refinements.store_op_of.insert(
                            dl,
                            StoreOpOutcome::Pop {
                                unique_store,
                                tracked_stores,
                                newly_invalidated,
                            },
                        );
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::StoreLen | IntrinsicOp::StoreCapacity) => {}
            Callee::Intrinsic(IntrinsicOp::StoreBorrow { .. }) => {
                // Borrowing an element (`&E` or `&mut E`) is NOT an actual modification:
                // do not invalidate the store at borrow creation. Propagate `param_alias`
                // and `store_borrow_parents` so only actual writes through the borrow
                // invalidate the store and update its element protocol summary.
                if let Some(dl) = dest_local
                    && let Some(store_arg) = args.first()
                {
                    let di = dl.index();
                    let (_, may_stores) = self.resolve_store_targets(&st, store_arg);
                    let store_params = self.collect_store_invalidation_targets(&st, store_arg);
                    if !store_params.is_empty() {
                        st.aliases.param_alias.insert(dl, store_params);
                    }
                    if let Some(Some(elem_info)) = self.local_info.get(di) {
                        let borrowed_slot = {
                            let mut iter = may_stores.iter().copied();
                            if let Some(first) = iter.next() {
                                let mut acc =
                                    st.store_slots[first].element_slot(elem_info.variant_count);
                                for other in iter {
                                    acc.join(
                                        &st.store_slots[other]
                                            .element_slot(elem_info.variant_count),
                                    );
                                }
                                acc
                            } else {
                                HandleProtocolSlot::unknown(elem_info.variant_count)
                            }
                        };
                        st.slots[di] = borrowed_slot;
                        st.aliases.alias.insert(dl, di);
                        let mut s = BTreeSet::new();
                        s.insert(di);
                        st.aliases.may_alias.insert(dl, s);
                        if !may_stores.is_empty() {
                            st.aliases.store_borrow_parents.insert(di, may_stores);
                        }
                        st.invalidate_handle(di);
                    }
                }
            }
            Callee::Intrinsic(IntrinsicOp::AsyncRegistrationValid) => {
                if let Some(a0) = args.first()
                    && let Some(h_idx) = st.resolve_handle(a0)
                    && let Some(dl) = dest_local
                {
                    st.refinements.discr_of.insert(
                        dl,
                        DiscrTarget::ValidCheck {
                            handle: h_idx,
                            inverted: false,
                        },
                    );
                }
            }
            Callee::Intrinsic(
                IntrinsicOp::TaskRequestCancel | IntrinsicOp::AsyncRegistrationRequestClose,
            ) => {
                if let Some(a0) = args.first()
                    && let Some(h_idx) = st.resolve_handle(a0)
                {
                    let cur = st.slots[h_idx].states;
                    let mut next = ProtocolStateSet(0);
                    if (cur.0 & (ProtocolStateSet::OPEN | ProtocolStateSet::CLOSING)) != 0 {
                        next = next.union(ProtocolStateSet(
                            ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED,
                        ));
                    }
                    if (cur.0 & ProtocolStateSet::CLOSED) != 0 {
                        next = next.union(ProtocolStateSet(ProtocolStateSet::CLOSED));
                    }
                    st.slots[h_idx].states = next;
                    st.sync_store_borrow_write(h_idx);
                    st.invalidate_handle(h_idx);
                }
            }
            Callee::Intrinsic(
                IntrinsicOp::TaskPollClose | IntrinsicOp::AsyncRegistrationPollClose,
            ) => {
                if let Some(a0) = args.first()
                    && let Some(h_idx) = st.resolve_handle(a0)
                {
                    st.slots[h_idx].states =
                        ProtocolStateSet(ProtocolStateSet::CLOSING | ProtocolStateSet::CLOSED);
                    st.sync_store_borrow_write(h_idx);
                    st.invalidate_handle(h_idx);
                    if let Some(dl) = dest_local {
                        st.refinements.poll_of.insert(
                            dl,
                            (
                                h_idx,
                                ProtocolStateSet(ProtocolStateSet::CLOSING),
                                ProtocolStateSet(ProtocolStateSet::CLOSED),
                            ),
                        );
                    }
                }
            }
            Callee::Intrinsic(
                IntrinsicOp::TaskFinishDestroy | IntrinsicOp::AsyncRegistrationFinishDestroy,
            ) => {
                if let Some(a0) = args.first()
                    && let Some(h_idx) = st.resolve_handle(a0)
                {
                    if !st.slots[h_idx]
                        .states
                        .is_subset_of(ProtocolStateSet(ProtocolStateSet::CLOSED))
                    {
                        return Err(
                            self.fail("finish_destroy called on protocol handle not proved Closed")
                        );
                    }
                    st.slots[h_idx].retired = true;
                    st.sync_store_borrow_write(h_idx);
                    st.invalidate_handle(h_idx);
                }
            }
            Callee::Intrinsic(IntrinsicOp::RegistrationClose) => {
                if let Some(a0) = args.first()
                    && let Some(h_idx) = st.resolve_handle(a0)
                {
                    st.slots[h_idx].retired = true;
                    st.sync_store_borrow_write(h_idx);
                    st.invalidate_handle(h_idx);
                }
            }

            Callee::Direct(cid) => {
                self.step_direct_call(&mut st, *cid, args, dest_local)?;
            }
            Callee::Closure { closure, .. } => {
                self.apply_place_effects(&mut st, closure, true, &[]);
                self.apply_all_args(&mut st, args, &[]);
            }
            Callee::DynSlot { object, slot } => {
                let slot_is_mut = match object {
                    Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                        if let PlaceRoot::Local(ol) = p.root
                            && let Some(ld) = self.func.body.locals.get(ol.index())
                            && let Some(TyKind::Ref { pointee, .. }) = self.tys.get(ld.ty)
                            && let Some(TyKind::Dyn(dyn_id) | TyKind::OwnedDyn { def: dyn_id, .. }) =
                                self.tys.get(*pointee)
                            && let Some(def) = self.tys.get_dyn(*dyn_id)
                            && let Some(sig) = def.slots.get(*slot as usize)
                        {
                            !matches!(sig.receiver, Some(Mutability::Shared))
                        } else {
                            true
                        }
                    }
                    Operand::Copy(_)
                    | Operand::Move(_)
                    | Operand::Const(_)
                    | Operand::Pooled(_) => true,
                };
                self.apply_operand_effects(&mut st, object, slot_is_mut, &[]);
                self.apply_all_args(&mut st, args, &[]);
            }
            Callee::Intrinsic(_) | Callee::Indirect(_) | Callee::Foreign(_) => {
                self.apply_all_args(&mut st, args, &[]);
            }
        }

        enqueue(target, st, in_states, queue);
        Ok(())
    }

    fn step_direct_call(
        &self,
        st: &mut ProtoState,
        cid: FnId,
        args: &[Operand],
        dest_local: Option<Local>,
    ) -> Result<(), CheckError> {
        if let Some(callee_fn) = self.module.functions.get(cid.index())
            && !callee_fn.sig.state_contracts.is_empty()
        {
            let mut pend_by_handle: HashMap<usize, ProtocolStateSet> = HashMap::new();
            let mut ready_by_handle: HashMap<usize, ProtocolStateSet> = HashMap::new();
            let mut ok_by_handle: HashMap<usize, ProtocolStateSet> = HashMap::new();
            let mut err_by_handle: HashMap<usize, ProtocolStateSet> = HashMap::new();
            // 1. Check all preconditions BEFORE applying postconditions.
            for c in &callee_fn.sig.state_contracts {
                match *c {
                    StateContractClause::RequiresState { param, states } => {
                        let Some(arg) = args.get(param as usize) else {
                            return Err(self.fail("callee requires(state(...)) argument missing"));
                        };
                        let Some(h_idx) = st.resolve_handle(arg) else {
                            return Err(self.fail(
                                                "callee requires(state(...)) argument has unknown protocol handle state",
                                            ));
                        };
                        if !st.slots[h_idx].states.is_subset_of(states) {
                            return Err(
                                self.fail("callee requires(state(...)) precondition not satisfied")
                            );
                        }
                    }
                    StateContractClause::RequiresApart { param_a, param_b } => {
                        let (Some(aa), Some(ab)) =
                            (args.get(param_a as usize), args.get(param_b as usize))
                        else {
                            return Err(self.fail("callee requires(apart(...)) argument missing"));
                        };
                        let collect_roots = |op: &Operand| -> Option<BTreeSet<usize>> {
                            match op {
                                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                                    let PlaceRoot::Local(l) = p.root else {
                                        return None;
                                    };
                                    let mut roots = BTreeSet::new();
                                    if let Some(lr) = st.aliases.local_roots.get(&l) {
                                        roots.extend(lr.iter().copied());
                                    } else if let Some(&h) = st.aliases.alias.get(&l) {
                                        roots.insert(h);
                                    } else {
                                        return None;
                                    }
                                    if let Some(ma) = st.aliases.may_alias.get(&l) {
                                        roots.extend(ma.iter().copied());
                                    }
                                    if let Some(mch) = st.caps.mut_cap_handles.get(&l) {
                                        for (_, h_idx) in mch {
                                            roots.insert(*h_idx);
                                        }
                                    }
                                    if roots.is_empty() { None } else { Some(roots) }
                                }
                                Operand::Copy(_)
                                | Operand::Move(_)
                                | Operand::Const(_)
                                | Operand::Pooled(_) => None,
                            }
                        };
                        let (Some(roots_a), Some(roots_b)) = (collect_roots(aa), collect_roots(ab))
                        else {
                            return Err(self.fail(
                                                "callee requires(apart(...)) argument identity cannot be proved distinct",
                                            ));
                        };
                        // Two distinct parameters can only be assumed disjoint if
                        // at least one is local-owned, or exclusive (&mut / by-value
                        // without shared/desc capabilities), or explicitly covered by
                        // the caller's own `requires(apart(...))`.
                        let param_can_alias_at_entry = |idx: usize| -> bool {
                            if idx >= self.param_count {
                                return false;
                            }
                            let pty = self.func.body.locals[idx].ty;
                            matches!(
                                self.tys.get(pty),
                                Some(
                                    TyKind::Ref {
                                        mutability: Mutability::Shared,
                                        ..
                                    } | TyKind::Desc(_)
                                )
                            ) || self.entry_mut_cap_params.contains_key(&Local(idx as u32))
                                || self.entry_mut_cap_handles.contains_key(&Local(idx as u32))
                        };
                        for &ra in &roots_a {
                            for &rb in &roots_b {
                                if ra == rb {
                                    return Err(self.fail(
                                                        "callee requires(apart(...)) violated: aliasing protocol handles",
                                                    ));
                                }
                                if param_can_alias_at_entry(ra)
                                    && param_can_alias_at_entry(rb)
                                    && !self.declared_apart(ra, rb)
                                {
                                    return Err(self.fail(
                                                        "callee requires(apart(...)) violated: shared parameters may alias",
                                                    ));
                                }
                            }
                        }
                    }
                    StateContractClause::ProvidesRetired { .. }
                    | StateContractClause::ProvidesState { .. }
                    | StateContractClause::ProvidesOnPending { .. }
                    | StateContractClause::ProvidesOnReady { .. }
                    | StateContractClause::ProvidesOnOk { .. }
                    | StateContractClause::ProvidesOnErr { .. }
                    | StateContractClause::InvalidationUpperBound
                    | StateContractClause::Invalidates { .. }
                    | StateContractClause::AllowsRollback { .. } => {}
                }
            }
            // 2. Apply postconditions & projection-scoped invalidation upper bounds.
            let callee_has_inv_upper = callee_fn
                .sig
                .state_contracts
                .contains(&StateContractClause::InvalidationUpperBound);
            let mut contracted_handles = Vec::new();
            let mut callee_invalidated_targets: Vec<(usize, ContractFieldPath)> = Vec::new();
            for c in &callee_fn.sig.state_contracts {
                match *c {
                    StateContractClause::RequiresState { .. }
                    | StateContractClause::RequiresApart { .. }
                    | StateContractClause::InvalidationUpperBound => {}
                    StateContractClause::Invalidates { param, path } => {
                        callee_invalidated_targets.push((param as usize, path));
                        if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(param as usize)
                        {
                            for (caller_p, mut caller_path) in st.resolve_param_places(p) {
                                caller_path.extend_from_slice(path.as_slice());
                                st.insert_invalidation((caller_p, caller_path));
                            }
                        }
                    }
                    StateContractClause::AllowsRollback { param } => {
                        if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(param as usize)
                            && p.projections.is_empty()
                            && let PlaceRoot::Local(al) = p.root
                            && let Some(ma_set) = st.aliases.may_alias.get(&al).cloned()
                        {
                            for h_idx in ma_set {
                                if let Some(Some(info)) = self.local_info.get(h_idx) {
                                    st.slots[h_idx].history =
                                        ProtocolStateSet::all_variants(info.variant_count);
                                    st.slots[h_idx].rolled_back = true;
                                    st.sync_store_borrow_write(h_idx);
                                }
                            }
                        }
                    }
                    StateContractClause::ProvidesRetired { param } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            st.slots[h_idx].retired = true;
                            st.sync_store_borrow_write(h_idx);
                            st.invalidate_handle(h_idx);
                            contracted_handles.push(h_idx);
                        }
                    }
                    StateContractClause::ProvidesState { param, states } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            st.set_handle_state(h_idx, states);
                            st.invalidate_handle(h_idx);
                            contracted_handles.push(h_idx);
                            if let Operand::Copy(p) | Operand::Move(p) = arg
                                && p.projections.is_empty()
                                && let PlaceRoot::Local(al) = p.root
                                && let Some(ma_set) = st.aliases.may_alias.get(&al).cloned()
                            {
                                for other_h in ma_set {
                                    if other_h != h_idx {
                                        st.weak_update_handle_state(other_h, states);
                                        st.invalidate_handle(other_h);
                                    }
                                }
                            }
                        }
                    }
                    StateContractClause::ProvidesOnPending { param, states } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            pend_by_handle.insert(h_idx, states);
                            contracted_handles.push(h_idx);
                        }
                    }
                    StateContractClause::ProvidesOnReady { param, states } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            ready_by_handle.insert(h_idx, states);
                            contracted_handles.push(h_idx);
                        }
                    }
                    StateContractClause::ProvidesOnOk { param, states } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            ok_by_handle.insert(h_idx, states);
                            contracted_handles.push(h_idx);
                        }
                    }
                    StateContractClause::ProvidesOnErr { param, states } => {
                        if let Some(arg) = args.get(param as usize)
                            && let Some(h_idx) = st.resolve_handle(arg)
                        {
                            err_by_handle.insert(h_idx, states);
                            contracted_handles.push(h_idx);
                        }
                    }
                }
            }
            if callee_has_inv_upper {
                // Projection-scoped invalidation: widen protocol handles whose
                // direct state field or nested capability path overlaps `path`.
                for &(c_param, path) in &callee_invalidated_targets {
                    if let Some(arg) = args.get(c_param)
                        && path.len == 0
                    {
                        let (_, may_stores) = self.resolve_store_targets(st, arg);
                        for s_idx in may_stores {
                            if let Some(Some(elem_info)) = self.local_store_info.get(s_idx) {
                                st.store_slots[s_idx] =
                                    StoreProtocolSummary::unknown_store(elem_info.variant_count);
                                st.refinements.invalidate_store(s_idx);
                            }
                        }
                    }
                    if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(c_param)
                        && p.projections.is_empty()
                        && let PlaceRoot::Local(al) = p.root
                    {
                        if path.len == 0 {
                            self.apply_local_effects(st, al, false, &contracted_handles);
                        } else {
                            if let Some(mch) = st.caps.mut_cap_handles.get(&al).cloned() {
                                for (cap_path, h_idx) in mch {
                                    if path.overlaps(&cap_path)
                                        && !contracted_handles.contains(&h_idx)
                                        && let Some(Some(info)) = self.local_info.get(h_idx)
                                    {
                                        st.slots[h_idx].widen_unknown_mutation(info.variant_count);
                                        st.sync_store_borrow_write(h_idx);
                                        st.invalidate_handle(h_idx);
                                    }
                                }
                            }
                            if let Some(mcp) = st.caps.mut_cap_params.get(&al).cloned() {
                                for (p_idx, cap_path) in mcp {
                                    if path.overlaps(&cap_path) {
                                        st.insert_invalidation((p_idx, cap_path));
                                    }
                                }
                            }
                        }
                        if let Some(ma_set) = st.aliases.may_alias.get(&al).cloned() {
                            for h_idx in ma_set {
                                if !contracted_handles.contains(&h_idx)
                                    && let Some(Some(info)) = self.local_info.get(h_idx)
                                {
                                    let touches_state = match info.state_field {
                                        None => true,
                                        Some((_, sf)) => path.overlaps(&[sf]),
                                    };
                                    if touches_state {
                                        st.set_handle_state(
                                            h_idx,
                                            ProtocolStateSet::all_variants(info.variant_count),
                                        );
                                        st.invalidate_handle(h_idx);
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Callee has no closed invalidation upper bound: conservatively
                // apply effects to all &mut/Desc/capability-bearing arguments
                // while preserving explicitly contracted handle postconditions.
                self.apply_all_args(st, args, &contracted_handles);
            }
            if let Some(dl) = dest_local {
                for (&h_idx, &pend) in &pend_by_handle {
                    let ready = ready_by_handle
                        .get(&h_idx)
                        .copied()
                        .unwrap_or(st.slots[h_idx].states);
                    st.refinements.poll_of.insert(dl, (h_idx, pend, ready));
                }
                for (&h_idx, &ready) in &ready_by_handle {
                    if !pend_by_handle.contains_key(&h_idx) {
                        st.refinements
                            .poll_of
                            .insert(dl, (h_idx, st.slots[h_idx].states, ready));
                    }
                }
                if !ok_by_handle.is_empty() || !err_by_handle.is_empty() {
                    let mut entries = Vec::new();
                    let mut seen = Vec::new();
                    for (&h_idx, &ok_st) in &ok_by_handle {
                        let err_st = err_by_handle.get(&h_idx).copied();
                        let joined =
                            err_st.map_or(st.slots[h_idx].states.union(ok_st), |e| ok_st.union(e));
                        st.set_handle_state(h_idx, joined);
                        st.invalidate_handle(h_idx);
                        entries.push((h_idx, Some(ok_st), err_st));
                        seen.push(h_idx);
                    }
                    for (&h_idx, &err_st) in &err_by_handle {
                        if !seen.contains(&h_idx) {
                            let joined = st.slots[h_idx].states.union(err_st);
                            st.set_handle_state(h_idx, joined);
                            st.invalidate_handle(h_idx);
                            entries.push((h_idx, None, Some(err_st)));
                        }
                    }
                    st.refinements.eu_of.insert(dl, entries);
                }
            }
        } else {
            self.apply_all_args(st, args, &[]);
        }

        Ok(())
    }
}
