//! Roles reprove an entry's invocation equations. They cannot change its
//! persistent domain or turn a static occurrence into an input dependency.
use super::*;

pub(crate) fn check(
    module: &TypedModule,
    definition: &ClosureDefinition,
    role: &StorageLifeContract,
    budget: &mut Budget,
) -> CompileResult<()> {
    super::super::signature_life::charge(role, budget)?;
    let Some(actual) = &definition.storage_life else {
        // Legacy invocation-only roles still need their own body proof. An
        // absent definition contract supplies no persistent class authority.
        return super::super::borrowed_fp::relation(
            &definition.parameters,
            &definition.result,
            role,
        );
    };
    super::super::signature_life::charge(actual, budget)?;
    budget.spend(
        actual
            .bindings
            .len()
            .saturating_add(role.bindings.len())
            .saturating_add(actual.classes.len())
            .saturating_add(role.classes.len()),
        "actual role persistent occurrence comparison",
    )?;
    if actual.fixed_binder_count != role.fixed_binder_count
        || actual.static_bindings != role.static_bindings
        || !actual
            .bindings
            .iter()
            .filter(|r| matches!(r.binder, StorageBinder::Fixed(_)))
            .eq(role
                .bindings
                .iter()
                .filter(|r| matches!(r.binder, StorageBinder::Fixed(_))))
        || !actual
            .classes
            .iter()
            .filter(|r| matches!(r.binder, StorageBinder::Fixed(_)))
            .eq(role
                .classes
                .iter()
                .filter(|r| matches!(r.binder, StorageBinder::Fixed(_))))
    {
        return Err(bad(
            "role changes the complete actual persistent or static domain",
        ));
    }
    super::super::borrowed_fp::qualified_array_relation_work(
        &definition.parameters,
        &definition.result,
        budget,
    )?;
    super::super::borrowed_fp::qualified_array_relation_work(
        &definition.captures,
        &Type::Unit,
        budget,
    )?;
    for relation in [actual, role] {
        super::super::signature_life::closure_relation(
            &module.closures,
            &module.dyn_interfaces,
            definition.kind,
            &definition.parameters,
            &definition.captures,
            super::life_result(definition),
            relation,
        )?;
    }
    Ok(())
}
