//! Complete signature occurrences from an already interpreted whole type.
//! These rows propose no permission, constructor or class equality.
use crate::*;
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
pub(super) fn append(
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
    let opaque = matches!(
        ty,
        Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::MutexGuard { .. }
            | Type::Task { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
    );
    if opaque {
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
    } else {
        for row in &life.retained {
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
    if matches!(ty, Type::Address { .. } | Type::Desc(_))
        && !life.slots.iter().any(|slot| slot.path.is_empty())
    {
        statics.push(StoragePosition {
            base,
            path: Vec::new(),
        });
    }
    if covered.iter().any(|covered| !*covered) {
        return Err(bad(
            "header whole type has an unrepresented intrinsic class occurrence",
        ));
    }
    Ok(())
}
