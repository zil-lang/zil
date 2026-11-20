//! Certificate checking for one function.
//!
//! Obligations (`spec/semantic-kernel.md` §6) and where each is discharged:
//! 1. entry coverage — `entry_state ⊑ I(entry)` when the entry is certified;
//! 2. every transfer — kernel `instruction`/`terminator` from each start;
//! 3. every edge — `F_e(I_src) ⊑ I_dst` for certified targets; uncertified
//!    targets must have exactly one predecessor and inherit the state;
//! 4. exits — kernel exit obligations at every `Return`;
//! 5. unreachable — only infeasible edges (kernel semantics) skip a block;
//!    a reached `Unreachable` is a kernel violation;
//! 6. calls — callee signatures via OpSpec; summaries arrive with P4;
//! 7. regions — loan carriage is checked like every other fact (claims are
//!    resolved against the checker's own loan table); region contracts of
//!    calls arrive with P4.

use zil_ir::body::{
    BlockId, Body, FnId, FunctionCertificate, OpLocation, PlaceTypes, Sigs, TyTable, type_place,
};
use zil_semantics::{Kernel, State, Violation};

#[derive(Clone, Copy, Debug)]
pub struct CheckBudget {
    pub steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckErrorKind {
    CertificateShape(&'static str),
    /// A claim names a place the body does not track.
    UnknownPlace,
    /// A loan claim names a location that creates no loan.
    UnknownLoan,
    DuplicateInvariant(BlockId),
    UnknownBlock(BlockId),
    /// A join point or re-entered block carries no invariant.
    MissingInvariant(BlockId),
    /// Initial state not covered by the entry invariant.
    EntryNotCovered,
    /// The state along an edge is not covered by the target invariant.
    EdgeNotCovered {
        from: BlockId,
        to: BlockId,
    },
    Violation(Violation),
    /// A malformed `impl Drop` binding.
    DropBinding(&'static str),
    ResourceLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckError {
    pub function: FnId,
    pub kind: CheckErrorKind,
}

pub fn check_function(
    body: &Body,
    tys: &TyTable,
    sigs: Sigs<'_>,
    id: FnId,
    cert: &FunctionCertificate,
    budget: &mut CheckBudget,
) -> Result<(), CheckError> {
    let err = |kind| CheckError { function: id, kind };
    let kernel = Kernel::new(body, tys, sigs);
    kernel
        .check_structure()
        .map_err(|v| err(CheckErrorKind::Violation(v)))?;
    let n = body.blocks.len();
    let mut charge = |units: u64| -> Result<(), CheckError> {
        budget.steps = budget
            .steps
            .checked_sub(units)
            .ok_or(err(CheckErrorKind::ResourceLimit))?;
        Ok(())
    };
    charge(cert.invariants.len() as u64)?;

    // Decode invariants into kernel states; unknown places are rejected.
    let mut invariant: Vec<Option<State>> = vec![None; n];
    for inv in &cert.invariants {
        if inv.block.index() >= n {
            return Err(err(CheckErrorKind::UnknownBlock(inv.block)));
        }
        if invariant[inv.block.index()].is_some() {
            return Err(err(CheckErrorKind::DuplicateInvariant(inv.block)));
        }
        charge(
            (inv.init.len()
                + inv.variants.len()
                + inv.tags.len()
                + inv.flags.len()
                + inv.loans.len()) as u64,
        )?;
        let mut state = State::claims_base(&kernel.paths, body.locals.len(), kernel.loans.len());
        for claim in &inv.loans {
            if claim.holder.index() >= body.locals.len() {
                return Err(err(CheckErrorKind::UnknownPlace));
            }
            let id = kernel
                .loans
                .resolve(claim.origin)
                .ok_or(err(CheckErrorKind::UnknownLoan))?;
            if state.carries.of(claim.holder).contains(id) {
                return Err(err(CheckErrorKind::CertificateShape(
                    "duplicate loan claim",
                )));
            }
            let mut one = zil_semantics::BitSet::new(kernel.loans.len());
            one.insert(id);
            state.carries.add(claim.holder, &one);
        }
        for claim in &inv.init {
            let path = kernel
                .paths
                .lookup(&claim.place)
                .ok_or(err(CheckErrorKind::UnknownPlace))?;
            state.init.set(path, claim.state);
        }
        let cx = body.cx(tys);
        for v in &inv.variants {
            type_place(&v.place, &cx).map_err(|_| err(CheckErrorKind::UnknownPlace))?;
            state.variants.insert(v.place.clone(), v.variant);
        }
        for t in &inv.tags {
            cx.local_ty(t.local)
                .ok_or(err(CheckErrorKind::UnknownPlace))?;
            type_place(&t.place, &cx).map_err(|_| err(CheckErrorKind::UnknownPlace))?;
            state.tags.insert(t.local, t.place.clone());
        }
        for f in &inv.flags {
            // A flag claim must restate a declaration; the declaration is
            // validated by the structure check, the claim by the edges.
            if !body
                .drop_flags
                .iter()
                .any(|d| d.flag == f.flag && d.place == f.place)
            {
                return Err(err(CheckErrorKind::UnknownPlace));
            }
            state.flags.insert(f.flag, f.place.clone());
        }
        invariant[inv.block.index()] = Some(state);
    }

    // Which blocks may inherit a state: exactly one predecessor, not the
    // entry re-entered by an edge.
    let preds = body.predecessors();
    for (b, p) in preds.iter().enumerate() {
        let needs = p.len() > 1 || (b == 0 && !p.is_empty());
        if needs && invariant[b].is_none() {
            return Err(err(CheckErrorKind::MissingInvariant(BlockId::from_index(
                b,
            ))));
        }
    }

    // Starts: the entry (from the initial state) and every certified block.
    let initial = kernel.entry_state();
    let mut work: Vec<(BlockId, State)> = Vec::new();
    match &invariant[0] {
        Some(inv) => {
            if !initial.leq(inv) {
                return Err(err(CheckErrorKind::EntryNotCovered));
            }
        }
        None => work.push((BlockId(0), initial)),
    }
    for (b, inv) in invariant.iter().enumerate() {
        if let Some(s) = inv {
            work.push((BlockId::from_index(b), s.clone()));
        }
    }
    let mut visited = vec![false; n];
    while let Some((b, mut state)) = work.pop() {
        // Each uncertified block has one predecessor, so it is reached at
        // most once; a second visit means a malformed CFG reading.
        if visited[b.index()] {
            return Err(err(CheckErrorKind::CertificateShape("block reached twice")));
        }
        visited[b.index()] = true;
        let block = &body.blocks[b.index()];
        charge(block.instructions.len() as u64 + 1)?;
        for (i, inst) in block.instructions.iter().enumerate() {
            kernel
                .instruction(
                    &mut state,
                    &inst.kind,
                    OpLocation {
                        block: b,
                        instruction: Some(i as u32),
                    },
                )
                .map_err(|v| err(CheckErrorKind::Violation(v)))?;
        }
        let successors = kernel
            .terminator(
                state,
                &block.terminator,
                OpLocation {
                    block: b,
                    instruction: None,
                },
            )
            .map_err(|v| err(CheckErrorKind::Violation(v)))?;
        for (_, target, s) in successors {
            let Some(s) = s else { continue };
            match &invariant[target.index()] {
                Some(inv) => {
                    charge(1)?;
                    if !s.leq(inv) {
                        return Err(err(CheckErrorKind::EdgeNotCovered {
                            from: b,
                            to: target,
                        }));
                    }
                }
                None => work.push((target, s)),
            }
        }
    }
    Ok(())
}
