//! Sealed Option address-actual proof. Metadata never supplies a target root.
use super::*;

pub(super) fn declaration(d: &LifeNominalDeclaration) -> bool {
    d.id.compiler
        && d.id.module.0 == u32::MAX
        && d.id.declaration.0 == 0
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["option"]
        && d.identity.declaration_path == ["enum", "Option"]
        && d.identity.generation_path.is_empty()
        && d.kind == LifeNominalKind::Enum
        && d.parameters == [LifeParameterKind::Type]
}

pub(super) fn shape(
    ty: &Type,
    left: &mut usize,
    depth: usize,
    span: Span,
    unresolved_named: bool,
) -> R<bool> {
    let Type::BorrowedStorage { storage, life } = ty else {
        return Ok(false);
    };
    let Type::Enum { name, variants } = storage.as_ref() else {
        return Ok(false);
    };
    if life.slots.is_empty() {
        return Ok(false);
    }
    fee(left, 8, span)?;
    if name.is_empty()
        || variants.len() != 2
        || variants[0].0 != "None"
        || !variants[0].1.is_empty()
        || variants[1].0 != "Some"
        || variants[1].1.len() != 1
    {
        return Ok(false);
    }
    let Type::Address {
        mutable: false,
        inner,
    } = &variants[1].1[0]
    else {
        return Ok(false);
    };
    let pointee = match inner.as_ref() {
        Type::Slice { element } => element.as_ref(),
        other => other,
    };
    // Core inventory runs before nominal resolution. Only its authentic
    // compiler Option entry may defer a named pointee to resolved_inventory,
    // which repeats this proof on the actual structural type.
    let closed = if unresolved_named && matches!(pointee, Type::Named(name) if !name.is_empty()) {
        fee(left, 1, span)?;
        true
    } else {
        closed_copy(pointee, left, 0, depth, span)?
    };
    if !closed {
        return Ok(false);
    }
    Ok(life.binder_count == 1
        && life.nominal_arguments.is_empty()
        && life.slots.len() == 1
        && life.slots[0].path
            == [PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 1,
                index: 0,
            })]
        && life.slots[0].binder == 0
        && life.slots[0].mode == StoredBorrowMode::SharedAddress
        && life.retained.len() == 1
        && life.retained[0].path == [LifeTypeProjection::TypeArgument(0)]
        && life.retained[0].class == 0
        && life.retained[0].binder == 0)
}
