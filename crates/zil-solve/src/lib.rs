//! Solver: iterates the semantic kernel to a fixpoint, produces the
//! certificate and complete diagnostics. Grants no authority: its output is
//! a candidate for `zil-check`.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

use zil_ir::body::{
    BlockId, BlockInvariant, Body, Certificate, FlagClaim, FnId, FunctionCertificate, InitClaim,
    LoanClaim, Local, Module, OpLocation, PlaceInit, Sigs, TagClaim, VariantClaim,
};
use zil_semantics::{BitSet, Kernel, State, Violation};

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

/// Opaque 32-byte key identifying a function's exact solver input closure.
pub type CertCacheKey = [u8; 32];

/// Untrusted candidate certificate cache consulted by [`solve_module_with_cache`].
/// Cached certificates grant ZERO safety authority and must always be verified
/// fresh by `zil-check`.
pub trait CertificateCache: Sync {
    fn get(&self, key: CertCacheKey) -> Option<FunctionCertificate>;
    fn put(&self, key: CertCacheKey, cert: &FunctionCertificate);
}

/// Seeds `cache` with candidate [`FunctionCertificate`] entries from an existing
/// verified `(module, cert)` snapshot.
pub fn seed_certificate_cache(module: &Module, cert: &Certificate, cache: &dyn CertificateCache) {
    let ty_fp = zil_ir::body::ty_table_fingerprint(&module.tys).0;
    let return_summaries = zil_semantics::compute_return_summaries(module);
    for (fi, fc) in cert.functions.iter().enumerate() {
        if fi < module.functions.len() {
            let key = zil_ir::body::function_solve_cache_key(module, fi, &ty_fp, &return_summaries);
            cache.put(key, fc);
        }
    }
}

/// Solves every function; the first failure of each function is reported.
pub fn solve_module(module: &Module, budget: Budget) -> Result<Certificate, Vec<SolveError>> {
    solve_module_with_cache(module, budget, None).map(|(cert, _)| cert)
}

/// Solves every function, optionally reusing candidate [`FunctionCertificate`]s
/// from `cache` for unchanged functions. Returns `(certificate, cache_hits)`.
pub fn solve_module_with_cache(
    module: &Module,
    budget: Budget,
    cache: Option<&dyn CertificateCache>,
) -> Result<(Certificate, usize), Vec<SolveError>> {
    solve_module_with_prebuilt_and_cache(module, budget, &[], cache)
}

/// Solves every function, optionally reusing pre-remapped candidate
/// [`FunctionCertificate`]s (`prebuilt[fn_idx]`) or `cache` entries for
/// unchanged functions. All candidate certificates remain untrusted and MUST
/// still be verified fresh by `zil-check`. Returns `(certificate, cache_hits)`.
pub fn solve_module_with_prebuilt_and_cache(
    module: &Module,
    budget: Budget,
    prebuilt: &[Option<FunctionCertificate>],
    cache: Option<&dyn CertificateCache>,
) -> Result<(Certificate, usize), Vec<SolveError>> {
    if let Err((function, reason)) = zil_semantics::check_dyn_impl_region_contract(module) {
        return Err(vec![SolveError::Violation {
            function,
            violation: Violation {
                at: OpLocation {
                    block: BlockId(0),
                    instruction: None,
                },
                kind: zil_semantics::ViolationKind::Constraint(
                    zil_ir::body::ConstraintError::Operator { reason },
                ),
            },
        }]);
    }
    let fn_sigs = module.fn_sigs();
    let return_summaries = zil_semantics::compute_return_summaries(module);
    let sigs = Sigs {
        functions: &fn_sigs,
        foreigns: &module.foreigns,
        static_count: module.statics.len(),
        return_summaries: Some(&return_summaries),
    };
    let ty_fp = cache.map(|_| zil_ir::body::ty_table_fingerprint(&module.tys).0);
    let n = module.functions.len();
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 16)
        .min(n.max(1));

    if workers <= 1 || n < 16 {
        let mut remaining = budget.steps;
        let mut certificate = Certificate::default();
        let mut errors = Vec::new();
        let mut cache_hits = 0usize;
        for (i, function) in module.functions.iter().enumerate() {
            let id = FnId::from_index(i);
            if let Some(Some(pre)) = prebuilt.get(i) {
                let mut hit = pre.clone();
                hit.function = id;
                certificate.functions.push(hit);
                cache_hits += 1;
                continue;
            }
            let cache_key = cache.zip(ty_fp.as_ref()).map(|(_, fp)| {
                zil_ir::body::function_solve_cache_key(module, i, fp, &return_summaries)
            });
            if let (Some(c), Some(key)) = (cache, cache_key)
                && let Some(mut hit) = c.get(key)
            {
                hit.function = id;
                certificate.functions.push(hit);
                cache_hits += 1;
                continue;
            }
            let kernel = Kernel::new(&function.body, &module.tys, sigs);
            match solve_function(&kernel, id, &mut remaining) {
                Ok(cert) => {
                    if let (Some(c), Some(key)) = (cache, cache_key) {
                        c.put(key, &cert);
                    }
                    certificate.functions.push(cert);
                }
                Err(e) => errors.push(e),
            }
        }
        return if errors.is_empty() {
            Ok((certificate, cache_hits))
        } else {
            Err(errors)
        };
    }

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct FnSlot {
        result: Result<FunctionCertificate, SolveError>,
        used: u64,
        budget_at_start: u64,
        from_cache: bool,
    }

    let shared_budget = AtomicU64::new(budget.steps);
    let next_idx = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<FnSlot>>> = (0..n).map(|_| Mutex::new(None)).collect();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next_idx.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let id = FnId::from_index(i);
                    if let Some(Some(pre)) = prebuilt.get(i) {
                        let mut hit = pre.clone();
                        hit.function = id;
                        if let Ok(mut guard) = slots[i].lock() {
                            *guard = Some(FnSlot {
                                result: Ok(hit),
                                used: 0,
                                budget_at_start: budget.steps,
                                from_cache: true,
                            });
                        }
                        continue;
                    }
                    let cache_key = cache.zip(ty_fp.as_ref()).map(|(_, fp)| {
                        zil_ir::body::function_solve_cache_key(module, i, fp, &return_summaries)
                    });
                    if let (Some(c), Some(key)) = (cache, cache_key)
                        && let Some(mut hit) = c.get(key)
                    {
                        hit.function = id;
                        if let Ok(mut guard) = slots[i].lock() {
                            *guard = Some(FnSlot {
                                result: Ok(hit),
                                used: 0,
                                budget_at_start: budget.steps,
                                from_cache: true,
                            });
                        }
                        continue;
                    }
                    let avail = shared_budget.load(Ordering::Relaxed);
                    if avail == 0 {
                        break;
                    }
                    let kernel = Kernel::new(&module.functions[i].body, &module.tys, sigs);
                    let mut local_rem = avail;
                    let result = solve_function(&kernel, id, &mut local_rem);
                    let used = avail.saturating_sub(local_rem);
                    if let (Ok(cert), Some(c), Some(key)) = (&result, cache, cache_key) {
                        c.put(key, cert);
                    }
                    let mut cur = shared_budget.load(Ordering::Relaxed);
                    while let Err(next) = shared_budget.compare_exchange_weak(
                        cur,
                        cur.saturating_sub(used),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        cur = next;
                    }
                    if let Ok(mut guard) = slots[i].lock() {
                        *guard = Some(FnSlot {
                            result,
                            used,
                            budget_at_start: avail,
                            from_cache: false,
                        });
                    }
                }
            });
        }
    });

    // Deterministic replay in exact `FnId` order so certificates, diagnostics,
    // and budget exhaustion match sequential execution bit-for-bit.
    let mut remaining = budget.steps;
    let mut certificate = Certificate {
        functions: Vec::with_capacity(n),
    };
    let mut errors = Vec::new();
    let mut cache_hits = 0usize;
    for (i, slot_mutex) in slots.into_iter().enumerate() {
        let id = FnId::from_index(i);
        let slot = slot_mutex.into_inner().ok().flatten();
        match slot {
            Some(FnSlot {
                result: Ok(cert),
                used,
                from_cache,
                ..
            }) if used <= remaining => {
                remaining -= used;
                if from_cache {
                    cache_hits += 1;
                }
                certificate.functions.push(cert);
            }
            Some(FnSlot {
                result: Err(e @ SolveError::Violation { .. }),
                used,
                ..
            }) if used <= remaining => {
                remaining -= used;
                errors.push(e);
            }
            Some(FnSlot {
                result: Err(SolveError::ResourceLimit { function }),
                budget_at_start,
                ..
            }) if budget_at_start >= remaining => {
                remaining = 0;
                errors.push(SolveError::ResourceLimit { function });
            }
            _ => {
                let kernel = Kernel::new(&module.functions[i].body, &module.tys, sigs);
                match solve_function(&kernel, id, &mut remaining) {
                    Ok(c) => certificate.functions.push(c),
                    Err(e) => errors.push(e),
                }
            }
        }
    }
    if errors.is_empty() {
        Ok((certificate, cache_hits))
    } else {
        Err(errors)
    }
}

/// Blocks whose entry state the certificate must state: join points and
/// any block (including the entry) reached by a back or side edge.
pub fn certified_blocks(body: &Body) -> Vec<bool> {
    let preds = body.predecessors();
    preds
        .iter()
        .enumerate()
        .map(|(i, p)| p.len() > 1 || (i == 0 && !p.is_empty()))
        .collect()
}

/// Reverse Post-Order numbering of blocks reachable from `BlockId(0)`.
/// Returns `(rpo_blocks, block_to_rpo)` where `rpo_blocks[rank]` is the block
/// at RPO rank `rank`.
fn compute_rpo(body: &Body) -> (Vec<BlockId>, Vec<usize>) {
    let n = body.blocks.len();
    let mut visited = vec![false; n];
    let mut postorder = Vec::with_capacity(n);
    if n > 0 {
        // Iterative DFS to avoid stack overflow on deep CFGs.
        let mut stack: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        visited[0] = true;
        let succs0: Vec<usize> = body.blocks[0]
            .terminator
            .edges()
            .into_iter()
            .map(|(_, t)| t.index())
            .collect();
        stack.push((0, 0, succs0));
        while let Some((b, idx, succs)) = stack.last_mut() {
            if *idx < succs.len() {
                let next = succs[*idx];
                *idx += 1;
                if next < n && !visited[next] {
                    visited[next] = true;
                    let next_succs: Vec<usize> = body.blocks[next]
                        .terminator
                        .edges()
                        .into_iter()
                        .map(|(_, t)| t.index())
                        .collect();
                    stack.push((next, 0, next_succs));
                }
            } else {
                let finished = *b;
                stack.pop();
                postorder.push(BlockId::from_index(finished));
            }
        }
    }
    postorder.reverse();
    let mut block_to_rpo = vec![usize::MAX; n];
    for (rank, &blk) in postorder.iter().enumerate() {
        block_to_rpo[blk.index()] = rank;
    }
    (postorder, block_to_rpo)
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
    let (rpo_blocks, block_to_rpo) = compute_rpo(body);
    let mut entry: Vec<Option<State>> = vec![None; body.blocks.len()];
    entry[0] = Some(kernel.entry_state());
    let mut queued_rpo = BitSet::new(rpo_blocks.len());
    if !rpo_blocks.is_empty() {
        queued_rpo.insert(0);
    }
    while let Some(rank) = queued_rpo.first_set() {
        queued_rpo.remove(rank);
        let b = rpo_blocks[rank];
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
                Some(old) => old.join_with_changed(&s),
            };
            if changed {
                let target_rank = block_to_rpo[target.index()];
                if target_rank < rpo_blocks.len() {
                    queued_rpo.insert(target_rank);
                }
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
        flags: state
            .flags
            .iter()
            .map(|(l, p)| FlagClaim {
                flag: *l,
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
