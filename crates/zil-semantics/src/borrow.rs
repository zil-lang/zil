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
        field_loans: Option<Vec<BitSet>>,
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
    /// Optional per-field loan breakdown for a closure returned by a `Call`.
    ret_field_loans: Option<Vec<BitSet>>,
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
                if let Some(dr) = loan.deref_region
                    && (more == EXTERNAL || self.loans.loan_region(more).is_some_and(|r| r != dr))
                {
                    continue;
                }
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

    /// Top-level field index for a projection on a single-variant aggregate
    /// (`Record`/`Package`/`Resource` or `Tuple`). Enums and other projections
    /// return `None` so callers fall back conservatively.
    fn tracked_field_index(&self, proj: &Projection) -> Option<usize> {
        match proj {
            Projection::Field(fr) => {
                let adt = self.tys.get_adt(fr.adt)?;
                match adt.kind {
                    zil_ir::body::AdtKind::Record
                    | zil_ir::body::AdtKind::Package
                    | zil_ir::body::AdtKind::Resource => Some(fr.field.index()),
                    zil_ir::body::AdtKind::Enum => None,
                }
            }
            Projection::TupleField(idx) => Some(*idx as usize),
            Projection::ErrorArm(_)
            | Projection::Index(_)
            | Projection::ConstIndex(_)
            | Projection::Deref => None,
        }
    }

    /// Loans reachable from `place`, narrowing to a single field's carried set
    /// when per-field tracking is active on the root local.
    pub(crate) fn reach_place(&self, state: &State, place: &Place) -> BitSet {
        if let PlaceRoot::Local(l) = place.root
            && let Some(first) = place.projections.first()
            && let Some(idx) = self.tracked_field_index(first)
            && let Some(fields) = state.carries.fields_of(l)
            && let Some(field_loans) = fields.get(idx)
        {
            return self.close(state, field_loans);
        }
        self.reach(state, place.root)
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
        if state.carries.is_empty() {
            return Ok(());
        }
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
        if state.carries.is_empty() {
            return Ok(());
        }
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
                let mut r = self.reach_place(state, place);
                if !place.projections.is_empty()
                    && let Ok(typing) = type_place(place, &cx)
                {
                    let allowed = self.tys.regions_in(typing.ty());
                    let no_unannotated = !self.tys.has_unannotated_ref(typing.ty());
                    if !allowed.is_empty() {
                        for id in r.clone().iter() {
                            if (no_unannotated && id == EXTERNAL)
                                || self
                                    .loans
                                    .loan_region(id)
                                    .is_some_and(|reg| !allowed.contains(&reg))
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
                let field_loans = if dest.projections.is_empty() && self.place_holds_address(dest) {
                    match value {
                        Rvalue::Aggregate { kind, operands } => {
                            let track = match kind {
                                zil_ir::body::AggregateKind::Closure { .. }
                                | zil_ir::body::AggregateKind::Tuple => true,
                                zil_ir::body::AggregateKind::Adt { adt, .. } => {
                                    self.tys.get_adt(*adt).is_some_and(|def| match def.kind {
                                        zil_ir::body::AdtKind::Record
                                        | zil_ir::body::AdtKind::Package
                                        | zil_ir::body::AdtKind::Resource => true,
                                        zil_ir::body::AdtKind::Enum => false,
                                    })
                                }
                                zil_ir::body::AggregateKind::Array { .. } => false,
                            };
                            if track {
                                Some(
                                    operands
                                        .iter()
                                        .map(|op| match op {
                                            zil_ir::body::Operand::Copy(p)
                                            | zil_ir::body::Operand::Move(p) => {
                                                if self.place_holds_address(p) {
                                                    self.reach_place(state, p)
                                                } else {
                                                    self.empty_loans()
                                                }
                                            }
                                            zil_ir::body::Operand::Const(_)
                                            | zil_ir::body::Operand::Pooled(_) => {
                                                self.empty_loans()
                                            }
                                        })
                                        .collect(),
                                )
                            } else {
                                None
                            }
                        }
                        Rvalue::Use(
                            zil_ir::body::Operand::Copy(p) | zil_ir::body::Operand::Move(p),
                        ) if p.projections.is_empty() => match p.root {
                            PlaceRoot::Local(src) => {
                                state.carries.fields_of(src).map(|s| s.to_vec())
                            }
                            PlaceRoot::Capture(_) => None,
                        },
                        Rvalue::Use(_)
                        | Rvalue::ErrorUnion { .. }
                        | Rvalue::Ref { .. }
                        | Rvalue::SliceRef { .. }
                        | Rvalue::Describe { .. }
                        | Rvalue::Unary { .. }
                        | Rvalue::Binary { .. }
                        | Rvalue::Convert { .. }
                        | Rvalue::Discriminant(_)
                        | Rvalue::ErrorArm(_)
                        | Rvalue::Len(_)
                        | Rvalue::FnRef(_)
                        | Rvalue::ForeignFnRef(_)
                        | Rvalue::StaticUtf(_) => None,
                    }
                } else {
                    None
                };
                flows.push(Flow::Write {
                    place: dest.clone(),
                    loans,
                    field_loans,
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
                    self.reach_place(state, place)
                } else {
                    self.empty_loans()
                };
                flows.push(Flow::Write {
                    place: place.clone(),
                    loans: incoming,
                    field_loans: None,
                });
                flows.push(Flow::Write {
                    place: Place::local(*old),
                    loans: outgoing,
                    field_loans: None,
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
                Flow::Write {
                    place,
                    loans,
                    field_loans,
                } => {
                    self.write(state, &place, &loans)?;
                    if let Some(fl) = field_loans
                        && place.projections.is_empty()
                        && let PlaceRoot::Local(d) = place.root
                    {
                        state.carries.set_fields(d, fl);
                    }
                }
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
                    } else if let Some(first) = place.projections.first()
                        && let Some(idx) = self.tracked_field_index(first)
                    {
                        if place.projections.len() == 1 {
                            if !state.carries.set_field(d, idx, loans.clone()) {
                                state.carries.add(d, loans);
                            }
                        } else if !state.carries.add_field(d, idx, loans) {
                            state.carries.add(d, loans);
                        }
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

    pub(crate) fn terminator_flows(
        &self,
        state: &State,
        term: &Terminator,
        at: OpLocation,
    ) -> TermFlows {
        let accesses = term.accesses();
        let loans = self.operand_loans(
            state,
            accesses
                .iter()
                .map(|a| (a.role, a.access, a.place.as_ref())),
        );
        let (ret_loans, ret_field_loans) = match term {
            Terminator::Call {
                callee: zil_ir::body::Callee::Direct(f),
                args,
                ..
            } => {
                let summary = self.sigs.return_summaries.and_then(|ms| ms.get(f.index()));
                let rf = summary.and_then(|s| {
                    if s.ret_field_param_masks.is_empty() {
                        return None;
                    }
                    let mut out = Vec::with_capacity(s.ret_field_param_masks.len());
                    for &fmask in &s.ret_field_param_masks {
                        let mut fl = self.empty_loans();
                        for (ai, arg) in args.iter().enumerate() {
                            if ai >= 64 || (fmask & (1u64 << ai)) != 0 {
                                match arg {
                                    zil_ir::body::Operand::Copy(p)
                                    | zil_ir::body::Operand::Move(p) => {
                                        fl.union_with(&self.reach(state, p.root));
                                    }
                                    zil_ir::body::Operand::Const(_)
                                    | zil_ir::body::Operand::Pooled(_) => {}
                                }
                            }
                        }
                        out.push(fl);
                    }
                    Some(out)
                });
                let rl = if let Some(sig) = self.sigs.functions.get(f.index())
                    && sig.region_params > 0
                {
                    let ret_regions = self.tys.regions_in(sig.result);
                    if ret_regions.is_empty() {
                        if self.tys.has_unannotated_ref(sig.result) {
                            // Purely unannotated/elided return reference on a function with
                            // lifetime parameters (e.g. `impl<life a> Trait for View<'a> {
                            // fn view(&self) -> &[u8] }`): propagate loans from arguments
                            // flowing into the return value (`param_mask`).
                            let param_mask = summary.map_or(u64::MAX, |s| s.param_mask);
                            let mut out = self.empty_loans();
                            for a in &accesses {
                                let OperandRole::Arg(idx) = a.role else {
                                    continue;
                                };
                                if idx >= 64 || (param_mask & (1u64 << idx)) != 0 {
                                    out.union_with(&self.operand_loans(
                                        state,
                                        std::iter::once((a.role, a.access, a.place.as_ref())),
                                    ));
                                }
                            }
                            out
                        } else {
                            self.empty_loans()
                        }
                    } else {
                        let param_mask = summary.map_or(u64::MAX, |s| s.param_mask);
                        let mut out = self.empty_loans();
                        for a in &accesses {
                            let OperandRole::Arg(idx) = a.role else {
                                continue;
                            };
                            let bit_ok = idx >= 64 || (param_mask & (1u64 << idx)) != 0;
                            let Some(pt) = sig.params.get(idx as usize) else {
                                continue;
                            };
                            if !bit_ok
                                || !self
                                    .tys
                                    .regions_in(*pt)
                                    .iter()
                                    .any(|r| ret_regions.contains(r))
                            {
                                continue;
                            }
                            let outer_ref_unwrapped = matches!(
                                self.tys.get(*pt),
                                Some(zil_ir::body::TyKind::Ref { region, .. })
                                    if !matches!(region, zil_ir::body::Region::Param(r) if ret_regions.contains(r))
                            );
                            if outer_ref_unwrapped {
                                let cx = self.body.cx(self.tys);
                                let pointee_named_only = type_place(&a.place, &cx)
                                    .ok()
                                    .and_then(|t| match self.tys.get(t.ty()) {
                                        Some(zil_ir::body::TyKind::Ref { pointee, .. }) => {
                                            Some(*pointee)
                                        }
                                        _ => None,
                                    })
                                    .is_some_and(|p| {
                                        !self.tys.regions_in(p).is_empty()
                                            && !self.tys.has_unannotated_ref(p)
                                    });
                                if let PlaceRoot::Local(rl) = a.place.root {
                                    for id in state.carries.of(rl).iter() {
                                        if let Some(loan) = self.loans.get(id) {
                                            if let PlaceRoot::Local(target) = loan.place.root {
                                                out.union_with(state.carries.of(target));
                                            } else {
                                                out.insert(EXTERNAL);
                                            }
                                        } else if id == EXTERNAL && pointee_named_only {
                                            // Outer `&` invocation borrow does not flow into
                                            // a named-region return.
                                        } else {
                                            out.insert(id);
                                        }
                                    }
                                } else {
                                    out.insert(EXTERNAL);
                                }
                            } else {
                                out.union_with(&self.operand_loans(
                                    state,
                                    std::iter::once((a.role, a.access, a.place.as_ref())),
                                ));
                            }
                        }
                        out
                    }
                } else {
                    loans.clone()
                };
                (rl, rf)
            }
            Terminator::Call {
                callee: zil_ir::body::Callee::Closure { closure, .. },
                ..
            } => {
                let cx = self.body.cx(self.tys);
                let closure_def = type_place(closure, &cx).ok().and_then(|t| {
                    let ty = t.ty();
                    if let zil_ir::body::TyKind::Closure { def, .. } = self.tys.kind(ty) {
                        Some(*def)
                    } else if let zil_ir::body::TyKind::Managed(inner)
                    | zil_ir::body::TyKind::Ref { pointee: inner, .. } = self.tys.kind(ty)
                    {
                        if let zil_ir::body::TyKind::Closure { def, .. } = self.tys.kind(*inner) {
                            Some(*def)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                let summary = closure_def.and_then(|def| {
                    self.sigs
                        .return_summaries
                        .and_then(|ms| ms.iter().find(|s| s.closure == Some(def)))
                });
                if let Some(csum) = summary
                    && csum.capture_mask == 0
                {
                    let matching_args = accesses.iter().filter(|a| {
                        let OperandRole::Arg(idx) = a.role else {
                            return false;
                        };
                        idx >= 64 || (csum.param_mask & (1u64 << idx)) != 0
                    });
                    let out = self.operand_loans(
                        state,
                        matching_args.map(|a| (a.role, a.access, a.place.as_ref())),
                    );
                    (out, None)
                } else if let Some(csum) = summary
                    && closure.projections.is_empty()
                    && let PlaceRoot::Local(rl_local) = closure.root
                    && let Some(rfields) = state.carries.fields_of(rl_local)
                {
                    let max_bit = if rfields.len() >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << rfields.len()) - 1
                    };
                    if (csum.capture_mask & !max_bit) == 0 {
                        let mut out = self.empty_loans();
                        for (ci, fl) in rfields.iter().enumerate() {
                            if (csum.capture_mask & (1u64 << ci)) != 0 {
                                out.union_with(fl);
                            }
                        }
                        let matching_args = accesses.iter().filter(|a| {
                            let OperandRole::Arg(idx) = a.role else {
                                return false;
                            };
                            idx >= 64 || (csum.param_mask & (1u64 << idx)) != 0
                        });
                        out.union_with(&self.operand_loans(
                            state,
                            matching_args.map(|a| (a.role, a.access, a.place.as_ref())),
                        ));
                        (out, None)
                    } else {
                        (loans.clone(), None)
                    }
                } else {
                    (loans.clone(), None)
                }
            }
            Terminator::Call {
                callee: zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::TaskPoll),
                args,
                ..
            } => (
                self.task_poll_ret_loans(state, args, &loans)
                    .unwrap_or_else(|| loans.clone()),
                None,
            ),
            Terminator::Call {
                callee:
                    zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::StoreBorrow { .. }),
                ..
            } => {
                let mut rl = loans.clone();
                if let Some(id) = self.loans.created_at(at) {
                    rl.insert(id);
                }
                (rl, None)
            }
            Terminator::Call {
                callee: zil_ir::body::Callee::DynSlot { slot, .. },
                dest,
                ..
            } => {
                let cx = self.body.cx(self.tys);
                let dest_ty = type_place(dest, &cx).ok().map(|t| t.ty());
                if dest_ty.is_some_and(|t| !self.tys.holds_address(t)) {
                    (self.empty_loans(), None)
                } else if let Some(recv_access) =
                    accesses.iter().find(|a| a.role == OperandRole::Callee)
                    && let Ok(recv_t) = type_place(&recv_access.place, &cx)
                    && let Some(zil_ir::body::TyKind::Ref { pointee, .. }) =
                        self.tys.get(recv_t.ty())
                    && let Some((dyn_id, pointee_named_only)) = match self.tys.get(*pointee) {
                        Some(zil_ir::body::TyKind::OwnedDyn {
                            def,
                            regions,
                            context,
                        }) => {
                            let named_only = match context {
                                Some(ctx) => {
                                    !self.tys.regions_in(*ctx).is_empty()
                                        && !self.tys.has_unannotated_ref(*ctx)
                                        && !regions
                                            .iter()
                                            .any(|r| matches!(r, zil_ir::body::Region::Static))
                                }
                                None => !regions
                                    .iter()
                                    .any(|r| matches!(r, zil_ir::body::Region::Static)),
                            };
                            Some((*def, named_only))
                        }
                        Some(zil_ir::body::TyKind::Dyn { def, regions }) => {
                            let named_only = !regions.is_empty()
                                && !regions
                                    .iter()
                                    .any(|r| matches!(r, zil_ir::body::Region::Static));
                            Some((*def, named_only))
                        }
                        _ => None,
                    }
                    && let Some(def) = self.tys.get_dyn(dyn_id)
                    && let Some(sig) = def.slots.get(*slot as usize)
                    && self.tys.dyn_slot_has_fixed_regions(sig.result)
                {
                    let mut out = self.empty_loans();
                    // The slot returns borrows tied strictly to trait lifetime
                    // parameters (independently verified against every DynImpl),
                    // never to `&self`. Unwrap the direct receiver borrow so the
                    // `OwnedDyn` / `&dyn` handle's own local borrow is released
                    // while all external/context loans remain tracked.
                    if let PlaceRoot::Local(rl) = recv_access.place.root {
                        for id in state.carries.of(rl).iter() {
                            if let Some(loan) = self.loans.get(id) {
                                if let PlaceRoot::Local(owner) = loan.place.root {
                                    out.union_with(state.carries.of(owner));
                                } else {
                                    out.insert(EXTERNAL);
                                }
                            } else if id == EXTERNAL && pointee_named_only {
                                // Outer `&` invocation borrow on a parameter
                                // `&OwnedDyn<..., Context<'a>>` / `&dyn Trait<'a>`
                                // does not flow into a Fixed trait-lifetime return.
                            } else {
                                out.insert(id);
                            }
                        }
                    } else {
                        out.insert(EXTERNAL);
                    }
                    let non_recv = accesses.iter().filter(|a| a.role != OperandRole::Callee);
                    out.union_with(&self.operand_loans(
                        state,
                        non_recv.map(|a| (a.role, a.access, a.place.as_ref())),
                    ));
                    (out, None)
                } else {
                    (loans.clone(), None)
                }
            }
            Terminator::Call {
                callee: zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::StorePop),
                dest,
                ..
            } => {
                let cx = self.body.cx(self.tys);
                let dest_ty = type_place(dest, &cx).ok().map(|t| t.ty());
                if dest_ty.is_some_and(|t| !self.tys.holds_address(t)) {
                    (self.empty_loans(), None)
                } else {
                    let mut out = self.empty_loans();
                    if let Some(store_access) =
                        accesses.iter().find(|a| a.role == OperandRole::Arg(0))
                        && let PlaceRoot::Local(dl) = store_access.place.root
                    {
                        for id in state.carries.of(dl).iter() {
                            if let Some(loan) = self.loans.get(id) {
                                if let PlaceRoot::Local(store_local) = loan.place.root {
                                    out.union_with(state.carries.of(store_local));
                                } else {
                                    out.insert(EXTERNAL);
                                }
                            } else {
                                out.insert(id);
                            }
                        }
                    } else {
                        out.insert(EXTERNAL);
                    }
                    (out, None)
                }
            }
            Terminator::Call { .. }
            | Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => (loans.clone(), None),
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
                        | zil_ir::body::IntrinsicOp::RegistrationClose
                        | zil_ir::body::IntrinsicOp::StoreReserve
                        | zil_ir::body::IntrinsicOp::StorePush
                        | zil_ir::body::IntrinsicOp::StorePop
                        | zil_ir::body::IntrinsicOp::StoreLen
                        | zil_ir::body::IntrinsicOp::StoreCapacity
                        | zil_ir::body::IntrinsicOp::StoreBorrow { .. },
                    ),
                ..
            } => false,
            Terminator::Call {
                callee: zil_ir::body::Callee::DynSlot { .. },
                ..
            } if !accesses.iter().any(|a| {
                a.role != OperandRole::Callee
                    && flows_value(a.role, a.access)
                    && self.place_holds_address(&a.place)
            }) =>
            {
                false
            }
            Terminator::Call { .. } => {
                let cx = self.body.cx(self.tys);
                accesses.iter().any(|a| {
                    if !flows_value(a.role, a.access) || !self.place_may_store_address(&a.place) {
                        return false;
                    }
                    let direct_external = self.storage_targets(state, a.place.root, true).1;
                    if direct_external {
                        return true;
                    }
                    let nested_stores = type_place(&a.place, &cx).map_or(true, |t| {
                        let ty = t.ty();
                        match self.tys.get(ty) {
                            Some(zil_ir::body::TyKind::Ref {
                                mutability: zil_ir::body::Mutability::Exclusive,
                                pointee,
                                ..
                            }) => self.tys.may_store_address(*pointee),
                            _ => self.tys.may_store_address(ty),
                        }
                    });
                    nested_stores
                        && self
                            .reach(state, a.place.root)
                            .iter()
                            .any(|id| id == EXTERNAL || self.loans.loan_region(id).is_some())
                })
            }
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
            ret_field_loans,
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
                let has_unannotated = self.tys.has_unannotated_ref(self.body.result);
                let unannotated_only = allowed.is_empty() && has_unannotated;
                let require_named =
                    self.body.closure.is_none() && !allowed.is_empty() && !has_unannotated;
                for id in self.close(state, &flows.loans).iter() {
                    if !unannotated_only
                        && let Some(region) = self.loans.loan_region(id)
                        && !allowed.contains(&region)
                    {
                        return Err(ViolationKind::RegionEscape { region });
                    }
                    if id == EXTERNAL && require_named {
                        return Err(ViolationKind::RegionEscape { region: u32::MAX });
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
                        let has_invariant = sig.params.iter().any(|pt| {
                            if !self.tys.regions_in(*pt).contains(&r) {
                                return false;
                            }
                            let inner = match self.tys.get(*pt) {
                                Some(
                                    zil_ir::body::TyKind::Managed(t)
                                    | zil_ir::body::TyKind::Ref { pointee: t, .. },
                                ) => *t,
                                _ => *pt,
                            };
                            matches!(
                                self.tys.get(inner),
                                Some(
                                    zil_ir::body::TyKind::Opaque(zil_ir::body::OpaqueTy::Task {
                                        context: Some(_),
                                        ..
                                    }) | zil_ir::body::TyKind::OwnedDyn {
                                        context: Some(_),
                                        ..
                                    } | zil_ir::body::TyKind::Closure { .. }
                                )
                            )
                        });
                        if !has_invariant {
                            continue;
                        }
                        let mut prev_local_loans: Option<Vec<_>> = None;
                        for (idx, pt) in sig.params.iter().enumerate() {
                            if !self.tys.regions_in(*pt).contains(&r) {
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
                    // address-holding storage it receives. For a Closure whose
                    // captures cannot store addresses (`!may_store_address`), a
                    // direct callee can only introduce a loan into it if the
                    // callee actually writes an address-holding value through
                    // that parameter (`addr_store_params`).
                    let direct_writable_loans = match callee {
                        zil_ir::body::Callee::Direct(f) => self
                            .sigs
                            .return_summaries
                            .and_then(|ms| ms.get(f.index()))
                            .map(|summary| {
                                let accesses = term.accesses();
                                let matching = accesses.iter().filter(|a| {
                                    let OperandRole::Arg(idx) = a.role else {
                                        return false;
                                    };
                                    idx >= 64 || (summary.addr_store_params & (1u64 << idx)) != 0
                                });
                                self.operand_loans(
                                    state,
                                    matching.map(|a| (a.role, a.access, a.place.as_ref())),
                                )
                            }),
                        zil_ir::body::Callee::Indirect(_)
                        | zil_ir::body::Callee::Closure { .. }
                        | zil_ir::body::Callee::DynSlot { .. }
                        | zil_ir::body::Callee::Foreign(_)
                        | zil_ir::body::Callee::Intrinsic(_) => None,
                    };
                    let cx = self.body.cx(self.tys);
                    let mut writable = Vec::new();
                    for id in flows.loans.iter() {
                        let Some(loan) = self.loans.get(id) else {
                            continue;
                        };
                        if loan.kind.writes() && loan.stores_addresses && loan.anchored {
                            let is_non_storing_closure =
                                type_place(&loan.place, &cx).ok().is_some_and(|t| {
                                    matches!(
                                        self.tys.get(t.ty()),
                                        Some(zil_ir::body::TyKind::Closure { .. })
                                    ) && !self.tys.may_store_address(t.ty())
                                });
                            if is_non_storing_closure
                                && let Some(dw) = &direct_writable_loans
                                && !dw.contains(id)
                            {
                                continue;
                            }
                            match loan.place.root {
                                PlaceRoot::Local(y) => writable.push(y),
                                PlaceRoot::Capture(c) => {
                                    let mut filtered = self.empty_loans();
                                    for fid in flows.loans.iter() {
                                        if self
                                            .loans
                                            .get(fid)
                                            .is_some_and(|l| l.place.root == PlaceRoot::Capture(c))
                                        {
                                            continue;
                                        }
                                        filtered.insert(fid);
                                    }
                                    self.no_escape(state, &filtered)?;
                                }
                            }
                        }
                    }
                    for y in writable {
                        let mut filtered = self.empty_loans();
                        for id in flows.loans.iter() {
                            if self
                                .loans
                                .get(id)
                                .is_some_and(|l| l.place.root == PlaceRoot::Local(y))
                            {
                                continue;
                            }
                            filtered.insert(id);
                        }
                        state.carries.add(y, &filtered);
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
                let holds = self.place_holds_address(dest);
                let loans = if holds {
                    flows.ret_loans.clone()
                } else {
                    self.empty_loans()
                };
                self.write(state, dest, &loans)?;
                if holds
                    && let Some(fl) = &flows.ret_field_loans
                    && dest.projections.is_empty()
                    && let PlaceRoot::Local(d) = dest.root
                {
                    state.carries.set_fields(d, fl.clone());
                }
                Ok(())
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
        if !state.carries.has_fields() && state.carries.all_non_empty_in(live) {
            return;
        }
        let n = state.carries.locals();
        let mut keep = BitSet::new(n);
        let mut work = Vec::new();
        for l in live.iter() {
            if l >= n {
                break;
            }
            keep.insert(l);
            if !state.carries.of(Local::from_index(l)).is_empty() {
                work.push(l);
            }
        }
        while let Some(l) = work.pop() {
            for id in state.carries.of(Local::from_index(l)).iter() {
                if let Some(loan) = self.loans.get(id)
                    && let PlaceRoot::Local(y) = loan.place.root
                    && y.index() < n
                    && !keep.contains(y.index())
                {
                    keep.insert(y.index());
                    if !state.carries.of(y).is_empty() {
                        work.push(y.index());
                    }
                }
            }
        }
        let mut to_clear: Vec<usize> = state
            .carries
            .non_empty_locals()
            .filter(|&l| !keep.contains(l))
            .collect();
        if state.carries.has_fields() {
            for l in state.carries.field_locals() {
                if !keep.contains(l) {
                    to_clear.push(l);
                }
            }
        }
        for l in to_clear {
            state.carries.clear(Local::from_index(l));
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
                | zil_ir::body::TyKind::Dyn { .. }
                | zil_ir::body::TyKind::OwnedDyn { .. }
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

    fn loans_overlap(&self, a: &BitSet, b: &BitSet) -> Option<OpLocation> {
        for id_a in a.iter() {
            for id_b in b.iter() {
                if id_a == id_b {
                    if let Some(la) = self.loans.get(id_a) {
                        return Some(la.origin);
                    }
                } else if let (Some(la), Some(lb)) = (self.loans.get(id_a), self.loans.get(id_b))
                    && la.place.root == lb.place.root
                    && la.place.relation(&lb.place) != PlaceRelation::Disjoint
                {
                    return Some(la.origin);
                }
            }
        }
        None
    }

    fn closure_def_of_place(
        &self,
        place: &Place,
    ) -> Option<(zil_ir::body::ClosureId, zil_ir::body::ArgsId)> {
        let cx = self.body.cx(self.tys);
        type_place(place, &cx).ok().and_then(|t| {
            let ty = t.ty();
            if let zil_ir::body::TyKind::Closure { def, args } = self.tys.kind(ty) {
                Some((*def, *args))
            } else if let zil_ir::body::TyKind::Managed(inner)
            | zil_ir::body::TyKind::Ref { pointee: inner, .. } = self.tys.kind(ty)
            {
                if let zil_ir::body::TyKind::Closure { def, args } = self.tys.kind(*inner) {
                    Some((*def, *args))
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    fn add_closure_mutated_loans(
        &self,
        state: &State,
        closure: &Place,
        mutated: &mut BitSet,
    ) -> u64 {
        let csum = self.closure_def_of_place(closure).and_then(|(def, _)| {
            self.sigs
                .return_summaries
                .and_then(|ms| ms.iter().find(|s| s.closure == Some(def)))
        });
        let cmut = csum.map_or(u64::MAX, |s| s.mutating_captures);
        if cmut != 0 {
            let mut used_fields = false;
            if closure.projections.is_empty()
                && let PlaceRoot::Local(rl) = closure.root
                && let Some(rfields) = state.carries.fields_of(rl)
            {
                let max_bit = if rfields.len() >= 64 {
                    u64::MAX
                } else {
                    (1u64 << rfields.len()) - 1
                };
                if (cmut & !max_bit) == 0 {
                    used_fields = true;
                    for (ci, fl) in rfields.iter().enumerate() {
                        if (cmut & (1u64 << ci)) != 0 {
                            mutated.union_with(fl);
                        }
                    }
                }
            }
            if !used_fields {
                mutated.union_with(&self.reach(state, closure.root));
            }
        }
        csum.map_or(u64::MAX, |s| s.mutating_params)
    }

    fn check_place_ref_conflict(
        &self,
        state: &State,
        place: &Place,
        mutated: &BitSet,
        skip_caps: u64,
    ) -> Result<(), ViolationKind> {
        let cx = self.body.cx(self.tys);
        let Ok(typing) = type_place(place, &cx) else {
            return Ok(());
        };
        let ty = typing.ty();
        if !crate::ty_holds_reference(self.tys, ty) {
            return Ok(());
        }
        if place.projections.is_empty()
            && let Some((def, subst)) = self.closure_def_of_place(place)
            && let Some(cdef) = self.tys.closure(def)
        {
            let has_unskipped_ref_cap = cdef.captures.iter().enumerate().any(|(ci, cap_ty)| {
                let skipped = ci < 64 && (skip_caps & (1u64 << ci)) != 0;
                if skipped {
                    return false;
                }
                let concrete = self.tys.subst_lookup(*cap_ty, subst).unwrap_or(*cap_ty);
                crate::ty_holds_reference(self.tys, concrete)
            });
            if !has_unskipped_ref_cap {
                return Ok(());
            }
            if let PlaceRoot::Local(l) = place.root
                && let Some(rfields) = state.carries.fields_of(l)
                && cdef.captures.len() == rfields.len()
            {
                for (ci, (cap_ty, fl)) in cdef.captures.iter().zip(rfields.iter()).enumerate() {
                    if ci < 64 && (skip_caps & (1u64 << ci)) != 0 {
                        continue;
                    }
                    let concrete = self.tys.subst_lookup(*cap_ty, subst).unwrap_or(*cap_ty);
                    if crate::ty_holds_reference(self.tys, concrete)
                        && let Some(loan) = self.loans_overlap(fl, mutated)
                    {
                        return Err(ViolationKind::LoanConflict {
                            place: place.clone(),
                            loan,
                        });
                    }
                }
                return Ok(());
            }
        }
        let loans = self.reach_place(state, place);
        if let Some(loan) = self.loans_overlap(&loans, mutated) {
            return Err(ViolationKind::LoanConflict {
                place: place.clone(),
                loan,
            });
        }
        Ok(())
    }

    /// Rejects `StoreBorrow` (`store_at` / `store_at_mut`) when an overlapping
    /// live shared/exclusive loan on the same store conflicts with the requested
    /// borrow mutability.
    pub(crate) fn check_store_borrow_conflict(
        &self,
        state: &State,
        term: &Terminator,
        at: OpLocation,
    ) -> Result<(), ViolationKind> {
        let Terminator::Call {
            callee:
                zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::StoreBorrow { mutability }),
            ..
        } = term
        else {
            return Ok(());
        };
        let Some(new_id) = self.loans.created_at(at) else {
            return Ok(());
        };
        let Some(new_loan) = self.loans.get(new_id) else {
            return Ok(());
        };
        if state.carries.is_empty() {
            return Ok(());
        }
        for id in state.carries.all().iter() {
            let Some(loan) = self.loans.get(id) else {
                continue;
            };
            if loan.kind == LoanKind::Desc {
                continue;
            }
            if loan.place.root != new_loan.place.root {
                if loan.is_store_borrow {
                    if new_loan.proven_unique_owner && loan.proven_unique_owner {
                        continue;
                    }
                } else {
                    let cx = self.body.cx(self.tys);
                    let loan_holds_store = type_place(&loan.place, &cx).map_or(true, |t| {
                        crate::ty_holds_store_or_bytes_desc(self.tys, t.ty())
                    });
                    if !loan_holds_store
                        || (new_loan.proven_unique_owner && loan.proven_unique_owner)
                    {
                        continue;
                    }
                }
            } else if new_loan.place.relation(&loan.place) == PlaceRelation::Disjoint {
                continue;
            }
            if conflicts(PlaceAccess::Borrow(*mutability), loan.kind) {
                return Err(ViolationKind::LoanConflict {
                    place: new_loan.place.clone(),
                    loan: loan.origin,
                });
            }
        }
        Ok(())
    }

    /// Rejects any call that may structurally mutate a `Store` or `ByteBuffer`
    /// (`store_reserve`/`store_push`/`store_pop`/`bytes_reserve`/`bytes_push`/
    /// `bytes_truncate`, directly or transitively through helpers, closures, or
    /// `thread_scope` callbacks) while a reference borrowing from overlapping
    /// storage is live or passed concurrently into the same call.
    pub(crate) fn check_store_mutation_conflict(
        &self,
        state: &State,
        term: &Terminator,
        at: OpLocation,
    ) -> Result<(), ViolationKind> {
        use zil_ir::body::{Callee, IntrinsicOp, Operand};
        let Terminator::Call {
            callee, args, dest, ..
        } = term
        else {
            return Ok(());
        };
        let mut mutated = self.empty_loans();
        let mut thread_parent_mut_caps = 0u64;
        let mut closure_mut_caps = 0u64;
        let mut mutated_arg_mask = 0u64;
        match callee {
            Callee::Intrinsic(op) if crate::is_structural_mutation(*op) => {
                mutated_arg_mask = 1u64;
                if let Some(Operand::Copy(p) | Operand::Move(p)) = args.first() {
                    mutated.union_with(&self.reach(state, p.root));
                }
            }
            Callee::Intrinsic(IntrinsicOp::ThreadScope) => {
                if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(2) {
                    thread_parent_mut_caps = self
                        .closure_def_of_place(p)
                        .and_then(|(def, _)| {
                            self.sigs
                                .return_summaries
                                .and_then(|ms| ms.iter().find(|s| s.closure == Some(def)))
                        })
                        .map_or(0, |s| s.mutating_captures);
                    let _ = self.add_closure_mutated_loans(state, p, &mut mutated);
                }
                if let Some(Operand::Copy(p) | Operand::Move(p)) = args.get(1) {
                    let cx = self.body.cx(self.tys);
                    if type_place(p, &cx).map_or(true, |t| {
                        crate::ty_holds_store_or_bytes_desc(self.tys, t.ty())
                    }) {
                        mutated_arg_mask |= 1u64 << 1;
                        mutated.union_with(&self.reach(state, p.root));
                    }
                }
            }
            Callee::Direct(g) => {
                let gmut = self
                    .sigs
                    .return_summaries
                    .and_then(|ms| ms.get(g.index()))
                    .map_or(0, |s| s.mutating_params);
                mutated_arg_mask = gmut;
                for (ai, arg) in args.iter().enumerate() {
                    if (ai >= 64 || (gmut & (1u64 << ai)) != 0)
                        && let Operand::Copy(p) | Operand::Move(p) = arg
                    {
                        mutated.union_with(&self.reach(state, p.root));
                    }
                }
            }
            Callee::Closure { closure, .. } => {
                closure_mut_caps = self
                    .closure_def_of_place(closure)
                    .and_then(|(def, _)| {
                        self.sigs
                            .return_summaries
                            .and_then(|ms| ms.iter().find(|s| s.closure == Some(def)))
                    })
                    .map_or(0, |s| s.mutating_captures);
                let pmut = self.add_closure_mutated_loans(state, closure, &mut mutated);
                mutated_arg_mask = pmut;
                for (ai, arg) in args.iter().enumerate() {
                    if (ai >= 64 || (pmut & (1u64 << ai)) != 0)
                        && let Operand::Copy(p) | Operand::Move(p) = arg
                    {
                        mutated.union_with(&self.reach(state, p.root));
                    }
                }
            }
            Callee::Indirect(_) | Callee::DynSlot { .. } | Callee::Foreign(_) => {
                let cx = self.body.cx(self.tys);
                for (ai, arg) in args.iter().enumerate() {
                    if let Operand::Copy(p) | Operand::Move(p) = arg
                        && type_place(p, &cx).map_or(true, |t| {
                            crate::ty_holds_store_or_bytes_desc(self.tys, t.ty())
                        })
                    {
                        if ai < 64 {
                            mutated_arg_mask |= 1u64 << ai;
                        }
                        mutated.union_with(&self.reach(state, p.root));
                    }
                }
            }
            Callee::Intrinsic(_) => {}
        }

        if mutated.is_empty() {
            return Ok(());
        }

        // 1. Live references surviving past this call cannot alias the mutated store.
        let live_after = self.live.after(at);
        let overwritten_local = if dest.projections.is_empty() {
            dest.as_local()
        } else {
            None
        };
        let self_closure_skip = match callee {
            Callee::Intrinsic(IntrinsicOp::ThreadScope) => match args.get(2) {
                Some(Operand::Copy(p) | Operand::Move(p)) if p.projections.is_empty() => {
                    p.as_local().map(|l| (l, thread_parent_mut_caps))
                }
                _ => None,
            },
            Callee::Closure { closure, .. } if closure.projections.is_empty() => {
                closure.as_local().map(|l| (l, closure_mut_caps))
            }
            Callee::Direct(_)
            | Callee::Indirect(_)
            | Callee::Closure { .. }
            | Callee::DynSlot { .. }
            | Callee::Foreign(_)
            | Callee::Intrinsic(_) => None,
        };
        let mut mutated_arg_locals: Vec<Local> = Vec::new();
        for (ai, arg) in args.iter().enumerate() {
            if (ai >= 64 || (mutated_arg_mask & (1u64 << ai)) != 0)
                && let Operand::Copy(p) | Operand::Move(p) = arg
                && p.projections.is_empty()
                && let Some(l) = p.as_local()
            {
                mutated_arg_locals.push(l);
            }
        }
        for l_idx in live_after.iter() {
            let l = Local::from_index(l_idx);
            if Some(l) == overwritten_local || mutated_arg_locals.contains(&l) {
                continue;
            }
            let skip = match self_closure_skip {
                Some((cl, mask)) if cl == l => mask,
                _ => 0,
            };
            self.check_place_ref_conflict(state, &Place::local(l), &mutated, skip)?;
        }

        // 2. Reference arguments passed into this call (e.g., `run(p, s)` or
        //    `thread_scope(read, p, body)`) cannot alias the mutated store.
        for (ai, arg) in args.iter().enumerate() {
            if ai < 64 && (mutated_arg_mask & (1u64 << ai)) != 0 {
                continue;
            }
            if let Operand::Copy(p) | Operand::Move(p) = arg {
                let skip =
                    if matches!(callee, Callee::Intrinsic(IntrinsicOp::ThreadScope)) && ai == 2 {
                        thread_parent_mut_caps
                    } else {
                        0
                    };
                self.check_place_ref_conflict(state, p, &mutated, skip)?;
            }
        }

        // 3. Reference captures of the invoked closure (e.g., `outer()` capturing
        //    both `p` and `s`) cannot alias the mutated store.
        if let Callee::Closure { closure, .. } = callee {
            self.check_place_ref_conflict(state, closure, &mutated, closure_mut_caps)?;
        }

        Ok(())
    }
}
