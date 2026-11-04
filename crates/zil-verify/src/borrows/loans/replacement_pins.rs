//! Prepared positions retain actual loan ancestry without creating an ordinary
//! exclusive payload loan. Commit activates a write through exactly its parent.
use super::*;
use crate::place_replacements::{Inventory, Origin};
use zil_ir::ReservationId;
fn roots(
    f: &TypedFunction,
    place: &crate::place_replacements::Place,
    facts: &Facts,
    b: &mut Budget,
) -> Check<Roots> {
    match place.origin {
        Origin::Local(local) => Ok(vec![super::super::roots::mixed_root(
            f,
            local,
            &place.steps,
            false,
            b,
        )?]),
        Origin::Address(address) => super::super::roots::mixed_address_roots(
            f,
            address,
            &place.steps,
            false,
            &facts.values[address.0 as usize],
            b,
        ),
    }
}
fn precise(
    f: &TypedFunction,
    i: &Instruction,
    facts: &Facts,
    b: &mut Budget,
) -> Check<Option<(Roots, bool)>> {
    Ok(Some(match &i.kind {
        InstructionKind::BorrowPlaceLocal {
            local,
            path,
            mutable,
        } => (
            vec![super::super::roots::mixed_root(f, *local, path, false, b)?],
            *mutable,
        ),
        InstructionKind::BorrowFieldLocal {
            local,
            path,
            mutable,
        } => (
            vec![super::super::roots::inline_field_root(f, *local, path, b)?],
            *mutable,
        ),
        InstructionKind::BorrowVariantLocal {
            local,
            path,
            mutable,
        } => (
            vec![super::super::roots::variant_root(f, *local, path, b)?],
            *mutable,
        ),
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => (
            vec![super::super::roots::mixed_root(f, *local, path, false, b)?],
            true,
        ),
        InstructionKind::BorrowPlaceAddress {
            address,
            path,
            mutable,
        } => (
            super::super::roots::mixed_address_roots(
                f,
                *address,
                path,
                false,
                &facts.values[address.0 as usize],
                b,
            )?,
            *mutable,
        ),
        InstructionKind::BorrowVariantAddress {
            address,
            path,
            mutable,
        } => (
            super::super::roots::address_roots(
                f,
                *address,
                path,
                &facts.values[address.0 as usize],
                b,
            )?,
            *mutable,
        ),
        InstructionKind::WriteAddress { address, .. } => {
            b.spend(facts.values[address.0 as usize].len())?;
            (facts.values[address.0 as usize].clone(), true)
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            b.spend(facts.values[address.0 as usize].len())?;
            (facts.values[address.0 as usize].clone(), *mutable)
        }
        InstructionKind::StorePackageField { local, index, .. }
        | InstructionKind::DropPackageField { local, index }
        | InstructionKind::TakePackageField { local, index } => {
            (vec![field_root(f, *local, *index, b)?], true)
        }
        InstructionKind::BorrowPackageField {
            local,
            index,
            mutable,
        } => (vec![field_root(f, *local, *index, b)?], *mutable),
        InstructionKind::Store { local, .. }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::UnpackTuple { local, .. }
        | InstructionKind::UnpackVariant { local, .. } => (vec![Root::Local(*local)], true),
        InstructionKind::Load(_)
        | InstructionKind::ReadAddress { .. }
        | InstructionKind::BorrowLocal { .. }
        | InstructionKind::PlaceTagLocal { .. }
        | InstructionKind::PlaceTagAddress { .. }
        | InstructionKind::VariantTagLocal { .. }
        | InstructionKind::VariantTagAddress { .. }
        | InstructionKind::ReadVariantLocal { .. }
        | InstructionKind::AddressField { .. }
        | InstructionKind::AddressIndex { .. }
        | InstructionKind::AddressSlice { .. } => (vec![], false),
        _ => return Ok(None),
    }))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    f: &TypedFunction,
    i: &Instruction,
    facts: &Facts,
    flow: &Flow,
    locals: &Locals,
    pins: &BTreeSet<ReservationId>,
    inventory: &Inventory,
    access: &Access<'_>,
    b: &mut Budget,
) -> Check<()> {
    let settlement = match i.kind {
        InstructionKind::CommitPlaceReplace { reservation, .. }
        | InstructionKind::AbortPlaceReservation { reservation } => Some(reservation),
        _ => None,
    };
    let ticket = match i.kind {
        InstructionKind::ReservePlaceLocal { reservation, .. }
        | InstructionKind::ReservePlaceAddress { reservation, .. }
        | InstructionKind::CommitPlaceReplace { reservation, .. } => Some(reservation),
        _ => None,
    };
    let mut writes = Vec::new();
    if let Some(id) = ticket {
        let place = inventory.get(id, b)?;
        let selected = roots(f, place, facts, b)?;
        if matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }) {
            let empty = Loans::new();
            let own = match place.origin {
                Origin::Address(value) => &flow.values[value.0 as usize],
                Origin::Local(_) => &empty,
            };
            access.check(own, &selected, true, b)?;
        }
        writes = selected;
    } else if matches!(i.kind, InstructionKind::AbortPlaceReservation { .. }) {
    } else if let Some((selected, true)) = precise(f, i, facts, b)? {
        writes = selected;
    } else if precise(f, i, facts, b)?.is_none() {
        // Opaque callbacks and mutations use all actual mutable loan roots.
        // Shared inputs remain readable throughout preparation.
        for value in instruction_operands(&i.kind) {
            b.spend(flow.values[value.0 as usize].len())?;
            for loan in &flow.values[value.0 as usize] {
                if flow.mutable[*loan] {
                    roots_join(&mut writes, &flow.roots[*loan], b)?;
                }
            }
        }
        if let InstructionKind::CallClosureMut { callee, .. }
        | InstructionKind::CallClosureShared { callee, .. } = i.kind
        {
            lookup(b, locals.len())?;
            if let Some(held) = locals.get(&callee) {
                for loan in held {
                    b.spend(1)?;
                    if flow.mutable[*loan] {
                        roots_join(&mut writes, &flow.roots[*loan], b)?;
                    }
                }
            }
        }
    }
    b.spend(pins.len())?;
    for id in pins {
        if Some(*id) == settlement {
            continue;
        }
        if overlaps(&roots(f, inventory.get(*id, b)?, facts, b)?, &writes, b)? {
            return Err(reject(
                "E-BORROW-REPLACEMENT-PIN",
                Some(i.span),
                "write, transfer or new exclusive access overlaps a prepared owning replacement",
            ));
        }
    }
    Ok(())
}
