//! Compile the retained lexical declaration to an untrusted return bound.
use super::*;

fn invalid(f: &CoreFunction, message: &str) -> Diagnostic {
    error("E-LIFE-CONTRACT", f.span, message)
}
pub(super) fn preflight(f: &CoreFunction, remaining: &mut usize) -> R<()> {
    if let Some(contract) = &f.storage_life {
        for binding in &contract.bindings {
            *remaining = remaining
                .checked_sub(binding.position.path.len().saturating_add(1))
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "stored signature metadata budget exceeded",
                        Some(f.span),
                    )
                })?;
        }
        if f.life.is_some() {
            return Err(invalid(
                f,
                "rich and flat life contracts cannot be independent authorities",
            ));
        }
    }
    let Some(life) = &f.life else { return Ok(()) };
    let mut charge = |work: usize| -> R<()> {
        *remaining = remaining.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "life metadata budget exceeded", Some(f.span))
        })?;
        Ok(())
    };
    charge(
        life.binders
            .len()
            .saturating_add(life.parameter_lives.len())
            .saturating_add(1),
    )?;
    for name in &life.binders {
        charge(
            name.len()
                .saturating_mul(life.binders.len().saturating_add(1)),
        )?;
    }
    Ok(())
}
pub(super) fn compile(
    f: &CoreFunction,
    closures: &mut closures::State,
    limits: &Limits,
) -> R<Option<Vec<u32>>> {
    let Some(life) = &f.life else { return Ok(None) };
    let work = life.binders.iter().fold(
        life.binders
            .len()
            .saturating_add(life.parameter_lives.len())
            .saturating_add(1),
        |n, s| n.saturating_add(s.len().saturating_mul(life.binders.len().saturating_add(1))),
    );
    closures.snapshot(work, f.span)?;
    if f.foreign.is_some()
        || !f.type_parameters.is_empty()
        || !f.callable_parameters.is_empty()
        || !f.state_parameters.is_empty()
        || f.callable_return.is_some()
    {
        return Err(invalid(f, "life requires an ordinary direct function"));
    }
    if life.binders.is_empty() || life.parameter_lives.len() != f.parameters.len() {
        return Err(invalid(f, "life binder or dense parameter map is invalid"));
    }
    for (i, name) in life.binders.iter().enumerate() {
        if name.is_empty() || life.binders[..i].contains(name) {
            return Err(invalid(f, "life binders must be nonempty and unique"));
        }
    }
    for (parameter, tag) in f.parameters.iter().zip(&life.parameter_lives) {
        if let Some(tag) = tag {
            if *tag as usize >= life.binders.len() {
                return Err(invalid(f, "parameter life index is outside its binder"));
            }
            closures.snapshot(
                inference::type_snapshot_work(&parameter.ty, parameter.span, limits)?,
                parameter.span,
            )?;
            if !matches!(&parameter.ty,Type::Address{inner,..} if inner.is_address_pointee()) {
                return Err(invalid(f, "life parameter must be a Copy or slice address"));
            }
        }
    }
    let Some(result) = life.result_life else {
        return Ok(None);
    };
    closures.snapshot(
        inference::type_snapshot_work(&f.return_type, f.span, limits)?,
        f.span,
    )?;
    if result as usize >= life.binders.len()
        || !matches!(&f.return_type,Type::Address{inner,..} if inner.is_address_pointee())
    {
        return Err(invalid(
            f,
            "returned life needs a declared binder and Copy or slice address",
        ));
    }
    let bound = life
        .parameter_lives
        .iter()
        .enumerate()
        .filter_map(|(i, tag)| (*tag == Some(result)).then_some(i as u32))
        .collect::<Vec<_>>();
    if bound.is_empty() {
        return Err(invalid(f, "returned life has no address input parameter"));
    }
    Ok(Some(bound))
}
