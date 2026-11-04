//! Actual storage-leaf instruction typing. This is not a loan/init proof.
//! BorrowedStorage admission stays closed until the independent CFG consumer
//! reconstructs target loans, fixed bounds and slot initialization/epochs.
use super::*;

fn bad(span: Option<Span>, message: &str) -> Diagnostic {
    reject("E-VERIFY-STORED-BORROW", span, message)
}

fn leaf<'a>(
    function: &'a TypedFunction,
    storage: ValueId,
    path: &[PlaceProjection],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<(bool, &'a Type, StoredBorrowMode)> {
    let Type::Address { mutable, inner } = value_type(function, storage, span)? else {
        return Err(bad(
            span,
            "stored borrow requires an actual storage Address",
        ));
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return Err(bad(
            span,
            "stored borrow base lacks its actual qualified carrier",
        ));
    };
    if path.is_empty() {
        return Err(bad(
            span,
            "stored borrow requires a bounded nonempty physical path",
        ));
    }
    // Each comparison is prepaid before visiting or comparing either path.
    // Declared slot order/canonical classes need the separate full shape proof.
    let mut selected = None;
    budget.spend(life.slots.len())?;
    for slot in &life.slots {
        budget.spend(path.len().saturating_add(slot.path.len()))?;
        if slot.path == path && selected.replace(slot.mode).is_some() {
            return Err(bad(span, "stored leaf occurs twice in its declared shape"));
        }
    }
    let mode =
        selected.ok_or_else(|| bad(span, "physical leaf has no declared stored relation"))?;
    let mut ty = storage.as_ref();
    for step in path {
        budget.spend(1)?;
        ty = match (ty, step) {
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(*index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            (Type::Enum { variants, .. }, PlaceProjection::Variant(projection)) => variants
                .get(projection.variant as usize)
                .and_then(|(_, fields)| fields.get(projection.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                Some(if *is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            _ => None,
        }
        .ok_or_else(|| {
            bad(
                span,
                "stored path crosses a non-inline boundary or absent field",
            )
        })?;
    }
    if !matches!(
        (ty, mode),
        (
            Type::Address { mutable: false, .. },
            StoredBorrowMode::SharedAddress
        ) | (
            Type::Address { mutable: true, .. },
            StoredBorrowMode::ExclusiveAddress
        ) | (
            Type::Desc(_),
            StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription
        )
    ) {
        return Err(bad(
            span,
            "stored leaf mode differs from its actual Address/Desc type",
        ));
    }
    Ok((*mutable, ty, mode))
}

pub(super) fn instruction(
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match &instruction.kind {
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode,
        } => {
            let (mutable, ty, _) = leaf(function, *storage, path, span, budget)?;
            resources::charge_payload(ty, budget)?;
            let expected = match (mode, ty) {
                (StoredBorrowLoad::CopyShared, Type::Address { mutable: false, .. })
                | (StoredBorrowLoad::CopyDescription, Type::Desc(_)) => ty.clone(),
                (
                    StoredBorrowLoad::Reborrow { mutable: requested },
                    Type::Address {
                        mutable: permission,
                        inner,
                    },
                ) if !requested || mutable && *permission => Type::Address {
                    mutable: *requested,
                    inner: inner.clone(),
                },
                (StoredBorrowLoad::Take, _) if mutable => ty.clone(),
                _ => {
                    return Err(bad(
                        span,
                        "stored load lacks its actual shared/copy/exclusive mode",
                    ));
                }
            };
            Ok(Some(expected))
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            let (mutable, ty, _) = leaf(function, *storage, path, span, budget)?;
            if !mutable {
                return Err(bad(
                    span,
                    "stored leaf replacement requires an exclusive carrier Address",
                ));
            }
            resources::charge_payload(ty, budget)?;
            if value_type(function, *value, span)? != ty {
                return Err(bad(
                    span,
                    "stored replacement differs from the actual leaf type",
                ));
            }
            Ok(None)
        }
        _ => unreachable!("stored-borrow instruction dispatch"),
    }
}
