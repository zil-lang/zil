//! The single transfer rules of the place/initialization/variant family.
//!
//! Solver and checker call exactly these functions. The solver iterates them
//! to a fixpoint; the checker applies them once per instruction and edge
//! starting from certified invariants. Neither re-implements a rule.

use crate::loans::{Liveness, LoanTable};
use crate::paths::{MovePaths, PathId, tracked_len};
use crate::state::{Carries, InitState, MustFacts, State};
use zil_ir::body::{
    Access, AdtKind, AggregateKind, BlockId, Body, ClosureKind, ConstraintCx, ConstraintError,
    EdgeKind, FieldIdx, FieldRef, InitClaim, InstructionKind, Local, OpLocation, Operand, Place,
    PlaceAccess, PlaceRelation, PlaceRoot, PlaceTypes, Projection, Rvalue, Sigs, Terminator,
    TyKind, TyTable, check_instruction, check_terminator, type_place,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViolationKind {
    Constraint(ConstraintError),
    /// A place the move-path table does not track (malformed candidate).
    Untracked {
        place: Place,
    },
    UseOfUninit {
        place: Place,
    },
    UseOfMaybeUninit {
        place: Place,
    },
    /// Writing a field of an aggregate that is not itself initialized.
    WriteIntoUninitParent {
        place: Place,
    },
    /// Initializing storage that still holds a cleanup obligation.
    OverwriteLive {
        place: Place,
    },
    /// Projecting into a variant or arm that is not known to be active.
    UnknownVariant {
        place: Place,
        expected: u32,
    },
    /// A cleanup obligation reaches a function exit or storage end.
    LeakedObligation {
        place: Place,
    },
    /// `Unreachable` is reachable under the semantics.
    ReachableUnreachable,
    /// The access conflicts with a live loan created at `loan`.
    LoanConflict {
        place: Place,
        loan: OpLocation,
    },
    /// The returned value may keep a loan of this frame's storage alive.
    ReturnsLocalBorrow {
        loan: OpLocation,
    },
    /// A loan of this frame's storage may be stored where it outlives the frame.
    EscapingLocalBorrow {
        loan: OpLocation,
    },
    /// The operation has no transition rule yet; rejected explicitly.
    Unspecified(&'static str),
    /// A malformed drop-flag declaration.
    DropFlagDecl {
        flag: Local,
        reason: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub at: OpLocation,
    pub kind: ViolationKind,
}

/// One successor of a block: `None` state means the edge is infeasible.
pub type Successor = (EdgeKind, BlockId, Option<State>);

pub struct Kernel<'a> {
    pub body: &'a Body,
    pub tys: &'a TyTable,
    pub sigs: Sigs<'a>,
    pub paths: MovePaths,
    pub loans: LoanTable,
    pub live: Liveness,
}

impl<'a> Kernel<'a> {
    pub fn new(body: &'a Body, tys: &'a TyTable, sigs: Sigs<'a>) -> Self {
        Kernel {
            body,
            tys,
            sigs,
            paths: MovePaths::build(body),
            loans: LoanTable::build(body, tys),
            live: Liveness::compute(body),
        }
    }

    fn cx(&self) -> ConstraintCx<'a> {
        ConstraintCx {
            body: self.body.cx(self.tys),
            sigs: self.sigs,
        }
    }

    /// Number of tags a tagged place can hold: the variants of an Enum, or
    /// the two arms of an error union. `None` for any other place.
    fn tag_count(&self, place: &Place) -> Option<u32> {
        let ty = type_place(place, &self.body.cx(self.tys)).ok()?.ty();
        match self.tys.get(ty)? {
            TyKind::Adt { adt, .. } if self.tys.adt(*adt).kind == AdtKind::Enum => {
                u32::try_from(self.tys.adt(*adt).variants.len()).ok()
            }
            TyKind::ErrorUnion { .. } => Some(2),
            TyKind::Adt { .. }
            | TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Tuple(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Managed(_)
            | TyKind::Desc(_)
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn(_)
            | TyKind::Param(_)
            | TyKind::Opaque(_) => None,
        }
    }

    /// OpSpec constraints of every operation. Part of every verdict.
    pub fn check_structure(&self) -> Result<(), Violation> {
        if self.body.blocks.is_empty() {
            return Err(Violation {
                at: OpLocation {
                    block: BlockId(0),
                    instruction: None,
                },
                kind: ViolationKind::Constraint(ConstraintError::UnknownBlock(BlockId(0))),
            });
        }
        let cx = self.cx();
        for (b, block) in self.body.blocks.iter().enumerate() {
            let block_id = BlockId::from_index(b);
            for (i, inst) in block.instructions.iter().enumerate() {
                check_instruction(&inst.kind, &cx).map_err(|e| Violation {
                    at: OpLocation {
                        block: block_id,
                        instruction: Some(i as u32),
                    },
                    kind: ViolationKind::Constraint(e),
                })?;
            }
            check_terminator(&block.terminator, &cx).map_err(|e| Violation {
                at: OpLocation {
                    block: block_id,
                    instruction: None,
                },
                kind: ViolationKind::Constraint(e),
            })?;
        }
        self.check_drop_flags().map_err(|kind| Violation {
            at: OpLocation {
                block: BlockId(0),
                instruction: None,
            },
            kind,
        })
    }

    fn check_drop_flags(&self) -> Result<(), ViolationKind> {
        let cx = self.body.cx(self.tys);
        for (i, decl) in self.body.drop_flags.iter().enumerate() {
            let bad = |reason| ViolationKind::DropFlagDecl {
                flag: decl.flag,
                reason,
            };
            let ty = cx.local_ty(decl.flag).ok_or(bad("unknown flag local"))?;
            if !matches!(self.tys.get(ty), Some(TyKind::Bool)) {
                return Err(bad("flag is not Bool"));
            }
            if self.body.is_param(decl.flag) {
                return Err(bad("flag is a parameter"));
            }
            if self.body.drop_flags[..i]
                .iter()
                .any(|d| d.flag == decl.flag)
            {
                return Err(bad("flag declared twice"));
            }
            let PlaceRoot::Local(root) = decl.place.root else {
                return Err(bad("flagged place is not local storage"));
            };
            if root == decl.flag {
                return Err(bad("flag guards itself"));
            }
            if tracked_len(&decl.place) != decl.place.projections.len() {
                return Err(bad("flagged place is not inline"));
            }
            type_place(&decl.place, &cx).map_err(|_| bad("flagged place is ill-typed"))?;
        }
        Ok(())
    }

    /// The flag declared for `flag`, if any.
    fn declared_flag(&self, flag: Local) -> Option<&Place> {
        self.body
            .drop_flags
            .iter()
            .find(|d| d.flag == flag)
            .map(|d| &d.place)
    }

    /// State on function entry: parameters and captures initialized.
    pub fn entry_state(&self) -> State {
        let mut init = InitState::all_uninit(&self.paths);
        for p in self.body.params() {
            if let Some(path) = self.paths.root(PlaceRoot::Local(p)) {
                init.set_subtree(&self.paths, path, InitClaim::Init);
            }
        }
        for c in 0..self.body.captures.len() {
            if let Some(path) = self.paths.root(PlaceRoot::Capture(c as u32)) {
                init.set_subtree(&self.paths, path, InitClaim::Init);
            }
        }
        let mut carries = Carries::empty(self.body.locals.len(), self.loans.len());
        for p in self.body.params() {
            if self.local_holds_address(p) {
                carries.set(p, self.external());
            }
        }
        let mut state = State {
            init,
            variants: MustFacts::default(),
            tags: MustFacts::default(),
            flags: MustFacts::default(),
            carries,
        };
        self.prune(&mut state, &self.live.entry(BlockId(0)).clone());
        state
    }

    pub fn instruction(
        &self,
        state: &mut State,
        kind: &InstructionKind,
        at: OpLocation,
    ) -> Result<(), Violation> {
        let fail = |kind| Violation { at, kind };
        match kind {
            InstructionKind::StorageLive(l) => {
                let path = self.root_path(*l).map_err(fail)?;
                self.storage_conflict(state, *l).map_err(fail)?;
                self.obligation(state, path).map_err(fail)?;
                state.init.set_subtree(&self.paths, path, InitClaim::Uninit);
                self.invalidate_root(state, PlaceRoot::Local(*l));
                state.carries.clear(*l);
                self.prune(state, &self.live.after(at).clone());
                return Ok(());
            }
            InstructionKind::StorageDead(l) => {
                let path = self.root_path(*l).map_err(fail)?;
                self.storage_conflict(state, *l).map_err(fail)?;
                self.obligation(state, path).map_err(fail)?;
                state.init.set_subtree(&self.paths, path, InitClaim::Uninit);
                self.invalidate_root(state, PlaceRoot::Local(*l));
                state.carries.clear(*l);
                self.prune(state, &self.live.after(at).clone());
                return Ok(());
            }
            InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => {
                self.prune(state, &self.live.after(at).clone());
                return Ok(());
            }
            InstructionKind::RegisterDefer { .. } => {
                return Err(fail(ViolationKind::Unspecified(
                    "deferred cleanup registration",
                )));
            }
            InstructionKind::Assign { .. }
            | InstructionKind::Drop { .. }
            | InstructionKind::Replace { .. } => {}
        }
        // Loans flowing into written places are read from the state before
        // the operation consumes its operands.
        let flows = self.instruction_flows(state, kind, at);
        for access in kind.accesses() {
            self.access(state, &access).map_err(fail)?;
        }
        self.apply_flows(state, flows).map_err(fail)?;
        // Facts established by the completed operation.
        if let InstructionKind::Assign { dest, value } = kind {
            match value {
                Rvalue::Aggregate {
                    kind: AggregateKind::Adt { adt, variant, .. },
                    ..
                } if self.tys.adt(*adt).kind == AdtKind::Enum => {
                    state.variants.insert(dest.clone(), variant.0);
                }
                Rvalue::ErrorUnion { ok, .. } => {
                    state.variants.insert(dest.clone(), u32::from(*ok))
                }
                Rvalue::Discriminant(q) | Rvalue::ErrorArm(q) => {
                    if let Some(d) = dest.as_local() {
                        state.tags.insert(d, q.clone());
                    }
                }
                Rvalue::Use(Operand::Const(zil_ir::body::ConstValue::Bool(set))) => {
                    self.establish_flag(state, dest, *set);
                }
                Rvalue::Use(_)
                | Rvalue::Ref { .. }
                | Rvalue::SliceRef { .. }
                | Rvalue::Describe { .. }
                | Rvalue::Aggregate { .. }
                | Rvalue::Unary { .. }
                | Rvalue::Binary { .. }
                | Rvalue::Convert { .. }
                | Rvalue::Len(_)
                | Rvalue::FnRef(_)
                | Rvalue::ForeignFnRef(_)
                | Rvalue::StaticUtf(_) => {}
            }
        }
        self.prune(state, &self.live.after(at).clone());
        Ok(())
    }

    /// Applies the terminator and returns every successor with its state.
    pub fn terminator(
        &self,
        mut state: State,
        term: &Terminator,
        at: OpLocation,
    ) -> Result<Vec<Successor>, Violation> {
        let fail = |kind| Violation { at, kind };
        // Facts read by edge conditions are taken before operands are used.
        let discr_tag = match term {
            Terminator::Switch {
                discr: Operand::Copy(p) | Operand::Move(p),
                ..
            } => p.as_local().and_then(|d| state.tags.get(&d).cloned()),
            Terminator::Switch { .. }
            | Terminator::Goto { .. }
            | Terminator::Return { .. }
            | Terminator::Call { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => None,
        };
        let discr_flag = match term {
            Terminator::Switch {
                discr: Operand::Copy(p) | Operand::Move(p),
                ..
            } => p.as_local().and_then(|d| state.flags.get(&d).cloned()),
            Terminator::Switch { .. }
            | Terminator::Goto { .. }
            | Terminator::Return { .. }
            | Terminator::Call { .. }
            | Terminator::Assert { .. }
            | Terminator::Unreachable
            | Terminator::Trap(_) => None,
        };
        match term {
            Terminator::Unreachable => return Err(fail(ViolationKind::ReachableUnreachable)),
            Terminator::Trap(_) => return Ok(Vec::new()),
            Terminator::Call {
                callee: zil_ir::body::Callee::Intrinsic(op),
                ..
            } => intrinsic_rule(*op).map_err(|what| fail(ViolationKind::Unspecified(what)))?,
            Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Call { .. }
            | Terminator::Assert { .. } => {}
        }
        let flows = self.terminator_flows(&state, term);
        for access in term.accesses() {
            self.access(&mut state, &access).map_err(fail)?;
        }
        self.apply_terminator_flows(&mut state, term, &flows)
            .map_err(fail)?;
        if let Terminator::Call { .. } = term {
            // A callee may write through any address or descriptor it can
            // reach; facts about storage behind an indirection end here.
            state.variants.retain(|p, _| p.first_deref().is_none());
            state.tags.retain(|_, p| p.first_deref().is_none());
        }
        if let Terminator::Return { .. } = term {
            self.exit_obligations(&state).map_err(fail)?;
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for (kind, target) in term.edges() {
            let mut s = state.clone();
            for access in term.edge_accesses(kind) {
                self.access(&mut s, &access).map_err(fail)?;
            }
            self.apply_edge_flows(&mut s, term, kind, &flows)
                .map_err(fail)?;
            self.prune(&mut s, &self.live.entry(target).clone());
            if let (Some(q), Terminator::Switch { arms, .. }) = (&discr_flag, term) {
                self.refine_by_flag(&mut s, q, arms, kind);
            }
            let feasible = match (&discr_tag, kind) {
                (Some(q), EdgeKind::SwitchArm(v)) => {
                    let v = u32::try_from(v).ok();
                    match (s.variants.get(q).copied(), v) {
                        (Some(known), Some(v)) if known != v => false,
                        (_, None) => false,
                        (_, Some(v)) => {
                            s.variants.insert(q.clone(), v);
                            true
                        }
                    }
                }
                (Some(q), EdgeKind::SwitchOtherwise) => {
                    let Terminator::Switch { arms, .. } = term else {
                        return Err(fail(ViolationKind::Constraint(ConstraintError::Operator {
                            reason: "otherwise edge outside a switch",
                        })));
                    };
                    let covered = |v: u32| arms.iter().any(|(a, _)| *a == u128::from(v));
                    match (s.variants.get(q).copied(), self.tag_count(q)) {
                        (Some(known), _) => !covered(known),
                        (None, Some(count)) => {
                            let mut remaining = (0..count).filter(|v| !covered(*v));
                            match (remaining.next(), remaining.next()) {
                                (None, _) => false,
                                (Some(only), None) => {
                                    s.variants.insert(q.clone(), only);
                                    true
                                }
                                (Some(_), Some(_)) => true,
                            }
                        }
                        (None, None) => true,
                    }
                }
                (
                    _,
                    EdgeKind::Goto
                    | EdgeKind::SwitchArm(_)
                    | EdgeKind::SwitchOtherwise
                    | EdgeKind::CallReturn
                    | EdgeKind::CallCancel
                    | EdgeKind::AssertOk,
                ) => true,
            };
            out.push((kind, target, feasible.then_some(s)));
        }
        Ok(out)
    }

    // ---- access rule ---------------------------------------------------------

    fn access(&self, state: &mut State, access: &Access<'_>) -> Result<(), ViolationKind> {
        let place: &Place = &access.place;
        self.loan_conflict(state, place, access.access)?;
        self.check_variant_steps(state, place)?;
        let path = self
            .paths
            .tracked(place)
            .ok_or_else(|| ViolationKind::Untracked {
                place: place.clone(),
            })?;
        let through = tracked_len(place) < place.projections.len();
        match access.access {
            PlaceAccess::Copy
            | PlaceAccess::Describe
            | PlaceAccess::Borrow(_)
            | PlaceAccess::CallClosure(
                zil_ir::body::ClosureCallMode::Shared | zil_ir::body::ClosureCallMode::Mut,
            ) => self.require_full(state, path),
            PlaceAccess::InspectTag | PlaceAccess::InspectLen => {
                if through {
                    self.require_full(state, path)
                } else {
                    self.require_own(state, path)
                }
            }
            PlaceAccess::Move
            | PlaceAccess::CallClosure(zil_ir::body::ClosureCallMode::Once)
            | PlaceAccess::Drop => {
                // OpSpec has excluded indirect and borrowed storage.
                self.require_full(state, path)?;
                state.init.set_subtree(&self.paths, path, InitClaim::Uninit);
                self.invalidate(state, place);
                Ok(())
            }
            PlaceAccess::ReplaceInstance => {
                self.require_full(state, path)?;
                self.invalidate(state, place);
                Ok(())
            }
            PlaceAccess::Init => {
                if through {
                    // Writing behind an indirection: the base must be usable;
                    // OpSpec guarantees the old value carries no obligation.
                    self.require_full(state, path)?;
                } else {
                    for ancestor in self.paths.ancestors(path) {
                        if !state.init.is_definitely_init(ancestor) {
                            return Err(ViolationKind::WriteIntoUninitParent {
                                place: place.clone(),
                            });
                        }
                    }
                    self.obligation(state, path).map_err(|e| {
                        if let ViolationKind::LeakedObligation { place } = e {
                            ViolationKind::OverwriteLive { place }
                        } else {
                            e
                        }
                    })?;
                    state.init.set_subtree(&self.paths, path, InitClaim::Init);
                }
                self.invalidate(state, place);
                if let PlaceRoot::Local(d) = place.root {
                    state.tags.retain(|l, _| *l != d);
                }
                Ok(())
            }
        }
    }

    fn require_own(&self, state: &State, path: PathId) -> Result<(), ViolationKind> {
        match state.init.claim(path) {
            InitClaim::Init => Ok(()),
            InitClaim::Uninit => Err(ViolationKind::UseOfUninit {
                place: self.paths.place(path).clone(),
            }),
            InitClaim::Maybe => Err(ViolationKind::UseOfMaybeUninit {
                place: self.paths.place(path).clone(),
            }),
        }
    }

    fn require_full(&self, state: &State, path: PathId) -> Result<(), ViolationKind> {
        for p in self.paths.subtree(path) {
            self.require_own(state, p)?;
        }
        Ok(())
    }

    fn check_variant_steps(&self, state: &State, place: &Place) -> Result<(), ViolationKind> {
        for (i, step) in place.projections.iter().enumerate() {
            let expected = match step {
                Projection::Field(FieldRef { adt, variant, .. })
                    if self
                        .tys
                        .get_adt(*adt)
                        .is_some_and(|d| d.kind == AdtKind::Enum) =>
                {
                    variant.0
                }
                Projection::ErrorArm(ok) => u32::from(*ok),
                Projection::Field(_)
                | Projection::TupleField(_)
                | Projection::Index(_)
                | Projection::ConstIndex(_)
                | Projection::Deref => continue,
            };
            let base = place.prefix(i);
            if state.variants.get(&base) != Some(&expected) {
                return Err(ViolationKind::UnknownVariant {
                    place: base,
                    expected,
                });
            }
        }
        Ok(())
    }

    /// Removes facts a write/move of `written` (or a writing loan of it) may
    /// falsify. Facts about a strict sub-place's container survive: writing
    /// a payload field does not change its container's tag. Facts keyed by a
    /// local (tags, flags) die with any write into that local.
    pub(crate) fn invalidate(&self, state: &mut State, written: &Place) {
        let stale = |p: &Place| {
            !matches!(
                written.relation(p),
                PlaceRelation::Disjoint | PlaceRelation::ContainedBy
            )
        };
        let key_written = |l: &Local| written.root == PlaceRoot::Local(*l);
        state.variants.retain(|p, _| !stale(p));
        state.tags.retain(|l, p| !stale(p) && !key_written(l));
        // Any change inside a flagged place changes its initialization.
        state
            .flags
            .retain(|l, p| written.relation(p) == PlaceRelation::Disjoint && !key_written(l));
    }

    fn invalidate_root(&self, state: &mut State, root: PlaceRoot) {
        state.variants.retain(|p, _| p.root != root);
        state
            .tags
            .retain(|l, p| p.root != root && PlaceRoot::Local(*l) != root);
        state
            .flags
            .retain(|l, p| p.root != root && PlaceRoot::Local(*l) != root);
    }

    /// After `flag = set`: if `flag` is declared for a place whose whole
    /// subtree is definitely in the state `set` names, the two are equal.
    fn establish_flag(&self, state: &mut State, dest: &Place, set: bool) {
        let Some(flag) = dest.as_local() else { return };
        let Some(place) = self.declared_flag(flag) else {
            return;
        };
        let Some(path) = self.paths.lookup(place) else {
            return;
        };
        let holds = if set {
            state.init.subtree_definitely_init(&self.paths, path)
        } else {
            state.init.subtree_definitely_uninit(&self.paths, path)
        };
        if holds {
            state.flags.insert(flag, place.clone());
        }
    }

    /// On a switch over a flag known to equal "`place` is initialized", each
    /// edge whose flag value is fixed fixes the initialization of `place`.
    fn refine_by_flag(
        &self,
        state: &mut State,
        place: &Place,
        arms: &[(u128, BlockId)],
        kind: EdgeKind,
    ) {
        let value = match kind {
            EdgeKind::SwitchArm(v) => Some(v),
            EdgeKind::SwitchOtherwise => {
                let covered = |v: u128| arms.iter().any(|(a, _)| *a == v);
                match (covered(0), covered(1)) {
                    (true, false) => Some(1),
                    (false, true) => Some(0),
                    (true, true) | (false, false) => None,
                }
            }
            EdgeKind::Goto | EdgeKind::CallReturn | EdgeKind::CallCancel | EdgeKind::AssertOk => {
                None
            }
        };
        let Some(path) = self.paths.lookup(place) else {
            return;
        };
        match value {
            Some(0) => state.init.set_subtree(&self.paths, path, InitClaim::Uninit),
            Some(1) => state.init.set_subtree(&self.paths, path, InitClaim::Init),
            Some(_) | None => {}
        }
    }

    fn root_path(&self, local: Local) -> Result<PathId, ViolationKind> {
        self.paths
            .root(PlaceRoot::Local(local))
            .ok_or_else(|| ViolationKind::Untracked {
                place: Place::local(local),
            })
    }

    // ---- obligations ---------------------------------------------------------

    /// Fails if the value at `path` may still carry a cleanup obligation.
    pub fn obligation(&self, state: &State, path: PathId) -> Result<(), ViolationKind> {
        let place = self.paths.place(path).clone();
        let leak = || ViolationKind::LeakedObligation {
            place: place.clone(),
        };
        let env = &self.body.param_env;
        let ty = type_place(&place, &self.body.cx(self.tys))
            .map_err(|e| ViolationKind::Constraint(ConstraintError::Place(e)))?
            .ty();
        if state.init.claim(path) == InitClaim::Uninit {
            // Descendants of an uninitialized path must be uninitialized too;
            // a forged state that says otherwise is checked, not trusted.
            return match self
                .paths
                .subtree(path)
                .skip(1)
                .find(|c| state.init.claim(*c) != InitClaim::Uninit)
            {
                Some(c) => self.obligation(state, c),
                None => Ok(()),
            };
        }
        if !self.tys.needs_cleanup(ty, env) {
            return Ok(());
        }
        if self.paths.children(path).next().is_none() {
            return Err(leak());
        }
        // Partially moved: the obligation is that of each remaining part.
        let components: Vec<(Place, zil_ir::body::TyId)> = match self.tys.kind(ty) {
            TyKind::Adt { adt, args } => {
                let def = self.tys.adt(*adt);
                if def.user_drop {
                    return Err(leak());
                }
                let variant = match def.kind {
                    AdtKind::Enum => match state.variants.get(&place) {
                        Some(v) => zil_ir::body::VariantIdx(*v),
                        None => return Err(leak()),
                    },
                    AdtKind::Record | AdtKind::Package | AdtKind::Resource => {
                        zil_ir::body::VariantIdx(0)
                    }
                };
                let fields = def.variant(variant).map(|v| v.fields.len()).unwrap_or(0);
                let mut out = Vec::with_capacity(fields);
                for f in 0..fields {
                    let field = FieldRef {
                        adt: *adt,
                        variant,
                        field: FieldIdx::from_index(f),
                        args: *args,
                    };
                    let fty = self
                        .tys
                        .field_ty(*adt, variant, field.field, *args)
                        .ok_or_else(leak)?;
                    out.push((place.clone().field(field), fty));
                }
                out
            }
            TyKind::Tuple(items) => items
                .iter()
                .enumerate()
                .map(|(i, t)| (place.clone().project(Projection::TupleField(i as u32)), *t))
                .collect(),
            TyKind::ErrorUnion { ok, err } => match state.variants.get(&place) {
                Some(1) => vec![(place.clone().project(Projection::ErrorArm(true)), *ok)],
                Some(0) => vec![(place.clone().project(Projection::ErrorArm(false)), *err)],
                _ => return Err(leak()),
            },
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Managed(_)
            | TyKind::Desc(_)
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn(_)
            | TyKind::Param(_)
            | TyKind::Opaque(_) => return Err(leak()),
        };
        for (component, cty) in components {
            match self.paths.lookup(&component) {
                Some(child) => self.obligation(state, child)?,
                // Never moved separately: it shares the parent's state, which
                // is live or maybe-live here.
                None if self.tys.needs_cleanup(cty, env) => {
                    return Err(ViolationKind::LeakedObligation { place: component });
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Every owned root must be settled when the function returns.
    pub fn exit_obligations(&self, state: &State) -> Result<(), ViolationKind> {
        for i in 0..self.body.locals.len() {
            self.obligation(state, self.root_path(Local::from_index(i))?)?;
        }
        let owns_captures = self
            .body
            .closure
            .and_then(|c| self.tys.closure(c))
            .is_some_and(|c| c.kind == ClosureKind::FnOnce);
        if owns_captures {
            for c in 0..self.body.captures.len() {
                if let Some(path) = self.paths.root(PlaceRoot::Capture(c as u32)) {
                    self.obligation(state, path)?;
                }
            }
        }
        Ok(())
    }
}

/// Intrinsics whose effects are exactly the ordinary call effects: operands
/// are read or moved, the destination is initialized on return, and loans
/// flow as for any call. A Desc argument keeps its resource described for
/// the call only; none of these returns an address. Every other intrinsic
/// has a protocol that is not specified yet.
fn intrinsic_rule(op: zil_ir::body::IntrinsicOp) -> Result<(), &'static str> {
    use zil_ir::body::IntrinsicOp as I;
    match op {
        I::ManagedCreate
        | I::OwnedDynCreate
        | I::StoreNew
        | I::StoreReserve
        | I::StorePush
        | I::StorePop
        | I::StoreLen
        | I::StoreCapacity
        | I::StoreBorrow { .. }
        | I::ByteBufferNew
        | I::ByteBufferReserve
        | I::ByteBufferPush
        | I::ByteBufferTruncate
        | I::ByteBufferLen
        | I::ByteBufferCapacity
        | I::ByteBufferBorrow
        | I::MutexCreate
        | I::MutexLock
        | I::MutexValue
        | I::ThreadScope
        | I::TaskCreate
        | I::TaskYield
        | I::TaskPoll
        | I::TaskRequestCancel
        | I::TaskPollClose
        | I::TaskFinishDestroy
        | I::AsyncRegistrationStart
        | I::AsyncRegistrationValid
        | I::AsyncRegistrationRequestClose
        | I::AsyncRegistrationPollClose
        | I::AsyncRegistrationFinishDestroy => Ok(()),
        I::MutexUnlock
        | I::SyncLoad
        | I::SyncStore
        | I::SyncFetchAdd
        | I::SyncCompareExchange
        | I::TaskStart
        | I::TaskValid
        | I::RegistrationStart
        | I::RegistrationValid
        | I::RegistrationClose => Err(zil_ir::body::intrinsic_name(op)),
    }
}
