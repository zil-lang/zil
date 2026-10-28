//! Independent Product leaf provenance. Debt remains in the ordinary shell
//! and direct field slots; these lanes only preserve Created/Incoming through
//! complete actual values. They do not grant a deep Place or partial embedding.
use super::*;
use std::ops::Range;

#[derive(Default)]
pub(super) struct Plan {
    locals: Vec<Option<Shape>>,
    values: Vec<Option<Shape>>,
    width: usize,
}

struct Shape {
    whole: Range<usize>,
    fields: Vec<Range<usize>>,
}

// The nominal and bounded shape pass has already admitted every child.
// Charge each newly visited depth and the pending list before growth. Paths
// are represented by declaration-order intervals, not cloned Type trees.
fn leaves(ty: &Type, budget: &mut Budget) -> Check<usize> {
    budget.spend(2)?;
    let mut pending = vec![(ty, 1usize)];
    let mut width = 0usize;
    while let Some((ty, depth)) = pending.pop() {
        budget.spend(depth)?;
        match ty {
            Type::Managed(_) => {
                width = width
                    .checked_add(1)
                    .ok_or_else(|| limit("Product leaf overflow"))?;
            }
            Type::Package { fields, .. } => {
                budget.spend(fields.len().saturating_mul(2))?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(width)
}

impl Plan {
    pub(super) fn new(f: &TypedFunction, budget: &mut Budget) -> Check<Self> {
        // As with old has_array_fields/has_store_fields, classification only
        // borrows already bounded field lists. No leaf table exists for a flat
        // function. All new interval/table allocation is charged below.
        if !f
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(&f.values)
            .any(packages::has_children)
        {
            return Ok(Self::default());
        }
        budget.spend(f.locals.len().saturating_add(f.values.len()))?;
        let mut plan = Self::default();
        for local in &f.locals {
            let shape = plan.shape(&local.ty, budget)?;
            plan.locals.push(shape);
        }
        for ty in &f.values {
            let shape = plan.shape(ty, budget)?;
            plan.values.push(shape);
        }
        Ok(plan)
    }

    fn shape(&mut self, ty: &Type, budget: &mut Budget) -> Check<Option<Shape>> {
        let Type::Package { fields, .. } = ty else {
            return Ok(None);
        };
        budget.spend(fields.len().saturating_mul(2).saturating_add(2))?;
        let start = self.width;
        let mut ranges = Vec::with_capacity(fields.len());
        for (_, ty) in fields {
            let width = leaves(ty, budget)?;
            let end = self
                .width
                .checked_add(width)
                .ok_or_else(|| limit("Product state overflow"))?;
            // Prepay the new row's actual leaf width before retaining it.
            budget.spend(width)?;
            ranges.push(self.width..end);
            self.width = end;
        }
        Ok(Some(Shape {
            whole: start..self.width,
            fields: ranges,
        }))
    }

    pub(super) fn width(&self) -> usize {
        self.width
    }

    fn local(&self, local: LocalId) -> Option<&Shape> {
        self.locals.get(local.0 as usize).and_then(Option::as_ref)
    }

    fn value(&self, value: ValueId) -> Option<&Shape> {
        self.values.get(value.0 as usize).and_then(Option::as_ref)
    }

    fn copy(
        state: &mut State,
        from: Range<usize>,
        to: Range<usize>,
        budget: &mut Budget,
    ) -> Check<()> {
        // Exact type equality was checked independently before responsibility.
        // Each interval belongs to its actual SSA/local, even for equal types.
        if from.len() != to.len() {
            return Err(reject(
                "E-VERIFY-RESOURCE",
                None,
                "Product leaf shape differs at transfer",
            ));
        }
        budget.spend(from.len())?;
        state.product_fresh.copy_within(from, to.start);
        Ok(())
    }

    fn field_input(
        &self,
        f: &TypedFunction,
        slots: &Slots,
        value: ValueId,
        to: Range<usize>,
        state: &mut State,
        budget: &mut Budget,
    ) -> Check<()> {
        if let Some(source) = self.value(value) {
            Self::copy(state, source.whole.clone(), to, budget)
        } else if matches!(f.values[value.0 as usize], Type::Managed(_)) {
            budget.spend(1)?;
            let source = slots.managed_value[value.0 as usize].expect("checked Managed input");
            state.product_fresh[to.start] = state.fresh[source];
            Ok(())
        } else {
            debug_assert!(to.is_empty());
            Ok(())
        }
    }

    pub(super) fn apply(
        &self,
        f: &TypedFunction,
        slots: &Slots,
        instruction: &Instruction,
        state: &mut State,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(1)?;
        match &instruction.kind {
            InstructionKind::MakePackage { fields } => {
                let output = instruction.result.expect("checked Package result");
                let target = self.value(output).expect("actual Package lanes");
                budget.spend(fields.len())?;
                for (&value, range) in fields.iter().zip(&target.fields) {
                    self.field_input(f, slots, value, range.clone(), state, budget)?;
                }
            }
            InstructionKind::TakeLocal { local } => {
                if let Some(source) = self.local(*local) {
                    let output = instruction.result.expect("checked Package result");
                    let target = self.value(output).expect("actual Package lanes");
                    Self::copy(state, source.whole.clone(), target.whole.clone(), budget)?;
                }
            }
            InstructionKind::Store { local, value } => {
                if let Some(target) = self.local(*local) {
                    let source = self.value(*value).expect("checked complete Package source");
                    Self::copy(state, source.whole.clone(), target.whole.clone(), budget)?;
                }
            }
            InstructionKind::TakePackageField { local, index } => {
                let output = instruction.result.expect("checked owning field result");
                if let Some(target) = self.value(output) {
                    let source = self.local(*local).expect("actual parent lanes");
                    Self::copy(
                        state,
                        source.fields[*index as usize].clone(),
                        target.whole.clone(),
                        budget,
                    )?;
                }
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => {
                let target = self.local(*local).expect("actual parent lanes");
                self.field_input(
                    f,
                    slots,
                    *value,
                    target.fields[*index as usize].clone(),
                    state,
                    budget,
                )?;
            }
            InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                if let Some(target) = self.local(*local) {
                    budget.spend(target.whole.len())?;
                    state.product_fresh[target.whole.clone()].fill(INPUT);
                }
            }
            _ => {
                // Unknown ordinary Package call results, old union/array
                // extraction and capture transfer carry no leaf create contract.
                if let Some(target) = instruction.result.and_then(|v| self.value(v)) {
                    budget.spend(target.whole.len())?;
                    state.product_fresh[target.whole.clone()].fill(INPUT);
                }
            }
        }
        // Bridge exact direct Managed leaves back to the existing flat SSA
        // table. A taken flat child must retain its real leaves when the next
        // Store creates its ordinary local field debts.
        if let Some(output) = instruction.result
            && let Some(shape) = self.value(output)
            && let Type::Package { fields, .. } = &f.values[output.0 as usize]
        {
            budget.spend(fields.len())?;
            for (index, (_, ty)) in fields.iter().enumerate() {
                if matches!(ty, Type::Managed(_)) {
                    state.package_fresh[output.0 as usize][index] =
                        state.product_fresh[shape.fields[index].start];
                }
            }
        }
        Ok(())
    }
}
