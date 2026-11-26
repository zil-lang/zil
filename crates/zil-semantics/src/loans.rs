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
    /// Whether this loan was created by `IntrinsicOp::StoreBorrow` (`store_at`
    /// / `store_at_mut`).
    pub is_store_borrow: bool,
    /// Whether `place` is proven to denote a unique owning storage location in
    /// the current frame (not a `Desc` indirection and no `Deref` projection).
    /// Distinct-root `StoreBorrow` loans are proven apart only when both loans
    /// have `proven_unique_owner == true`.
    pub proven_unique_owner: bool,
    /// When `place` traverses one or more `Deref` projections, the explicit
    /// `Region::Param(r)` of the reference dereferenced by the last `Deref`,
    /// if any. `None` for anchored local loans or derefs through an unannotated
    /// (`Region::Static`) reference.
    pub deref_region: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct LoanTable {
    /// Index 0 is `EXTERNAL`; indices `1..=region_count` are `Region(0..region_count)`.
    region_count: u32,
    loans: Vec<Option<Loan>>,
    by_origin: std::collections::HashMap<OpLocation, usize>,
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
                let place_ty = type_place(place, &cx).ok().map(|t| t.ty());
                let is_desc = place_ty
                    .is_some_and(|t| matches!(tys.get(t), Some(zil_ir::body::TyKind::Desc(_))));
                let stores_addresses = place_ty.map(|t| tys.holds_address(t)).unwrap_or(true);
                let closure_kind = body.closure.and_then(|c| tys.closure(c)).map(|c| c.kind);
                let anchored = place.first_deref().is_none()
                    || (matches!(place.root, PlaceRoot::Capture(_))
                        && matches!(
                            closure_kind,
                            Some(zil_ir::body::ClosureKind::Fn | zil_ir::body::ClosureKind::FnMut)
                        )
                        && kind == LoanKind::Exclusive);
                let proven_unique_owner = !is_desc
                    && place.first_deref().is_none()
                    && matches!(place.root, PlaceRoot::Local(_));
                let deref_region = place
                    .projections
                    .iter()
                    .rposition(|p| matches!(p, zil_ir::body::Projection::Deref))
                    .and_then(|idx| {
                        let prefix = Place {
                            root: place.root,
                            projections: place.projections[..idx].to_vec(),
                        };
                        let t = type_place(&prefix, &cx).ok()?.ty();
                        match tys.get(t) {
                            Some(zil_ir::body::TyKind::Ref {
                                region: zil_ir::body::Region::Param(r),
                                ..
                            }) => Some(*r),
                            _ => None,
                        }
                    });
                loans.push(Some(Loan {
                    origin: OpLocation {
                        block: zil_ir::body::BlockId::from_index(b),
                        instruction: Some(i as u32),
                    },
                    place: place.clone(),
                    kind,
                    anchored,
                    stores_addresses,
                    is_store_borrow: false,
                    proven_unique_owner,
                    deref_region,
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
                let proven_unique_owner = !is_desc
                    && store_place.first_deref().is_none()
                    && matches!(store_place.root, PlaceRoot::Local(_));
                loans.push(Some(Loan {
                    origin: OpLocation {
                        block: zil_ir::body::BlockId::from_index(b),
                        instruction: None,
                    },
                    place: store_place,
                    kind,
                    anchored,
                    stores_addresses,
                    is_store_borrow: true,
                    proven_unique_owner,
                    deref_region: None,
                }));
            }
        }
        let mut by_origin = std::collections::HashMap::with_capacity(
            loans.len().saturating_sub(1 + region_count as usize),
        );
        for (idx, loan) in loans.iter().enumerate() {
            if let Some(l) = loan {
                by_origin.entry(l.origin).or_insert(idx);
            }
        }
        LoanTable {
            region_count,
            loans,
            by_origin,
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
            LoanOrigin::At(at) => self.by_origin.get(&at).copied(),
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

/// Summarizes one access backwards into `(uses, kill)` so `live_in = (live_out & !kill) | uses`.
fn step_back_gen_kill(uses: &mut BitSet, kill: &mut BitSet, place: &Place, access: PlaceAccess) {
    let Some(l) = root_local(place) else { return };
    let idx = l.index();
    match access {
        PlaceAccess::Init if place.projections.is_empty() => {
            uses.remove(idx);
            kill.insert(idx);
        }
        PlaceAccess::Init
        | PlaceAccess::Copy
        | PlaceAccess::Move
        | PlaceAccess::Borrow(_)
        | PlaceAccess::Describe
        | PlaceAccess::Drop
        | PlaceAccess::ReplaceInstance
        | PlaceAccess::InspectTag
        | PlaceAccess::InspectLen
        | PlaceAccess::CallClosure(_) => {
            uses.insert(idx);
            kill.remove(idx);
        }
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

fn instruction_back_gen_kill(uses: &mut BitSet, kill: &mut BitSet, kind: &InstructionKind) {
    match kind {
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
            let idx = l.index();
            uses.remove(idx);
            kill.insert(idx);
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
        step_back_gen_kill(uses, kill, &a.place, a.access);
    }
}

fn terminator_back(live: &mut BitSet, term: &Terminator) {
    for a in term.accesses().iter().rev() {
        step_back(live, &a.place, a.access);
    }
}

fn terminator_back_gen_kill(uses: &mut BitSet, kill: &mut BitSet, term: &Terminator) {
    for a in term.accesses().iter().rev() {
        step_back_gen_kill(uses, kill, &a.place, a.access);
    }
}

impl Liveness {
    pub fn compute(body: &Body) -> Self {
        let n = body.blocks.len();
        let locals = body.locals.len();

        struct EdgeLiveness {
            target: usize,
            gen_kill: Option<(BitSet, BitSet)>,
        }

        // Precompute per-block (uses, kill) and per-edge (target, Option<(uses, kill)>) once.
        let mut block_gen: Vec<BitSet> = Vec::with_capacity(n);
        let mut block_kill: Vec<BitSet> = Vec::with_capacity(n);
        let mut block_edges: Vec<Vec<EdgeLiveness>> = Vec::with_capacity(n);

        for block in &body.blocks {
            let mut uses = BitSet::new(locals);
            let mut kill = BitSet::new(locals);
            terminator_back_gen_kill(&mut uses, &mut kill, &block.terminator);
            for inst in block.instructions.iter().rev() {
                instruction_back_gen_kill(&mut uses, &mut kill, &inst.kind);
            }
            block_gen.push(uses);
            block_kill.push(kill);

            let mut edges = Vec::new();
            for (kind, target) in block.terminator.edges() {
                let ea = block.terminator.edge_accesses(kind);
                let gen_kill = if ea.is_empty() {
                    None
                } else {
                    let mut egen = BitSet::new(locals);
                    let mut ekill = BitSet::new(locals);
                    for a in ea.iter().rev() {
                        step_back_gen_kill(&mut egen, &mut ekill, &a.place, a.access);
                    }
                    Some((egen, ekill))
                };
                edges.push(EdgeLiveness {
                    target: target.index(),
                    gen_kill,
                });
            }
            block_edges.push(edges);
        }

        let mut entry: Vec<BitSet> = vec![BitSet::new(locals); n];
        let mut live_out = BitSet::new(locals);
        let mut edge_tmp = BitSet::new(locals);
        let mut changed = true;
        while changed {
            changed = false;
            for b in (0..n).rev() {
                live_out.clear_all();
                for edge in &block_edges[b] {
                    match &edge.gen_kill {
                        None => live_out.union_with(&entry[edge.target]),
                        Some((egen, ekill)) => {
                            edge_tmp.clear_all();
                            edge_tmp.assign_gen_kill_changed(&entry[edge.target], ekill, egen);
                            live_out.union_with(&edge_tmp);
                        }
                    }
                }
                if entry[b].assign_gen_kill_changed(&live_out, &block_kill[b], &block_gen[b]) {
                    changed = true;
                }
            }
        }

        let mut before = Vec::with_capacity(n);
        for (b, block) in body.blocks.iter().enumerate() {
            let mut live = BitSet::new(locals);
            for edge in &block_edges[b] {
                match &edge.gen_kill {
                    None => live.union_with(&entry[edge.target]),
                    Some((egen, ekill)) => {
                        edge_tmp.clear_all();
                        edge_tmp.assign_gen_kill_changed(&entry[edge.target], ekill, egen);
                        live.union_with(&edge_tmp);
                    }
                }
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
