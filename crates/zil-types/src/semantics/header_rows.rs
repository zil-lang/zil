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
/// Expand preserves the source's one class mapping per qualified boundary.
/// Standalone Core lacks an original semantic program to materialize its
/// Typed headers. Derive each transferred physical slot at this boundary.
pub(in crate::semantics) fn complete_standalone(module: &mut TypedModule, limits: &Limits, left: &mut usize) -> R<()> {
    fn append(
        ty: &Type, base: StorageBase, prefix: &mut Vec<PlaceProjection>,
        contract: &mut StorageLifeContract, limits: &Limits, left: &mut usize,
    ) -> R<()> {
        path(left, limits, prefix.len())?;
        match ty {
            Type::Tuple(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    prefix.push(PlaceProjection::Field(index as u32));
                    append(field, base, prefix, contract, limits, left)?;
                    prefix.pop();
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, field) in [(true, error.as_ref()), (false, result.as_ref())] {
                    prefix.push(PlaceProjection::ErrorArm(arm));
                    append(field, base, prefix, contract, limits, left)?;
                    prefix.pop();
                }
            }
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    pay(left, contract.classes.len().saturating_mul(prefix.len().saturating_add(1)))?;
                    let mut images = contract.classes.iter().filter(|row| {
                        row.position.base == base && row.position.class == slot.binder
                            && row.position.path.len() == prefix.len()
                            && row.position.path.iter().zip(prefix.iter()).all(|(step, actual)| {
                                matches!(step, LifeTypeProjection::Place(part) if part == actual)
                            })
                    });
                    let image = images.next().ok_or_else(|| bad("standalone slot lacks its source class binding"))?.binder;
                    if images.next().is_some() || slot.binder >= life.binder_count {
                        return Err(bad("standalone slot has an ambiguous or missing class"));
                    }
                    path(left, limits, prefix.len().saturating_add(slot.path.len()))?;
                    let position = StoragePosition {
                        base,
                        path: prefix.iter().chain(&slot.path).copied().map(ContractProjection::Place).collect(),
                    };
                    pay(left, contract.bindings.len().saturating_mul(position.path.len().saturating_add(2)))?;
                    // Physical slots follow the same traversal as the Typed
                    // header consumer. Error arms are visited before success.
                    let order = |other: &StoragePosition| {
                        other.base.cmp(&base).then_with(|| {
                            for (a, z) in other.path.iter().zip(&position.path) {
                                let cmp = match (a, z) {
                                    (ContractProjection::Place(PlaceProjection::ErrorArm(a)),
                                     ContractProjection::Place(PlaceProjection::ErrorArm(z))) => z.cmp(a),
                                    _ => a.cmp(z),
                                };
                                if !cmp.is_eq() { return cmp; }
                            }
                            other.path.len().cmp(&position.path.len())
                        })
                    };
                    let index = contract.bindings.iter().position(|row| !order(&row.position).is_lt())
                        .unwrap_or(contract.bindings.len());
                    if let Some(row) = contract.bindings.get(index) && row.position == position {
                        if row.binder != image { return Err(bad("standalone physical slot differs from its class")); }
                    } else {
                        if contract.bindings.len() >= limits.max_nodes {
                            return Err(Diagnostic::limit(Phase::Types, "standalone header occurrence limit exceeded", None));
                        }
                        contract.bindings.insert(index, StorageLifeBinding { position, binder: image });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    for function in &mut module.functions {
        pay(left, 1)?;
        let Some(header) = &mut function.storage_life else { continue };
        if header.classes.is_empty() || function.environment.is_some() { continue; }
        for (index, local) in function.parameters.iter().enumerate() {
            let ty = &function.locals.get(local.0 as usize)
                .filter(|decl| decl.id == *local).ok_or_else(|| bad("standalone header input has no actual local"))?.ty;
            append(ty, StorageBase::Parameter(index as u32), &mut Vec::new(), header, limits, left)?;
        }
        append(&function.return_type, StorageBase::Result, &mut Vec::new(), header, limits, left)?;
    }
    Ok(())
}
/// A reference to a qualified pointee exports one map of its intrinsic
/// classes. Descendant occurrences still belong to the pointee's own shape.
fn qualified_pointee_rows(
    ty: &Type, outer: &StorageLifeShape, expanded: &mut [bool], limits: &Limits, left: &mut usize,
) -> R<()> {
    let mut child_rows = |ty: &Type, projection| {
        if let Type::BorrowedStorage { life, .. } = ty {
            qualified_child_rows(life, projection, outer, expanded, limits, left)?;
        }
        Ok(())
    };
    match ty {
        Type::Address { inner, .. } | Type::Desc(inner) =>
            child_rows(inner, LifeTypeProjection::Pointee),
        Type::ErrorUnion { error, result } => {
            child_rows(error, LifeTypeProjection::Place(PlaceProjection::ErrorArm(true)))?;
            child_rows(result, LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)))
        }
        _ => Ok(()),
    }
}
fn qualified_child_rows(
    child: &StorageLifeShape, prefix: LifeTypeProjection, outer: &StorageLifeShape,
    expanded: &mut [bool], limits: &Limits, left: &mut usize,
) -> R<()> {
    let width = child.binder_count as usize;
    if width > limits.max_nodes { return Err(Diagnostic::limit(Phase::Types, "qualified pointee class limit exceeded", None)); }
    pay(left, width.saturating_add(outer.retained.len()))?;
    let mut map = vec![None; width];
    for row in &outer.retained {
        if row.path.as_slice() == [prefix] {
            let binder = map.get_mut(row.class as usize)
                .ok_or_else(|| bad("qualified pointee header exceeds its class domain"))?;
            if row.binder >= outer.binder_count || binder.replace(row.binder).is_some() {
                return Err(bad("qualified pointee header repeats or loses a class"));
            }
        }
    }
    if map.iter().any(Option::is_none) { return Err(bad("qualified pointee header omits a class")); }
    let mut descendants = std::collections::BTreeMap::new();
    for row in &child.retained {
        path(left, limits, row.path.len())?;
        pay(left, (row.path.len() + 1).saturating_mul((descendants.len() + 1).ilog2() as usize + 1))?;
        if descendants.insert((row.path.as_slice(), row.class), row.binder).is_some() {
            return Err(bad("qualified pointee repeats a descendant occurrence"));
        }
    }
    for (row, omit) in outer.retained.iter().zip(expanded) {
        let Some((parent, rest)) = row.path.split_first() else { continue };
        if *parent != prefix || rest.is_empty() { continue; }
        path(left, limits, rest.len())?;
        pay(left, (rest.len() + 1).saturating_mul((descendants.len() + 1).ilog2() as usize + 1))?;
        let child_class = descendants.get(&(rest, row.class))
            .ok_or_else(|| bad("qualified pointee header adds an unknown descendant"))?;
        if map.get(*child_class as usize).copied().flatten() != Some(row.binder) {
            return Err(bad("qualified pointee descendant changes its class embedding"));
        }
        *omit = true;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantics) fn append(
    value: &SemanticTypeValue, base: StorageBase,
    bindings: &mut Vec<(StoragePosition, SemanticClassExpr)>,
    classes_out: &mut Vec<(SignatureLifeClass, SemanticClassExpr)>,
    statics: &mut Vec<StoragePosition>, limits: &Limits, left: &mut usize,
) -> R<()> {
    let SemanticTypeValue::Resolved { classes, .. } = value else { return Err(bad("header retains an unresolved whole type")); };
    let mut physical = Vec::new();
    let mut intrinsic = Vec::new();
    append_ordinals(value, base, &mut physical, &mut intrinsic, statics, limits, left)?;
    pay(left, physical.len().saturating_add(intrinsic.len()))?;
    for (position, ordinal) in physical {
        let image = classes.get(ordinal as usize).ok_or_else(|| bad("header ordinal exceeds its complete class domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
        bindings.push((position, image.clone()));
    }
    for (position, ordinal) in intrinsic {
        let image = classes.get(ordinal as usize).ok_or_else(|| bad("header intrinsic ordinal exceeds its complete class domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, limits, left, 1)?;
        classes_out.push((position, image.clone()));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::semantics) fn append_ordinals(
    value: &SemanticTypeValue,
    base: StorageBase,
    bindings: &mut Vec<(StoragePosition, u32)>,
    classes_out: &mut Vec<(SignatureLifeClass, u32)>,
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
    pay(left, classes.len())?;
    let indices = (0..life.binder_count).collect::<Vec<_>>();
    let classes = indices.as_slice();
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
        pay(left, 1)?;
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
    qualified_pointee_rows(ty, life, &mut expanded, limits, left)?;
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
            pay(left, 1)?;
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
            pay(left, 1)?;
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
