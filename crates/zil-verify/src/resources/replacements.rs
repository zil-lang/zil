//! Actual Commit transfers one staged responsibility into an already live
//! owning position. The old destination has no second invented local debt.
use super::*;
use crate::place_replacements::{Inventory, actual};
pub(super) struct Plan {
    actual: actual::Program,
    inventories: Vec<Inventory>,
    written: Vec<BTreeSet<Root>>,
}
impl Plan {
    pub(super) fn new(
        module: &TypedModule,
        graphs: &[FunctionGraph],
        writes: &[BTreeSet<Root>],
        b: &mut Budget,
    ) -> Check<Self> {
        let actual = actual::Program::reconstruct(module, graphs, b)?;
        b.spend(module.functions.len().saturating_add(writes.len()))?;
        let mut inventories = Vec::with_capacity(module.functions.len());
        for f in &module.functions {
            inventories.push(Inventory::reconstruct(module, f, b)?);
        }
        for row in writes {
            b.spend(row.len())?;
        }
        Ok(Self {
            actual,
            inventories,
            written: writes.to_vec(),
        })
    }
    pub(super) fn extend_slots(
        &self,
        f: &TypedFunction,
        slots: &mut Slots,
        b: &mut Budget,
    ) -> Check<()> {
        let routes = &self.actual.functions[f.id.0 as usize].values;
        for (value, row) in routes.iter().enumerate() {
            b.spend(row.len().saturating_add(1))?;
            let ty = &f.values[value];
            if slots.desc_value[value].is_none()
                && (head_address(ty) || ty.is_package_address())
                && row
                    .iter()
                    .any(|target| matches!(target.base, actual::Base::Element(_)))
            {
                slots.desc_value[value] = Some(slots.desc_count);
                slots.desc_count = slots
                    .desc_count
                    .checked_add(1)
                    .ok_or_else(|| limit("Store address description slot count overflow"))?;
            }
        }
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if let InstructionKind::Store { local, value } = i.kind {
                    let row = &routes[value.0 as usize];
                    b.spend(row.len())?;
                    let ty = &f.locals[local.0 as usize].ty;
                    if slots.desc_local[local.0 as usize].is_none()
                        && (head_address(ty) || ty.is_package_address())
                        && row
                            .iter()
                            .any(|target| matches!(target.base, actual::Base::Element(_)))
                    {
                        slots.desc_local[local.0 as usize] = Some(slots.desc_count);
                        slots.desc_count = slots
                            .desc_count
                            .checked_add(1)
                            .ok_or_else(|| limit("Store local description slot count overflow"))?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn writes(&self, id: FunctionId, b: &mut Budget) -> Check<BTreeSet<Root>> {
        let row = &self.written[id.0 as usize];
        b.spend(row.len())?;
        Ok(row.clone())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn step(
        &self,
        transfer: &Transfer<'_>,
        state: &mut State,
        instruction: &Instruction,
        position: usize,
        block: usize,
        validate: bool,
        written: &mut BTreeSet<Root>,
        b: &mut Budget,
    ) -> Check<()> {
        let InstructionKind::CommitPlaceReplace { reservation, value } = instruction.kind else {
            return Ok(());
        };
        let f = transfer.f;
        let span = instruction.span;
        let place = self.inventories[f.id.0 as usize].get(reservation, b)?;
        let source = transfer.slots.managed_value[value.0 as usize].ok_or_else(|| {
            bad(
                span,
                "replacement RHS has no actual resource responsibility",
            )
        })?;
        if validate && state.debt[source] != LIVE {
            return Err(bad(
                span,
                "replacement RHS is absent or was consumed before Commit",
            ));
        }
        let fresh = state.fresh[source];
        for target in self.actual.selected(f, place, b)? {
            sums::replacements::at_type(transfer.module, f, &target, b)?;
            match target.base {
                actual::Base::Local(local) => {
                    let shell =
                        transfer.slots.managed_local[local.0 as usize].ok_or_else(|| {
                            bad(span, "replacement root has no actual owner responsibility")
                        })?;
                    if validate && state.debt[shell] != LIVE {
                        return Err(bad(span, "replacement owner shell is absent or moved"));
                    }
                    if let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty
                        && let Some(zil_ir::PlaceProjection::Field(index)) = target.steps.first()
                        && matches!(
                            fields[*index as usize].1,
                            Type::Managed(_) | Type::Package { .. }
                        )
                    {
                        let field =
                            transfer.slots.fields(local)[*index as usize].ok_or_else(|| {
                                bad(
                                    span,
                                    "replacement owning field has no original responsibility",
                                )
                            })?;
                        if validate && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "replacement owning field was already moved or cleaned",
                            ));
                        }
                        if validate {
                            no_live_description(
                                state,
                                &transfer.after[block][position],
                                Root::PackageField {
                                    local,
                                    index: *index,
                                },
                                span,
                                b,
                            )?;
                        }
                        if target.steps.len() == 1 {
                            state.fresh[field] = fresh;
                        }
                    } else if target.steps.is_empty() {
                        if validate {
                            no_live_description(
                                state,
                                &transfer.after[block][position],
                                Root::Local(local),
                                span,
                                b,
                            )?;
                        }
                        state.fresh[shell] = fresh;
                    }
                    // Preserve shell debt, initialization, partial siblings
                    // and lexical cleanup age; the old value is replaced once.
                }
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    b.spend(written.len().saturating_add(1))?;
                    written.insert(Root::Parameter(p));
                }
                actual::Base::Environment(c) => {
                    if f.environment.is_some_and(|id| {
                        transfer.module.closures[id.0 as usize].kind == CallableKind::Fn
                    }) {
                        return Err(bad(
                            span,
                            "shared Fn entry cannot replace through a captured exclusive address",
                        ));
                    }
                    b.spend(written.len().saturating_add(1))?;
                    written.insert(Root::BorrowedCapture(c));
                }
                actual::Base::Element(element) => {
                    let parent =
                        transfer.slots.desc_value[element.0 as usize].ok_or_else(|| {
                            bad(
                                span,
                                "Store element has no actual parent responsibility origin",
                            )
                        })?;
                    b.spend(state.desc[parent].len())?;
                    if state.desc[parent].iter().any(|root| {
                        matches!(
                            root.parent(),
                            Root::Parameter(_) | Root::OwnedCapture(_) | Root::BorrowedCapture(_)
                        )
                    }) {
                        return Err(unsupported(
                            "Store replacement through a borrowed/captured parent needs its instance invalidation summary",
                            Some(span),
                        ));
                    }
                    if validate {
                        description_live(state, transfer.slots, parent, span, b)?;
                        // Both kernels retain a Store-carrier qualifier only
                        // for actual Store Desc production. Element extraction
                        // strips it; a same-family resource Desc blocks Commit.
                        for (ty, slot) in f.values.iter().zip(&transfer.slots.desc_value).chain(
                            f.locals
                                .iter()
                                .map(|local| &local.ty)
                                .zip(&transfer.slots.desc_local),
                        ) {
                            b.spend(1)?;
                            if !matches!(ty, Type::Desc(_) | Type::Closure(_)) {
                                continue;
                            }
                            let Some(slot) = *slot else {
                                continue;
                            };
                            if !transfer.after[block][position][slot] {
                                continue;
                            }
                            b.spend(
                                state.desc[slot]
                                    .len()
                                    .saturating_mul(state.desc[parent].len().saturating_add(1)),
                            )?;
                            if state.desc[slot].iter().any(|root| {
                                !matches!(root, Root::StoreParent(_))
                                    && state.desc[parent]
                                        .iter()
                                        .any(|actual| root.parent() == actual.parent())
                            }) {
                                return Err(bad(
                                    span,
                                    "Store field replacement invalidates a live element resource description",
                                ));
                            }
                        }
                    }
                    transfer.write_roots(&state.desc[parent], written, span, validate, b)?;
                    // Store retains exactly its original complete initialized
                    // prefix debt. No field debt or fresh permission is added.
                }
            }
        }
        transfer.consume(state, value, span, validate)
    }
}

#[cfg(test)]
mod tests;
