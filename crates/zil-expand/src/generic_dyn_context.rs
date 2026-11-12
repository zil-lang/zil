//! Late dyn Context materialization in the actual source lifetime scope.
//! Compiler tokens point to complete catalog entries; generated nominal names
//! never recover a lifetime or an implementation witness.
use super::*;
use crate::stored_types::{self, QualifiedActual};

impl Engine<'_> {
    pub(super) fn prepare_owned_dyn_context(
        &mut self,
        ty: &mut TypeRef,
        depth: usize,
    ) -> R<Option<QualifiedActual>> {
        let TypeKind::Named { path, args } = &ty.kind else { return Ok(None); };
        if path.as_slice() != ["OwnedDyn"] || args.len() != 2 { return Ok(None); }
        let [GenericArg::Type(target), GenericArg::Type(context)] = args.as_slice() else {
            return Err(error(ty.span, "OwnedDyn requires a dyn interface and an actual Context type"));
        };
        self.budget.ty(target)?;
        self.budget.ty(context)?;
        let target = target.clone();
        let mut context = context.clone();
        let actual = self.qualified_type_actual(&mut context, depth + 1)?
            .ok_or_else(|| unsupported(ty.span, "dyn Context requires a complete qualified source application"))?;
        if !matches!(actual.ty, zil_ir::Type::BorrowedStorage { .. }) {
            return Err(unsupported(ty.span, "dyn Context requires an authenticated inline storage class shape"));
        }
        let (symbol, lowered) = self.products.dynamic.materialize_context(
            &self.traits, &target, actual.ty, &mut self.budget, ty.span,
        )?;
        *ty = named(&symbol, ty.span);
        Ok(Some(QualifiedActual { name: symbol, ty: lowered, classes: actual.classes }))
    }
}

pub(super) fn retained(
    context: &zil_ir::Type,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<zil_ir::RetainedLifeShape> {
    stored_types::pay_type(context, budget, span, 0)?;
    let zil_ir::Type::BorrowedStorage { storage, life } = context else {
        return Err(error(span, "dyn Context has no complete physical class shape"));
    };
    let mut ports = Vec::new();
    for slot in &life.slots {
        budget.node(span, 0)?;
        let mut ty = storage.as_ref();
        for step in &slot.path {
            budget.node(span, 0)?;
            if let zil_ir::Type::BorrowedStorage { storage, .. } = ty { ty = storage; }
            ty = match (ty, step) {
                (zil_ir::Type::Record { fields, .. } | zil_ir::Type::Package { fields, .. }, zil_ir::PlaceProjection::Field(index)) => fields.get(*index as usize).map(|(_, ty)| ty),
                (zil_ir::Type::Tuple(fields), zil_ir::PlaceProjection::Field(index)) => fields.get(*index as usize),
                (zil_ir::Type::Array { element, length }, zil_ir::PlaceProjection::Field(index)) if *index < *length => Some(element.as_ref()),
                (zil_ir::Type::Enum { variants, .. }, zil_ir::PlaceProjection::Variant(projection)) => variants.get(projection.variant as usize).and_then(|(_, fields)| fields.get(projection.index as usize)),
                (zil_ir::Type::ErrorUnion { error, result }, zil_ir::PlaceProjection::ErrorArm(error_arm)) => Some(if *error_arm { error } else { result }),
                _ => None,
            }.ok_or_else(|| error(span, "dyn Context slot crosses an absent physical field"))?;
        }
        stored_types::pay_type(ty, budget, span, 0)?;
        ports.push(zil_ir::RetainedBorrowPort { leaf: ty.clone(), mode: slot.mode, binder: slot.binder });
    }
    // These rows retain declaration occurrences and phantom classes. They
    // constrain type identity and never supply a target for an empty port.
    Ok(zil_ir::RetainedLifeShape {
        binder_count: life.binder_count, ports,
        retained: life.retained.clone(), nominal_arguments: life.nominal_arguments.clone(),
    })
}
