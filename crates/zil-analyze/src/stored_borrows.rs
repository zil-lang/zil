//! Independent physical stored-leaf typing. Actual target/loan/init/epoch CFG
//! state is a separate obligation. Full carrier/signature admission remains
//! Unsupported until that consumer is complete; this module never grants it.
use super::*;

fn fail(message: &str) -> Vec<Diagnostic> {
    bad(format!("stored borrow: {message}"))
}
fn operand(f: &TypedFunction, id: ValueId) -> CompileResult<&Type> {
    f.values
        .get(id.0 as usize)
        .ok_or_else(|| fail("operand outside the actual value table"))
}
fn selected<'a>(
    f: &'a TypedFunction,
    id: ValueId,
    path: &[PlaceProjection],
    budget: &mut Budget,
) -> CompileResult<(bool, &'a Type)> {
    let Type::Address { mutable, inner } = operand(f, id)? else {
        return Err(fail("base is not an actual storage Address"));
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return Err(fail("base does not address a qualified stored carrier"));
    };
    if path.is_empty() {
        return Err(fail("physical stored-leaf path must be nonempty"));
    }
    let mut declaration = None;
    budget.spend(life.slots.len(), "stored leaf declared slot scan")?;
    for slot in &life.slots {
        budget.spend(
            path.len().saturating_add(slot.path.len()),
            "stored leaf physical path equality",
        )?;
        if slot.path == path && declaration.replace(slot.mode).is_some() {
            return Err(fail("duplicate physical stored leaf declaration"));
        }
    }
    let mode = declaration.ok_or_else(|| fail("actual leaf lacks its stored relation"))?;
    let mut ty = storage.as_ref();
    for step in path {
        budget.spend(1, "actual inline stored path step")?;
        ty = match step {
            PlaceProjection::Field(index) => match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    fields.get(*index as usize).map(|(_, ty)| ty)
                }
                Type::Tuple(fields) => fields.get(*index as usize),
                _ => None,
            },
            PlaceProjection::Variant(step) => match ty {
                Type::Enum { variants, .. } => variants
                    .get(step.variant as usize)
                    .and_then(|(_, fields)| fields.get(step.index as usize)),
                _ => None,
            },
            PlaceProjection::ErrorArm(error) => match ty {
                Type::ErrorUnion { error: err, result } => Some(if *error {
                    err.as_ref()
                } else {
                    result.as_ref()
                }),
                _ => None,
            },
        }
        .ok_or_else(|| fail("path crosses non-inline storage or an absent ancestor"))?;
    }
    match (ty, mode) {
        (Type::Address { mutable: false, .. }, StoredBorrowMode::SharedAddress)
        | (Type::Address { mutable: true, .. }, StoredBorrowMode::ExclusiveAddress)
        | (
            Type::Desc(_),
            StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription,
        ) => Ok((*mutable, ty)),
        _ => Err(fail("declared slot mode differs from its actual leaf")),
    }
}

pub(super) fn instruction(
    f: &TypedFunction,
    i: &Instruction,
    budget: &mut Budget,
) -> CompileResult<()> {
    let expected = match &i.kind {
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode,
        } => {
            let (exclusive, ty) = selected(f, *storage, path, budget)?;
            budget.type_work(ty, false, "stored load actual type snapshot and comparison")?;
            let ty = match mode {
                StoredBorrowLoad::CopyShared
                    if matches!(ty, Type::Address { mutable: false, .. }) =>
                {
                    ty.clone()
                }
                StoredBorrowLoad::CopyDescription if matches!(ty, Type::Desc(_)) => ty.clone(),
                StoredBorrowLoad::Take if exclusive && matches!(ty, Type::Address { .. }) => {
                    ty.clone()
                }
                StoredBorrowLoad::Reborrow { mutable } => {
                    let Type::Address {
                        mutable: source,
                        inner,
                    } = ty
                    else {
                        return Err(fail("stored reborrow needs an actual Address leaf"));
                    };
                    if *mutable && (!exclusive || !source) {
                        return Err(fail(
                            "stored reborrow lacks exclusive actual carrier/leaf authority",
                        ));
                    }
                    Type::Address {
                        mutable: *mutable,
                        inner: inner.clone(),
                    }
                }
                _ => return Err(fail("stored load mode lacks its actual leaf permissions")),
            };
            Some(ty)
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            let (exclusive, ty) = selected(f, *storage, path, budget)?;
            if !exclusive {
                return Err(fail(
                    "stored replacement needs an exclusive carrier Address",
                ));
            }
            budget.type_work(ty, false, "stored replacement exact actual type")?;
            if operand(f, *value)? != ty {
                return Err(fail("new stored value differs from the actual leaf type"));
            }
            None
        }
        _ => unreachable!("stored-borrow analysis instruction dispatch"),
    };
    match (i.result, expected) {
        (None, None) => Ok(()),
        (Some(result), Some(expected)) if operand(f, result)? == &expected => Ok(()),
        _ => Err(fail(
            "stored instruction result differs from its actual physical operation",
        )),
    }
}
