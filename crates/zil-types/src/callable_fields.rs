//! Concrete, self-contained callable fields in ordinary owning Package values.
//! Source F remains symbolic until one actual environment is supplied.
use super::*;

pub(super) fn placeholder_package(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, t)| matches!(t, Type::Named(n) if n == name)))
}

pub(super) fn template(function: &CoreFunction) -> bool {
    owning_profile(
        &function.callable_parameters,
        &function.return_type,
        |index| function.parameters.get(index).map(|p| &p.ty),
    )
}

pub(super) fn signature(signature: &Signature) -> bool {
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

/// The complete function was charged by helper_snapshot_work before its clone;
/// this bounded replacement allocates only the one-node concrete Closure type.
pub(super) fn specialize(function: &mut CoreFunction, name: &str, closure: ClosureId) {
    fn ty(t: &mut Type, name: &str, id: ClosureId) {
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
                    ty(t, name, id)
                }
                if contains {
                    *nominal = format!("{}:closure{}", nominal_base(nominal), id.0)
                }
            }
            Type::Record { fields, .. } => {
                for (_, t) in fields {
                    ty(t, name, id)
                }
            }
            Type::Tuple(fields) => {
                for t in fields {
                    ty(t, name, id)
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        ty(t, name, id)
                    }
                }
            }
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::Opaque { hidden: t, .. }
            | Type::Array { element: t, .. }
            | Type::Slice { element: t }
            | Type::OwnedStore { element: t } => ty(t, name, id),
            Type::ErrorUnion { error, result } => {
                ty(error, name, id);
                ty(result, name, id)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for p in parameters {
                    ty(p, name, id)
                }
                ty(result, name, id)
            }
            _ => {}
        }
    }
    fn expr(e: &mut CoreExpr, n: &str, id: ClosureId) {
        match &mut e.kind {
            CoreExprKind::Unary { operand, .. } | CoreExprKind::Try(operand) => {
                expr(operand, n, id)
            }
            CoreExprKind::IntegerConvert { value, .. } => expr(value, n, id),
            CoreExprKind::Binary { left, right, .. } => {
                expr(left, n, id);
                expr(right, n, id)
            }
            CoreExprKind::Call { callee, arguments } => {
                expr(callee, n, id);
                for a in arguments {
                    expr(a, n, id)
                }
            }
            CoreExprKind::Member { object, .. } => expr(object, n, id),
            CoreExprKind::Index { object, index } => {
                expr(object, n, id);
                expr(index, n, id)
            }
            CoreExprKind::Slice { object, start, end } => {
                expr(object, n, id);
                for a in [start, end].into_iter().flatten() {
                    expr(a, n, id)
                }
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                expr(condition, n, id);
                expr(then_value, n, id);
                expr(else_value, n, id)
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for f in fields {
                    expr(f, n, id)
                }
            }
            CoreExprKind::Record { ty: t, fields } => {
                ty(t, n, id);
                for (_, f) in fields {
                    expr(f, n, id)
                }
            }
            CoreExprKind::RecordUpdate {
                ty: t,
                base,
                fields,
            } => {
                if let Some(t) = t {
                    ty(t, n, id)
                }
                expr(base, n, id);
                for (_, f) in fields {
                    expr(f, n, id)
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
                        ty(t, n, id)
                    }
                }
                if let Some(t) = return_type {
                    ty(t, n, id)
                }
                block(body, n, id)
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, e) in bindings {
                    expr(e, n, id)
                }
                expr(value, n, id)
            }
            _ => {}
        }
    }
    fn block(b: &mut CoreBlock, n: &str, id: ClosureId) {
        for s in &mut b.statements {
            match &mut s.kind {
                CoreStatementKind::Binding {
                    ty: t, initializer, ..
                } => {
                    if let Some(t) = t {
                        ty(t, n, id)
                    }
                    if let Some(e) = initializer {
                        expr(e, n, id)
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    expr(target, n, id);
                    expr(value, n, id)
                }
                CoreStatementKind::Return(Some(e)) | CoreStatementKind::Expression(e) => {
                    expr(e, n, id)
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    expr(condition, n, id);
                    block(then_body, n, id);
                    if let Some(b) = else_body {
                        block(b, n, id)
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    expr(condition, n, id);
                    block(body, n, id)
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    expr(value, n, id);
                    block(then_body, n, id);
                    if let Some(b) = else_body {
                        block(b, n, id)
                    }
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    expr(value, n, id);
                    block(body, n, id)
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    expr(start, n, id);
                    expr(end, n, id);
                    block(body, n, id)
                }
                CoreStatementKind::Match { value, arms } => {
                    expr(value, n, id);
                    for a in arms {
                        block(&mut a.body, n, id)
                    }
                }
                CoreStatementKind::Block(b) | CoreStatementKind::Unsafe(b) => block(b, n, id),
                _ => {}
            }
        }
    }
    for p in &mut function.parameters {
        ty(&mut p.ty, name, closure)
    }
    ty(&mut function.return_type, name, closure);
    block(&mut function.body, name, closure);
}
