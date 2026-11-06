//! A static trait host is a type name in the current lexical scope, never a
//! value expression. Keep this classification shared by definition checking
//! and specialization so neither phase can grant a type bound to a local value.
use super::*;

pub(super) enum Host {
    Nominal(TypeRef),
    Symbolic {
        ty: TypeRef,
        reference: trait_instances::Reference,
        method: Function,
    },
}

pub(super) fn classify(
    mut object: &Expr,
    member: &str,
    is_value: impl Fn(&str) -> bool,
    nominals: &BTreeMap<String, &Item>,
    variables: &BTreeSet<String>,
    bounds: &trait_bounds::Bounds,
    traits: &trait_bounds::Index<'_>,
    budget: &mut Budget<'_>,
) -> R<Option<Host>> {
    let mut depth = 0;
    loop {
        if let ExprKind::Group(inner) = &object.kind {
            object = inner;
            depth += 1;
        } else {
            break;
        }
    }
    let (name, args) = match &object.kind {
        ExprKind::Name(name) => (name, &[][..]),
        ExprKind::TypeApply { callee, arguments } => {
            let ExprKind::Name(name) = &callee.kind else {
                return Ok(None);
            };
            (name, arguments.as_slice())
        }
        _ => return Ok(None),
    };
    if is_value(name) {
        return Ok(None);
    }
    if variables.contains(name) {
        for layer in 0..=depth {
            budget.node(object.span, layer)?;
        }
        if matches!(&object.kind, ExprKind::TypeApply { .. }) {
            budget.node(object.span, depth + 1)?;
        }
        budget.text(name, object.span)?;
        if !args.is_empty() {
            return Err(error(
                object.span,
                "type parameter host does not take type arguments",
            ));
        }
        let method = traits.method(bounds, name, member, budget, object.span)?;
        let reference =
            traits.method_reference(bounds, name, member, &method.name, budget, object.span)?;
        let ty = named(name, object.span);
        return Ok(Some(Host::Symbolic {
            ty,
            reference,
            method: method.into_owned(),
        }));
    }
    if nominals.contains_key(name) {
        return trait_bounds::constants::host_expression(object, nominals, |_| true, budget)
            .map(|host| host.map(Host::Nominal));
    }
    Ok(None)
}
