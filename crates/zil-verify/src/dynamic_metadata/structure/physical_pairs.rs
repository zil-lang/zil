//! Complete physical equality of two immutable allocations, scoped to one
//! verification and its original bounds. Logical domains are checked by callers.
use super::*;
use std::rc::Rc;

pub(super) struct Pairs {
    limits: Limits,
    representatives: BTreeMap<u64, TypeCell>,
    // Holding both allocations prevents key reuse during this verification.
    entries: BTreeMap<(usize, usize), (TypeCell, TypeCell, bool)>,
}

impl Pairs {
    pub(super) fn new(limits: &Limits) -> Self {
        Self {
            limits: limits.clone(),
            representatives: BTreeMap::new(),
            entries: BTreeMap::new(),
        }
    }
}

pub(super) enum Representative {
    Existing(TypeCell),
    New(Option<u64>),
}

/// Only registered input nodes reach this lookup. A hash selects a candidate.
/// Its complete physical type is compared, including names and field order.
pub(super) fn representative(ty: &Type, b: &mut Budget) -> Check<Representative> {
    use std::hash::{Hash, Hasher};
    let Some(state) = &b.structural else { return Ok(Representative::New(None)); };
    if state.physical_pairs.limits != b.limits || !matches!(ty,
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } | Type::Address { .. }
        | Type::Managed(_) | Type::OwnedStore { .. } | Type::ErrorUnion { .. } | Type::Tuple(_))
    {
        return Ok(Representative::New(None));
    }
    super::super::bounds_type(ty, b)?;
    let payload = match ty { Type::Address { inner, .. } => inner.as_ref(), _ => ty };
    if !crate::semantic::closed_storage(payload, b)? { return Ok(Representative::New(None)); }

    super::super::ty(ty, b)?;
    let mut hasher = std::hash::DefaultHasher::new();
    ty.hash(&mut hasher);
    let hash = hasher.finish();
    let fee = lookup(b.structural.as_ref().unwrap().physical_pairs.representatives.len());
    b.spend(fee)?;
    let known = b.structural.as_ref().unwrap().physical_pairs.representatives.get(&hash).cloned();
    let Some(known) = known else { return Ok(Representative::New(Some(hash))); };
    super::super::ty(ty, b)?;
    super::super::ty(&known.ty, b)?;
    if known.ty == *ty {
        Ok(Representative::Existing(known))
    } else {
        Ok(Representative::New(None))
    }
}

pub(super) fn publish_representative(hash: Option<u64>, cell: &TypeCell, b: &mut Budget) -> Check<()> {
    if let Some(hash) = hash {
        let fee = lookup(b.structural.as_ref().unwrap().physical_pairs.representatives.len());
        b.spend(fee.saturating_add(2))?;
        b.structural.as_mut().unwrap().physical_pairs.representatives.insert(hash, cell.clone());
    }
    Ok(())
}

pub(crate) fn compare_type_cells(left: &TypeCell, right: &TypeCell, b: &mut Budget) -> Check<bool> {
    // These physical atoms contain only a tag and at most one fixed-width
    // kind or table identity. Every occurrence still owns its separate domain.
    if !super::cacheable(&left.ty) && !super::cacheable(&right.ty) {
        if b.limits.max_nodes == 0 {
            return Err(crate::limit("dyn metadata node count exceeded"));
        }
        b.spend(2)?;
        return Ok(left.ty == right.ty);
    }
    if std::mem::discriminant(&left.ty) != std::mem::discriminant(&right.ty) {
        // Equality stops at the constructor. Both complete input bounds are
        // still checked, including under a tighter profile or a fresh scope.
        super::super::bounds_type(&left.ty, b)?;
        super::super::bounds_type(&right.ty, b)?;
        b.spend(1)?;
        return Ok(false);
    }
    let Some(state) = &b.structural else {
        return compare(left, right, b);
    };
    if state.physical_pairs.limits != b.limits {
        return compare(left, right, b);
    }
    let pair = (Rc::as_ptr(left) as usize, Rc::as_ptr(right) as usize);
    let key = if pair.0 < pair.1 {
        pair
    } else {
        (pair.1, pair.0)
    };
    let fee = lookup(state.physical_pairs.entries.len());
    b.spend(fee)?;
    if let Some((_, _, equal)) = b
        .structural
        .as_ref()
        .unwrap()
        .physical_pairs
        .entries
        .get(&key)
    {
        return Ok(*equal);
    }
    let equal = compare(left, right, b)?;
    b.spend(fee.saturating_add(3))?;
    b.structural
        .as_mut()
        .unwrap()
        .physical_pairs
        .entries
        .insert(key, (left.clone(), right.clone(), equal));
    Ok(equal)
}

fn compare(left: &TypeCell, right: &TypeCell, b: &mut Budget) -> Check<bool> {
    super::super::ty(&left.ty, b)?;
    super::super::ty(&right.ty, b)?;
    Ok(left.ty == right.ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_inputs_share_only_their_complete_closed_physical_type() {
        let limits = Limits::default();
        let (mut module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../semantic/fixtures/ordinary-identity-complete-head.wire"), &limits,
        ).unwrap();
        let record = |name: &str, fields: [(&str, Type); 2]| Type::Record {
            name: name.into(), fields: fields.into_iter().map(|(name, ty)| (name.into(), ty)).collect(),
        };
        let declared = record("Point", [("x", Type::I32), ("y", Type::I32)]);
        let values = &mut module.functions[0].values;
        let start = values.len();
        values.extend([
            declared.clone(), declared.clone(),
            record("Other", [("x", Type::I32), ("y", Type::I32)]),
            record("Point", [("y", Type::I32), ("x", Type::I32)]),
        ]);
        let mut b = Budget::new(&limits);
        b.structural = Some(Structure::register(&module, &mut b).unwrap());
        let values = &module.functions[0].values[start..];
        let cell = |ty: &Type, b: &mut Budget| super::super::shared_type(ty, b).unwrap().unwrap();
        let first = cell(&values[0], &mut b);
        assert!(Rc::ptr_eq(&first, &cell(&values[1], &mut b)));
        assert!(!Rc::ptr_eq(&first, &cell(&values[2], &mut b)));
        assert!(!Rc::ptr_eq(&first, &cell(&values[3], &mut b)));
        assert!(super::super::shared_type(&declared, &mut b).unwrap().is_none());
        b.limits.max_nodes = 1;
        assert_eq!(super::super::shared_type(&values[1], &mut b).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit);
    }

    #[test]
    fn both_complete_types_and_the_current_bounds_are_required() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../semantic/fixtures/ordinary-identity-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let mut b = Budget::new(&limits);
        b.structural = Some(Structure::register(&module, &mut b).unwrap());
        let cell = |ty| crate::semantic::CheckedTypeData::new(Type::Tuple(vec![Type::Bool, ty]));
        let a = cell(Type::I32);
        let same = cell(Type::I32);
        let different = cell(Type::U32);
        assert!(compare_type_cells(&a, &same, &mut b).unwrap());
        assert!(compare_type_cells(&same, &a, &mut b).unwrap());
        assert!(!compare_type_cells(&different, &a, &mut b).unwrap());
        assert_eq!(
            b.structural.as_ref().unwrap().physical_pairs.entries.len(),
            2
        );
        b.limits.max_nodes = 1;
        assert_eq!(
            compare_type_cells(&a, &same, &mut b).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        let closure = crate::semantic::CheckedTypeData::new(Type::Closure(ClosureId(0)));
        assert_eq!(
            compare_type_cells(&closure, &a, &mut b)
                .unwrap_err()
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        let other_closure = crate::semantic::CheckedTypeData::new(Type::Closure(ClosureId(1)));
        assert!(!compare_type_cells(&closure, &other_closure, &mut b).unwrap());
        b.limits.max_nodes = 0;
        assert_eq!(
            compare_type_cells(&closure, &other_closure, &mut b)
                .unwrap_err()
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        b.limits = limits;
        b.structural = Some(Structure::register(&module, &mut b).unwrap());
        assert!(
            b.structural
                .as_ref()
                .unwrap()
                .physical_pairs
                .entries
                .is_empty()
        );
        assert!(!compare_type_cells(&a, &different, &mut b).unwrap());
    }
}
