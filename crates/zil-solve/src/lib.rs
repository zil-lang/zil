//! Solver: iterates the semantic kernel to a fixpoint, produces the
//! certificate and complete diagnostics. Grants no authority: its output is
//! a candidate for `zil-check`.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

use std::collections::VecDeque;
use zil_ir::body::{
    BlockId, BlockInvariant, Certificate, FnId, FunctionCertificate, InitClaim, LoanClaim, Local,
    Module, OpLocation, PlaceInit, TagClaim, VariantClaim,
};
use zil_semantics::{Kernel, State, Violation};

/// Work budget: transfer applications across one request.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolveError {
    Violation {
        function: FnId,
        violation: Violation,
    },
    ResourceLimit {
        function: FnId,
    },
}

/// Solves every function; the first failure of each function is reported.
pub fn solve_module(module: &Module, budget: Budget) -> Result<Certificate, Vec<SolveError>> {
    let sigs: Vec<_> = module.functions.iter().map(|f| f.sig.clone()).collect();
    let mut remaining = budget.steps;
    let mut certificate = Certificate::default();
    let mut errors = Vec::new();
    for (i, function) in module.functions.iter().enumerate() {
        let id = FnId::from_index(i);
        let kernel = Kernel::new(&function.body, &module.tys, &sigs);
        match solve_function(&kernel, id, &mut remaining) {
            Ok(c) => certificate.functions.push(c),
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(certificate)
    } else {
        Err(errors)
    }
}

/// Blocks whose entry state the certificate must state: join points and
/// any block (including the entry) reached by a back or side edge.
pub fn certified_blocks(body: &zil_ir::body::Body) -> Vec<bool> {
    let preds = body.predecessors();
    preds
        .iter()
        .enumerate()
        .map(|(i, p)| p.len() > 1 || (i == 0 && !p.is_empty()))
        .collect()
}

pub fn solve_function(
    kernel: &Kernel<'_>,
    id: FnId,
    remaining: &mut u64,
) -> Result<FunctionCertificate, SolveError> {
    let violation = |violation| SolveError::Violation {
        function: id,
        violation,
    };
    kernel.check_structure().map_err(violation)?;
    let body = kernel.body;
    let mut entry: Vec<Option<State>> = vec![None; body.blocks.len()];
    entry[0] = Some(kernel.entry_state());
    let mut queue: VecDeque<BlockId> = VecDeque::from([BlockId(0)]);
    let mut queued = vec![false; body.blocks.len()];
    queued[0] = true;
    while let Some(b) = queue.pop_front() {
        queued[b.index()] = false;
        let Some(mut state) = entry[b.index()].clone() else {
            continue;
        };
        let block = &body.blocks[b.index()];
        for (i, inst) in block.instructions.iter().enumerate() {
            *remaining = remaining
                .checked_sub(1)
                .ok_or(SolveError::ResourceLimit { function: id })?;
            kernel
                .instruction(
                    &mut state,
                    &inst.kind,
                    OpLocation {
                        block: b,
                        instruction: Some(i as u32),
                    },
                )
                .map_err(violation)?;
        }
        *remaining = remaining
            .checked_sub(1)
            .ok_or(SolveError::ResourceLimit { function: id })?;
        let successors = kernel
            .terminator(
                state,
                &block.terminator,
                OpLocation {
                    block: b,
                    instruction: None,
                },
            )
            .map_err(violation)?;
        for (_, target, s) in successors {
            let Some(s) = s else { continue };
            let slot = &mut entry[target.index()];
            let changed = match slot {
                None => {
                    *slot = Some(s);
                    true
                }
                Some(old) => {
                    let mut joined = old.clone();
                    joined.join_with(&s);
                    if joined == *old {
                        false
                    } else {
                        *old = joined;
                        true
                    }
                }
            };
            if changed && !queued[target.index()] {
                queued[target.index()] = true;
                queue.push_back(target);
            }
        }
    }
    let certified = certified_blocks(body);
    let invariants = entry
        .iter()
        .enumerate()
        .filter(|(i, s)| certified[*i] && s.is_some())
        .map(|(i, s)| {
            invariant(
                kernel,
                BlockId::from_index(i),
                s.as_ref().expect("filtered"),
            )
        })
        .collect();
    Ok(FunctionCertificate {
        function: id,
        invariants,
    })
}

/// Renders a kernel state as claims over IR places. `Maybe` is the default
/// for unmentioned places and is omitted.
pub fn invariant(kernel: &Kernel<'_>, block: BlockId, state: &State) -> BlockInvariant {
    let mut init = Vec::new();
    for i in 0..kernel.paths.len() {
        let path = zil_semantics::PathId(i as u32);
        let claim = state.init.claim(path);
        match claim {
            InitClaim::Init | InitClaim::Uninit => init.push(PlaceInit {
                place: kernel.paths.place(path).clone(),
                state: claim,
            }),
            InitClaim::Maybe => {}
        }
    }
    BlockInvariant {
        block,
        init,
        variants: state
            .variants
            .iter()
            .map(|(p, v)| VariantClaim {
                place: p.clone(),
                variant: *v,
            })
            .collect(),
        tags: state
            .tags
            .iter()
            .map(|(l, p)| TagClaim {
                local: *l,
                place: p.clone(),
            })
            .collect(),
        loans: (0..state.carries.locals())
            .flat_map(|l| {
                let holder = Local::from_index(l);
                state.carries.of(holder).iter().map(move |id| LoanClaim {
                    holder,
                    origin: kernel.loans.origin(id),
                })
            })
            .collect(),
    }
}
