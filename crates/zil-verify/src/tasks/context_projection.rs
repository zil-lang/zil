//! Verify a factory's reindexing of an immediately returned Task creation.
use crate::*;

pub(crate) type Definition = Option<(usize, usize)>;

/// Recover the physical input positions from the actual creation operands. A
/// reborrow may preserve the unchanged parameter, but another value or place
/// cannot establish that parameter's identity.
fn parameter_source(
    f: &TypedFunction,
    definitions: &[Definition],
    parameters: &BTreeMap<LocalId, u32>,
    mut value: ValueId,
    b: &mut Budget,
) -> Check<Option<u32>> {
    for _ in 0..=definitions.len() {
        b.spend(1)?;
        let Some(op) = defining_instruction(f, definitions, value) else {
            return Ok(None);
        };
        match op.kind {
            InstructionKind::Load(local) => {
                let Some(declaration) = f.locals.get(local.0 as usize) else {
                    return Ok(None);
                };
                b.spend(
                    parameters
                        .len()
                        .checked_ilog2()
                        .unwrap_or(0)
                        .saturating_add(1) as usize,
                )?;
                return Ok((!declaration.mutable)
                    .then(|| parameters.get(&local).copied())
                    .flatten());
            }
            InstructionKind::ReborrowAddress { address, .. } => value = address,
            _ => return Ok(None),
        }
    }
    Ok(None)
}

fn defining_instruction<'a>(
    f: &'a TypedFunction,
    definitions: &[Definition],
    value: ValueId,
) -> Option<&'a Instruction> {
    let (block, instruction) = definitions.get(value.0 as usize)?.as_ref()?;
    f.blocks.get(*block)?.instructions.get(*instruction)
}

/// Semantic replay precedes structural SSA validation. Build its own unique
/// definition directory on demand, including unreachable instructions.
pub(crate) fn definitions(f: &TypedFunction, b: &mut Budget) -> Check<Vec<Definition>> {
    if f.values.len() > b.limits.max_values || f.blocks.len() > b.limits.max_blocks {
        return Err(limit("Task return SSA definition directory exceeds verification limits"));
    }
    b.spend(f.values.len().saturating_add(f.blocks.len()))?;
    let mut definitions = vec![None; f.values.len()];
    for (block, body) in f.blocks.iter().enumerate() {
        b.spend(body.instructions.len())?;
        for (instruction, op) in body.instructions.iter().enumerate() {
            if let Some(value) = op.result {
                let position = definitions.get_mut(value.0 as usize).ok_or_else(|| {
                    reject(
                        "E-VERIFY-SSA",
                        Some(op.span),
                        "constructor input directory has an absent SSA result",
                    )
                })?;
                if position.replace((block, instruction)).is_some() {
                    return Err(reject(
                        "E-VERIFY-SSA",
                        Some(op.span),
                        "constructor input directory repeats an SSA definition",
                    ));
                }
            }
        }
    }
    Ok(definitions)
}

/// Only the leading worker input coordinate changes. In particular the exact
/// error, result, retained port order, modes, class map and nominal occurrences
/// have to survive the boundary. Constructor validation and actual Hold-root
/// verification still apply to the physical SSA definition.
pub(crate) fn matches(
    f: &TypedFunction,
    definitions: &[Definition],
    value: ValueId,
    actual: &Type,
    public: &Type,
    b: &mut Budget,
) -> Check<bool> {
    let Some(Instruction {
        kind:
            InstructionKind::TaskCreate {
                arguments,
                port_bindings,
                ..
            },
        ..
    }) = defining_instruction(f, definitions, value)
    else {
        return Ok(false);
    };
    let (
        Type::ErrorUnion {
            error: actual_error,
            result: actual_ok,
        },
        Type::ErrorUnion {
            error: public_error,
            result: public_ok,
        },
    ) = (actual, public)
    else {
        return Ok(false);
    };
    let (Type::Managed(actual_task), Type::Managed(public_task)) =
        (actual_ok.as_ref(), public_ok.as_ref())
    else {
        return Ok(false);
    };
    let (
        Type::Task {
            result: actual_result,
            retained_life: a,
        },
        Type::Task {
            result: public_result,
            retained_life: p,
        },
    ) = (actual_task.as_ref(), public_task.as_ref())
    else {
        return Ok(false);
    };
    super::charge_result_selection(actual, b)?;
    super::charge_result_selection(public, b)?;
    dynamic_metadata::ty(actual_error, b)?;
    dynamic_metadata::ty(public_error, b)?;
    if actual_error != public_error
        || actual_result != public_result
        || a.ports != p.ports
        || a.binder_count != p.binder_count
        || a.retained.len() != p.retained.len()
        || a.nominal_arguments.len() != p.nominal_arguments.len()
        || port_bindings.len() != a.ports.len()
    {
        return Ok(false);
    }

    let mut parameters = BTreeMap::new();
    for (ordinal, &local) in f.parameters.iter().enumerate() {
        b.spend(
            parameters
                .len()
                .checked_ilog2()
                .unwrap_or(0)
                .saturating_add(1) as usize,
        )?;
        let ordinal =
            u32::try_from(ordinal).map_err(|_| limit("Task factory parameter index overflow"))?;
        if parameters.insert(local, ordinal).is_some() {
            return Ok(false);
        }
    }
    let mut origins = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for (port, binding) in port_bindings.iter().enumerate() {
        b.spend(origins.len().checked_ilog2().unwrap_or(0).saturating_add(1) as usize)?;
        let StorageBase::Parameter(worker) = binding.source.base else {
            return Ok(false);
        };
        if binding.port as usize != port || !binding.source.path.is_empty() {
            return Ok(false);
        }
        let Some(&argument) = arguments.get(worker as usize) else {
            return Ok(false);
        };
        let Some(factory) = parameter_source(f, definitions, &parameters, argument, b)? else {
            return Ok(false);
        };
        if !sources.insert(factory) || origins.insert(worker, factory).is_some() {
            return Ok(false);
        }
    }

    let path_matches = |a: &[LifeTypeProjection], p: &[LifeTypeProjection]| match (
        a.split_first(),
        p.split_first(),
    ) {
        (
            Some((LifeTypeProjection::Parameter(worker), a)),
            Some((LifeTypeProjection::Parameter(factory), p)),
        ) => origins.get(worker) == Some(factory) && a == p,
        (Some((LifeTypeProjection::Parameter(_), _)), _) => false,
        _ => a == p,
    };
    for (a, p) in a.retained.iter().zip(&p.retained) {
        b.spend(origins.len().checked_ilog2().unwrap_or(0).saturating_add(1) as usize)?;
        if a.class != p.class || a.binder != p.binder || !path_matches(&a.path, &p.path) {
            return Ok(false);
        }
    }
    for (a, p) in a.nominal_arguments.iter().zip(&p.nominal_arguments) {
        b.spend(origins.len().checked_ilog2().unwrap_or(0).saturating_add(1) as usize)?;
        if a.declaration != p.declaration
            || a.argument != p.argument
            || a.binder != p.binder
            || !path_matches(&a.path, &p.path)
        {
            return Ok(false);
        }
    }
    Ok(true)
}
