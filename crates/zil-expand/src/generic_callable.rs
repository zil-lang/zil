//! Callable-bound templates keep their source body until an actual environment
//! identity is known. This module checks syntax shapes, never ownership facts.
use super::*;
use zil_ir::CallableKind;

#[derive(Clone)]
pub(crate) struct CallableTemplate {
    pub index: usize,
    pub generic_index: u32,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<TypeRef>,
    pub result: TypeRef,
}

#[derive(Clone)]
pub(crate) struct CallableTemplates {
    pub roles: Vec<CallableTemplate>,
    pub owned_state: bool,
}
impl CallableTemplates {
    pub(crate) fn contains_generic(&self, name: &str) -> bool {
        self.roles.iter().any(|role| role.generic_name == name)
    }

    pub(crate) fn single_generic(&self) -> Option<&str> {
        let first = self.roles.first()?;
        self.roles
            .iter()
            .all(|role| role.generic_index == first.generic_index)
            .then_some(first.generic_name.as_str())
    }

    pub(crate) fn store_generic(&self) -> Option<&str> {
        self.owned_state.then(|| self.single_generic()).flatten()
    }
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
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return false;
    };
    path.as_slice() != [name]
        && (1..=2).contains(&args.len())
        && args
            .iter()
            .filter(|argument| {
                matches!(argument, GenericArg::Type(t)
            if parameter_mode(t, name) == Some(None))
            })
            .count()
            == 1
        && args.iter().all(|argument| {
            matches!(argument, GenericArg::Type(t)
            if parameter_mode(t, name) == Some(None) || !refers(t, name))
        })
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

pub(crate) fn template(function: &Function) -> R<Option<CallableTemplates>> {
    template_with_state(function, state_type)
}

/// The generic engine has already replaced State<F> applications with nominal
/// declarations. Read their actual Package fields to recover the symbolic F
/// occurrence; a generated name alone grants no field or callable capability.
pub(crate) fn expanded_template(
    function: &Function,
    declarations: &BTreeMap<String, &Aggregate>,
    remaining: &mut usize,
) -> R<Option<CallableTemplates>> {
    // The classifier is queried for input discovery, result admission and
    // bound modes. Charge every potential field scan before it runs.
    for ty in function
        .params
        .iter()
        .map(|p| &p.ty)
        .chain(function.return_type.iter())
    {
        if let TypeKind::Named { path, args } = &grouped(ty).kind
            && path.len() == 1
            && args.is_empty()
            && let Some(declaration) = declarations.get(&path[0])
        {
            let work = declaration
                .fields
                .len()
                .saturating_add(path[0].len())
                .saturating_mul(4);
            *remaining = remaining.checked_sub(work).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "callable Package signature work budget exceeded",
                    Some(ty.span),
                )
            })?;
        }
    }
    template_with_state(function, |ty, name| {
        if state_type(ty, name) {
            return true;
        }
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return false;
        };
        if path.len() != 1 || !args.is_empty() || !path[0].starts_with("$generic_") {
            return false;
        }
        declarations.get(&path[0]).is_some_and(|aggregate| {
            aggregate.kind == AggregateKind::Package
                && aggregate.fields.iter().any(|field| {
                    matches!(&grouped(&field.ty).kind,
                    TypeKind::Named { path, args } if path.as_slice()==[name] && args.is_empty())
                })
        })
    })
}

fn template_with_state(
    function: &Function,
    is_state: impl Fn(&TypeRef, &str) -> bool,
) -> R<Option<CallableTemplates>> {
    let callables: Vec<_> = function.generics.iter().filter(|p| matches!(&p.kind,
        GenericParamKind::Type(bounds) if bounds.iter().any(|b| matches!(&b.kind,
            TypeKind::Named { path, .. } if path.len() == 1 && matches!(path[0].as_str(), "Fn" | "FnMut" | "FnOnce"))))).collect();
    if callables.is_empty() {
        return Ok(None);
    }
    if callables.len() > 2
        || function.generics.len() > callables.len() + 1
        || !function.where_clause.is_empty()
    {
        return Err(unsupported(
            function.span,
            "at most two callable types and one ordinary Copy type with direct bounds are implemented",
        ));
    }
    for ordinary in function
        .generics
        .iter()
        .filter(|p| !callables.iter().any(|c| c.name == p.name))
    {
        if !matches!(&ordinary.kind, GenericParamKind::Type(bounds)
            if matches!(bounds.as_slice(), [bound] if copy::is_bound(bound)))
        {
            return Err(unsupported(
                ordinary.span,
                "ordinary parameters beside callable types require one direct Copy bound",
            ));
        }
    }
    let mut roles = Vec::new();
    let mut owned_state = false;
    for (generic_index, parameter) in callables.iter().enumerate() {
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
        if callables.iter().any(|other| {
            parameters.iter().any(|t| refers(t, &other.name)) || refers(result, &other.name)
        }) {
            return Err(unsupported(
                parameter.span,
                "recursive callable signature is not implemented",
            ));
        }
        let returns_state = function
            .return_type
            .as_ref()
            .is_some_and(|t| is_state(t, &parameter.name));
        let takes_state = function
            .params
            .iter()
            .any(|p| is_state(&p.ty, &parameter.name));
        let owning = returns_state || takes_state;
        // Two occurrences of one F are admitted only at the complete owning
        // constructor boundary. A State plus another F, or two State inputs,
        // still requires a distinct profile and cannot use this exception.
        let constructor = returns_state && !takes_state;
        owned_state |= owning;
        if owning && callables.len() != 1 {
            return Err(unsupported(
                function.span,
                "owning State helpers require one callable type",
            ));
        }
        let mut count = 0;
        let mut first_mode = None;
        for (index, input) in function.params.iter().enumerate() {
            if !refers(&input.ty, &parameter.name) && !is_state(&input.ty, &parameter.name) {
                continue;
            }
            let mode = parameter_mode(&input.ty, &parameter.name);
            if (mode.is_none() && !is_state(&input.ty, &parameter.name)) || input.give {
                return Err(unsupported(
                    input.span,
                    "callable parameter must be F, &F, &mut F or a single owning State",
                ));
            }
            if owning && count != 0 && !constructor {
                return Err(unsupported(
                    input.span,
                    "owning State operations require one State input; two direct F inputs require a complete State result",
                ));
            }
            if constructor && mode != Some(None) {
                return Err(unsupported(
                    input.span,
                    "owning State constructors require direct by-value F inputs",
                ));
            }
            let mode = mode.unwrap_or(None);
            if first_mode.is_some_and(|prior| prior != mode) {
                return Err(unsupported(
                    input.span,
                    "one callable type cannot mix environment passing modes",
                ));
            }
            first_mode = Some(mode);
            match (kind, mode) {
                (CallableKind::FnMut, None) if !owning => {
                    return Err(unsupported(
                        input.span,
                        "by-value FnMut helper needs an explicit mutable binding; use &mut F in this slice",
                    ));
                }
                (CallableKind::FnMut, Some(false)) | (CallableKind::FnOnce, Some(_)) => {
                    return Err(error(
                        input.span,
                        "callable bound requires its matching mutable or owned environment passing mode",
                    ));
                }
                _ => {}
            }
            count += 1;
            if roles.len() >= 2 {
                return Err(unsupported(
                    input.span,
                    "at most two nonescaping callable environment roles are implemented",
                ));
            }
            roles.push(CallableTemplate {
                index,
                generic_index: generic_index as u32,
                generic_name: parameter.name.clone(),
                kind,
                parameters: parameters.clone(),
                result: result.clone(),
            });
        }
        if function
            .return_type
            .as_ref()
            .is_some_and(|t| refers(t, &parameter.name) && !is_state(t, &parameter.name))
        {
            return Err(unsupported(
                function.span,
                "escaping callable result is not implemented",
            ));
        }
        if count == 0 {
            return Err(error(
                parameter.span,
                "callable bound requires a concrete environment parameter",
            ));
        }
    }
    roles.sort_by_key(|role| role.index);
    Ok(Some(CallableTemplates { roles, owned_state }))
}
