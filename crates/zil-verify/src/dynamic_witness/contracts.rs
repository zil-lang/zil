//! Independent source-contract substitution. This creates a finite upper and
//! complete typed operands, never a proof that an executable body respects it.
use super::*;

pub(super) fn materialize(
    recipe: &DynContractRecipe,
    mut resolve: impl FnMut(&DynTypePattern, &mut Budget) -> Check<terms::Value>,
    b: &mut Budget,
) -> Check<(DynContract, Vec<requests::Operand>)> {
    dynamic_metadata::recipe(recipe, b)?;
    let mut operands = vec![];
    let mut requires = vec![];
    for (index, row) in recipe
        .requires
        .as_ref()
        .ok_or_else(|| bad("original method requirements are Unknown"))?
        .iter()
        .enumerate()
    {
        requires.push(match row {
            DynRequirementRecipe::Read(p) => DynRequirement::Read(p.clone()),
            DynRequirementRecipe::Write(p) => DynRequirement::Write(p.clone()),
            DynRequirementRecipe::Manage(p) => DynRequirement::Manage(p.clone()),
            DynRequirementRecipe::Apart(a, z) => DynRequirement::Apart(a.clone(), z.clone()),
            DynRequirementRecipe::State {
                position,
                ty,
                variant,
            } => {
                let value = resolve(ty, b)?;
                dynamic_metadata::recipe(&value.raw, b)?;
                let actual = value.raw.ty.clone();
                operands.push(requests::Operand {
                    position: requests::OperandPosition::RequirementState(index as u32),
                    value: value.raw,
                });
                DynRequirement::State {
                    position: position.clone(),
                    ty: actual,
                    variant: *variant,
                }
            }
        });
    }
    let mut guarantees = vec![];
    for (index, row) in recipe
        .guarantees
        .as_ref()
        .ok_or_else(|| bad("original method guarantees are Unknown"))?
        .iter()
        .enumerate()
    {
        guarantees.push(match row {
            DynGuaranteeRecipe::StorageSources(binding) => {
                DynGuarantee::StorageSources(binding.clone())
            }
            DynGuaranteeRecipe::ManageResult => DynGuarantee::ManageResult,
            DynGuaranteeRecipe::State {
                position,
                ty,
                variant,
            } => {
                let value = resolve(ty, b)?;
                dynamic_metadata::recipe(&value.raw, b)?;
                let actual = value.raw.ty.clone();
                operands.push(requests::Operand {
                    position: requests::OperandPosition::GuaranteeState(index as u32),
                    value: value.raw,
                });
                DynGuarantee::State {
                    position: position.clone(),
                    ty: actual,
                    variant: *variant,
                }
            }
        });
    }
    let mut effects = vec![];
    for (index, row) in recipe
        .effects
        .as_ref()
        .ok_or_else(|| bad("original method effects are Unknown"))?
        .iter()
        .enumerate()
    {
        effects.push(effect(
            row,
            requests::OperandPosition::EffectCleanup(index as u32),
            &mut resolve,
            &mut operands,
            b,
        )?);
    }
    let cleanup = recipe
        .cleanup
        .as_ref()
        .ok_or_else(|| bad("original method cleanup is Unknown"))?;
    let mut cleanup_effects = vec![];
    for (index, row) in cleanup.effects.iter().enumerate() {
        cleanup_effects.push(effect(
            row,
            requests::OperandPosition::CleanupEffectCleanup(index as u32),
            &mut resolve,
            &mut operands,
            b,
        )?);
    }
    Ok((
        DynContract {
            requires: Some(requires),
            guarantees: Some(guarantees),
            hold: Some(
                recipe
                    .hold
                    .as_ref()
                    .ok_or_else(|| bad("original method Hold is Unknown"))?
                    .clone(),
            ),
            call: Some(
                recipe
                    .call
                    .as_ref()
                    .ok_or_else(|| bad("original method access is Unknown"))?
                    .clone(),
            ),
            result: Some(
                recipe
                    .result
                    .as_ref()
                    .ok_or_else(|| bad("original method result sources are Unknown"))?
                    .clone(),
            ),
            cleanup: Some(DynCleanupContract {
                environment: cleanup.environment,
                positions: cleanup.positions.clone(),
                access: cleanup.access.clone(),
                effects: cleanup_effects,
            }),
            effects: Some(effects),
            resume: Some(
                recipe
                    .resume
                    .ok_or_else(|| bad("original method resume bound is Unknown"))?,
            ),
            threads: Some(
                recipe
                    .threads
                    .as_ref()
                    .ok_or_else(|| bad("original method thread requirements are Unknown"))?
                    .clone(),
            ),
        },
        operands,
    ))
}
fn effect(
    recipe: &DynEffectRecipe,
    position: requests::OperandPosition,
    resolve: &mut impl FnMut(&DynTypePattern, &mut Budget) -> Check<terms::Value>,
    operands: &mut Vec<requests::Operand>,
    b: &mut Budget,
) -> Check<DynEffect> {
    b.spend(1)?;
    Ok(match recipe {
        DynEffectRecipe::Alloc => DynEffect::Alloc,
        DynEffectRecipe::Io => DynEffect::Io,
        DynEffectRecipe::Block => DynEffect::Block,
        DynEffectRecipe::Suspend => DynEffect::Suspend,
        DynEffectRecipe::Spawn => DynEffect::Spawn,
        DynEffectRecipe::Ffi => DynEffect::Ffi,
        DynEffectRecipe::Reenter => DynEffect::Reenter,
        DynEffectRecipe::Panic => DynEffect::Panic,
        DynEffectRecipe::Call(p) => DynEffect::Call(p.clone()),
        DynEffectRecipe::Cleanup(ty) => {
            let value = resolve(ty, b)?;
            dynamic_metadata::recipe(&value.raw, b)?;
            let ty = value.raw.ty.clone();
            operands.push(requests::Operand {
                position,
                value: value.raw,
            });
            DynEffect::Cleanup(ty)
        }
    })
}

/// Prepay the entire already substituted upper before retaining it. This is
/// accounting only; absent components still carry no executable authority.
pub(super) fn charge(module: &TypedModule, contract: &DynContract, b: &mut Budget) -> Check<()> {
    fn position(p: &StoragePosition, b: &mut Budget) -> Check<()> {
        if p.path.len() > b.limits.max_nodes {
            return Err(limit("contract position exceeds selected bound"));
        }
        b.spend(p.path.len().saturating_add(1))
    }
    let rows = contract
        .requires
        .as_ref()
        .map_or(0, Vec::len)
        .saturating_add(contract.guarantees.as_ref().map_or(0, Vec::len))
        .saturating_add(contract.hold.as_ref().map_or(0, Vec::len))
        .saturating_add(contract.effects.as_ref().map_or(0, Vec::len));
    if rows > b.limits.max_nodes {
        return Err(limit("contract upper inventory exceeds selected bound"));
    }
    b.spend(rows.saturating_add(9))?;
    for requirement in contract.requires.iter().flatten() {
        match requirement {
            DynRequirement::Read(p) | DynRequirement::Write(p) | DynRequirement::Manage(p) => {
                position(p, b)?
            }
            DynRequirement::Apart(a, z) => {
                position(a, b)?;
                position(z, b)?;
            }
            DynRequirement::State {
                position: p, ty, ..
            } => {
                position(p, b)?;
                dynamic_metadata::ty(ty, b)?;
            }
        }
    }
    for guarantee in contract.guarantees.iter().flatten() {
        match guarantee {
            DynGuarantee::StorageSources(binding) => {
                position(&binding.position, b)?;
                if binding.sources.len() > b.limits.max_nodes {
                    return Err(limit("contract source inventory exceeds selected bound"));
                }
                b.spend(binding.sources.len())?;
                for source in &binding.sources {
                    position(crate::exact_storage_source(source)?, b)?;
                }
            }
            DynGuarantee::State {
                position: p, ty, ..
            } => {
                position(p, b)?;
                dynamic_metadata::ty(ty, b)?;
            }
            DynGuarantee::ManageResult => {}
        }
    }
    for p in contract.hold.iter().flatten() {
        position(p, b)?;
    }
    if let Some(access) = &contract.call {
        dynamic_metadata::recipe(access, b)?;
    }
    if let Some(DynResultSource::Borrow { sources, .. }) = &contract.result {
        if sources.len() > b.limits.max_nodes {
            return Err(limit(
                "contract result source inventory exceeds selected bound",
            ));
        }
        b.spend(sources.len())?;
        for source in sources {
            position(crate::exact_storage_source(source)?, b)?;
        }
    }
    for effect in contract.effects.iter().flatten() {
        match effect {
            DynEffect::Cleanup(ty) => dynamic_metadata::ty(ty, b)?,
            DynEffect::Call(p) => position(p, b)?,
            _ => {}
        }
    }
    crate::dynamic_cleanup::metadata(module, contract.cleanup.as_ref(), b)
}
