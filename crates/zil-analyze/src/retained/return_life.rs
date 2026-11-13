//! Compare the public result interface with the independently checked actual
//! closure relation. Extra captured Fixed classes remain rigid, including
//! classes not mentioned by Parameter/Result interface positions.
use super::*;

pub(super) fn check(
    module: &TypedModule,
    function: &TypedFunction,
    contract: &CallableReturnContract,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(expected) = &contract.life else {
        return Ok(());
    };
    let d = module
        .closures
        .get(contract.closure.0 as usize)
        .ok_or_else(|| bad("return life definition absent"))?;
    let actual = d
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("return life omits actual complete environment relation"))?;
    super::super::signature_life::charge(expected, b)?;
    super::super::signature_life::charge(actual, b)?;
    super::super::signature_life::closure_relation(
        &module.closures,
        &module.dyn_interfaces,
        d.kind,
        &d.parameters,
        &d.captures,
        &d.result,
        actual,
    )?;
    b.spend(
        expected
            .bindings
            .len()
            .saturating_add(expected.classes.len())
            .saturating_add(actual.bindings.len())
            .saturating_add(actual.classes.len()),
        "actual returned callable public positions",
    )?;
    if expected.fixed_binder_count != actual.fixed_binder_count
        || expected.invocation_binder_count != actual.invocation_binder_count
    {
        return Err(bad(
            "return life changes the complete actual environment domain",
        ));
    }
    let bindings = actual
        .bindings
        .iter()
        .filter(|row| !matches!(row.position.base, StorageBase::Capture(_)))
        .collect::<Vec<_>>();
    let classes = actual
        .classes
        .iter()
        .filter(|row| !matches!(row.position.base, StorageBase::Capture(_)))
        .collect::<Vec<_>>();
    let constants = actual
        .static_bindings
        .iter()
        .filter(|position| !matches!(position.base, StorageBase::Capture(_)))
        .collect::<Vec<_>>();
    if expected
        .static_bindings
        .iter()
        .ne(constants.iter().copied())
    {
        return Err(bad(
            "return constant positions differ from the complete actual interface",
        ));
    }
    if bindings.len() != expected.bindings.len() || classes.len() != expected.classes.len() {
        return Err(bad(
            "return life omits an actual parameter, result or receiver position",
        ));
    }
    let mut left = vec![None; expected.invocation_binder_count as usize];
    let mut right = vec![None; actual.invocation_binder_count as usize];
    let mut compare = |a: StorageBinder, z: StorageBinder, output: bool| -> CompileResult<()> {
        b.spend(1, "returned callable lifetime equation")?;
        match (a, z) {
            (StorageBinder::Fixed(x), StorageBinder::Fixed(y))
                if x == y && x < actual.fixed_binder_count =>
            {
                Ok(())
            }
            (StorageBinder::Invocation(x), StorageBinder::Invocation(y)) => {
                let a = left
                    .get_mut(x as usize)
                    .ok_or_else(|| bad("return invocation exceeds declared domain"))?;
                let z = right
                    .get_mut(y as usize)
                    .ok_or_else(|| bad("actual return invocation exceeds domain"))?;
                if output && (a.is_none() || z.is_none()) {
                    return Err(bad(
                        "returned invocation is not grounded in an actual input or receiver",
                    ));
                }
                if a.replace(y).is_some_and(|old| old != y)
                    || z.replace(x).is_some_and(|old| old != x)
                {
                    return Err(bad("return life merges independent invocation classes"));
                }
                Ok(())
            }
            _ => Err(bad(
                "return life changes a rigid class or erases an actual receiver dependency",
            )),
        }
    };
    // Both vectors have their own canonical positions. Establish all real
    // input equations before consulting any Result, including class-only inputs.
    for output in [false, true] {
        for (expected, actual) in expected.bindings.iter().zip(&bindings) {
            if expected.position != actual.position {
                return Err(bad("return borrow positions differ from actual interface"));
            }
            if (expected.position.base == StorageBase::Result) == output {
                compare(expected.binder, actual.binder, output)?;
            }
        }
        for (expected, actual) in expected.classes.iter().zip(&classes) {
            if expected.position != actual.position {
                return Err(bad("return class positions differ from actual interface"));
            }
            if (expected.position.base == StorageBase::Result) == output {
                compare(expected.binder, actual.binder, output)?;
            }
        }
    }
    if left.iter().any(Option::is_none) || right.iter().any(Option::is_none) {
        return Err(bad("return interface has an ungrounded invocation domain"));
    }
    if function.return_type != Type::Closure(d.id) {
        return Err(bad(
            "return life is not attached to its actual returned closure",
        ));
    }
    Ok(())
}
