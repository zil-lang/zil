//! Deferred compiler obligations retain actual environment identities. A
//! proposed zero domain is never itself a ClosedEnvironment proof.
use super::*;

pub(crate) fn check(
    module: &TypedModule,
    obligations: &BTreeMap<ClosureId, Span>,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    if obligations.is_empty() {
        return Ok(());
    }
    let proven = sources::prove_static_constructors(module, left, limits)?;
    let mut pending = obligations
        .iter()
        .map(|(id, span)| (*id, *span))
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    while let Some((id, span)) = pending.pop() {
        spend(left, seen.len().saturating_add(1), span)?;
        if !seen.insert(id) {
            continue;
        }
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| unsupported(span, "deferred actual ClosureDefinition is absent"))?;
        let life = d.storage_life.as_ref().ok_or_else(|| {
            unsupported(
                span,
                "ClosedEnvironment requires a complete actual domain; None is unknown",
            )
        })?;
        if life.fixed_binder_count != 0 {
            return Err(error(
                "E-CLOSED-ENVIRONMENT",
                span,
                "actual environment retains free input classes",
            ));
        }
        for (index, ty) in d.captures.iter().enumerate() {
            inference::charge_type_snapshot(ty, span, limits, left, 1)?;
            match ty {
                Type::Closure(child) => pending.push((*child, span)),
                Type::Address { mutable: false, .. } => {
                    let position = StoragePosition {
                        base: StorageBase::Capture(index as u32),
                        path: Vec::new(),
                    };
                    if !life.static_bindings.contains(&position)
                        || !proven.contains(&(id, position))
                    {
                        return Err(error(
                            "E-CLOSED-ENVIRONMENT",
                            span,
                            "constant environment field lacks complete actual constructor and write proofs",
                        ));
                    }
                }
                _ if local_scopes::closed(ty, module, left, span, 0, limits.max_call_depth)? => {
                    // Zero class metadata on a nested child is a candidate,
                    // not its environment proof. Inspect every actual child
                    // definition even when a Product/array hides its value.
                    let mut fields = vec![(ty, 0usize)];
                    while let Some((field, depth)) = fields.pop() {
                        spend(left, 1, span)?;
                        if depth > limits.max_call_depth {
                            return Err(unsupported(
                                span,
                                "nested closed environment inventory exceeds finite depth",
                            ));
                        }
                        match field {
                            Type::Closure(child) => pending.push((*child, span)),
                            Type::Managed(inner)
                            | Type::Array { element: inner, .. }
                            | Type::OwnedStore { element: inner }
                            | Type::BorrowedStorage { storage: inner, .. } => {
                                fields.push((inner, depth + 1))
                            }
                            Type::Tuple(children) => {
                                fields.extend(children.iter().map(|child| (child, depth + 1)))
                            }
                            Type::Record {
                                fields: children, ..
                            }
                            | Type::Package {
                                fields: children, ..
                            } => {
                                fields.extend(children.iter().map(|(_, child)| (child, depth + 1)))
                            }
                            Type::Enum { variants, .. } => {
                                for (_, children) in variants {
                                    fields.extend(children.iter().map(|child| (child, depth + 1)));
                                }
                            }
                            Type::ErrorUnion { error, result } => {
                                fields.push((error, depth + 1));
                                fields.push((result, depth + 1));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    return Err(error(
                        "E-CLOSED-ENVIRONMENT",
                        span,
                        "actual environment retains an unproved borrowed or nominal domain",
                    ));
                }
            }
        }
    }
    Ok(())
}
