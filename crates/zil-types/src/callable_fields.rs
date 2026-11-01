//! Concrete, self-contained callable fields in ordinary owning Package values.
//! Source F remains symbolic until one actual environment is supplied.
use super::*;

pub(super) fn placeholder_package(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, t)| matches!(t, Type::Named(n) if n == name)))
}

pub(super) fn template(function: &CoreFunction) -> bool {
    if !function.state_parameters.is_empty() {
        return true;
    }
    owning_profile(
        &function.callable_parameters,
        &function.return_type,
        |index| function.parameters.get(index).map(|p| &p.ty),
    )
}

pub(super) fn signature(signature: &Signature) -> bool {
    if signature.state_parameter.is_some() {
        return true;
    }
    owning_profile(&signature.callable, &signature.result, |index| {
        signature.parameters.get(index)
    })
}

/// A type binding may occur at two distinct constructor inputs. It is still
/// one F, and the concrete call stages each input separately. This predicate
/// classifies syntax only; it cannot certify self-contained captures, complete
/// transfer, or the real runtime identity of either value.
fn owning_profile<'a>(
    bounds: &[CoreCallableParameter],
    result: &Type,
    parameter: impl Fn(usize) -> Option<&'a Type>,
) -> bool {
    let Some(first) = bounds.first() else {
        return false;
    };
    if bounds.len() == 2 && bounds[0].generic_index != bounds[1].generic_index {
        return dual_constructor(bounds, result, parameter);
    }
    if bounds.len() > 2
        || bounds.iter().any(|bound| {
            bound.generic_index != first.generic_index || bound.generic_name != first.generic_name
        })
    {
        return false;
    }
    let returns_state = placeholder_package(result, &first.generic_name);
    if bounds.len() == 1 {
        return returns_state
            || parameter(first.index as usize)
                .is_some_and(|ty| placeholder_package(ty, &first.generic_name));
    }
    returns_state
        && bounds.iter().all(|bound| {
            parameter(bound.index as usize)
                .is_some_and(|ty| helpers::parameter_mode(ty, &bound.generic_name) == Some(None))
        })
}

pub(super) fn dual_constructor<'a>(
    bounds: &[CoreCallableParameter],
    result: &Type,
    parameter: impl Fn(usize) -> Option<&'a Type>,
) -> bool {
    if bounds.len() != 2 || bounds[0].generic_index == bounds[1].generic_index {
        return false;
    }
    let Type::Package { fields, .. } = result else {
        return false;
    };
    bounds.iter().all(|bound| {
        bound.result.is_copy_value()
            && parameter(bound.index as usize)
                .is_some_and(|ty| helpers::parameter_mode(ty, &bound.generic_name) == Some(None))
            && fields
                .iter()
                .filter(|(_, ty)| matches!(ty, Type::Named(name) if name == &bound.generic_name))
                .count()
                == 1
    }) && fields.iter().all(|(_, ty)| {
        ty.is_copy_value()
            || matches!(ty, Type::Managed(_) if resources::supported(ty))
            || bounds
                .iter()
                .any(|bound| matches!(ty, Type::Named(name) if name == &bound.generic_name))
    })
}

pub(super) fn nominal_base(name: &str) -> &str {
    if let Some((base, _)) = name.rsplit_once(":closure") {
        return base;
    }
    if name.contains("$generic_")
        && let Some((base, suffix)) = name.rsplit_once('_')
        && !suffix.is_empty()
        && suffix.bytes().all(|b| b.is_ascii_digit())
    {
        return base;
    }
    name
}

pub(super) fn has_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, t)| matches!(t, Type::Closure(_))))
}

/// Match the written ordinary owning shape. Exactly one concrete F identity is
/// inferred, even when more than one declared field refers to F.
pub(super) fn environment(pattern: &Type, actual: &Type, name: &str, span: Span) -> R<ClosureId> {
    let mut found = None;
    let pairs: Vec<(&Type, &Type)> = match (pattern, actual) {
        (Type::Named(n), Type::Closure(_)) if n == name => vec![(pattern, actual)],
        (
            Type::Package {
                name: a,
                fields: af,
            },
            Type::Package {
                name: b,
                fields: bf,
            },
        ) if nominal_base(a) == nominal_base(b) && af.len() == bf.len() => af
            .iter()
            .zip(bf)
            .map(|((an, at), (bn, bt))| {
                if an == bn {
                    Ok((at, bt))
                } else {
                    Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "callable State field names differ",
                    ))
                }
            })
            .collect::<R<_>>()?,
        _ => {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "expected the complete callable State or its owned environment",
            ));
        }
    };
    for (want, got) in pairs {
        if matches!(want, Type::Named(n) if n == name) {
            let Type::Closure(id) = got else {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "State F must be a concrete closure environment",
                ));
            };
            if found.replace(*id).is_some_and(|prior| prior != *id) {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "State fields require one concrete F identity",
                ));
            }
        } else if want != got {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "callable State ordinary field shape differs",
            ));
        }
    }
    found.ok_or_else(|| error("E-TYPE-MISMATCH", span, "State has no concrete F field"))
}

pub(super) fn specialize_type(t: &mut Type, name: &str, id: ClosureId) {
    match t {
        Type::Named(n) if n == name => *t = Type::Closure(id),
        Type::Package {
            name: nominal,
            fields,
        } => {
            let contains = fields
                .iter()
                .any(|(_, t)| matches!(t,Type::Named(n) if n==name));
            for (_, t) in fields {
                specialize_type(t, name, id)
            }
            if contains {
                *nominal = format!("{}:closure{}", nominal_base(nominal), id.0)
            }
        }
        Type::Record { fields, .. } => {
            for (_, t) in fields {
                specialize_type(t, name, id)
            }
        }
        Type::Tuple(fields) => {
            for t in fields {
                specialize_type(t, name, id)
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for t in fields {
                    specialize_type(t, name, id)
                }
            }
        }
        Type::Managed(t)
        | Type::Desc(t)
        | Type::Address { inner: t, .. }
        | Type::Opaque { hidden: t, .. }
        | Type::Array { element: t, .. }
        | Type::Slice { element: t }
        | Type::OwnedStore { element: t } => specialize_type(t, name, id),
        Type::ErrorUnion { error, result } => {
            specialize_type(error, name, id);
            specialize_type(result, name, id)
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for p in parameters {
                specialize_type(p, name, id)
            }
            specialize_type(result, name, id)
        }
        _ => {}
    }
}

/// The complete function was charged by helper_snapshot_work before its clone;
/// this bounded replacement allocates only the one-node concrete Closure type.
pub(super) fn specialize(function: &mut CoreFunction, name: &str, closure: ClosureId) {
    visit(function, &mut |ty| specialize_type(ty, name, closure));
}

pub(super) fn specialize_all(function: &mut CoreFunction, bindings: &[(&str, ClosureId)]) {
    visit(function, &mut |ty| specialize_type_all(ty, bindings));
}

fn visit(function: &mut CoreFunction, replace: &mut impl FnMut(&mut Type)) {
    fn expr(e: &mut CoreExpr, replace: &mut impl FnMut(&mut Type)) {
        match &mut e.kind {
            CoreExprKind::Unary { operand, .. } | CoreExprKind::Try(operand) => {
                expr(operand, replace)
            }
            CoreExprKind::IntegerConvert { value, .. } => expr(value, replace),
            CoreExprKind::Binary { left, right, .. } => {
                expr(left, replace);
                expr(right, replace)
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for argument in type_arguments {
                    replace(argument);
                }
                expr(callee, replace);
            }
            CoreExprKind::Call { callee, arguments } => {
                expr(callee, replace);
                for a in arguments {
                    expr(a, replace)
                }
            }
            CoreExprKind::Member { object, .. } => expr(object, replace),
            CoreExprKind::Index { object, index } => {
                expr(object, replace);
                expr(index, replace)
            }
            CoreExprKind::Slice { object, start, end } => {
                expr(object, replace);
                for a in [start, end].into_iter().flatten() {
                    expr(a, replace)
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                expr(condition, replace);
                expr(then_value, replace);
                expr(else_value, replace)
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for f in fields {
                    expr(f, replace)
                }
            }
            CoreExprKind::Record { ty: t, fields } => {
                replace(t);
                for (_, f) in fields {
                    expr(f, replace)
                }
            }
            CoreExprKind::RecordUpdate {
                ty: t,
                base,
                fields,
            } => {
                if let Some(t) = t {
                    replace(t)
                }
                expr(base, replace);
                for (_, f) in fields {
                    expr(f, replace)
                }
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                for p in parameters {
                    if let Some(t) = &mut p.ty {
                        replace(t)
                    }
                }
                if let Some(t) = return_type {
                    replace(t)
                }
                block(body, replace)
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, e) in bindings {
                    expr(e, replace)
                }
                expr(value, replace)
            }
            _ => {}
        }
    }
    fn block(b: &mut CoreBlock, replace: &mut impl FnMut(&mut Type)) {
        for s in &mut b.statements {
            match &mut s.kind {
                CoreStatementKind::Binding {
                    ty: t, initializer, ..
                } => {
                    if let Some(t) = t {
                        replace(t)
                    }
                    if let Some(e) = initializer {
                        expr(e, replace)
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    expr(target, replace);
                    expr(value, replace)
                }
                CoreStatementKind::Return(Some(e)) | CoreStatementKind::Expression(e) => {
                    expr(e, replace)
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    expr(condition, replace);
                    block(then_body, replace);
                    if let Some(b) = else_body {
                        block(b, replace)
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    expr(condition, replace);
                    block(body, replace)
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    expr(value, replace);
                    block(then_body, replace);
                    if let Some(b) = else_body {
                        block(b, replace)
                    }
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    expr(value, replace);
                    block(body, replace)
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    expr(start, replace);
                    expr(end, replace);
                    block(body, replace)
                }
                CoreStatementKind::Match { value, arms } => {
                    expr(value, replace);
                    for a in arms {
                        block(&mut a.body, replace)
                    }
                }
                CoreStatementKind::Block(b)
                | CoreStatementKind::Unsafe(b)
                | CoreStatementKind::Defer(b) => block(b, replace),
                _ => {}
            }
        }
    }
    for p in &mut function.parameters {
        replace(&mut p.ty)
    }
    replace(&mut function.return_type);
    block(&mut function.body, replace);
}

/// New two-binder path. Callback field order belongs to the nominal family,
/// independently of the order in which a function names its binders.
pub(super) fn specialize_type_all(ty: &mut Type, bindings: &[(&str, ClosureId)]) {
    match ty {
        Type::Named(name) => {
            if let Some((_, id)) = bindings.iter().find(|(binder, _)| *binder == name) {
                *ty = Type::Closure(*id);
            }
        }
        Type::Package { name, fields } => {
            let changes = fields.iter().any(|(_, ty)|
                matches!(ty, Type::Named(name) if bindings.iter().any(|(binder,_)| *binder == name)));
            for (_, field) in fields.iter_mut() {
                specialize_type_all(field, bindings);
            }
            if changes {
                let suffix = fields
                    .iter()
                    .filter_map(|(_, ty)| {
                        if let Type::Closure(id) = ty {
                            Some(id.0.to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("_");
                *name = format!("{}:closure{}", nominal_base(name), suffix);
            }
        }
        Type::Record { fields, .. } => {
            for (_, ty) in fields {
                specialize_type_all(ty, bindings);
            }
        }
        Type::Tuple(fields) => {
            for ty in fields {
                specialize_type_all(ty, bindings);
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for ty in fields {
                    specialize_type_all(ty, bindings);
                }
            }
        }
        Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Address { inner, .. }
        | Type::Opaque { hidden: inner, .. }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner } => specialize_type_all(inner, bindings),
        Type::ErrorUnion { error, result } => {
            specialize_type_all(error, bindings);
            specialize_type_all(result, bindings);
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            for ty in parameters {
                specialize_type_all(ty, bindings);
            }
            specialize_type_all(result, bindings);
        }
        _ => {}
    }
}
