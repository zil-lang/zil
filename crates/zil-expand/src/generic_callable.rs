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
    pub fields: Vec<u32>,
}

#[derive(Clone)]
pub(crate) struct CallableTemplates {
    pub roles: Vec<CallableTemplate>,
    pub state_bindings: Vec<CallableTemplate>,
    pub owned_state: bool,
}
/// A borrowed lexical capability set. This is never an ownership certificate.
/// Empty/single paths preserve their previous traversal and accounting.
#[derive(Clone, Copy, Default)]
pub(super) enum StoreCallbacks<'a> {
    #[default]
    None,
    Single(&'a str),
    Pair([&'a str; 2]),
}
impl<'a> StoreCallbacks<'a> {
    pub(super) fn from_names(single: Option<&'a str>, pair: Option<&'a [String; 2]>) -> Self {
        if let Some(pair) = pair {
            Self::Pair([pair[0].as_str(), pair[1].as_str()])
        } else {
            single.map_or(Self::None, Self::Single)
        }
    }
    pub(super) fn contains(self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Single(bound) => bound == name,
            Self::Pair(bounds) => bounds.contains(&name),
        }
    }
    pub(super) fn field(self, ty: &TypeRef) -> bool {
        match self {
            Self::None => false,
            Self::Single(name) => parameter_mode(ty, name) == Some(None),
            Self::Pair(names) => names
                .iter()
                .any(|name| parameter_mode(ty, name) == Some(None)),
        }
    }
    pub(super) fn active(self, ty: &TypeRef) -> bool {
        match self {
            Self::None => false,
            Self::Single(name) => state_type(ty, name),
            Self::Pair(names) => names.iter().any(|name| dual_state_type(ty, name)),
        }
    }
}
impl CallableTemplates {
    pub(crate) fn bindings(&self) -> impl Iterator<Item = &CallableTemplate> {
        self.roles.iter().chain(&self.state_bindings)
    }
    pub(crate) fn contains_generic(&self, name: &str) -> bool {
        self.bindings().any(|role| role.generic_name == name)
    }
    pub(crate) fn single_generic(&self) -> Option<&str> {
        let first = self.bindings().next()?;
        self.bindings()
            .all(|role| role.generic_index == first.generic_index)
            .then_some(first.generic_name.as_str())
    }
    pub(crate) fn dual_names(&self) -> Option<[&str; 2]> {
        if !self.owned_state || self.single_generic().is_some() {
            return None;
        }
        Some([
            self.bindings()
                .find(|b| b.generic_index == 0)?
                .generic_name
                .as_str(),
            self.bindings()
                .find(|b| b.generic_index == 1)?
                .generic_name
                .as_str(),
        ])
    }
    pub(super) fn store_callbacks(&self) -> StoreCallbacks<'_> {
        if !self.owned_state {
            StoreCallbacks::None
        } else if let Some(name) = self.single_generic() {
            StoreCallbacks::Single(name)
        } else if let Some(names) = self.dual_names() {
            StoreCallbacks::Pair(names)
        } else {
            StoreCallbacks::None
        }
    }
}

pub(super) fn dual_scope(
    bounds: Option<&CallableTemplates>,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<[String; 2]>> {
    let Some(names) = bounds.and_then(CallableTemplates::dual_names) else {
        return Ok(None);
    };
    budget.node(span, 0)?;
    budget.text(names[0], span)?;
    budget.text(names[1], span)?;
    Ok(Some([names[0].to_owned(), names[1].to_owned()]))
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

/// Two callable slots may sit beside the already finite ordinary Copy slot.
/// The actual aggregate and its two direct selectors are checked after expansion.
fn dual_state_type(ty: &TypeRef, name: &str) -> bool {
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return false;
    };
    path.as_slice() != [name]
        && (1..=3).contains(&args.len())
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

fn state_union_result(
    ty: &TypeRef,
    name: &str,
    is_state: &impl Fn(&TypeRef, &str, bool) -> bool,
) -> bool {
    let TypeKind::Error { error, result } = &grouped(ty).kind else {
        return false;
    };
    [error.as_ref(), result.as_ref()].into_iter().all(|side| {
        !refers(side, name)
            || is_state(side, name, true)
            || state_union_result(side, name, is_state)
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
    template_with_state(function, |ty, name, dual| {
        if dual {
            dual_state_type(ty, name)
        } else {
            state_type(ty, name)
        }
    })
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
    let mut classified = template_with_state(function, |ty, name, dual| {
        if if dual {
            dual_state_type(ty, name)
        } else {
            state_type(ty, name)
        } {
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
    })?;
    if let Some(bounds) = classified.as_mut()
        && let Some(names) = bounds.dual_names()
    {
        // Only the new two-root profile scans and copies selectors here. Pay
        // before either allocation; legacy single-root charges stay exact.
        let ty = if let Some(binding) = bounds.state_bindings.first() {
            &function.params[binding.index].ty
        } else {
            function.return_type.as_ref().ok_or_else(|| {
                error(
                    function.span,
                    "two callable constructor requires a complete State result",
                )
            })?
        };
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return Err(unsupported(
                ty.span,
                "two callable State requires a direct Package",
            ));
        };
        let aggregate = (path.len() == 1 && args.is_empty())
            .then(|| declarations.get(&path[0]))
            .flatten()
            .filter(|aggregate| aggregate.kind == AggregateKind::Package)
            .ok_or_else(|| {
                unsupported(ty.span, "two callable State requires a complete Package")
            })?;
        let work = aggregate
            .fields
            .len()
            .saturating_add(names[0].len())
            .saturating_add(names[1].len())
            .saturating_add(2);
        *remaining = remaining.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "State binding selector work budget exceeded",
                Some(ty.span),
            )
        })?;
        let mut selectors = [Vec::new(), Vec::new()];
        for (index, field) in aggregate.fields.iter().enumerate() {
            for (slot, name) in names.iter().enumerate() {
                if parameter_mode(&field.ty, name) == Some(None) && !field.manages {
                    selectors[slot].push(index as u32);
                } else if refers(&field.ty, name) {
                    return Err(unsupported(
                        field.span,
                        "State bindings require direct callback fields",
                    ));
                }
            }
        }
        if selectors.iter().any(|fields| fields.len() != 1) {
            return Err(unsupported(
                ty.span,
                "two callable State requires exactly one direct field per declared callable type",
            ));
        }
        for binding in &mut bounds.state_bindings {
            binding.fields = std::mem::take(&mut selectors[binding.generic_index as usize]);
        }
    }
    Ok(classified)
}

fn template_with_state(
    function: &Function,
    is_state: impl Fn(&TypeRef, &str, bool) -> bool,
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
    let mut state_bindings = Vec::new();
    let dual = callables.len() == 2;
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
            .is_some_and(|t| is_state(t, &parameter.name, dual));
        let takes_state = function
            .params
            .iter()
            .any(|p| is_state(&p.ty, &parameter.name, dual));
        let owning = returns_state || takes_state;
        // Two occurrences of one F are admitted only at the complete owning
        // constructor boundary. A State plus another F, or two State inputs,
        // still requires a distinct profile and cannot use this exception.
        let constructor = returns_state && !takes_state;
        owned_state |= owning;
        let mut count = 0;
        let mut first_mode = None;
        for (index, input) in function.params.iter().enumerate() {
            if !refers(&input.ty, &parameter.name) && !is_state(&input.ty, &parameter.name, dual) {
                continue;
            }
            let mode = parameter_mode(&input.ty, &parameter.name);
            if (mode.is_none() && !is_state(&input.ty, &parameter.name, dual)) || input.give {
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
            let binding = CallableTemplate {
                index,
                generic_index: generic_index as u32,
                generic_name: parameter.name.clone(),
                kind,
                parameters: parameters.clone(),
                result: result.clone(),
                fields: Vec::new(),
            };
            if dual && owning && !constructor {
                state_bindings.push(binding);
            } else {
                roles.push(binding);
            }
        }
        if function.return_type.as_ref().is_some_and(|t| {
            refers(t, &parameter.name)
                && !is_state(t, &parameter.name, dual)
                && !(dual && takes_state && state_union_result(t, &parameter.name, &is_state))
        }) {
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
    if dual && owned_state {
        if !state_bindings.is_empty() {
            if !roles.is_empty()
                || state_bindings.len() != 2
                || state_bindings[0].index != state_bindings[1].index
            {
                return Err(unsupported(
                    function.span,
                    "two callable State bindings require one actual owning State parameter",
                ));
            }
        } else if roles.len() != 2 || roles[0].generic_index == roles[1].generic_index {
            return Err(unsupported(
                function.span,
                "two callable State constructors require two distinct direct declared inputs",
            ));
        }
    }
    roles.sort_by_key(|role| role.index);
    Ok(Some(CallableTemplates {
        roles,
        state_bindings,
        owned_state,
    }))
}
