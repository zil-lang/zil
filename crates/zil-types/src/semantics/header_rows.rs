//! Complete signature occurrences from an already interpreted whole type.
//! These rows propose no permission, constructor or class equality.
use crate::*;
#[path = "header_task_rows.rs"]
mod tasks;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-HEADER-ROWS", Span::default(), message)
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "whole header occurrence work exceeded", None)
    })?;
    Ok(())
}
fn path(left: &mut usize, limits: &Limits, n: usize) -> R<()> {
    pay(left, n.saturating_add(1))?;
    if n > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "whole header occurrence depth exceeded",
            None,
        ));
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(in crate::semantics) fn append(
    value: &SemanticTypeValue,
    base: StorageBase,
    bindings: &mut Vec<(StoragePosition, SemanticClassExpr)>,
    classes_out: &mut Vec<(SignatureLifeClass, SemanticClassExpr)>,
    statics: &mut Vec<StoragePosition>,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let SemanticTypeValue::Resolved { ty, life, classes } = value else {
        return Err(bad("header retains an unresolved whole type"));
    };
    if life.binder_count as usize != classes.len() {
        return Err(bad("header omits a whole-Type intrinsic class"));
    }
    let count = life
        .slots
        .len()
        .saturating_add(life.retained.len())
        .saturating_add(classes.len());
    if count > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "whole header occurrence node limit exceeded",
            None,
        ));
    }
    pay(left, count.saturating_add(1))?;
    pay(left, classes.len().saturating_mul(2))?;
    let mut covered = vec![false; classes.len()];
    pay(left, life.retained.len())?;
    let mut expanded = vec![false; life.retained.len()];
    for slot in &life.slots {
        path(left, limits, slot.path.len())?;
        let image = classes
            .get(slot.binder as usize)
            .ok_or_else(|| bad("header physical slot exceeds its entire domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
        covered[slot.binder as usize] = true;
        bindings.push((
            StoragePosition {
                base,
                path: slot
                    .path
                    .iter()
                    .copied()
                    .map(ContractProjection::Place)
                    .collect(),
            },
            image.clone(),
        ));
    }
    let task_root = tasks::append(
        ty,
        life,
        classes,
        base,
        bindings,
        classes_out,
        &mut covered,
        &mut expanded,
        limits,
        left,
    )?;
    let opaque = matches!(
        ty,
        Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::MutexGuard { .. }
            | Type::Task { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
    );
    if opaque && !task_root {
        for (class, image) in classes.iter().enumerate() {
            crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
            covered[class] = true;
            classes_out.push((
                SignatureLifeClass {
                    base,
                    path: Vec::new(),
                    class: class as u32,
                },
                image.clone(),
            ));
        }
    } else if !opaque {
        for (index, row) in life.retained.iter().enumerate() {
            if expanded[index] {
                continue;
            }
            path(left, limits, row.path.len())?;
            let image = classes
                .get(row.binder as usize)
                .ok_or_else(|| bad("header retained child exceeds its entire domain"))?;
            crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
            covered[row.binder as usize] = true;
            classes_out.push((
                SignatureLifeClass {
                    base,
                    path: row.path.clone(),
                    class: row.class,
                },
                image.clone(),
            ));
        }
    }
    physical_statics(ty, life, base, statics, limits, left)?;
    if covered.iter().any(|covered| !*covered) {
        return Err(bad(
            "header whole type has an unrepresented intrinsic class occurrence",
        ));
    }
    Ok(())
}

// Resolved here means evaluated in the complete original Gamma, not an
// arbitrary physical hint. Static contributes no variable slot, but remains
// a real physical borrow occurrence even inside an inline product or sum.
fn physical_statics(
    ty: &Type, life: &StorageLifeShape, base: StorageBase,
    out: &mut Vec<StoragePosition>, limits: &Limits, left: &mut usize,
) -> R<()> {
    struct Walk<'a, 'b> {
        life: &'a StorageLifeShape,
        seen: Vec<bool>,
        base: StorageBase,
        out: &'b mut Vec<StoragePosition>,
        limits: &'b Limits,
        left: &'b mut usize,
        nodes: usize,
    }
    impl Walk<'_, '_> {
        fn leaf(&mut self, ty: &Type, prefix: &[PlaceProjection]) -> R<()> {
            pay(self.left, self.life.slots.len().saturating_mul(prefix.len().saturating_add(1)))?;
            let mut selected = None;
            for (ordinal, slot) in self.life.slots.iter().enumerate() {
                if slot.path.as_slice() != prefix { continue; }
                if selected.replace(ordinal).is_some() {
                    return Err(bad("physical header leaf has multiple variable slots"));
                }
                let valid = matches!((ty, slot.mode),
                    (Type::Address { mutable: false, .. }, StoredBorrowMode::SharedAddress)
                    | (Type::Address { mutable: true, .. }, StoredBorrowMode::ExclusiveAddress)
                    | (Type::Desc(_), StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription));
                if !valid { return Err(bad("physical header slot changes its actual borrow mode")); }
            }
            if let Some(ordinal) = selected {
                self.seen[ordinal] = true;
                return Ok(());
            }
            if !matches!(ty, Type::Address { mutable: false, .. }) {
                return Err(bad("non-shared header leaf has no complete variable occurrence"));
            }
            path(self.left, self.limits, prefix.len())?;
            self.out.push(StoragePosition {
                base: self.base,
                path: prefix.iter().copied().map(ContractProjection::Place).collect(),
            });
            Ok(())
        }
        fn visit(&mut self, ty: &Type, prefix: &mut Vec<PlaceProjection>, depth: usize) -> R<()> {
            path(self.left, self.limits, depth)?;
            if self.nodes >= self.limits.max_nodes {
                return Err(Diagnostic::limit(Phase::Types, "physical header occurrence node limit exceeded", None));
            }
            self.nodes += 1;
            match ty {
                // This is one physical leaf. Its pointee is metadata, not a
                // second inline borrow occurrence or permission to dereference.
                Type::Address { .. } | Type::Desc(_) => self.leaf(ty, prefix),
                Type::BorrowedStorage { storage, .. } => self.visit(storage, prefix, depth + 1),
                Type::Tuple(fields) => {
                    pay(self.left, fields.len())?;
                    for (ordinal, field) in fields.iter().enumerate() {
                        let ordinal = u32::try_from(ordinal).map_err(|_| bad("header Tuple field ordinal overflows"))?;
                        prefix.push(PlaceProjection::Field(ordinal));
                        self.visit(field, prefix, depth + 1)?;
                        prefix.pop();
                    }
                    Ok(())
                }
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    pay(self.left, fields.len())?;
                    for (ordinal, (_, field)) in fields.iter().enumerate() {
                        let ordinal = u32::try_from(ordinal).map_err(|_| bad("header Product field ordinal overflows"))?;
                        prefix.push(PlaceProjection::Field(ordinal));
                        self.visit(field, prefix, depth + 1)?;
                        prefix.pop();
                    }
                    Ok(())
                }
                Type::Array { element, length } => {
                    if *length as usize > self.limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "physical header array length exceeded", None));
                    }
                    pay(self.left, *length as usize)?;
                    for ordinal in 0..*length {
                        prefix.push(PlaceProjection::Field(ordinal));
                        self.visit(element, prefix, depth + 1)?;
                        prefix.pop();
                    }
                    Ok(())
                }
                Type::Enum { variants, .. } => {
                    pay(self.left, variants.len())?;
                    for (variant, (_, fields)) in variants.iter().enumerate() {
                        let variant = u32::try_from(variant).map_err(|_| bad("header Enum variant ordinal overflows"))?;
                        pay(self.left, fields.len())?;
                        for (ordinal, field) in fields.iter().enumerate() {
                            let index = u32::try_from(ordinal).map_err(|_| bad("header Enum field ordinal overflows"))?;
                            prefix.push(PlaceProjection::Variant(VariantProjection { variant, index }));
                            self.visit(field, prefix, depth + 1)?;
                            prefix.pop();
                        }
                    }
                    Ok(())
                }
                Type::ErrorUnion { error, result } => {
                    for (arm, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                        prefix.push(PlaceProjection::ErrorArm(arm));
                        self.visit(ty, prefix, depth + 1)?;
                        prefix.pop();
                    }
                    Ok(())
                }
                // Managed/opaque storage and code signatures are not physical
                // inline fields. Their complete child class/port/context and
                // Static source obligations retain their separate consumers.
                _ => Ok(()),
            }
        }
    }
    pay(left, life.slots.len().saturating_add(1))?;
    let mut walk = Walk { life, seen: vec![false; life.slots.len()], base, out, limits, left, nodes: 0 };
    walk.visit(ty, &mut Vec::new(), 0)?;
    if walk.seen.iter().any(|seen| !seen) {
        return Err(bad("header variable slot is not an actual physical borrow leaf"));
    }
    Ok(())
}

#[cfg(test)]
mod static_physical_tests {
    use super::*;
    #[test]
    fn compound_statics_remain_distinct_from_variable_and_exclusive_slots() {
        let leaf = Type::Address { mutable: false,
            inner: Box::new(Type::Slice { element: Box::new(Type::U8) }) };
        let value = SemanticTypeValue::Resolved {
            ty: Type::Tuple(vec![leaf.clone(), leaf]),
            life: StorageLifeShape { binder_count: 1,
                slots: vec![StorageLifeSlot { path: vec![PlaceProjection::Field(1)],
                    binder: 0, mode: StoredBorrowMode::SharedAddress }],
                retained: Vec::new(), nominal_arguments: Vec::new() },
            classes: vec![SemanticClassExpr::Implicit { scope: SemanticScopeRef::Ancestor(0), ordinal: 0 }],
        };
        let limits = Limits::default(); let mut left = limits.max_analysis_steps;
        let mut bindings = Vec::new(); let mut classes = Vec::new(); let mut statics = Vec::new();
        append(&value, StorageBase::Capture(0), &mut bindings, &mut classes,
            &mut statics, &limits, &mut left).unwrap();
        assert_eq!(statics, vec![StoragePosition { base: StorageBase::Capture(0),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))] }]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0.path, vec![ContractProjection::Place(PlaceProjection::Field(1))]);
        let mut orphan = value.clone();
        let SemanticTypeValue::Resolved { life, .. } = &mut orphan else { unreachable!() };
        life.slots[0].path = vec![PlaceProjection::Field(2)];
        assert!(append(&orphan, StorageBase::Capture(0), &mut Vec::new(), &mut Vec::new(),
            &mut Vec::new(), &limits, &mut left).is_err());
        let exclusive = SemanticTypeValue::Resolved {
            ty: Type::Tuple(vec![Type::Address { mutable: true, inner: Box::new(Type::I32) }]),
            life: StorageLifeShape { binder_count: 0, slots: Vec::new(), retained: Vec::new(), nominal_arguments: Vec::new() },
            classes: Vec::new(),
        };
        assert!(append(&exclusive, StorageBase::Capture(0), &mut Vec::new(), &mut Vec::new(),
            &mut Vec::new(), &limits, &mut left).is_err());
    }
}
