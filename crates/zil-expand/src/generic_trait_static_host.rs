//! A static trait host is a type name in the current lexical scope, never a
//! value expression. Keep this classification shared by definition checking
//! and specialization so neither phase can grant a type bound to a local value.
use super::*;

pub(super) enum Host {
    Nominal(TypeRef),
    Symbolic {
        ty: TypeRef,
        reference: trait_instances::Reference,
        method: Box<Function>,
    },
}

pub(super) struct Scope<'a, 'l> {
    pub nominals: &'a BTreeMap<String, &'l Item>,
    pub variables: &'a BTreeSet<String>,
    pub bounds: &'a trait_bounds::Bounds,
    pub traits: &'a trait_bounds::Index<'l>,
}

pub(super) fn classify(
    mut object: &Expr,
    member: &str,
    is_value: impl Fn(&str) -> bool,
    scope: Scope<'_, '_>,
    budget: &mut Budget<'_>,
) -> R<Option<Host>> {
    let original = object;
    let mut depth = 0;
    while let ExprKind::Group(inner) = &object.kind {
        object = inner;
        depth += 1;
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
    if scope.variables.contains(name) {
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
        let method = scope
            .traits
            .method(scope.bounds, name, member, budget, object.span)?;
        let reference = scope.traits.method_reference(
            scope.bounds,
            name,
            member,
            &method.name,
            budget,
            object.span,
        )?;
        if matches!(&method, Cow::Borrowed(_)) {
            budget.function_snapshot(&method)?;
        }
        budget.lineage_bytes(std::mem::size_of::<Function>(), object.span)?;
        let ty = named(name, object.span);
        return Ok(Some(Host::Symbolic {
            ty,
            reference,
            method: Box::new(method.into_owned()),
        }));
    }
    if scope.nominals.contains_key(name) {
        return trait_bounds::constants::host_expression(
            original,
            scope.nominals,
            |_| true,
            budget,
        )
        .map(|host| host.map(Host::Nominal));
    }
    Ok(None)
}
