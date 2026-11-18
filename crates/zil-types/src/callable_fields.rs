//! Concrete, self-contained callable fields in ordinary owning Package values.
//! Source F remains symbolic until one actual environment is supplied.
use super::*;

pub(super) fn placeholder_package(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, t)| matches!(t, Type::Named(n) if n == name)))
}

pub(super) fn placeholder_value(ty: &Type, name: &str) -> bool {
    fn contains(ty: &Type, name: &str) -> bool {
        match ty {
            Type::Named(actual) => actual == name,
            Type::Package { fields, .. } => fields.iter().any(|(_, field)| contains(field, name)),
            Type::Tuple(fields) => fields.iter().any(|field| contains(field, name)),
            Type::Enum { variants, .. } => variants
                .iter()
                .any(|(_, fields)| fields.iter().any(|field| contains(field, name))),
            _ => false,
        }
    }
    matches!(
        ty,
        Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) && contains(ty, name)
}

pub(super) fn normalize_placeholder(ty: &mut Type, binder: &str) {
    replace_composed(ty, binder, None);
}

// None is the inference-only placeholder. A concrete closure supplies a
// real environment identity and a nominal name shared across helper scopes.
fn replace_composed(ty: &mut Type, binder: &str, actual: Option<ClosureId>) -> bool {
    fn replace_name(name: &mut String, actual: Option<ClosureId>) {
        let base = nominal_base(name);
        *name = actual.map_or_else(|| base.to_owned(), |id| format!("{base}:closure{}", id.0));
    }
    match ty {
        Type::Named(name) if name == binder => {
            *ty = actual.map_or_else(|| Type::Named("$callback".into()), Type::Closure);
            true
        }
        Type::Package { name, fields } => {
            let mut found = false;
            for (_, field) in fields {
                found |= replace_composed(field, binder, actual);
            }
            if found {
                replace_name(name, actual);
            }
            found
        }
        Type::Tuple(fields) => {
            let mut found = false;
            for field in fields {
                found |= replace_composed(field, binder, actual);
            }
            found
        }
        Type::Enum { name, variants } => {
            let mut found = false;
            for (_, fields) in variants {
                for field in fields {
                    found |= replace_composed(field, binder, actual);
                }
            }
            if found {
                replace_name(name, actual);
            }
            found
        }
        _ => {
            if let Some(id) = actual {
                specialize_other(ty, binder, id);
            }
            false
        }
    }
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
    let returns_state = placeholder_value(result, &first.generic_name);
    if bounds.len() == 1 {
        return returns_state
            || parameter(first.index as usize)
                .is_some_and(|ty| placeholder_value(ty, &first.generic_name));
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
    fn bind(
        want: &Type,
        got: &Type,
        name: &str,
        span: Span,
        found: &mut Option<ClosureId>,
    ) -> R<()> {
        let mismatch = || {
            error(
                "E-TYPE-MISMATCH",
                span,
                "callable value has a different complete shape",
            )
        };
        match (want, got) {
            (Type::Named(n), Type::Closure(id)) if n == name => {
                if found.replace(*id).is_some_and(|prior| prior != *id) {
                    return Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "callable value has several differing environment identities",
                    ));
                }
            }
            (Type::Tuple(a), Type::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    bind(a, b, name, span, found)?;
                }
            }
            (
                Type::Package {
                    name: a,
                    fields: af,
                },
                Type::Package {
                    name: b,
                    fields: bf,
                },
            ) if nominal_base(a) == nominal_base(b) && af.len() == bf.len() => {
                for ((an, a), (bn, b)) in af.iter().zip(bf) {
                    if an != bn {
                        return Err(mismatch());
                    }
                    bind(a, b, name, span, found)?;
                }
            }
            (
                Type::Enum {
                    name: a,
                    variants: av,
                },
                Type::Enum {
                    name: b,
                    variants: bv,
                },
            ) if nominal_base(a) == nominal_base(b) && av.len() == bv.len() => {
                for ((an, av), (bn, bv)) in av.iter().zip(bv) {
                    if an != bn || av.len() != bv.len() {
                        return Err(mismatch());
                    }
                    for (a, b) in av.iter().zip(bv) {
                        bind(a, b, name, span, found)?;
                    }
                }
            }
            _ if want == got => {}
            _ => return Err(mismatch()),
        }
        Ok(())
    }
    let mut found = None;
    bind(pattern, actual, name, span, &mut found)?;
    found.ok_or_else(|| {
        error(
            "E-TYPE-MISMATCH",
            span,
            "callable value has no concrete environment",
        )
    })
}

pub(super) fn specialize_type(ty: &mut Type, name: &str, id: ClosureId) {
    replace_composed(ty, name, Some(id));
}

fn specialize_other(t: &mut Type, name: &str, id: ClosureId) {
    match t {
        Type::Record { fields, .. } => {
            for (_, t) in fields {
                specialize_type(t, name, id)
            }
        }
        Type::Task {
            result,
            retained_life,
        } => {
            specialize_type(result, name, id);
            for port in &mut retained_life.ports {
                specialize_type(&mut port.leaf, name, id);
            }
        }
        Type::Managed(t)
        | Type::Desc(t)
        | Type::Address { inner: t, .. }
        | Type::Opaque { hidden: t, .. }
        | Type::Array { element: t, .. }
        | Type::Slice { element: t }
        | Type::Registration { environment: t }
        | Type::AsyncRegistration { environment: t }
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

pub(super) fn visit(function: &mut CoreFunction, replace: &mut impl FnMut(&mut Type)) {
    fn pattern(p: &mut CorePattern, replace: &mut impl FnMut(&mut Type)) {
        match &mut p.kind {
            CorePatternKind::Variant { ty, fields, .. } => {
                replace(ty);
                for field in fields {
                    pattern(field, replace);
                }
            }
            CorePatternKind::Record { ty, fields } => {
                replace(ty);
                for (_, field) in fields {
                    pattern(field, replace);
                }
            }
            CorePatternKind::Reference { pattern: inner, .. }
            | CorePatternKind::ErrorArm { pattern: inner, .. } => pattern(inner, replace),
            CorePatternKind::Tuple(fields) | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    pattern(field, replace);
                }
            }
            CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    pattern(field, replace);
                }
            }
            CorePatternKind::Name(_)
            | CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {}
        }
    }
    fn expr(e: &mut CoreExpr, replace: &mut impl FnMut(&mut Type)) {
        match &mut e.kind {
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::Try(operand)
            | CoreExprKind::Group(operand) => expr(operand, replace),
            CoreExprKind::IntegerConvert { value, target }
            | CoreExprKind::NumericConvert { value, target }
            | CoreExprKind::Ascribed { value, ty: target } => {
                replace(target);
                expr(value, replace);
            }
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
            CoreExprKind::Member { object, owner, .. } => {
                if let Some(owner) = owner {
                    replace(owner);
                }
                expr(object, replace);
            }
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
            CoreExprKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    expr(field, replace);
                }
            }
            CoreExprKind::MakeDynBorrow { source, .. }
            | CoreExprKind::MakeDynOwned { source, .. } => expr(source, replace),
            CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                expr(callee, replace);
                for argument in arguments {
                    expr(argument, replace);
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
                CoreStatementKind::TupleBinding {
                    pattern: binding,
                    ty,
                    initializer,
                    ..
                } => {
                    pattern(binding, replace);
                    if let Some(t) = ty {
                        replace(t);
                    }
                    expr(initializer, replace);
                }
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
                    pattern: binding,
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    pattern(binding, replace);
                    expr(value, replace);
                    block(then_body, replace);
                    if let Some(b) = else_body {
                        block(b, replace)
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern: binding,
                    value,
                    body,
                } => {
                    pattern(binding, replace);
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
                        pattern(&mut a.pattern, replace);
                        block(&mut a.body, replace)
                    }
                }
                CoreStatementKind::ForEach {
                    pattern: binding,
                    iterator,
                    body,
                } => {
                    pattern(binding, replace);
                    expr(iterator, replace);
                    block(body, replace);
                }
                CoreStatementKind::With { value, body, .. } => {
                    expr(value, replace);
                    block(body, replace);
                }
                CoreStatementKind::Yield(value) => expr(value, replace),
                CoreStatementKind::CompoundAssign { target, value, .. } => {
                    expr(target, replace);
                    expr(value, replace);
                }
                CoreStatementKind::Loop(b)
                | CoreStatementKind::Block(b)
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
        Type::Task {
            result,
            retained_life,
        } => {
            specialize_type_all(result, bindings);
            for port in &mut retained_life.ports {
                specialize_type_all(&mut port.leaf, bindings);
            }
        }
        Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Address { inner, .. }
        | Type::Opaque { hidden: inner, .. }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
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
