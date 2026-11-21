//! Loan rules of the kernel: conflicts between accesses and live loans,
//! flow of loans into written storage, escapes, and liveness pruning.
//!
//! Model (`spec/semantic-kernel.md` §5): each local carries the set of loans
//! its value may keep alive; `EXTERNAL` stands for storage outside the frame.
//! A loan is live while a kept local carries it, where kept = live locals
//! closed under "borrowed by a kept local's loan" (reading through a live
//! address reaches the borrowed local's own loans). Cross-root aliasing is
//! never assumed: storage reached through an address is related to locals
//! only by the loans that address carries, and every loan creation is
//! checked against all live loans.

use crate::loans::{EXTERNAL, LoanKind};
use crate::state::{BitSet, State};
use crate::transfer::{Kernel, ViolationKind};
use zil_ir::body::{
    EdgeKind, InstructionKind, Local, Mutability, OpLocation, OperandRole, Place, PlaceAccess,
    PlaceRelation, PlaceRoot, Projection, Rvalue, Terminator, type_place,
};

/// Effects of one instruction on carried loans, computed from the state
/// before the instruction consumes its operands.
pub(crate) enum Flow {
    Write {
        place: Place,
        loans: BitSet,
    },
    /// A writing loan was created: facts about the place no longer hold.
    KillFacts(Place),
}

/// Loans carried by the operands of a terminator.
pub(crate) struct TermFlows {
    loans: BitSet,
    /// Loans that flow into the destination of a `Call` according to its region
    /// contract (equals `loans` when the callee has no region parameters).
    ret_loans: BitSet,
    /// Some operand grants a callee write access to external storage that
    /// can hold addresses.
    external_store: bool,
}

fn deref_count(place: &Place) -> usize {
    place
        .projections
        .iter()
        .filter(|p| matches!(p, Projection::Deref))
        .count()
}

/// Whether an access conflicts with a live loan of `kind` on overlapping
/// storage. Exhaustive in the access kind.
fn conflicts(access: PlaceAccess, kind: LoanKind) -> bool {
    match access {
        PlaceAccess::Copy
        | PlaceAccess::InspectLen
        | PlaceAccess::Borrow(Mutability::Shared)
        | PlaceAccess::Describe
        | PlaceAccess::CallClosure(zil_ir::body::ClosureCallMode::Shared) => {
            kind == LoanKind::Exclusive
        }
        // A tag read establishes facts; any writer could falsify them.
        PlaceAccess::InspectTag => kind.writes(),
        PlaceAccess::Move
        | PlaceAccess::Drop
        | PlaceAccess::Init
        | PlaceAccess::ReplaceInstance
        | PlaceAccess::Borrow(Mutability::Exclusive)
        | PlaceAccess::CallClosure(
            zil_ir::body::ClosureCallMode::Mut | zil_ir::body::ClosureCallMode::Once,
        ) => true,
    }
}

/// Operand reads whose value flows into the result.
fn flows_value(role: OperandRole, access: PlaceAccess) -> bool {
    let reads = match access {
        PlaceAccess::Copy | PlaceAccess::Move | PlaceAccess::CallClosure(_) => true,
        PlaceAccess::Init
        | PlaceAccess::Borrow(_)
        | PlaceAccess::Describe
        | PlaceAccess::Drop
        | PlaceAccess::ReplaceInstance
        | PlaceAccess::InspectTag
        | PlaceAccess::InspectLen => false,
    };
    let carries = match role {
        OperandRole::Index
        | OperandRole::Discriminant
        | OperandRole::Condition
        | OperandRole::Target
        | OperandRole::ReplacedOld => false,
        OperandRole::Value
        | OperandRole::Component(_)
        | OperandRole::Lhs
        | OperandRole::Rhs
        | OperandRole::Arg(_)
        | OperandRole::Callee
        | OperandRole::ReturnValue
        | OperandRole::DeferClosure => true,
    };
    reads && carries
}

impl Kernel<'_> {
    pub(crate) fn external(&self) -> BitSet {
        let mut b = BitSet::new(self.loans.len());
        b.insert(EXTERNAL);
        b
    }

    fn empty_loans(&self) -> BitSet {
        BitSet::new(self.loans.len())
    }

    pub(crate) fn local_holds_address(&self, local: Local) -> bool {
        self.body
            .locals
            .get(local.index())
            .is_none_or(|d| self.tys.holds_address(d.ty))
    }

    fn place_holds_address(&self, place: &Place) -> bool {
        type_place(place, &self.body.cx(self.tys)).map_or(true, |t| self.tys.holds_address(t.ty()))
    }

    fn place_may_store_address(&self, place: &Place) -> bool {
        type_place(place, &self.body.cx(self.tys))
            .map_or(true, |t| self.tys.may_store_address(t.ty()))
    }

    /// Loans directly held by storage rooted at `root`.
    fn direct(&self, state: &State, root: PlaceRoot) -> BitSet {
        match root {
            PlaceRoot::Local(l) => state
                .carries
                .get(l)
                .cloned()
                .unwrap_or_else(|| self.external()),
            PlaceRoot::Capture(c) => {
                let holds = self
                    .body
                    .captures
                    .get(c as usize)
                    .is_none_or(|t| self.tys.holds_address(*t));
                if holds {
                    self.external()
                } else {
                    self.empty_loans()
                }
            }
        }
    }

    /// Every loan reachable from `set` by following borrowed storage.
    fn close(&self, state: &State, set: &BitSet) -> BitSet {
        let mut out = set.clone();
        let mut work: Vec<usize> = set.iter().collect();
        while let Some(id) = work.pop() {
            let Some(loan) = self.loans.get(id) else {
                continue;
            };
            for more in self.direct(state, loan.place.root).iter() {
                if !out.contains(more) {
                    out.insert(more);
                    work.push(more);
                }
            }
        }
        out
    }

    pub(crate) fn reach(&self, state: &State, root: PlaceRoot) -> BitSet {
        self.close(state, &self.direct(state, root))
    }

    /// A loan of this frame's own storage in `set`, if any.
    fn anchored(&self, set: &BitSet) -> Option<OpLocation> {
        set.iter()
            .filter_map(|id| self.loans.get(id))
            .find(|l| l.anchored)
            .map(|l| l.origin)
    }

    // ---- conflicts -----------------------------------------------------------

    pub(crate) fn loan_conflict(
        &self,
        state: &State,
        place: &Place,
        access: PlaceAccess,
    ) -> Result<(), ViolationKind> {
        for id in state.carries.all().iter() {
            let Some(loan) = self.loans.get(id) else {
                continue;
            };
            if loan.place.root != place.root {
                continue;
            }
            if place.relation(&loan.place) == PlaceRelation::Disjoint {
                continue;
            }
            if conflicts(access, loan.kind) {
                return Err(ViolationKind::LoanConflict {
                    place: place.clone(),
                    loan: loan.origin,
                });
            }
        }
        Ok(())
    }

    /// Storage of `local` begins or ends: no loan anchored to `local`'s own
    /// frame storage may be live. Loans behind a `Deref` of a reference local
    /// are carried by the reborrow itself and do not borrow the pointer slot.
    pub(crate) fn storage_conflict(
        &self,
        state: &State,
        local: Local,
    ) -> Result<(), ViolationKind> {
        let place = Place::local(local);
        for id in state.carries.all().iter() {
            let Some(loan) = self.loans.get(id) else {
                continue;
            };
            if !loan.anchored || loan.place.root != place.root {
                continue;
            }
            if place.relation(&loan.place) == PlaceRelation::Disjoint {
                continue;
            }
            return Err(ViolationKind::LoanConflict {
                place,
                loan: loan.origin,
            });
        }
        Ok(())
    }

    // ---- flows ---------------------------------------------------------------

    fn operand_loans<'p>(
        &self,
        state: &State,
        accesses: impl Iterator<Item = (OperandRole, PlaceAccess, &'p Place)>,
    ) -> BitSet {
        let mut loans = self.empty_loans();
        let cx = self.body.cx(self.tys);
        for (role, access, place) in accesses {
            if flows_value(role, access) {
                let mut r = self.reach(state, place.root);
                if !place.projections.is_empty()
                    && let Ok(typing) = type_place(place, &cx)
                {
                    let allowed = self.tys.regions_in(typing.ty());
                    if !allowed.is_empty() {
                        for id in r.clone().iter() {
                            if let Some(reg) = self.loans.loan_region(id)
                                && !allowed.contains(&reg)
                            {
                                r.remove(id);
                            }
                        }
                    }
                }
                loans.union_with(&r);
            }
        }
        loans
    }

    pub(crate) fn instruction_flows(
        &self,
        state: &State,
        kind: &InstructionKind,
        at: OpLocation,
    ) -> Vec<Flow> {
        let accesses = kind.accesses();
        let mut flows = Vec::new();
        match kind {
            InstructionKind::Assign { dest, value } => {
                let loans = match value {
                    Rvalue::Ref { place, .. }
                    | Rvalue::SliceRef { place, .. }
                    | Rvalue::Describe { place } => {
                        let mut b = self.empty_loans();
                        if let Some(id) = self.loans.created_at(at) {
                            b.insert(id);
                            if self.loans.get(id).is_some_and(|l| l.kind.writes()) {
                                flows.push(Flow::KillFacts(place.clone()));
                            }
                        }
                        b
                    }
                    Rvalue::Use(_)
                    | Rvalue::Aggregate { .. }
                    | Rvalue::ErrorUnion { .. }
                    | Rvalue::Unary { .. }
                    | Rvalue::Binary { .. }
                    | Rvalue::Convert { .. }
                    | Rvalue::Discriminant(_)
                    | Rvalue::ErrorArm(_)
                    | Rvalue::Len(_)
                    | Rvalue::FnRef(_)
                    | Rvalue::ForeignFnRef(_)
                    | Rvalue::StaticUtf(_) => self.operand_loans(
                        state,
                        accesses
                            .iter()
                            .map(|a| (a.role, a.access, a.place.as_ref())),
                    ),
                };
                let loans = if self.place_holds_address(dest) {
                    loans
                } else {
                    self.empty_loans()
                };
                flows.push(Flow::Write {
                    place: dest.clone(),
                    loans,
                });
            }
            InstructionKind::Replace { place, old, .. } => {
                let incoming = self.operand_loans(
                    state,
                    accesses
                        .iter()
                        .filter(|a| a.role == OperandRole::Value)
                        .map(|a| (a.role, a.access, a.place.as_ref())),
                );
                let outgoing = if self.local_holds_address(*old) {
                    self.reach(state, place.root)
                } else {
                    self.empty_loans()
                };
                flows.push(Flow::Write {
                    place: place.clone(),
                    loans: incoming,
                });
                flows.push(Flow::Write {
                    place: Place::local(*old),
                    loans: outgoing,
                });
            }
            InstructionKind::Drop { .. }
            | InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter(_)
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::RegisterDefer { .. } => {}
        }
        flows
    }

    pub(crate) fn apply_flows(
        &self,
        state: &mut State,
        flows: Vec<Flow>,
    ) -> Result<(), ViolationKind> {
        for flow in flows {
            match flow {
                Flow::Write { place, loans } => self.write(state, &place, &loans)?,
                Flow::KillFacts(place) => self.invalidate(state, &place),
            }
        }
        Ok(())
    }

    /// Records that the value written at `place` carries `loans`.
    fn write(&self, state: &mut State, place: &Place, loans: &BitSet) -> Result<(), ViolationKind> {
        let derefs = deref_count(place);
        if derefs == 0 {
            match place.root {
                PlaceRoot::Local(d) => {
                    if place.projections.is_empty() {
                        state.carries.set(d, loans.clone());
                    } else {
                        state.carries.add(d, loans);
                    }
                }
                // The environment outlives this call.
                PlaceRoot::Capture(_) => self.no_escape(state, loans)?,
            }
            return Ok(());
        }
        if loans.is_empty() {
            return Ok(());
        }
        let (targets, external) = self.storage_targets(state, place.root, derefs == 1);
        for y in targets {
            state.carries.add(y, loans);
        }
        if external {
            self.no_escape(state, loans)?;
        }
        Ok(())
    }

    fn no_escape(&self, state: &State, loans: &BitSet) -> Result<(), ViolationKind> {
        match self.anchored(&self.close(state, loans)) {
            Some(loan) => Err(ViolationKind::EscapingLocalBorrow { loan }),
            None => Ok(()),
        }
    }

    /// Locals whose storage a write through an address rooted at `root` may
    /// reach, and whether external storage may be reached. With a single
    /// `Deref` only the root's direct loans (and the pointees of reborrows
    /// among them) are targets; deeper paths use the full closure.
    fn storage_targets(&self, state: &State, root: PlaceRoot, single: bool) -> (Vec<Local>, bool) {
        let mut targets = Vec::new();
        let mut external = false;
        let mut seen = self.empty_loans();
        let start = if single {
            self.direct(state, root)
        } else {
            self.reach(state, root)
        };
        let mut work: Vec<usize> = start.iter().collect();
        while let Some(id) = work.pop() {
            if seen.contains(id) {
                continue;
            }
            seen.insert(id);
            let Some(loan) = self.loans.get(id) else {
                external = true;
                continue;
            };
            if !loan.kind.writes() {
                continue;
            }
            if loan.anchored {
                match loan.place.root {
                    PlaceRoot::Local(y) => targets.push(y),
                    PlaceRoot::Capture(_) => external = true,
                }
            } else if single && deref_count(&loan.place) == 1 {
                // A reborrow through `z`: its pointee is what `z` points to.
                work.extend(self.direct(state, loan.place.root).iter());
            } else {
                work.extend(self.reach(state, loan.place.root).iter());
            }
        }
        (targets, external)
    }

    pub(crate) fn terminator_flows(&self, state: &State, term: &Terminator) -> TermFlows {
        let accesses = term.accesses();
        let loans = self.operand_loans(
            state,
            accesses
                .iter()
                .map(|a| (a.role, a.access, a.place.as_ref())),
        );
        let ret_loans = match term {
            Terminator::Call {
                callee: zil_ir::body::Callee::Direct(f),
                ..
            } => {
                if let Some(sig) = self.sigs.functions.get(f.index())
                    && sig.region_params > 0
                {
                    let ret_regions = self.tys.regions_in(sig.result);
                    if ret_regions.is_empty() {
                        self.empty_loans()
                    } else {
                        let param_mask = self
                            .sigs
                            .return_param_masks
                            .and_then(|ms| ms.get(f.index()).copied())
                            .unwrap_or(u64::MAX);
                        let matching = accesses.iter().filter(|a| {
                            let OperandRole::Arg(idx) = a.role else {
                                return false;
                            };
                            let bit_ok = idx >= 64 || (param_mask & (1u64 << idx)) != 0;
                            bit_ok
                                && sig.params.get(idx as usize).is_some_and(|pt| {
                                    self.tys
                                        .regions_in(*pt)
                                        .iter()
                                        .any(|r| ret_regions.contains(r))
                                })
                        });
                        self.operand_loans(
                            state,
                            matching.map(|a| (a.role, a.access, a.place.as_ref())),
                        )
                    }
                } else {
                    loans.clone()
                }
            }
            Terminator::Call {
                callee: zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::TaskPoll),
                args,
                ..
            } => self
                .task_poll_ret_loans(state, args, &loans)
                .unwrap_or_else(|| loans.clone()),
            Terminator::Call { .. }
            | Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => loans.clone(),
        };
        let external_store = match term {
            Terminator::Call {
                callee:
                    zil_ir::body::Callee::Intrinsic(
                        zil_ir::body::IntrinsicOp::TaskPoll
                        | zil_ir::body::IntrinsicOp::TaskRequestCancel
                        | zil_ir::body::IntrinsicOp::TaskPollClose
                        | zil_ir::body::IntrinsicOp::TaskFinishDestroy
                        | zil_ir::body::IntrinsicOp::TaskValid
                        | zil_ir::body::IntrinsicOp::AsyncRegistrationValid
                        | zil_ir::body::IntrinsicOp::AsyncRegistrationRequestClose
                        | zil_ir::body::IntrinsicOp::AsyncRegistrationPollClose
                        | zil_ir::body::IntrinsicOp::AsyncRegistrationFinishDestroy
                        | zil_ir::body::IntrinsicOp::RegistrationValid
                        | zil_ir::body::IntrinsicOp::RegistrationClose,
                    ),
                ..
            } => false,
            Terminator::Call { .. } => accesses.iter().any(|a| {
                flows_value(a.role, a.access)
                    && self
                        .reach(state, a.place.root)
                        .iter()
                        .any(|id| id == EXTERNAL || self.loans.loan_region(id).is_some())
                    && self.place_may_store_address(&a.place)
            }),
            Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => false,
        };
        TermFlows {
            loans,
            ret_loans,
            external_store,
        }
    }

    pub(crate) fn apply_terminator_flows(
        &self,
        state: &mut State,
        term: &Terminator,
        flows: &TermFlows,
    ) -> Result<(), ViolationKind> {
        match term {
            Terminator::Return { .. } => {
                if let Some(loan) = self.anchored(&flows.loans) {
                    return Err(ViolationKind::ReturnsLocalBorrow { loan });
                }
                let allowed = self.tys.regions_in(self.body.result);
                for id in self.close(state, &flows.loans).iter() {
                    if let Some(region) = self.loans.loan_region(id)
                        && !allowed.contains(&region)
                    {
                        return Err(ViolationKind::RegionEscape { region });
                    }
                }
                Ok(())
            }
            Terminator::Call { callee, .. } => {
                // Region-contract invariance check: if a direct callee binds
                // multiple Task<_, &'r mut _> parameters to the same region
                // parameter `r`, their carried local loan sets cannot be
                // disjoint (they belong to distinct local borrow classes).
                if let zil_ir::body::Callee::Direct(f) = callee
                    && let Some(sig) = self.sigs.functions.get(f.index())
                    && sig.region_params > 0
                {
                    let accesses = term.accesses();
                    for r in 0..sig.region_params {
                        let mut prev_local_loans: Option<Vec<_>> = None;
                        for (idx, pt) in sig.params.iter().enumerate() {
                            if !self.tys.regions_in(*pt).contains(&r) {
                                continue;
                            }
                            let is_task = matches!(
                                self.tys.get(*pt),
                                Some(zil_ir::body::TyKind::Managed(inner))
                                    if matches!(self.tys.get(*inner), Some(zil_ir::body::TyKind::Opaque(zil_ir::body::OpaqueTy::Task { context: Some(_), .. })))
                            );
                            if !is_task {
                                continue;
                            }
                            let arg_loans = self.operand_loans(
                                state,
                                accesses
                                    .iter()
                                    .filter(|a| a.role == OperandRole::Arg(idx as u32))
                                    .map(|a| (a.role, a.access, a.place.as_ref())),
                            );
                            let closed = self.close(state, &arg_loans);
                            let local_only: Vec<_> = closed
                                .iter()
                                .filter(|id| {
                                    self.loans.get(*id).is_some()
                                        || self.loans.loan_region(*id).is_some()
                                })
                                .collect();
                            if local_only.is_empty() {
                                continue;
                            }
                            if let Some(prev) = &prev_local_loans {
                                let overlaps = prev.iter().any(|p_id| {
                                    let pr = self.loans.loan_region(*p_id);
                                    local_only.iter().any(|c_id| {
                                        p_id == c_id
                                            || (pr.is_some() && pr == self.loans.loan_region(*c_id))
                                    })
                                });
                                if !overlaps {
                                    return Err(ViolationKind::RegionEscape { region: r });
                                }
                            } else {
                                prev_local_loans = Some(local_only);
                            }
                        }
                    }
                }
                let is_protocol_intrinsic = matches!(
                    callee,
                    zil_ir::body::Callee::Intrinsic(
                        zil_ir::body::IntrinsicOp::TaskPoll
                            | zil_ir::body::IntrinsicOp::TaskRequestCancel
                            | zil_ir::body::IntrinsicOp::TaskPollClose
                            | zil_ir::body::IntrinsicOp::TaskFinishDestroy
                            | zil_ir::body::IntrinsicOp::TaskValid
                            | zil_ir::body::IntrinsicOp::AsyncRegistrationValid
                            | zil_ir::body::IntrinsicOp::AsyncRegistrationRequestClose
                            | zil_ir::body::IntrinsicOp::AsyncRegistrationPollClose
                            | zil_ir::body::IntrinsicOp::AsyncRegistrationFinishDestroy
                            | zil_ir::body::IntrinsicOp::RegistrationValid
                            | zil_ir::body::IntrinsicOp::RegistrationClose,
                    )
                );
                if !is_protocol_intrinsic {
                    // The callee may store any argument loan into any writable,
                    // address-holding storage it receives.
                    let mut writable = Vec::new();
                    for id in flows.loans.iter() {
                        let Some(loan) = self.loans.get(id) else {
                            continue;
                        };
                        if loan.kind.writes() && loan.stores_addresses && loan.anchored {
                            match loan.place.root {
                                PlaceRoot::Local(y) => writable.push(y),
                                PlaceRoot::Capture(_) => self.no_escape(state, &flows.loans)?,
                            }
                        }
                    }
                    for y in writable {
                        state.carries.add(y, &flows.loans);
                    }
                }
                if flows.external_store {
                    self.no_escape(state, &flows.loans)?;
                }
                Ok(())
            }
            Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => Ok(()),
        }
    }

    pub(crate) fn apply_edge_flows(
        &self,
        state: &mut State,
        term: &Terminator,
        kind: EdgeKind,
        flows: &TermFlows,
    ) -> Result<(), ViolationKind> {
        match (term, kind) {
            (Terminator::Call { dest, .. }, EdgeKind::CallReturn) => {
                let loans = if self.place_holds_address(dest) {
                    flows.ret_loans.clone()
                } else {
                    self.empty_loans()
                };
                self.write(state, dest, &loans)
            }
            (
                Terminator::Goto { .. }
                | Terminator::Switch { .. }
                | Terminator::Return { .. }
                | Terminator::Call { .. }
                | Terminator::Assert { .. }
                | Terminator::Unreachable
                | Terminator::Trap(_),
                _,
            ) => Ok(()),
        }
    }

    // ---- pruning -------------------------------------------------------------

    /// Drops carried loans of locals that are neither live nor borrowed by a
    /// kept local's loan.
    pub(crate) fn prune(&self, state: &mut State, live: &BitSet) {
        let n = state.carries.locals();
        let mut keep = BitSet::new(n);
        let mut work = Vec::new();
        for l in live.iter().filter(|l| *l < n) {
            keep.insert(l);
            work.push(l);
        }
        while let Some(l) = work.pop() {
            for id in state.carries.of(Local::from_index(l)).iter() {
                if let Some(loan) = self.loans.get(id)
                    && let PlaceRoot::Local(y) = loan.place.root
                    && y.index() < n
                    && !keep.contains(y.index())
                {
                    keep.insert(y.index());
                    work.push(y.index());
                }
            }
        }
        for l in 0..n {
            if !keep.contains(l) {
                state.carries.clear(Local::from_index(l));
            }
        }
    }

    fn extract_task_result_ty(&self, mut ty: zil_ir::body::TyId) -> Option<zil_ir::body::TyId> {
        for _ in 0..8 {
            match self.tys.get(ty)? {
                zil_ir::body::TyKind::Opaque(zil_ir::body::OpaqueTy::Task { result, .. }) => {
                    return Some(*result);
                }
                zil_ir::body::TyKind::Managed(inner)
                | zil_ir::body::TyKind::Desc(inner)
                | zil_ir::body::TyKind::Ref { pointee: inner, .. } => ty = *inner,
                zil_ir::body::TyKind::ErrorUnion { ok, .. } => ty = *ok,
                zil_ir::body::TyKind::Unit
                | zil_ir::body::TyKind::Bool
                | zil_ir::body::TyKind::Never
                | zil_ir::body::TyKind::Int(_)
                | zil_ir::body::TyKind::Float(_)
                | zil_ir::body::TyKind::Adt { .. }
                | zil_ir::body::TyKind::Tuple(_)
                | zil_ir::body::TyKind::Array { .. }
                | zil_ir::body::TyKind::Slice(_)
                | zil_ir::body::TyKind::FnPtr { .. }
                | zil_ir::body::TyKind::Closure { .. }
                | zil_ir::body::TyKind::Dyn(_)
                | zil_ir::body::TyKind::OwnedDyn(_)
                | zil_ir::body::TyKind::Param(_)
                | zil_ir::body::TyKind::Opaque(_) => return None,
            }
        }
        None
    }

    fn collect_local_origin_loans(&self, root: Local, out: &mut BitSet) {
        let mut work = vec![root];
        let mut seen = Vec::new();
        while let Some(l) = work.pop() {
            if seen.contains(&l) {
                continue;
            }
            seen.push(l);
            if self.body.is_param(l)
                && let Some(decl) = self.body.locals.get(l.index())
            {
                let regs = self.tys.regions_in(decl.ty);
                if regs.is_empty() {
                    out.insert(EXTERNAL);
                } else {
                    for r in regs {
                        if let Some(id) = self.loans.region_loan(r) {
                            out.insert(id);
                        }
                    }
                }
            }
            for (b, block) in self.body.blocks.iter().enumerate() {
                for (i, inst) in block.instructions.iter().enumerate() {
                    if let InstructionKind::Assign { dest, value } = &inst.kind
                        && dest.as_local() == Some(l)
                    {
                        match value {
                            Rvalue::Ref { place, .. }
                            | Rvalue::SliceRef { place, .. }
                            | Rvalue::Describe { place } => {
                                if let Some(id) = self.loans.created_at(OpLocation {
                                    block: zil_ir::body::BlockId::from_index(b),
                                    instruction: Some(i as u32),
                                }) {
                                    out.insert(id);
                                }
                                if let PlaceRoot::Local(src) = place.root {
                                    work.push(src);
                                }
                            }
                            Rvalue::Use(
                                zil_ir::body::Operand::Move(p) | zil_ir::body::Operand::Copy(p),
                            ) => {
                                if let PlaceRoot::Local(src) = p.root {
                                    work.push(src);
                                }
                            }
                            Rvalue::Use(
                                zil_ir::body::Operand::Const(_) | zil_ir::body::Operand::Pooled(_),
                            )
                            | Rvalue::Aggregate { .. }
                            | Rvalue::ErrorUnion { .. }
                            | Rvalue::Unary { .. }
                            | Rvalue::Binary { .. }
                            | Rvalue::Convert { .. }
                            | Rvalue::Discriminant(_)
                            | Rvalue::ErrorArm(_)
                            | Rvalue::Len(_)
                            | Rvalue::FnRef(_)
                            | Rvalue::ForeignFnRef(_)
                            | Rvalue::StaticUtf(_) => {}
                        }
                    }
                }
            }
        }
    }

    fn task_poll_ret_loans(
        &self,
        state: &State,
        args: &[zil_ir::body::Operand],
        carried: &BitSet,
    ) -> Option<BitSet> {
        let arg0 = match args.first()? {
            zil_ir::body::Operand::Copy(p) | zil_ir::body::Operand::Move(p) => p.as_local()?,
            zil_ir::body::Operand::Const(_) | zil_ir::body::Operand::Pooled(_) => return None,
        };
        let mut work: Vec<Local> = Vec::new();
        for id in self.direct(state, PlaceRoot::Local(arg0)).iter() {
            if let Some(loan) = self.loans.get(id)
                && let Some(owner) = loan.place.as_local()
            {
                work.push(owner);
            }
        }
        if work.is_empty() {
            return None;
        }
        let mut seen: Vec<Local> = Vec::new();
        let mut allowed = self.empty_loans();
        let mut found_source = false;
        while let Some(cur) = work.pop() {
            if seen.contains(&cur) {
                continue;
            }
            seen.push(cur);
            if self.body.is_param(cur) {
                return None;
            }
            for block in &self.body.blocks {
                for inst in &block.instructions {
                    if let InstructionKind::Assign { dest, value } = &inst.kind
                        && dest.as_local() == Some(cur)
                        && let Rvalue::Use(
                            zil_ir::body::Operand::Move(p) | zil_ir::body::Operand::Copy(p),
                        ) = value
                        && let PlaceRoot::Local(src) = p.root
                    {
                        work.push(src);
                    }
                }
                if let Terminator::Call {
                    callee,
                    args: call_args,
                    dest,
                    ..
                } = &block.terminator
                    && dest.as_local() == Some(cur)
                {
                    match callee {
                        zil_ir::body::Callee::Direct(f) => {
                            let sig = self.sigs.functions.get(f.index())?;
                            let task_res = self.extract_task_result_ty(sig.result)?;
                            if sig.region_params == 0 {
                                return None;
                            }
                            if call_args.len() == 1
                                && sig
                                    .params
                                    .first()
                                    .and_then(|pt| self.extract_task_result_ty(*pt))
                                    .is_some()
                            {
                                if let zil_ir::body::Operand::Move(p)
                                | zil_ir::body::Operand::Copy(p) = &call_args[0]
                                    && let PlaceRoot::Local(src) = p.root
                                {
                                    work.push(src);
                                    continue;
                                }
                                return None;
                            }
                            let ret_regions = self.tys.regions_in(task_res);
                            found_source = true;
                            for (idx, a) in call_args.iter().enumerate() {
                                let matches_ret = sig.params.get(idx).is_some_and(|pt| {
                                    self.tys
                                        .regions_in(*pt)
                                        .iter()
                                        .any(|r| ret_regions.contains(r))
                                });
                                if matches_ret {
                                    if let zil_ir::body::Operand::Move(p)
                                    | zil_ir::body::Operand::Copy(p) = a
                                        && let PlaceRoot::Local(al) = p.root
                                    {
                                        self.collect_local_origin_loans(al, &mut allowed);
                                    } else {
                                        return None;
                                    }
                                }
                            }
                        }
                        zil_ir::body::Callee::Indirect(_)
                        | zil_ir::body::Callee::Closure { .. }
                        | zil_ir::body::Callee::DynSlot { .. }
                        | zil_ir::body::Callee::Foreign(_)
                        | zil_ir::body::Callee::Intrinsic(_) => return None,
                    }
                }
            }
        }
        if !found_source {
            return None;
        }
        let mut out = self.empty_loans();
        for id in carried.iter() {
            if allowed.contains(id) {
                out.insert(id);
            }
        }
        Some(out)
    }
}
