//! Callable-bound templates keep their source body until an actual environment
//! identity is known. This module checks syntax shapes, never ownership facts.
use super::*;
use zil_ir::CallableKind;

#[derive(Clone)]
pub(crate) struct CallableTemplate {
    pub index: usize,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<TypeRef>,
    pub result: TypeRef,
    pub owned_state: bool,
}

pub(crate) fn refers(ty: &TypeRef, name: &str) -> bool {
    match &ty.kind {
        TypeKind::Named { path, args } | TypeKind::Dyn { path, args } => {
            path.as_slice() == [name]
                || args
                    .iter()
                    .any(|a| matches!(a, GenericArg::Type(t) if refers(t, name)))
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Group(inner)
        | TypeKind::Optional(inner)
        | TypeKind::ForLifetime { inner, .. } => refers(inner, name),
        TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
            ts.iter().any(|t| refers(t, name))
        }
        TypeKind::Array { element, .. } => refers(element, name),
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(|t| refers(t, name)) || refers(result, name)
        }
        TypeKind::Error { error, result } => refers(error, name) || refers(result, name),
        _ => false,
    }
}

/// A nominal container carrying F is resolved to a real Package by Types.
/// This is syntax classification only, never an owning admission proof.
pub(crate) fn state_type(ty: &TypeRef, name: &str) -> bool {
    matches!(&grouped(ty).kind, TypeKind::Named { path, args }
        if path.as_slice() != [name] && matches!(args.as_slice(),[GenericArg::Type(t)]
            if matches!(&grouped(t).kind, TypeKind::Named{path,args} if path.as_slice()==[name] && args.is_empty())) )
}

pub(crate) fn parameter_mode(ty: &TypeRef, name: &str) -> Option<Option<bool>> {
    let ty = grouped(ty);
    if matches!(&ty.kind, TypeKind::Named { path, args } if path.as_slice() == [name] && args.is_empty())
    {
        return Some(None);
    }
    match &ty.kind {
        TypeKind::Reference {
            mutable,
            lifetime: None,
            inner,
        } if matches!(&grouped(inner).kind, TypeKind::Named { path, args } if path.as_slice() == [name] && args.is_empty()) => {
            Some(Some(*mutable))
        }
        _ => None,
    }
}

/// A local opaque result carries its own visible callable signature. This
/// admits syntax only: Types and both safety phases still prove that its
/// producer is an already resolved self-contained factory. Addresses to such
/// locals deliberately do not match the direct-call slice.
pub(crate) fn opaque_signature(ty: &TypeRef) -> Option<(CallableKind, &[TypeRef], &TypeRef)> {
    let TypeKind::Impl(bounds) = &grouped(ty).kind else {
        return None;
    };
    let [bound] = bounds.as_slice() else {
        return None;
    };
    let TypeKind::Named { path, args } = &grouped(bound).kind else {
        return None;
    };
    let kind = match path.as_slice() {
        [name] if name == "Fn" => CallableKind::Fn,
        [name] if name == "FnMut" => CallableKind::FnMut,
        [name] if name == "FnOnce" => CallableKind::FnOnce,
        _ => return None,
    };
    let [GenericArg::Type(parameters), GenericArg::Type(result)] = args.as_slice() else {
        return None;
    };
    let TypeKind::Tuple(parameters) = &grouped(parameters).kind else {
        return None;
    };
    Some((kind, parameters, result))
}

pub(crate) fn template(function: &Function) -> R<Option<CallableTemplate>> {
    let callable = function.generics.iter().any(|p| matches!(&p.kind,
        GenericParamKind::Type(bounds) if bounds.iter().any(|b| matches!(&b.kind,
            TypeKind::Named { path, .. } if path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce")))));
    if !callable {
        return Ok(None);
    }
    if function.generics.len() != 1 || !function.where_clause.is_empty() {
        return Err(unsupported(
            function.span,
            "one callable type parameter with a direct bound is implemented",
        ));
    }
    let parameter = &function.generics[0];
    let GenericParamKind::Type(bounds) = &parameter.kind else {
        unreachable!()
    };
    if bounds.len() != 1 {
        return Err(unsupported(
            parameter.span,
            "one callable bound is implemented",
        ));
    }
    let TypeKind::Named { path, args } = &bounds[0].kind else {
        unreachable!()
    };
    let kind = match path[0].as_str() {
        "Fn" => CallableKind::Fn,
        "FnMut" => CallableKind::FnMut,
        "FnOnce" => CallableKind::FnOnce,
        _ => return Err(unsupported(parameter.span, "unsupported callable bound")),
    };
    let [GenericArg::Type(params), GenericArg::Type(result)] = args.as_slice() else {
        return Err(error(
            parameter.span,
            "callable bound requires its argument tuple and result",
        ));
    };
    let TypeKind::Tuple(parameters) = &params.kind else {
        return Err(error(
            params.span,
            "callable bound arguments are not a tuple",
        ));
    };
    if parameters.iter().any(|t| refers(t, &parameter.name)) || refers(result, &parameter.name) {
        return Err(unsupported(
            parameter.span,
            "recursive callable signature is not implemented",
        ));
    }
    let owned_state = function.return_type.as_ref().is_some_and(|t| state_type(t, &parameter.name))
        || function.params.iter().any(|p| state_type(&p.ty, &parameter.name));
    let mut index = None;
    for (i, input) in function.params.iter().enumerate() {
        if refers(&input.ty, &parameter.name) {
            if index.is_some() || (parameter_mode(&input.ty, &parameter.name).is_none() && !state_type(&input.ty, &parameter.name)) || input.give
            {
                return Err(unsupported(
                    input.span,
                    "callable parameter must occur once as F, &F, &mut F or an owning State<F>",
                ));
            }
            index = Some(i);
        }
    }
    if function
        .return_type
        .as_ref()
        .is_some_and(|t| refers(t, &parameter.name) && !state_type(t, &parameter.name))
    {
        return Err(unsupported(
            function.span,
            "escaping callable result is not implemented",
        ));
    }
    let index = index.ok_or_else(|| {
        error(
            parameter.span,
            "callable bound requires a concrete environment parameter",
        )
    })?;
    let mode = parameter_mode(&function.params[index].ty, &parameter.name).unwrap_or(None);
    match (kind, mode) {
        (CallableKind::FnMut, None) if !owned_state => {
            return Err(unsupported(
                function.params[index].span,
                "by-value FnMut helper needs an explicit mutable binding; use &mut F in this slice",
            ));
        }
        (CallableKind::FnMut, Some(false)) | (CallableKind::FnOnce, Some(_)) => {
            return Err(error(
                function.params[index].span,
                "callable bound requires its matching mutable or owned environment passing mode",
            ));
        }
        _ => {}
    }
    Ok(Some(CallableTemplate {
        index,
        generic_name: parameter.name.clone(),
        kind,
        parameters: parameters.clone(),
        result: result.clone(),
        owned_state,
    }))
}
