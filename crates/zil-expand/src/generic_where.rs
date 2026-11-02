//! Finite `where` is ordinary bound syntax, never a capability proof.
//! One budget follows the new work through aliases and specialization.
use super::*;

fn nominal_bound<'a>(
    module: &'a Module,
    bound: &TypeRef,
    index: &mut Option<trait_bounds::Index<'a>>,
    budget: &mut Budget<'_>,
) -> R<()> {
    // Keep the established pure-Copy path and its exact costs. A nominal bound
    // uses the existing declaration checker before aliases can rewrite it.
    if stores::is_bound(bound) {
        return Err(unsupported(
            bound.span,
            "StoreElement is outside the ordinary nominal where profile",
        ));
    }
    if index.is_none() {
        // This new invocation must prepay the names copied into Index's
        // owned nominal map as well as the borrowed declaration/impl keys.
        // Do it before constructing any part of the index; old Copy-only
        // inputs never enter this scan or acquire its cost.
        for item in &module.items {
            budget.node(item.span, 0)?;
            match &item.kind {
                ItemKind::Aggregate(aggregate) => budget.text(&aggregate.name, item.span)?,
                ItemKind::Enum(enumeration) => budget.text(&enumeration.name, item.span)?,
                ItemKind::Trait(declaration) => budget.text(&declaration.name, item.span)?,
                ItemKind::Impl(implementation) => {
                    if let Some(trait_type) = &implementation.trait_type {
                        budget.ty(trait_type)?;
                    }
                    budget.ty(&implementation.target)?;
                }
                _ => {}
            }
        }
        *index = Some(trait_bounds::Index::new(module, budget)?);
    }
    index
        .as_ref()
        .expect("nominal index initialized")
        .declaration(bound, budget)?;
    Ok(())
}

pub(super) fn normalize(module: &Module, budget: &mut Budget<'_>) -> R<Module> {
    // Prepay the entire clone, including unused bodies and unsupported clauses.
    // The caller only enters this path for an actual free-function where clause.
    for item in &module.items {
        budget.item(item)?;
    }
    let mut output = module.clone();
    let mut nominal_index = None;
    for item in &mut output.items {
        budget.node(item.span, 0)?;
        let ItemKind::Function(function) = &mut item.kind else {
            continue;
        };
        if function.where_clause.is_empty() {
            continue;
        }
        let mut has_nominal = false;
        for parameter in &mut function.generics {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            let GenericParamKind::Type(bounds) = &mut parameter.kind else {
                return Err(unsupported(
                    parameter.span,
                    "where Copy requires ordinary type parameters, not lifetime or const parameters",
                ));
            };
            let mut all_copy = true;
            for bound in bounds.iter() {
                budget.ty(bound)?;
                if !copy::is_bound(bound) {
                    nominal_bound(module, bound, &mut nominal_index, budget).map_err(|mut e| {
                        e.message = format!(
                            "where bounds cannot mix callable, StoreElement or unsupported inline bounds: {}",
                            e.message
                        );
                        e
                    })?;
                    all_copy = false;
                    has_nominal = true;
                }
            }
            if all_copy {
                // Preserve 229's exact path, including repeated Copy bounds.
                bounds.truncate(1);
            } else {
                // Nominal repetitions remain visible to the ordinary checker;
                // only the established structural Copy repetition is idempotent.
                let mut copy_seen = false;
                for bound in std::mem::take(bounds) {
                    budget.ty(&bound)?;
                    if copy::is_bound(&bound) {
                        if copy_seen {
                            continue;
                        }
                        copy_seen = true;
                    }
                    bounds.push(bound);
                }
            }
        }
        for clause in std::mem::take(&mut function.where_clause) {
            budget.node(clause.span, 0)?;
            if clause.lifetime_bound.is_some() {
                return Err(unsupported(
                    clause.span,
                    "where Copy does not support lifetime constraints",
                ));
            }
            let BoundSubject::Type(subject) = &clause.subject else {
                return Err(unsupported(
                    clause.span,
                    "where Copy requires a direct type parameter subject",
                ));
            };
            budget.ty(subject)?;
            let TypeKind::Named { path, args } = &subject.kind else {
                return Err(unsupported(
                    subject.span,
                    "where Copy requires a direct type parameter subject",
                ));
            };
            let [name] = path.as_slice() else {
                return Err(unsupported(
                    subject.span,
                    "where Copy does not support qualified or projected subjects",
                ));
            };
            if !args.is_empty() {
                return Err(unsupported(
                    subject.span,
                    "where Copy does not support applied type subjects",
                ));
            }
            let mut selected = None;
            for (index, parameter) in function.generics.iter().enumerate() {
                budget.node(parameter.span, 0)?;
                budget.text(name, subject.span)?;
                budget.text(&parameter.name, parameter.span)?;
                if parameter.name == *name {
                    selected = Some(index);
                    break;
                }
            }
            let Some(index) = selected else {
                return Err(error(
                    subject.span,
                    "where Copy subject must name a type parameter declared by this function",
                ));
            };
            if clause.bounds.is_empty() {
                return Err(error(clause.span, "where Copy requires a Copy bound"));
            }
            for bound in &clause.bounds {
                budget.ty(bound)?;
                if matches!(&bound.kind, TypeKind::Named { path, args }
                    if path.as_slice() == [copy::BOUND] && args.is_empty())
                {
                    continue;
                }
                // Pure grouping of an already supported nominal bound has the
                // same meaning as its inline form. Copy retains 229's direct
                // built-in spelling; applied and projected bounds stay closed.
                if trait_bounds::plain(bound).is_none() || copy::is_bound(bound) {
                    return Err(unsupported(
                        bound.span,
                        "where supports only the direct built-in Copy bound or a nominal trait without type arguments",
                    ));
                }
                nominal_bound(module, bound, &mut nominal_index, budget).map_err(|mut e| {
                    e.message = format!(
                        "where supports only the direct built-in Copy bound or a declared nominal trait: {}",
                        e.message
                    );
                    e
                })?;
                has_nominal = true;
            }
            let GenericParamKind::Type(bounds) = &mut function.generics[index].kind else {
                unreachable!("type parameters validated above");
            };
            if !has_nominal && bounds.is_empty() {
                let bound = clause.bounds.into_iter().next().expect("nonempty bounds");
                // Charge the new bound vector entry before growing it. Move the
                // original bound node so its source span remains the where site.
                budget.ty(&bound)?;
                bounds.push(bound);
            } else if has_nominal {
                for bound in clause.bounds {
                    budget.ty(&bound)?;
                    if copy::is_bound(&bound) {
                        let mut copy_seen = false;
                        for existing in bounds.iter() {
                            budget.ty(existing)?;
                            if copy::is_bound(existing) {
                                copy_seen = true;
                                break;
                            }
                        }
                        if copy_seen {
                            continue;
                        }
                    }
                    // Duplicate nominal identities deliberately reach the same
                    // diagnostic as repeated inline trait bounds.
                    bounds.push(bound);
                }
            }
        }
    }
    Ok(output)
}
