//! Paid metadata traversal only. Independent kernels validate relations.
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ct::CtExecutionBudget;
use zil_ir::{FunctionLifeContract, StorageLifeContract};

pub(super) fn signature_cost(
    life: &StorageLifeContract,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(
        (life.bindings.len() as u64)
            .saturating_add(life.classes.len() as u64)
            .saturating_add(2),
    )?;
    let mut bytes = (life.bindings.len() as u64)
        .saturating_mul(std::mem::size_of::<zil_ir::StorageLifeBinding>() as u64)
        .saturating_add(
            (life.classes.len() as u64)
                .saturating_mul(std::mem::size_of::<zil_ir::SignatureLifeClassBinding>() as u64),
        );
    for row in &life.bindings {
        budget.reserve_analysis_work(row.position.path.len() as u64)?;
        bytes = bytes.saturating_add(
            (row.position.path.len() as u64)
                .saturating_mul(std::mem::size_of::<zil_ir::ContractProjection>() as u64),
        );
    }
    for row in &life.classes {
        budget.reserve_analysis_work(row.position.path.len() as u64)?;
        bytes = bytes.saturating_add(
            (row.position.path.len() as u64)
                .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>() as u64),
        );
    }
    Ok(bytes)
}
pub(super) fn closed_signature(
    life: &StorageLifeContract,
    parameters: usize,
    budget: &mut CtExecutionBudget,
) -> CompileResult<FunctionLifeContract> {
    signature_cost(life, budget)?;
    budget.reserve_analysis_work(parameters as u64)?;
    budget.reserve_temporary_bytes(
        (parameters as u64).saturating_mul(std::mem::size_of::<Option<u32>>() as u64),
    )?;
    life.to_flat(parameters).ok_or_else(|| {
        vec![Diagnostic::unsupported(
            Phase::Ct,
            "rich CT5 source signature reconstruction is not yet implemented",
            None,
        )]
    })
}
