//! Finite `where T: Copy` is ordinary bound syntax, never a capability proof.
//! One budget follows the new work through aliases and specialization.
use super::*;

pub(super) fn normalize(module: &Module, budget: &mut Budget<'_>) -> R<Module> {
    // Prepay the entire clone, including unused bodies and unsupported clauses.
    // The caller only enters this path for an actual free-function where clause.
    for item in &module.items {
        budget.item(item)?;
    }
    let mut output = module.clone();
    for item in &mut output.items {
        budget.node(item.span, 0)?;
        let ItemKind::Function(function) = &mut item.kind else {
            continue;
        };
        if function.where_clause.is_empty() {
            continue;
        }
        for parameter in &mut function.generics {
            budget.node(parameter.span, 0)?;
            budget.text(&parameter.name, parameter.span)?;
            let GenericParamKind::Type(bounds) = &mut parameter.kind else {
                return Err(unsupported(
                    parameter.span,
                    "where Copy requires ordinary type parameters, not lifetime or const parameters",
                ));
            };
            for bound in bounds.iter() {
                budget.ty(bound)?;
                if !copy::is_bound(bound) {
                    return Err(unsupported(
                        bound.span,
                        "where Copy cannot mix callable, StoreElement or other inline bounds",
                    ));
                }
            }
            // Inline repetitions and later where repetitions have one meaning.
            bounds.truncate(1);
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
                if !matches!(&bound.kind, TypeKind::Named { path, args }
                    if path.as_slice() == [copy::BOUND] && args.is_empty())
                {
                    return Err(unsupported(
                        bound.span,
                        "where Copy supports only the direct built-in Copy bound without type arguments",
                    ));
                }
            }
            let GenericParamKind::Type(bounds) = &mut function.generics[index].kind else {
                unreachable!("type parameters validated above");
            };
            if bounds.is_empty() {
                let bound = clause.bounds.into_iter().next().expect("nonempty bounds");
                // Charge the new bound vector entry before growing it. Move the
                // original bound node so its source span remains the where site.
                budget.ty(&bound)?;
                bounds.push(bound);
            }
        }
    }
    Ok(output)
}
