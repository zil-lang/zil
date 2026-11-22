//! Loan table and local liveness of one body.
//!
//! Both are pure functions of the IR, built deterministically, so a solver
//! and a checker obtain identical numbering without trusting each other.
//! Certificates name loans by [`LoanOrigin`] and the checker resolves them
//! here; the place and kind of a loan are never taken from a claim.
//!
//! Liveness is not a certificate claim: it is derived from the body alone by
//! a monotone backward fixpoint bounded by `blocks × (locals + 1)` rounds.

use crate::state::BitSet;
use zil_ir::body::{
    Body, InstructionKind, LoanOrigin, Local, Mutability, OpLocation, Place, PlaceAccess,
    PlaceRoot, Rvalue, Terminator, TyTable, type_place,
};

/// Loan id 0 is the pseudo-loan standing for storage outside the body.
pub const EXTERNAL: usize = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoanKind {
    Shared,
    Exclusive,
    /// A resource descriptor: copies share identity and may write the target.
    Desc,
}

impl LoanKind {
    /// Whether the holder may write the borrowed storage.
    pub fn writes(self) -> bool {
        match self {
            LoanKind::Shared => false,
            LoanKind::Exclusive | LoanKind::Desc => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loan {
    pub origin: OpLocation,
    pub place: Place,
    pub kind: LoanKind,
    /// The borrowed storage itself belongs to this body's frame (no `Deref`
    /// on the path): letting the loan outlive the body is always dangling.
    pub anchored: bool,
    /// The borrowed storage may itself hold addresses, so writes through the
    /// loan can store further loans into it.
    pub stores_addresses: bool,
}

#[derive(Clone, Debug)]
pub struct LoanTable {
    /// Index 0 is `EXTERNAL`; indices `1..=region_count` are `Region(0..region_count)`.
    region_count: u32,
    loans: Vec<Option<Loan>>,
}

fn resolve_store_borrow_place(body: &Body, arg_place: &Place) -> Place {
    let mut cur = arg_place.clone();
    let mut seen: Vec<Local> = Vec::new();
    while cur.projections.is_empty() {
        let PlaceRoot::Local(l) = cur.root else {
            break;
        };
        if seen.contains(&l) || body.is_param(l) {
            break;
        }
        seen.push(l);
        let mut def_rhs: Option<&Rvalue> = None;
        let mut multiple = false;
        for block in &body.blocks {
            for inst in &block.instructions {
                if let InstructionKind::Assign { dest, value } = &inst.kind
                    && dest.as_local() == Some(l)
                {
                    if def_rhs.is_some() {
                        multiple = true;
                        break;
                    }
                    def_rhs = Some(value);
                }
            }
            if multiple {
                break;
            }
            if let Terminator::Call { dest, .. } = &block.terminator
                && dest.as_local() == Some(l)
            {
                multiple = true;
                break;
            }
        }
        if multiple {
            break;
        }
        match def_rhs {
            Some(Rvalue::Describe { place }) => return place.clone(),
            Some(Rvalue::Use(zil_ir::body::Operand::Copy(p) | zil_ir::body::Operand::Move(p))) => {
                cur = p.clone();
            }
            None
            | Some(
                Rvalue::Use(zil_ir::body::Operand::Const(_) | zil_ir::body::Operand::Pooled(_))
                | Rvalue::Ref { .. }
                | Rvalue::SliceRef { .. }
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
                | Rvalue::StaticUtf(_),
            ) => break,
        }
    }
    cur
}

impl LoanTable {
    pub fn build(body: &Body, tys: &TyTable) -> Self {
        let mut region_count = 0u32;
        for p in body.params() {
            if let Some(d) = body.locals.get(p.index()) {
                for r in tys.regions_in(d.ty) {
                    region_count = region_count.max(r + 1);
                }
            }
        }
        for r in tys.regions_in(body.result) {
            region_count = region_count.max(r + 1);
        }
        let mut loans = vec![None; 1 + region_count as usize];
        let cx = body.cx(tys);
        for (b, block) in body.blocks.iter().enumerate() {
            for (i, inst) in block.instructions.iter().enumerate() {
                let value = match &inst.kind {
                    InstructionKind::Assign { value, .. } => value,
                    InstructionKind::Drop { .. }
                    | InstructionKind::Replace { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter(_)
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::RegisterDefer { .. } => continue,
                };
                let (place, kind) = match value {
                    Rvalue::Ref {
                        place, mutability, ..
                    }
                    | Rvalue::SliceRef {
                        place, mutability, ..
                    } => (
                        place,
                        match mutability {
                            Mutability::Shared => LoanKind::Shared,
                            Mutability::Exclusive => LoanKind::Exclusive,
                        },
                    ),
                    Rvalue::Describe { place } => (place, LoanKind::Desc),
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
                    | Rvalue::StaticUtf(_) => continue,
                };
                let stores_addresses = type_place(place, &cx)
                    .map(|t| tys.holds_address(t.ty()))
                    .unwrap_or(true);
                let closure_kind = body.closure.and_then(|c| tys.closure(c)).map(|c| c.kind);
                let anchored = place.first_deref().is_none()
                    || (matches!(place.root, PlaceRoot::Capture(_))
                        && matches!(
                            closure_kind,
                            Some(zil_ir::body::ClosureKind::Fn | zil_ir::body::ClosureKind::FnMut)
                        )
                        && kind == LoanKind::Exclusive);
                loans.push(Some(Loan {
                    origin: OpLocation {
                        block: zil_ir::body::BlockId::from_index(b),
                        instruction: Some(i as u32),
                    },
                    place: place.clone(),
                    kind,
                    anchored,
                    stores_addresses,
                }));
            }
            if let Terminator::Call {
                callee:
                    zil_ir::body::Callee::Intrinsic(zil_ir::body::IntrinsicOp::StoreBorrow {
                        mutability,
                    }),
                args,
                ..
            } = &block.terminator
                && let Some(
                    zil_ir::body::Operand::Copy(arg_place) | zil_ir::body::Operand::Move(arg_place),
                ) = args.first()
            {
                let store_place = resolve_store_borrow_place(body, arg_place);
                let kind = match mutability {
                    Mutability::Shared => LoanKind::Shared,
                    Mutability::Exclusive => LoanKind::Exclusive,
                };
                let place_ty = type_place(&store_place, &cx).ok().map(|t| t.ty());
                let is_desc = place_ty
                    .is_some_and(|t| matches!(tys.get(t), Some(zil_ir::body::TyKind::Desc(_))));
                let stores_addresses = place_ty.map(|t| tys.holds_address(t)).unwrap_or(true);
                let closure_kind = body.closure.and_then(|c| tys.closure(c)).map(|c| c.kind);
                let anchored = !is_desc
                    && (store_place.first_deref().is_none()
                        || (matches!(store_place.root, PlaceRoot::Capture(_))
                            && matches!(
                                closure_kind,
                                Some(
                                    zil_ir::body::ClosureKind::Fn
                                        | zil_ir::body::ClosureKind::FnMut
                                )
                            )
                            && kind == LoanKind::Exclusive));
                loans.push(Some(Loan {
                    origin: OpLocation {
                        block: zil_ir::body::BlockId::from_index(b),
                        instruction: None,
                    },
                    place: store_place,
                    kind,
                    anchored,
                    stores_addresses,
                }));
            }
        }
        LoanTable {
            region_count,
            loans,
        }
    }

    /// Number of loan ids including `EXTERNAL` and region parameter pseudo-loans.
    pub fn len(&self) -> usize {
        self.loans.len()
    }
    pub fn is_empty(&self) -> bool {
        self.loans.len() <= 1 + self.region_count as usize
    }
    pub fn region_loan(&self, region: u32) -> Option<usize> {
        (region < self.region_count).then_some(1 + region as usize)
    }
    pub fn loan_region(&self, id: usize) -> Option<u32> {
        if id >= 1 && id <= self.region_count as usize {
            Some((id - 1) as u32)
        } else {
            None
        }
    }
    pub fn get(&self, id: usize) -> Option<&Loan> {
        self.loans.get(id).and_then(Option::as_ref)
    }
    pub fn resolve(&self, origin: LoanOrigin) -> Option<usize> {
        match origin {
            LoanOrigin::External => Some(EXTERNAL),
            LoanOrigin::Region(r) => self.region_loan(r),
            LoanOrigin::At(at) => self
                .loans
                .iter()
                .position(|l| l.as_ref().is_some_and(|l| l.origin == at)),
        }
    }
    pub fn origin(&self, id: usize) -> LoanOrigin {
        if let Some(r) = self.loan_region(id) {
            return LoanOrigin::Region(r);
        }
        match self.get(id) {
            Some(l) => LoanOrigin::At(l.origin),
            None => LoanOrigin::External,
        }
    }
    /// The loan created by the instruction at `at`, if any.
    pub fn created_at(&self, at: OpLocation) -> Option<usize> {
        self.resolve(LoanOrigin::At(at))
    }
}

/// Live locals before every instruction and before every terminator.
#[derive(Clone, Debug)]
pub struct Liveness {
    /// `before[b][i]` for `i < len`; `before[b][len]` precedes the terminator.
    before: Vec<Vec<BitSet>>,
}

fn root_local(place: &Place) -> Option<Local> {
    match place.root {
        PlaceRoot::Local(l) => Some(l),
        PlaceRoot::Capture(_) => None,
    }
}

/// Applies one access backwards: whole-local initialization kills, every
/// other access of a local-rooted place is a use.
fn step_back(live: &mut BitSet, place: &Place, access: PlaceAccess) {
    let Some(l) = root_local(place) else { return };
    match access {
        PlaceAccess::Init if place.projections.is_empty() => live.remove(l.index()),
        PlaceAccess::Init
        | PlaceAccess::Copy
        | PlaceAccess::Move
        | PlaceAccess::Borrow(_)
        | PlaceAccess::Describe
        | PlaceAccess::Drop
        | PlaceAccess::ReplaceInstance
        | PlaceAccess::InspectTag
        | PlaceAccess::InspectLen
        | PlaceAccess::CallClosure(_) => live.insert(l.index()),
    }
}

fn instruction_back(live: &mut BitSet, kind: &InstructionKind) {
    match kind {
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
            live.remove(l.index());
            return;
        }
        InstructionKind::Assign { .. }
        | InstructionKind::Drop { .. }
        | InstructionKind::Replace { .. }
        | InstructionKind::ScopeEnter(_)
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::RegisterDefer { .. } => {}
    }
    for a in kind.accesses().iter().rev() {
        step_back(live, &a.place, a.access);
    }
}

fn terminator_back(live: &mut BitSet, term: &Terminator) {
    for a in term.accesses().iter().rev() {
        step_back(live, &a.place, a.access);
    }
}

impl Liveness {
    pub fn compute(body: &Body) -> Self {
        let n = body.blocks.len();
        let locals = body.locals.len();
        let mut entry: Vec<BitSet> = vec![BitSet::new(locals); n];
        let mut changed = true;
        while changed {
            changed = false;
            for b in (0..n).rev() {
                let block = &body.blocks[b];
                let mut live = BitSet::new(locals);
                for (kind, target) in block.terminator.edges() {
                    let mut edge = entry[target.index()].clone();
                    for a in block.terminator.edge_accesses(kind).iter().rev() {
                        step_back(&mut edge, &a.place, a.access);
                    }
                    live.union_with(&edge);
                }
                terminator_back(&mut live, &block.terminator);
                for inst in block.instructions.iter().rev() {
                    instruction_back(&mut live, &inst.kind);
                }
                if live != entry[b] {
                    entry[b] = live;
                    changed = true;
                }
            }
        }
        let mut before = Vec::with_capacity(n);
        for block in &body.blocks {
            let mut live = BitSet::new(locals);
            for (kind, target) in block.terminator.edges() {
                let mut edge = entry[target.index()].clone();
                for a in block.terminator.edge_accesses(kind).iter().rev() {
                    step_back(&mut edge, &a.place, a.access);
                }
                live.union_with(&edge);
            }
            terminator_back(&mut live, &block.terminator);
            let mut rows = vec![live.clone(); block.instructions.len() + 1];
            for (i, inst) in block.instructions.iter().enumerate().rev() {
                instruction_back(&mut live, &inst.kind);
                rows[i] = live.clone();
            }
            before.push(rows);
        }
        Liveness { before }
    }

    /// Locals live before the operation at `at` (`None` = terminator).
    pub fn before(&self, at: OpLocation) -> &BitSet {
        let rows = &self.before[at.block.index()];
        let i = at.instruction.map_or(rows.len() - 1, |i| i as usize);
        &rows[i]
    }

    /// Locals live after instruction `at` (before the next operation).
    pub fn after(&self, at: OpLocation) -> &BitSet {
        let rows = &self.before[at.block.index()];
        let i = at.instruction.map_or(rows.len() - 1, |i| i as usize + 1);
        &rows[i.min(rows.len() - 1)]
    }

    /// Locals live on entry to `block`.
    pub fn entry(&self, block: zil_ir::body::BlockId) -> &BitSet {
        &self.before[block.index()][0]
    }
}
