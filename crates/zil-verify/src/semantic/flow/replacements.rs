//! Complete original assignment and child-domain transport for affine replace
//! tickets. The ordinary ticket/resource kernels still prove activation,
//! initialization, tags, owner debt, all parent loans and retirement.
use super::*;
use crate::place_replacements::{Inventory, Origin};
use zil_ir::ReservationId;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn replacement_instruction(
        &self, module: &TypedModule, caller: &SemanticInstance, f: &TypedFunction,
        instruction: &Instruction, state: &mut State, values: &[Option<CheckedType>],
        environment: &EvalEnv, external: &mut impl ExternalProofs,
        active: &mut ActiveQueries, b: &mut Budget,
    ) -> Check<Option<CheckedType>> {
        let reservation = match instruction.kind {
            InstructionKind::ReservePlaceLocal { reservation, .. }
            | InstructionKind::ReservePlaceAddress { reservation, .. }
            | InstructionKind::CommitPlaceReplace { reservation, .. }
            | InstructionKind::AbortPlaceReservation { reservation } => reservation,
            _ => return Err(bad("replacement consumer selected another actual opcode")),
        };
        if instruction.result.is_some() {
            return Err(bad("replacement ticket operation invents an SSA result"));
        }
        // Reconstruction checks all static definitions, complete routes and
        // owner types, including unreachable blocks. Dynamic activation is a
        // separate mandatory CFG/resource proof later in verification.
        let inventory = Inventory::reconstruct(module, f, b)?;
        let place = inventory.get(reservation, b)?;
        let reserve = replacement_reserve(f, reservation, b)?;
        let originals = self.original.borrow().as_ref()
            .ok_or_else(|| bad("replacement lacks its independently checked original body"))?
            .assignments(module, caller.definition, b)?;
        b.spend(originals.len())?;
        let mut candidates = originals.iter().filter(|original| original.span == reserve.span);
        let original = candidates.next()
            .ok_or_else(|| bad("replacement has no corresponding original assignment"))?;
        if candidates.next().is_some() || original.compound.is_some() {
            return Err(bad("replacement does not select one ordinary original assignment"));
        }
        for block in &f.blocks {
            b.spend(block.instructions.len().saturating_add(1))?;
            for other in &block.instructions {
                if other.span == original.span && matches!(other.kind,
                    InstructionKind::ReservePlaceLocal { reservation: other, .. }
                    | InstructionKind::ReservePlaceAddress { reservation: other, .. } if other != reservation)
                {
                    return Err(bad("one original assignment supplies several actual reservations"));
                }
            }
        }
        // A span only finds a candidate: exact original binder, every ordered
        // field and the actual prepared address/local prove the destination.
        self.replacement_target_source(module, caller, f, place, original.target, b)?;
        let expected = self.evaluate_type(module, &SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Expression(original.target),
        }, environment, external, active, b)?;
        dynamic_metadata::ty(&place.ty, b)?;
        dynamic_metadata::ty(&expected.ty, b)?;
        if expected.ty != place.ty {
            return Err(bad("reserved endpoint changes its complete original assignment type"));
        }
        let (parent, pointed) = match place.origin {
            Origin::Address(address) => (Some(value_at(values, address)?), true),
            Origin::Local(local) => (state.locals.get(local.0 as usize)
                .ok_or_else(|| bad("replacement local has no semantic state"))?.as_deref(), false),
        };
        if let Some(parent) = parent {
            b.spend(place.steps.len().saturating_add(usize::from(pointed)))?;
            let mut path = Vec::with_capacity(place.steps.len() + usize::from(pointed));
            if pointed { path.push(LifeTypeProjection::Pointee); }
            path.extend(place.steps.iter().copied().map(LifeTypeProjection::Place));
            let actual = self.child_value(module, parent, &path, b)?;
            if !self.same_type(environment, &actual, &expected, b)? {
                return Err(bad("reserved owner changes its complete original child class domain"));
            }
        } else if !place.steps.is_empty() {
            return Err(bad("replacement field has no initialized complete semantic parent"));
        }
        if let InstructionKind::CommitPlaceReplace { value, .. } = instruction.kind {
            let actual = value_at(values, value)?;
            if !self.same_type(environment, actual, &expected, b)? {
                return Err(bad("replacement RHS changes the complete original child type or classes"));
            }
            self.assignment_value_source(module, caller, f, value, original.value, b)?;
            // A whole-local commit installs a new initialized occupant. A
            // field/address commit preserves its fixed declared ghost domain;
            // the ordinary ledger publishes the actual new sources and debt.
            if let Origin::Local(local) = place.origin {
                if place.steps.is_empty() {
                    actual.charge_clone(b)?;
                    state.edit_locals(b)?[local.0 as usize] = Some(Rc::new(actual.clone()));
                }
            }
        }
        // Reserve/abort do not fabricate or erase an owner or data source.
        Ok(None)
    }
}

fn replacement_reserve<'a>(f: &'a TypedFunction, id: ReservationId, b: &mut Budget) -> Check<&'a Instruction> {
    let mut result = None;
    for block in &f.blocks {
        b.spend(block.instructions.len().saturating_add(1))?;
        for instruction in &block.instructions {
            if matches!(instruction.kind,
                InstructionKind::ReservePlaceLocal { reservation, .. }
                | InstructionKind::ReservePlaceAddress { reservation, .. } if reservation == id)
            {
                if result.replace(instruction).is_some() {
                    return Err(bad("replacement repeats its actual static reservation identity"));
                }
            }
        }
    }
    result.ok_or_else(|| bad("replacement references no actual static reservation"))
}
