//! Invocation-local relations for thin ordinary code pointers. These checks
//! create untrusted IR; neither source annotations nor this module prove roots.
use super::*;

pub(super) fn from_life(life: &CoreLifeContract) -> FunctionLifeContract {
    let mut result = FunctionLifeContract {
        binder_count: life.binders.len() as u32,
        parameter_lives: life.parameter_lives.clone(),
        result_life: life.result_life,
    };
    let mut remap = vec![None; life.binders.len()];
    let mut next = 0;
    for label in result
        .parameter_lives
        .iter_mut()
        .chain(std::iter::once(&mut result.result_life))
    {
        if let Some(old) = *label {
            let Some(entry) = remap.get_mut(old as usize) else {
                return FunctionLifeContract {
                    binder_count: life.binders.len() as u32,
                    parameter_lives: life.parameter_lives.clone(),
                    result_life: life.result_life,
                };
            };
            let dense = *entry.get_or_insert_with(|| {
                let dense = next;
                next += 1;
                dense
            });
            *label = Some(dense);
        }
    }
    result.binder_count = next;
    result
}

pub(super) fn default_life(parameters: &[Type], result: &Type) -> FunctionLifeContract {
    FunctionLifeContract {
        binder_count: 1,
        parameter_lives: parameters
            .iter()
            .map(|ty| matches!(ty, Type::Address { .. }).then_some(0))
            .collect(),
        result_life: matches!(result, Type::Address { .. }).then_some(0),
    }
}

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &StorageLifeContract,
    span: Span,
) -> R<()> {
    let invalid = || {
        error(
            "E-FUNCTION-LIFE",
            span,
            "borrowed function pointer requires a complete invocation-local address relation",
        )
    };
    // Callable values and roles have no retained enclosing Fixed-class context
    // in this lane. Input-grounded direct definition rows are a separate case.
    if life.fixed_binder_count != 0 {
        return Err(unsupported(
            span,
            "rich borrowed callable class relation requires its complete signature consumer",
        ));
    }
    if !life.classes.is_empty() {
        return borrowed_fp_qualified::relation(parameters, result, life, span);
    }
    if life.invocation_binder_count == 0 {
        return Err(invalid());
    }
    let mut rows = life.bindings.iter();
    let mut next = 0;
    for (index, parameter) in parameters.iter().enumerate() {
        match parameter {
            Type::Address { inner, .. } if inner.is_address_pointee() => {
                let row = rows.next().ok_or_else(invalid)?;
                let StorageBinder::Invocation(tag) = row.binder else {
                    return Err(invalid());
                };
                if row.position.base != StorageBase::Parameter(index as u32)
                    || !row.position.path.is_empty()
                    || tag >= life.invocation_binder_count
                    || tag > next
                {
                    return Err(invalid());
                }
                if tag == next {
                    next += 1;
                }
            }
            ty if ty.is_copy_value() => {}
            _ => return Err(invalid()),
        }
    }
    if next != life.invocation_binder_count {
        return Err(invalid());
    }
    match result {
        Type::Address { mutable, inner } if inner.is_address_pointee() => {
            let row = rows.next().ok_or_else(invalid)?;
            let StorageBinder::Invocation(tag) = row.binder else {
                return Err(invalid());
            };
            if row.position.base != StorageBase::Result
                || !row.position.path.is_empty()
                || tag >= life.invocation_binder_count
            {
                return Err(invalid());
            }
            if !life.bindings.iter().any(|row| {
                let StorageBase::Parameter(index) = row.position.base else { return false; };
                row.binder == StorageBinder::Invocation(tag)
                    && matches!(parameters.get(index as usize), Some(Type::Address { mutable: input_mutable, .. }) if !mutable || *input_mutable)
            }) { return Err(invalid()); }
        }
        ty if ty.is_copy_value() => {}
        _ => return Err(invalid()),
    }
    if rows.next().is_some() {
        return Err(invalid());
    }
    Ok(())
}

/// O(1) reservation for ordinary flat and direct qualified-boundary profiles.
/// Their selectors are empty or exactly Pointee; longer paths reject by length.
pub(super) fn metadata_work(life: &StorageLifeContract) -> usize {
    (life.invocation_binder_count as usize)
        .saturating_add(life.fixed_binder_count as usize)
        .saturating_mul(4)
        .saturating_add(life.bindings.len().saturating_mul(8))
        .saturating_add(life.classes.len().saturating_mul(8))
        .saturating_add(2)
}

/// Pay the additional qualified-storage slot inspection from the live caller
/// ledger. The ordinary recursive type eligibility walk is reserved by each
/// existing full-signature caller; this new scan is a separate occurrence.
pub(super) fn qualified_inputs(
    life: &StorageLifeContract,
    parameters: &[Type],
    result: &Type,
    mut charge: impl FnMut(usize) -> R<()>,
) -> R<()> {
    if life.classes.is_empty() {
        return Ok(());
    }
    charge(parameters.len().saturating_add(1))?;
    for ty in parameters.iter().chain(std::iter::once(result)) {
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Type::BorrowedStorage { life, .. } = ty {
            charge(life.slots.len().saturating_add(1))?;
        }
    }
    Ok(())
}

/// Prepay a complete relation before traversal, clone or identity encoding.
/// Semantic class/position validation is performed separately by consumers.
pub(super) fn charge_contract(
    life: &StorageLifeContract,
    mut charge: impl FnMut(usize) -> R<()>,
    multiplier: usize,
) -> R<()> {
    charge(
        life.bindings
            .len()
            .saturating_add(life.classes.len().saturating_mul(8))
            .saturating_add(2)
            .saturating_mul(multiplier),
    )?;
    for row in &life.bindings {
        charge(row.position.path.len().saturating_mul(multiplier))?;
    }
    for row in &life.classes {
        charge(row.position.path.len().saturating_mul(multiplier))?;
    }
    Ok(())
}

/// Extra complete flat-to-rich construction pass when making a code value.
pub(super) fn pointer_work(signature: &Signature) -> usize {
    signature
        .parameters
        .len()
        .saturating_add(1)
        .saturating_mul(4)
        .saturating_add(
            signature
                .storage_life
                .as_ref()
                .map_or(0, |life| metadata_work(life).saturating_mul(4)),
        )
        .saturating_add(signature.life.as_ref().map_or(0, |life| {
            life.parameter_lives
                .len()
                .saturating_add(life.binder_count as usize)
        }))
}

/// Pay the qualified-slot planner and both subsequent relation inspections.
/// Type formation separately reserves its own walk, two eligibility walks,
/// the output clone, and the extra result Copy qualification.
pub(super) fn static_pointer_slots(
    signature: &Signature,
    mut charge: impl FnMut(usize) -> R<()>,
) -> R<()> {
    if let Some(life) = &signature.storage_life {
        qualified_inputs(life, &signature.parameters, &signature.result, |work| {
            charge(work.saturating_mul(3))
        })?;
    }
    Ok(())
}

/// Abstract a static definition's complete incoming equations into fresh
/// invocation formals. The actual definition relation remains unchanged.
/// Caller prepays metadata, type walks, mapping storage and returned snapshots.
pub(super) fn static_pointer_life(
    parameters: &[Type],
    result: &Type,
    definition: &StorageLifeContract,
) -> Option<StorageLifeContract> {
    if definition.classes.is_empty() {
        relation(parameters, result, definition, Span::default()).ok()?;
    } else {
        // This is the unchanged, independently checked static definition,
        // not a callable value with an unrepresented captured class context.
        borrowed_fp_qualified::relation(parameters, result, definition, Span::default()).ok()?;
    }
    let mut fixed = vec![None; definition.fixed_binder_count as usize];
    let mut invocation = vec![None; definition.invocation_binder_count as usize];
    let mut next = 0u32;
    fn map(
        binder: StorageBinder,
        fixed: &mut [Option<u32>],
        invocation: &mut [Option<u32>],
        next: &mut u32,
    ) -> Option<StorageBinder> {
        let slot = match binder {
            StorageBinder::Fixed(n) => fixed.get_mut(n as usize)?,
            StorageBinder::Invocation(n) => invocation.get_mut(n as usize)?,
        };
        let label = match *slot {
            Some(label) => label,
            None => {
                let label = *next;
                *next = next.checked_add(1)?;
                *slot = Some(label);
                label
            }
        };
        Some(StorageBinder::Invocation(label))
    }
    let mut output = definition.clone();
    let (mut borrowed, mut class) = (0, 0);
    for base in (0..parameters.len())
        .map(|index| StorageBase::Parameter(index as u32))
        .chain(std::iter::once(StorageBase::Result))
    {
        while let Some(row) = output
            .bindings
            .get_mut(borrowed)
            .filter(|row| row.position.base == base)
        {
            row.binder = map(row.binder, &mut fixed, &mut invocation, &mut next)?;
            borrowed += 1;
        }
        while let Some(row) = output
            .classes
            .get_mut(class)
            .filter(|row| row.position.base == base)
        {
            row.binder = map(row.binder, &mut fixed, &mut invocation, &mut next)?;
            class += 1;
        }
    }
    if borrowed != output.bindings.len() || class != output.classes.len() {
        return None;
    }
    output.fixed_binder_count = 0;
    output.invocation_binder_count = next;
    relation(parameters, result, &output, Span::default()).ok()?;
    Some(output)
}

pub(super) fn function_reference(
    signature: &Signature,
    expected: Option<&Type>,
    span: Span,
) -> R<Option<Type>> {
    let Some(Type::BorrowedFunctionPointer {
        parameters,
        result,
        life,
    }) = expected
    else {
        return Ok(None);
    };
    relation(parameters, result, life, span)?;
    if signature.id.is_none()
        || signature.foreign
        || signature.cleanup
        || signature.type_parameter.is_some()
        || !signature.callable.is_empty()
        || signature.parameters != *parameters
        || signature.result != **result
    {
        return Err(error(
            "E-FUNCTION-LIFE",
            span,
            "borrowed function pointer target has a different complete signature or requires an environment",
        ));
    }
    Ok(Some(expected.expect("borrowed pointer context").clone()))
}

/// A concrete call returns only invocation input storage in this profile.
/// This classifier triggers a full actual-return proof; it grants no origins.
pub(super) fn invocation(parameters: &[Type], result: &Type) -> bool {
    matches!(result, Type::Address { .. })
        && parameters.iter().any(|p| matches!(p, Type::Address { .. }))
}

pub(super) fn invocation_call<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &'a InstructionKind,
    mut charge: impl FnMut(usize) -> R<()>,
) -> R<Option<(&'a ClosureDefinition, &'a [ValueId])>> {
    let selected = (|| {
        let (ty, args) = match kind {
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => (f.values.get(callee.0 as usize)?, arguments),
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                (&f.locals.get(callee.0 as usize)?.ty, arguments)
            }
            _ => return None,
        };
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        let Type::Closure(id) = ty else { return None };
        let d = module.closures.get(id.0 as usize)?;
        (d.lending_captures.is_empty() && matches!(d.result, Type::Address { .. }))
            .then_some((d, args.as_slice()))
    })();
    let Some((d, args)) = selected else {
        return Ok(None);
    };
    charge(d.parameters.len())?;
    Ok(invocation(&d.parameters, &d.result).then_some((d, args)))
}

#[cfg(test)]
mod rich_contract_tests {
    use super::*;

    #[test]
    fn flat_only_consumer_preserves_order_and_rejects_rich_class_erasure() {
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        };
        let parameters = vec![address.clone(), Type::I32, address.clone()];
        let original = StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: 2,
            parameter_lives: vec![Some(0), None, Some(1)],
            result_life: Some(0),
        })
        .unwrap();
        assert!(relation(&parameters, &address, &original, Span::default()).is_ok());
        for n in 0..5 {
            let mut changed = original.clone();
            match n {
                0 => changed.bindings.swap(0, 1),
                1 => changed.bindings[0]
                    .position
                    .path
                    .push(ContractProjection::RetainedClass(0)),
                2 => changed.bindings[1].position.base = StorageBase::Parameter(1),
                3 => changed.fixed_binder_count = 1,
                _ => changed.classes.push(SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Pointee],
                        class: 0,
                    },
                    binder: StorageBinder::Invocation(1),
                }),
            }
            assert!(
                relation(&parameters, &address, &changed, Span::default()).is_err(),
                "mutation {n}"
            );
        }
        let mutable = Type::Address {
            mutable: true,
            inner: Box::new(Type::I64),
        };
        assert!(relation(&parameters, &mutable, &original, Span::default()).is_err());
    }

    #[test]
    fn complete_class_metadata_is_reserved_from_the_same_remaining_balance() {
        let mut contract = StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: 1,
            parameter_lives: vec![Some(0)],
            result_life: Some(0),
        })
        .unwrap();
        contract.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::TypeArgument(0); 23],
                class: 0,
            },
            binder: StorageBinder::Invocation(0),
        });
        let run = |allowance: usize| {
            let mut remaining = allowance;
            charge_contract(
                &contract,
                |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| {
                        Diagnostic::limit(Phase::Types, "test contract budget", None)
                    })?;
                    Ok(())
                },
                3,
            )
            .map(|()| remaining)
        };
        let n = (0..1000).find(|n| run(*n).is_ok()).unwrap();
        assert!(n > 23 * 3);
        assert_eq!(run(n).unwrap(), 0);
        assert!(run(n - 1).is_err());
        assert_eq!(run(n).unwrap(), 0);
    }
}
