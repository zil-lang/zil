//! Paid transport of raw retained-life inventory. This grants no CT value or
//! runtime borrowing admission; the ordinary kernels consume the preserved data.
use super::*;
fn quota() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "Core retained metadata budget exceeded",
        None,
    )]
}
fn item(depth: usize, nodes: &mut usize, work: &mut usize, limits: &Limits) -> CompileResult<()> {
    if depth > limits.max_call_depth || *nodes >= limits.max_nodes {
        return Err(quota());
    }
    *work = work.checked_sub(1).ok_or_else(quota)?;
    *nodes += 1;
    Ok(())
}
fn path<T>(
    path: &[T],
    depth: usize,
    nodes: &mut usize,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<()> {
    for (index, _) in path.iter().enumerate() {
        item(depth.saturating_add(index), nodes, work, limits)?;
    }
    Ok(())
}
pub(super) fn shape(
    shape: &StorageLifeShape,
    depth: usize,
    nodes: &mut usize,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<()> {
    item(depth, nodes, work, limits)?;
    for row in &shape.slots {
        item(depth + 1, nodes, work, limits)?;
        path(&row.path, depth + 2, nodes, work, limits)?;
    }
    for row in &shape.retained {
        item(depth + 1, nodes, work, limits)?;
        path(&row.path, depth + 2, nodes, work, limits)?;
    }
    for row in &shape.nominal_arguments {
        item(depth + 1, nodes, work, limits)?;
        path(&row.path, depth + 2, nodes, work, limits)?;
    }
    Ok(())
}
pub(super) fn contract(
    shape: &StorageLifeContract,
    nodes: &mut usize,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<()> {
    item(0, nodes, work, limits)?;
    for row in &shape.bindings {
        item(1, nodes, work, limits)?;
        path(&row.position.path, 2, nodes, work, limits)?;
    }
    Ok(())
}
pub(super) fn inventory(core: &CoreModule, limits: &Limits) -> CompileResult<(usize, usize)> {
    let mut nodes = 0;
    let mut work = limits.max_analysis_steps;
    for declaration in &core.life_declarations {
        item(0, &mut nodes, &mut work, limits)?;
        let identity = &declaration.identity;
        work = work.checked_sub(identity.package.len()).ok_or_else(quota)?;
        for part in identity
            .module_path
            .iter()
            .chain(&identity.declaration_path)
        {
            item(1, &mut nodes, &mut work, limits)?;
            work = work.checked_sub(part.len()).ok_or_else(quota)?;
        }
        for _ in &identity.generation_path {
            item(1, &mut nodes, &mut work, limits)?;
        }
        for parameter in &declaration.parameters {
            item(1, &mut nodes, &mut work, limits)?;
            if let LifeParameterKind::Const(ty) = parameter {
                hint_type(ty, 2, &mut nodes, &mut work, limits)?;
            }
        }
    }
    for instance in &core.life_instances {
        item(0, &mut nodes, &mut work, limits)?;
        hint_type(&instance.ty, 1, &mut nodes, &mut work, limits)?;
        for argument in &instance.arguments {
            item(1, &mut nodes, &mut work, limits)?;
            match argument {
                LifeNominalActual::QualifiedType { ty, life } => {
                    hint_type(ty, 2, &mut nodes, &mut work, limits)?;
                    if life.binder_count as usize > limits.max_nodes {
                        return Err(quota());
                    }
                    for _ in 0..life.binder_count {
                        item(2, &mut nodes, &mut work, limits)?;
                    }
                    shape(life, 2, &mut nodes, &mut work, limits)?;
                }
                LifeNominalActual::Type(ty) => {
                    hint_type(ty, 2, &mut nodes, &mut work, limits)?;
                }
                LifeNominalActual::Const(Constant::WideString(value)) => {
                    work = work.checked_sub(value.len()).ok_or_else(quota)?;
                }
                LifeNominalActual::Const(_) | LifeNominalActual::Lifetime(_) => {}
            }
        }
    }
    Ok((nodes, limits.max_analysis_steps - work))
}
