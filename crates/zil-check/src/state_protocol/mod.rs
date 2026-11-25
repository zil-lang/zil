//! Forward CFG verification of built-in and user-defined finite-state protocol contracts,
//! projection invalidation upper bounds, history rollback `H`, and `Replace` obligation transfer.

mod calls;
mod env;
mod transfer;

use std::collections::{BTreeSet, HashMap, VecDeque};

use zil_ir::body::{
    BlockId, ContractFieldPath, FnId, Function, Instruction, InstructionKind, Local, Module,
    Mutability, Operand, Place, PlaceRoot, Projection, ProtocolStateSet, StateContractClause,
    Terminator, TyKind, TyTable,
};
use zil_semantics::{
    HandleProtocolSlot, ProtoHandleInfo, StoreProtocolSummary, proto_handle_info,
    store_element_proto_info,
};

use self::env::{
    AliasEnv, CapabilityEnv, DiscrTarget, EntryCapScan, HandleCapSet, NestedCapSet,
    NestedCapTarget, ParamAliasSet, ProtoState, RefinementEnv, StoreOpOutcome, is_describable_ty,
    is_error_union_ty, is_mut_ref_or_desc_ty, is_poll_ty, strip_indirection,
};
use crate::{CheckBudget, CheckError, CheckErrorKind};

/// Independently verifies built-in and user-defined `StateContractClause`s via
/// budget-bounded forward CFG abstract interpretation over finite state bitmasks,
/// `retired` flags, `ErrorUnion`/`Poll` exit refinements, and `apart(a, b)` aliasing.
pub(crate) fn verify_fn_state_contracts(
    module: &Module,
    fn_id: FnId,
    func: &Function,
    budget: CheckBudget,
) -> Result<(), CheckError> {
    let (verifier, entry_state) = ProtocolVerifier::validate_and_build(module, fn_id, func)?;
    verifier.run(entry_state, budget)
}

fn enqueue(
    target: BlockId,
    st: ProtoState,
    in_states: &mut [Option<ProtoState>],
    queue: &mut VecDeque<usize>,
) {
    let ti = target.index();
    if ti >= in_states.len() {
        return;
    }
    match &mut in_states[ti] {
        None => {
            in_states[ti] = Some(st);
            queue.push_back(ti);
        }
        Some(existing) => {
            if existing.join(&st) {
                queue.push_back(ti);
            }
        }
    }
}

struct ProtocolVerifier<'a> {
    module: &'a Module,
    fn_id: FnId,
    func: &'a Function,
    tys: &'a TyTable,
    param_count: usize,
    local_info: Vec<Option<ProtoHandleInfo>>,
    local_store_info: Vec<Option<ProtoHandleInfo>>,
    has_invalidation_upper: bool,
    declared_invalidations: Vec<(usize, ContractFieldPath)>,
    allows_rollback: Vec<bool>,
    declared_requires_state: Vec<Option<ProtocolStateSet>>,
    entry_mut_cap_params: HashMap<Local, ParamAliasSet>,
    entry_mut_cap_handles: HashMap<Local, HandleCapSet>,
}

impl<'a> ProtocolVerifier<'a> {
    fn fail(&self, reason: &'static str) -> CheckError {
        CheckError {
            function: self.fn_id,
            kind: CheckErrorKind::CertificateShape(reason),
        }
    }

    fn declared_apart(&self, pa: usize, pb: usize) -> bool {
        self.func.sig.state_contracts.iter().any(|c| {
            matches!(
                *c,
                StateContractClause::RequiresApart { param_a, param_b }
                    if (param_a as usize == pa && param_b as usize == pb)
                        || (param_a as usize == pb && param_b as usize == pa)
            )
        })
    }

    /// Resolves `(must_store, may_stores)` root local indices for an `OwnedStore<E>`
    /// operand where `E` is a tracked protocol handle type.
    pub(super) fn resolve_store_targets(
        &self,
        st: &ProtoState,
        op: &Operand,
    ) -> (Option<usize>, BTreeSet<usize>) {
        let mut may = BTreeSet::new();
        let (Operand::Copy(p) | Operand::Move(p)) = op else {
            return (None, may);
        };
        if !p.projections.is_empty() {
            return (None, may);
        }
        let PlaceRoot::Local(al) = p.root else {
            return (None, may);
        };
        let must = st
            .aliases
            .alias
            .get(&al)
            .copied()
            .filter(|&i| self.local_store_info.get(i).is_some_and(Option::is_some));
        if let Some(m) = must {
            may.insert(m);
        }
        if let Some(roots) = st.aliases.local_roots.get(&al) {
            for &r in roots {
                if self.local_store_info.get(r).is_some_and(Option::is_some) {
                    may.insert(r);
                }
            }
        }
        (must, may)
    }

    /// Applies `.err` failure recovery for `StorePush` (OOM) and `StorePop` (empty container).
    fn apply_store_err_recovery(branch_st: &mut ProtoState, outcome: &StoreOpOutcome) {
        match outcome {
            StoreOpOutcome::Push {
                pre_summaries,
                newly_invalidated,
            } => {
                for &(s_idx, pre) in pre_summaries {
                    if let Some(slot) = branch_st.store_slots.get_mut(s_idx) {
                        *slot = pre;
                    }
                }
                for item in newly_invalidated {
                    branch_st.invalidated.remove(item);
                }
            }
            StoreOpOutcome::Pop {
                unique_store,
                newly_invalidated,
                ..
            } => {
                if let Some(s_idx) = *unique_store
                    && let Some(slot) = branch_st.store_slots.get_mut(s_idx)
                {
                    *slot = StoreProtocolSummary::empty_store();
                }
                for item in newly_invalidated {
                    branch_st.invalidated.remove(item);
                }
            }
        }
    }

    fn place_targets_state_field(&self, place: &Place, st: &ProtoState) -> Option<usize> {
        let PlaceRoot::Local(rl) = place.root else {
            return None;
        };
        let &h_idx = st.aliases.alias.get(&rl)?;
        let (_, sf) = self.local_info.get(h_idx).and_then(|o| *o)?.state_field?;
        let (last, prefix) = place.projections.split_last()?;
        if matches!(last, Projection::Field(f) if f.field == sf)
            && prefix.iter().all(|p| matches!(p, Projection::Deref))
        {
            Some(h_idx)
        } else {
            None
        }
    }

    fn resolve_incoming_slot(
        &self,
        st: &ProtoState,
        op: &Operand,
        variant_count: usize,
    ) -> HandleProtocolSlot {
        if let Operand::Copy(src) | Operand::Move(src) = op
            && src.projections.is_empty()
            && let PlaceRoot::Local(sl) = src.root
        {
            if let Some(ma) = st.aliases.may_alias.get(&sl)
                && !ma.is_empty()
            {
                let mut iter = ma.iter().copied();
                if let Some(first) = iter.next() {
                    let mut acc = st.slots[first];
                    for other in iter {
                        acc.join(&st.slots[other]);
                    }
                    return acc;
                }
            }
            if let Some(&src_h) = st.aliases.alias.get(&sl)
                && self.local_info.get(src_h).is_some_and(Option::is_some)
            {
                return st.slots[src_h];
            }
            if let Some(&v) = st.refinements.enum_variant.get(&sl) {
                return HandleProtocolSlot::fresh(ProtocolStateSet::single(v));
            }
        }
        HandleProtocolSlot::unknown(variant_count)
    }

    fn validate_and_build(
        module: &'a Module,
        fn_id: FnId,
        func: &'a Function,
    ) -> Result<(Self, ProtoState), CheckError> {
        let tys = &module.tys;
        let fail = |reason: &'static str| CheckError {
            function: fn_id,
            kind: CheckErrorKind::CertificateShape(reason),
        };

        let nlocals = func.body.locals.len();
        let param_count = func.body.param_count as usize;
        let local_info: Vec<Option<ProtoHandleInfo>> = func
            .body
            .locals
            .iter()
            .map(|l| proto_handle_info(tys, l.ty))
            .collect();
        let local_store_info: Vec<Option<ProtoHandleInfo>> = func
            .body
            .locals
            .iter()
            .map(|l| store_element_proto_info(tys, l.ty))
            .collect();
        let mut init_slots: Vec<HandleProtocolSlot> = local_info
            .iter()
            .map(|info| {
                info.map_or(HandleProtocolSlot::empty(), |i| {
                    HandleProtocolSlot::unknown(i.variant_count)
                })
            })
            .collect();
        let init_store_slots: Vec<StoreProtocolSummary> = local_store_info
            .iter()
            .map(|info| {
                info.map_or(StoreProtocolSummary::empty_store(), |i| {
                    StoreProtocolSummary::unknown_store(i.variant_count)
                })
            })
            .collect();

        let mut has_poll_post = false;
        let mut has_eu_post = false;
        let mut has_invalidation_upper = false;
        let mut declared_invalidations: Vec<(usize, ContractFieldPath)> = Vec::new();
        let mut allows_rollback = vec![false; nlocals];
        let mut declared_requires_state: Vec<Option<ProtocolStateSet>> = vec![None; nlocals];
        for clause in &func.sig.state_contracts {
            match *clause {
                StateContractClause::InvalidationUpperBound => {
                    has_invalidation_upper = true;
                }
                StateContractClause::Invalidates { param, path } => {
                    has_invalidation_upper = true;
                    let p_idx = param as usize;
                    if p_idx >= param_count || p_idx >= func.sig.params.len() {
                        return Err(fail(
                            "invalidation contract references out-of-range parameter",
                        ));
                    }
                    if (path.len as usize) > ContractFieldPath::MAX_DEPTH {
                        return Err(fail("invalidation contract field path exceeds max depth"));
                    }
                    let mut cur_ty = func.sig.params[p_idx];
                    for &fidx in path.as_slice() {
                        cur_ty = strip_indirection(tys, cur_ty);
                        if let Some(TyKind::Adt { adt, .. }) = tys.get(cur_ty)
                            && let Some(def) = tys.get_adt(*adt)
                            && def.variants.len() == 1
                            && let Some(fdef) = def.variants[0].fields.get(fidx.0 as usize)
                        {
                            cur_ty = fdef.ty;
                        } else {
                            return Err(fail(
                                "invalidation contract field path does not match parameter ADT",
                            ));
                        }
                    }
                    declared_invalidations.push((p_idx, path));
                }
                StateContractClause::AllowsRollback { param } => {
                    let p_idx = param as usize;
                    if p_idx >= param_count || p_idx >= func.sig.params.len() {
                        return Err(fail("rollback contract references out-of-range parameter"));
                    }
                    if proto_handle_info(tys, func.sig.params[p_idx]).is_none() {
                        return Err(fail(
                            "rollback contract parameter is not a protocol handle type",
                        ));
                    }
                    allows_rollback[p_idx] = true;
                }
                StateContractClause::RequiresApart { param_a, param_b } => {
                    let pa = param_a as usize;
                    let pb = param_b as usize;
                    if pa == pb
                        || pa >= param_count
                        || pb >= param_count
                        || pa >= func.sig.params.len()
                        || pb >= func.sig.params.len()
                    {
                        return Err(fail(
                            "state contract apart(...) references invalid parameter",
                        ));
                    }
                }
                StateContractClause::RequiresState { param, states }
                | StateContractClause::ProvidesState { param, states }
                | StateContractClause::ProvidesOnPending { param, states }
                | StateContractClause::ProvidesOnReady { param, states }
                | StateContractClause::ProvidesOnOk { param, states }
                | StateContractClause::ProvidesOnErr { param, states } => {
                    if matches!(
                        clause,
                        StateContractClause::ProvidesOnPending { .. }
                            | StateContractClause::ProvidesOnReady { .. }
                    ) {
                        has_poll_post = true;
                    }
                    if matches!(
                        clause,
                        StateContractClause::ProvidesOnOk { .. }
                            | StateContractClause::ProvidesOnErr { .. }
                    ) {
                        has_eu_post = true;
                    }
                    let p_idx = param as usize;
                    if p_idx >= param_count || p_idx >= func.sig.params.len() {
                        return Err(fail("state contract references out-of-range parameter"));
                    }
                    let Some(info) = proto_handle_info(tys, func.sig.params[p_idx]) else {
                        return Err(fail(
                            "state contract parameter is not a protocol handle type",
                        ));
                    };
                    if !states.is_valid_for(info.variant_count) {
                        return Err(fail("state contract has invalid ProtocolStateSet bitmask"));
                    }
                    if matches!(clause, StateContractClause::RequiresState { .. }) {
                        init_slots[p_idx] = HandleProtocolSlot::fresh(states);
                        declared_requires_state[p_idx] = Some(states);
                    }
                }
                StateContractClause::ProvidesRetired { param } => {
                    let p_idx = param as usize;
                    if p_idx >= param_count || p_idx >= func.sig.params.len() {
                        return Err(fail("state contract references out-of-range parameter"));
                    }
                    if proto_handle_info(tys, func.sig.params[p_idx]).is_none() {
                        return Err(fail(
                            "state contract parameter is not a protocol handle type",
                        ));
                    }
                }
            }
        }
        if has_poll_post && !is_poll_ty(tys, func.sig.result) {
            return Err(fail(
                "on_pending/on_ready state contract requires Poll return type",
            ));
        }
        if has_eu_post && !is_error_union_ty(tys, func.sig.result) {
            return Err(fail(
                "on_ok/on_err state contract requires ErrorUnion return type",
            ));
        }

        let declared_apart = |pa: usize, pb: usize| -> bool {
            func.sig.state_contracts.iter().any(|c| {
                matches!(
                    *c,
                    StateContractClause::RequiresApart { param_a, param_b }
                        if (param_a as usize == pa && param_b as usize == pb)
                            || (param_a as usize == pb && param_b as usize == pa)
                )
            })
        };

        let mut entry_alias = HashMap::new();
        let mut entry_may_alias: HashMap<Local, BTreeSet<usize>> = HashMap::new();
        let mut entry_local_roots: HashMap<Local, BTreeSet<usize>> = HashMap::new();
        for (i, info) in local_info.iter().enumerate() {
            entry_alias.insert(Local(i as u32), i);
            let mut root_set = BTreeSet::new();
            root_set.insert(i);
            entry_local_roots.insert(Local(i as u32), root_set);
            if info.is_some() {
                let mut s = BTreeSet::new();
                s.insert(i);
                entry_may_alias.insert(Local(i as u32), s);
            }
        }
        let mut entry_param_alias: HashMap<Local, ParamAliasSet> = HashMap::new();
        for i in 0..param_count {
            let mut s = ParamAliasSet::new();
            s.insert((i, Vec::new()));
            entry_param_alias.insert(Local(i as u32), s);
        }

        // Cross-parameter Desc may-aliasing: two `Desc` parameters (or a `Desc` and a shared `&`
        // parameter) with compatible pointee types may alias at function entry unless the function
        // explicitly declares `requires(apart(a, b))`.
        for i in 0..param_count {
            for j in (i + 1)..param_count {
                if declared_apart(i, j) {
                    continue;
                }
                let ti = func.body.locals[i].ty;
                let tj = func.body.locals[j].ty;
                let is_shared_ref = |ty| {
                    matches!(
                        tys.get(ty),
                        Some(TyKind::Ref {
                            mutability: Mutability::Shared,
                            ..
                        })
                    )
                };
                let is_desc_capable_shared = |ty| {
                    matches!(tys.get(ty), Some(TyKind::Desc(_)))
                        || (is_shared_ref(ty) && is_describable_ty(tys, ty))
                };
                let pair_can_alias = (is_desc_capable_shared(ti)
                    && (is_desc_capable_shared(tj) || is_shared_ref(tj)))
                    || (is_desc_capable_shared(tj) && is_shared_ref(ti));
                if !pair_can_alias {
                    continue;
                }
                let inner_i = strip_indirection(tys, ti);
                let inner_j = strip_indirection(tys, tj);
                let types_compatible = inner_i == inner_j
                    || matches!(
                        tys.get(inner_i),
                        Some(TyKind::Param(_) | TyKind::Dyn { .. } | TyKind::OwnedDyn { .. })
                    )
                    || matches!(
                        tys.get(inner_j),
                        Some(TyKind::Param(_) | TyKind::Dyn { .. } | TyKind::OwnedDyn { .. })
                    );
                if types_compatible {
                    entry_param_alias
                        .entry(Local(i as u32))
                        .or_default()
                        .insert((j, Vec::new()));
                    entry_param_alias
                        .entry(Local(j as u32))
                        .or_default()
                        .insert((i, Vec::new()));
                    if local_info[i].is_some() && local_info[j].is_some() {
                        entry_may_alias
                            .entry(Local(i as u32))
                            .or_default()
                            .insert(j);
                        entry_may_alias
                            .entry(Local(j as u32))
                            .or_default()
                            .insert(i);
                    }
                }
            }
        }

        let mut entry_mut_cap_params: HashMap<Local, ParamAliasSet> = HashMap::new();
        let mut entry_mut_cap_handles: HashMap<Local, HandleCapSet> = HashMap::new();
        for (i, local_decl) in func.body.locals.iter().enumerate().take(param_count) {
            let pty = local_decl.ty;
            if is_describable_ty(tys, pty)
                && let Some(ma) = entry_may_alias.get(&Local(i as u32))
            {
                let entry = entry_mut_cap_handles.entry(Local(i as u32)).or_default();
                for &h in ma {
                    entry.insert((Vec::new(), h));
                }
            }
            let scanner = EntryCapScan {
                tys,
                dyn_impls: &module.dyn_impls,
                param_idx: i,
            };
            let mut caps = ParamAliasSet::new();
            let mut nested_res = NestedCapSet::new();
            let mut prefix = Vec::new();
            let mut visiting = Vec::new();
            scanner.scan(
                pty,
                &mut prefix,
                false,
                false,
                0,
                &mut visiting,
                &mut caps,
                &mut nested_res,
            );
            if !is_mut_ref_or_desc_ty(tys, pty) && !caps.is_empty() {
                entry_mut_cap_params.insert(Local(i as u32), caps);
            }
            // Associate nested aliasable resource capabilities (e.g. `ctx.handle: Managed<Channel>`
            // inside `ctx: &Context` or `ctx: ContextBorrow<'a>`) with every compatible aliasable
            // protocol-handle parameter `j`, unless `requires(apart(i, j))` is declared.
            for (path, target) in &nested_res {
                for j in 0..param_count {
                    if j == i || local_info[j].is_none() || declared_apart(i, j) {
                        continue;
                    }
                    let tj = func.body.locals[j].ty;
                    let j_is_aliasable = matches!(
                        tys.get(tj),
                        Some(
                            TyKind::Desc(_)
                                | TyKind::Ref {
                                    mutability: Mutability::Shared,
                                    ..
                                }
                        )
                    );
                    if !j_is_aliasable {
                        continue;
                    }
                    let inner_j = strip_indirection(tys, tj);
                    let target_compatible = match *target {
                        NestedCapTarget::Wildcard => true,
                        NestedCapTarget::Exact(inner_i) => {
                            inner_i == inner_j
                                || matches!(
                                    tys.get(inner_i),
                                    Some(
                                        TyKind::Param(_)
                                            | TyKind::Dyn { .. }
                                            | TyKind::OwnedDyn { .. }
                                    )
                                )
                                || matches!(
                                    tys.get(inner_j),
                                    Some(
                                        TyKind::Param(_)
                                            | TyKind::Dyn { .. }
                                            | TyKind::OwnedDyn { .. }
                                    )
                                )
                        }
                    };
                    if target_compatible {
                        entry_mut_cap_handles
                            .entry(Local(i as u32))
                            .or_default()
                            .insert((path.clone(), j));
                        entry_param_alias
                            .entry(Local(j as u32))
                            .or_default()
                            .insert((i, path.clone()));
                    }
                }
            }
        }

        let entry_state = ProtoState {
            slots: init_slots,
            store_slots: init_store_slots,
            invalidated: ParamAliasSet::new(),
            aliases: AliasEnv {
                param_alias: entry_param_alias,
                alias: entry_alias,
                may_alias: entry_may_alias,
                local_roots: entry_local_roots,
                store_borrow_parents: HashMap::new(),
            },
            caps: CapabilityEnv {
                mut_cap_params: entry_mut_cap_params.clone(),
                mut_cap_handles: entry_mut_cap_handles.clone(),
                eu_payload_caps: HashMap::new(),
            },
            refinements: RefinementEnv::default(),
        };

        let verifier = Self {
            module,
            fn_id,
            func,
            tys,
            param_count,
            local_info,
            local_store_info,
            has_invalidation_upper,
            declared_invalidations,
            allows_rollback,
            declared_requires_state,
            entry_mut_cap_params,
            entry_mut_cap_handles,
        };
        Ok((verifier, entry_state))
    }

    fn run(&self, entry_state: ProtoState, budget: CheckBudget) -> Result<(), CheckError> {
        let nblocks = self.func.body.blocks.len();
        let mut in_states: Vec<Option<ProtoState>> = vec![None; nblocks];
        let mut queue: VecDeque<usize> = VecDeque::new();
        if nblocks > 0 {
            in_states[0] = Some(entry_state);
            queue.push_back(0);
        }

        let mut steps: u64 = 0;
        while let Some(bi) = queue.pop_front() {
            steps += 1;
            if steps > budget.steps.max(1024) {
                return Err(CheckError {
                    function: self.fn_id,
                    kind: CheckErrorKind::ResourceLimit,
                });
            }
            let mut st = in_states[bi].clone().expect("enqueued block has state");
            let block = &self.func.body.blocks[bi];

            for inst in &block.instructions {
                self.step_instruction(&mut st, inst);
            }

            match &block.terminator {
                Terminator::Goto { target } | Terminator::Assert { target, .. } => {
                    enqueue(*target, st, &mut in_states, &mut queue);
                }
                Terminator::Switch {
                    discr,
                    arms,
                    otherwise,
                } => {
                    self.step_switch(st, discr, arms, *otherwise, &mut in_states, &mut queue);
                }
                Terminator::Call {
                    callee,
                    args,
                    dest,
                    target,
                    cancel,
                } => {
                    self.step_call(
                        st,
                        callee,
                        args,
                        dest,
                        *target,
                        *cancel,
                        &mut in_states,
                        &mut queue,
                    )?;
                }
                Terminator::Return { value } => {
                    self.verify_return(&st, value)?;
                }
                Terminator::Unreachable | Terminator::Trap(_) => {}
            }
        }
        Ok(())
    }

    fn step_instruction(&self, st: &mut ProtoState, inst: &Instruction) {
        match &inst.kind {
            InstructionKind::Drop { place } => {
                st.record_place_invalidation(place);
            }
            InstructionKind::Replace { place, value, old } => {
                self.step_replace(st, place, value, *old);
            }
            InstructionKind::StorageDead(dl) => {
                st.clear_local(*dl);
            }
            InstructionKind::StorageLive(_)
            | InstructionKind::ScopeEnter(_)
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::RegisterDefer { .. } => {}
            InstructionKind::Assign { dest, value } => {
                self.step_assign(st, dest, value);
            }
        }
    }

    fn step_switch(
        &self,
        st: ProtoState,
        discr: &Operand,
        arms: &[(u128, BlockId)],
        otherwise: BlockId,
        in_states: &mut [Option<ProtoState>],
        queue: &mut VecDeque<usize>,
    ) {
        let target_info = match discr {
            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => match p.root {
                PlaceRoot::Local(l) => st.refinements.discr_of.get(&l).copied(),
                PlaceRoot::Capture(_) => None,
            },
            Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => None,
        };
        let mut covered_mask = ProtocolStateSet(0);
        let mut covered_ok = false;
        let mut covered_err = false;
        let mut covered_valid_true = false;
        let mut covered_valid_false = false;
        for &(val, target) in arms {
            let mut branch_st = st.clone();
            match target_info {
                Some(DiscrTarget::Poll(pl)) => {
                    if let Some((h_idx, pend_st, ready_st)) =
                        st.refinements.poll_of.get(&pl).copied()
                    {
                        if val == 0 {
                            branch_st.slots[h_idx].states = pend_st;
                        } else if val == 1 {
                            branch_st.slots[h_idx].states = ready_st;
                        }
                    }
                }
                Some(DiscrTarget::ErrorUnion(eu_l)) => {
                    if val == 1 {
                        covered_ok = true;
                    } else if val == 0 {
                        covered_err = true;
                        if let Some(outcome) = st.refinements.store_op_of.get(&eu_l) {
                            Self::apply_store_err_recovery(&mut branch_st, outcome);
                        }
                    }
                    if let Some(entries) = st.refinements.eu_of.get(&eu_l) {
                        for &(h_idx, ok_st, err_st) in entries {
                            if val == 1 {
                                if let Some(s) = ok_st {
                                    branch_st.slots[h_idx].states = s;
                                }
                            } else if val == 0
                                && let Some(s) = err_st
                            {
                                branch_st.slots[h_idx].states = s;
                            }
                        }
                    }
                }
                Some(DiscrTarget::HandleState(h_idx)) => {
                    let single = ProtocolStateSet::single(val as u32);
                    covered_mask = covered_mask.union(single);
                    let refined = branch_st.slots[h_idx].states.intersect(single);
                    if refined.is_empty() {
                        continue;
                    }
                    branch_st.slots[h_idx].states = refined;
                }
                Some(DiscrTarget::NullableCreateValidCheck {
                    handle,
                    kind,
                    inverted,
                }) => {
                    let is_valid_true = (val != 0) != inverted;
                    if is_valid_true {
                        covered_valid_true = true;
                        if branch_st
                            .refinements
                            .nullable_fresh_handles
                            .remove(&handle)
                            .is_some()
                        {
                            branch_st.slots[handle] =
                                HandleProtocolSlot::fresh(ProtocolStateSet(ProtocolStateSet::OPEN));
                        }
                    } else {
                        // `valid == false` NEVER proves protocol state `Closed`.
                        // For `AsyncRegistration`, `valid == false` proves the frame
                        // pointer is null, permitting ONLY its null-guarded no-op
                        // `AsyncRegistrationFinishDestroy`.
                        covered_valid_false = true;
                        branch_st.refinements.nullable_fresh_handles.remove(&handle);
                        if kind == self::env::NullableStartKind::AsyncRegistration {
                            branch_st
                                .refinements
                                .null_uninstalled_async_reg
                                .insert(handle);
                        }
                    }
                }
                None => {}
            }
            enqueue(target, branch_st, in_states, queue);
        }
        let mut other_st = st;
        match target_info {
            Some(DiscrTarget::HandleState(h_idx)) => {
                let rem = other_st.slots[h_idx].states.subtract(covered_mask);
                if !rem.is_empty() {
                    other_st.slots[h_idx].states = rem;
                    enqueue(otherwise, other_st, in_states, queue);
                }
            }
            Some(DiscrTarget::ErrorUnion(eu_l)) => {
                if covered_ok
                    && !covered_err
                    && let Some(outcome) = other_st.refinements.store_op_of.get(&eu_l).cloned()
                {
                    Self::apply_store_err_recovery(&mut other_st, &outcome);
                }
                if let Some(entries) = other_st.refinements.eu_of.get(&eu_l).cloned() {
                    for (h_idx, ok_st, err_st) in entries {
                        if covered_ok && !covered_err {
                            if let Some(s) = err_st {
                                other_st.slots[h_idx].states = s;
                            }
                        } else if covered_err
                            && !covered_ok
                            && let Some(s) = ok_st
                        {
                            other_st.slots[h_idx].states = s;
                        }
                    }
                }
                enqueue(otherwise, other_st, in_states, queue);
            }
            Some(DiscrTarget::NullableCreateValidCheck { handle, kind, .. }) => {
                if covered_valid_false
                    && !covered_valid_true
                    && other_st
                        .refinements
                        .nullable_fresh_handles
                        .remove(&handle)
                        .is_some()
                {
                    other_st.slots[handle] =
                        HandleProtocolSlot::fresh(ProtocolStateSet(ProtocolStateSet::OPEN));
                } else if covered_valid_true && !covered_valid_false {
                    other_st.refinements.nullable_fresh_handles.remove(&handle);
                    if kind == self::env::NullableStartKind::AsyncRegistration {
                        other_st
                            .refinements
                            .null_uninstalled_async_reg
                            .insert(handle);
                    }
                }
                enqueue(otherwise, other_st, in_states, queue);
            }
            Some(DiscrTarget::Poll(_)) | None => {
                enqueue(otherwise, other_st, in_states, queue);
            }
        }
    }

    fn verify_return(&self, st: &ProtoState, value: &Operand) -> Result<(), CheckError> {
        let ret_local = match value {
            Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => match p.root {
                PlaceRoot::Local(l) => Some(l),
                PlaceRoot::Capture(_) => None,
            },
            Operand::Copy(_) | Operand::Move(_) | Operand::Const(_) | Operand::Pooled(_) => None,
        };
        for clause in &self.func.sig.state_contracts {
            match *clause {
                StateContractClause::RequiresState { .. }
                | StateContractClause::RequiresApart { .. }
                | StateContractClause::InvalidationUpperBound
                | StateContractClause::Invalidates { .. }
                | StateContractClause::AllowsRollback { .. } => {}
                StateContractClause::ProvidesRetired { param } => {
                    if !st.slots[param as usize].retired {
                        return Err(self.fail(
                            "function fails to satisfy provides(retired(...)) on all return paths",
                        ));
                    }
                }
                StateContractClause::ProvidesState { param, states } => {
                    if !st.slots[param as usize].states.is_subset_of(states) {
                        return Err(self.fail(
                            "function fails to satisfy provides(state(...)) on all return paths",
                        ));
                    }
                }
                StateContractClause::ProvidesOnPending { param, states } => {
                    let p_idx = param as usize;
                    let ok = if let Some(rl) = ret_local
                        && let Some(&(po_p, pend_st, _)) = st.refinements.poll_of.get(&rl)
                        && po_p == p_idx
                    {
                        pend_st.is_subset_of(states)
                    } else if let Some(rl) = ret_local
                        && st.refinements.enum_variant.get(&rl) == Some(&1)
                    {
                        // Returning Poll.Ready directly; on_pending vacuously holds.
                        true
                    } else {
                        st.slots[p_idx].states.is_subset_of(states)
                    };
                    if !ok {
                        return Err(
                            self.fail("function fails to satisfy provides(on_pending(state(...)))")
                        );
                    }
                }
                StateContractClause::ProvidesOnReady { param, states } => {
                    let p_idx = param as usize;
                    let ok = if let Some(rl) = ret_local
                        && let Some(&(po_p, _, ready_st)) = st.refinements.poll_of.get(&rl)
                        && po_p == p_idx
                    {
                        ready_st.is_subset_of(states)
                    } else if let Some(rl) = ret_local
                        && st.refinements.enum_variant.get(&rl) == Some(&0)
                    {
                        // Returning Poll.Pending directly; on_ready vacuously holds.
                        true
                    } else {
                        st.slots[p_idx].states.is_subset_of(states)
                    };
                    if !ok {
                        return Err(
                            self.fail("function fails to satisfy provides(on_ready(state(...)))")
                        );
                    }
                }
                StateContractClause::ProvidesOnOk { param, states } => {
                    let p_idx = param as usize;
                    let ok = if let Some(rl) = ret_local
                        && let Some(entries) = st.refinements.eu_of.get(&rl)
                        && let Some(&(_, Some(ok_st), _)) =
                            entries.iter().find(|(h, _, _)| *h == p_idx)
                    {
                        ok_st.is_subset_of(states)
                    } else if let Some(rl) = ret_local
                        && st.refinements.error_arm_const.get(&rl) == Some(&false)
                    {
                        // Returning .err(...) directly; on_ok vacuously holds.
                        true
                    } else {
                        st.slots[p_idx].states.is_subset_of(states)
                    };
                    if !ok {
                        return Err(
                            self.fail("function fails to satisfy provides(on_ok(state(...)))")
                        );
                    }
                }
                StateContractClause::ProvidesOnErr { param, states } => {
                    let p_idx = param as usize;
                    let ok = if let Some(rl) = ret_local
                        && let Some(entries) = st.refinements.eu_of.get(&rl)
                        && let Some(&(_, _, Some(err_st))) =
                            entries.iter().find(|(h, _, _)| *h == p_idx)
                    {
                        err_st.is_subset_of(states)
                    } else if let Some(rl) = ret_local
                        && st.refinements.error_arm_const.get(&rl) == Some(&true)
                    {
                        // Returning .ok(...) directly; on_err vacuously holds.
                        true
                    } else {
                        st.slots[p_idx].states.is_subset_of(states)
                    };
                    if !ok {
                        return Err(
                            self.fail("function fails to satisfy provides(on_err(state(...)))")
                        );
                    }
                }
            }
        }
        // Verify declared exported projection-invalidation upper bound (`actual ⊆ upper`).
        if self.has_invalidation_upper {
            for (p_idx, actual_path) in &st.invalidated {
                let covered = self
                    .declared_invalidations
                    .iter()
                    .any(|(dp, dpath)| *dp == *p_idx && dpath.subsumes(actual_path));
                if !covered {
                    return Err(self.fail(
                                "function invalidates projection outside declared effects(invalidates(...)) upper bound",
                            ));
                }
            }
        }
        // Verify history rollback `H`: a function cannot hide a transient state
        // rollback (e.g. Open -> Closed -> Open) unless `effects(rollback(p))`
        // or an invalidation upper bound covering the handle's state is declared.
        for (p_idx, info_opt) in self.local_info.iter().enumerate().take(self.param_count) {
            let Some(info) = info_opt else { continue };
            if self.allows_rollback[p_idx] {
                continue;
            }
            let has_hidden_rollback = st.slots[p_idx].has_hidden_rollback();
            if has_hidden_rollback
                && (self.declared_requires_state[p_idx].is_some() || self.has_invalidation_upper)
            {
                let state_covered = self.has_invalidation_upper
                    && self.declared_invalidations.iter().any(|(dp, dpath)| {
                        *dp == p_idx
                            && match info.state_field {
                                None => dpath.as_slice().is_empty(),
                                Some((_, sf)) => dpath.subsumes(&[sf]),
                            }
                    });
                if !state_covered {
                    return Err(self.fail(
                                "hidden protocol state rollback H detected; declare effects(rollback(...)) or effects(invalidates(...))",
                            ));
                }
            }
        }

        Ok(())
    }
}
