//! Bounded research-only two-phase native pipeline. No cache or JSON authority.
use super::*;
use crate::machine_cgu::complete_visibility::CheckedProof;

#[cfg(feature = "llvm-api")]
pub(super) fn compile_units(
    inputs: &[UnitInput],
    target: &str,
    options: &BuildOptions,
    proof: &CheckedProof,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<Vec<UnitResult>> {
    use zil_llvm_api::research_prelink;
    if inputs.len() != 8 || proof.units.len() != inputs.len() || options.object_cache.is_some() {
        return Err(infra(
            "E-RESEARCH-PRELINK-PROOF",
            "current canonical partition missing",
        ));
    }
    let config = zil_llvm_api::Config {
        triple: target,
        optimization: zil_llvm_api::Optimization::O2,
    };
    let merging = zil_llvm_api::FunctionMerging::AfterOptimization;
    let mut schedule = (0..inputs.len()).collect::<Vec<_>>();
    // Cost is computed once per unit, never inside the comparison function.
    schedule.sort_by_cached_key(|&id| {
        (
            std::cmp::Reverse(
                inputs[id]
                    .module
                    .items
                    .iter()
                    .map(|item| match item {
                        zil_llvm_api::model::Item::Function(f) => f.cost(),
                        _ => 1,
                    })
                    .sum::<usize>(),
            ),
            id,
        )
    });
    let units = parallel(&schedule, options.codegen.jobs, |id| {
        check_cancelled(cancelled)?;
        let unit =
            research_prelink::prelink_module(id, &inputs[id].module, &config, merging, cancelled)
                .map_err(api_error)?;
        let actual = unit
            .facts()
            .canonical
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if unit.facts().unit != id
            || actual != proof.units[id].canonical
            || actual.len() != unit.facts().canonical.len()
        {
            return Err(infra(
                "E-RESEARCH-PRELINK-PROOF",
                "actual prelink owners differ from current planner",
            ));
        }
        Ok(unit)
    })?;
    // The first phase has fully joined; only immutable owned bytes cross this
    // barrier. No LLVM context, module, pointer or held producer lease escapes.
    parallel(&schedule, options.codegen.jobs, |id| {
        check_cancelled(cancelled)?;
        let allowed = proof.units[id].allowed.iter().cloned().collect::<Vec<_>>();
        let output = research_prelink::compile_prelinked_unit(
            id, &units, &allowed, &config, merging, cancelled,
        )
        .map_err(api_error)?;
        check_cancelled(cancelled)?;
        fs::write(&inputs[id].object, &output.object.bytes)
            .map_err(|e| io_error("write prelinked LLVM API object", &inputs[id].object, e))?;
        let r = &output.object.receipt;
        let q = zil_base::json_string;
        let receipt = format!(
            "{{\"backend\":\"llvm-api\",\"llvm_version\":{},\"triple\":{},\"cpu\":{},\"features\":{},\"data_layout\":{},\"pipeline\":{},\"relocation\":{},\"code_model\":{},\"verify_before_after\":{},\"object_bytes\":{},\"input_kind\":{},\"preflight_ns\":{},\"builder_ns\":{},\"parse_ns\":{},\"optimize_ns\":{},\"emit_ns\":{},\"total_ns\":{},\"prelink\":{},\"research\":{}}}",
            q(&r.llvm_version),
            q(&r.triple),
            q(&r.cpu),
            q(&r.features),
            q(&r.data_layout),
            q(&r.pipeline),
            q(&r.relocation),
            q(&r.code_model),
            r.verify_before_after,
            output.object.bytes.len(),
            q(&r.input_kind),
            r.preflight_ns,
            r.builder_ns,
            r.parse_ns,
            r.optimize_ns,
            r.emit_ns,
            r.total_ns,
            units[id].receipt_json(),
            output.receipt.to_json(),
        );
        Ok(UnitResult {
            commands: vec![],
            receipt: Some(receipt),
            access: None,
            pending: None,
        })
    })
}

#[cfg(feature = "llvm-api")]
fn parallel<T: Send>(
    schedule: &[usize],
    jobs: usize,
    action: impl Fn(usize) -> CompileResult<T> + Sync,
) -> CompileResult<Vec<T>> {
    let next = AtomicUsize::new(0);
    let mut results = std::thread::scope(|scope| {
        let workers = (0..jobs.min(schedule.len()))
            .map(|_| {
                let next = &next;
                let action = &action;
                scope.spawn(move || {
                    let mut results = Vec::new();
                    while let Some(&id) = schedule.get(next.fetch_add(1, Ordering::Relaxed)) {
                        results.push((id, action(id)));
                    }
                    results
                })
            })
            .collect::<Vec<_>>();
        let mut results = Vec::with_capacity(schedule.len());
        let mut panicked = false;
        for worker in workers {
            match worker.join() {
                Ok(rows) => results.extend(rows),
                Err(_) => panicked = true,
            }
        }
        if panicked {
            Err(infra("E-CODEGEN-WORKER", "prelink worker panicked"))
        } else {
            Ok(results)
        }
    })?;
    results.sort_by_key(|(id, _)| *id);
    results.into_iter().map(|(_, result)| result).collect()
}

#[cfg(not(feature = "llvm-api"))]
pub(super) fn compile_units(
    _inputs: &[UnitInput],
    _target: &str,
    _options: &BuildOptions,
    _proof: &CheckedProof,
    _cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<Vec<UnitResult>> {
    Err(infra(
        "E-LLVM-API-UNAVAILABLE",
        "manual prelink requires LLVM API support",
    ))
}
