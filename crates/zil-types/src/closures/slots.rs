//! A private single-element shell preserves the lifetime of a reassignable
//! owning callback position. Source expressions still denote the Closure.
use super::*;

impl Builder<'_, '_> {
    pub(crate) fn closure_slot(
        &mut self,
        name: &str,
        id: ClosureId,
        value: ValueId,
        span: Span,
    ) -> R<()> {
        let element = Type::Closure(id);
        self.check_owned_array_closure(&element, 1, span)?;
        let storage = Type::Array {
            element: Box::new(element),
            length: 1,
        };
        let value = self.value(
            InstructionKind::MakeOwnedArray {
                elements: vec![value],
            },
            storage.clone(),
            span,
        )?;
        let local = self.local(name, storage, true, span)?;
        self.store(local, value, span)?;
        self.closure_slots.insert(local, id);
        Ok(())
    }

    pub(crate) fn take_closure_slot(&mut self, local: LocalId, span: Span) -> R<(ValueId, Type)> {
        let ty = Type::Closure(self.closure_slots[&local]);
        let value = self.value(
            InstructionKind::TakeOwnedArrayElement { local, index: 0 },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(crate) fn borrow_closure_slot(
        &mut self,
        local: LocalId,
        mutable: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let ty = Type::Address {
            mutable,
            inner: Box::new(Type::Closure(self.closure_slots[&local])),
        };
        let value = self.value(
            InstructionKind::BorrowOwnedArrayElement {
                local,
                index: 0,
                mutable,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }

    pub(crate) fn assign_closure_slot(
        &mut self,
        local: LocalId,
        rhs: &CoreExpr,
        span: Span,
    ) -> R<()> {
        let element = Type::Closure(self.closure_slots[&local]);
        let (value, actual) = self.owned_array_input(rhs, Some(&element))?;
        self.same(&actual, &element, rhs.span)?;
        // Keep the prepared value responsible if evaluation exits early, and
        // retain the existing lexical shell instead of retiring a whole owner.
        let prepared = self.stage_argument(value, &element, rhs.span)?;
        self.emit(
            InstructionKind::DropOwnedArrayElement { local, index: 0 },
            None,
            span,
        )?;
        let value = self.commit_arguments(vec![prepared], rhs.span)?[0];
        self.emit(
            InstructionKind::StoreOwnedArrayElement {
                local,
                index: 0,
                value,
            },
            None,
            span,
        )?;
        Ok(())
    }
}
