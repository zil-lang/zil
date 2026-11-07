//! Explicit owning prefix storage. These source operations only build raw IR.
use super::*;

pub(super) fn element(ty: &Type) -> Option<&Type> {
    let (Type::Managed(payload) | Type::Desc(payload)) = ty else {
        return None;
    };
    let Type::OwnedStore { element } = payload.as_ref() else {
        return None;
    };
    Some(element)
}

pub(super) fn is_carrier(ty: &Type) -> bool {
    element(ty).is_some()
}

/// The original path and its charging remain unchanged for non-callback Stores.
fn legacy_element(ty: &Type) -> bool {
    legacy_fields(ty) && has_storage(ty)
}

fn legacy_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| t.is_copy_value()
            || matches!(t, Type::Managed(payload) if **payload == Type::ByteBuffer
                || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))))
}

/// Ordinary closure capture qualification. Callback Store elements call the
/// legacy-only branch below, so ordinary ownership cannot grant StoreElement.
pub(super) fn owned_capture(ty: &Type, work: &mut usize, limits: &Limits, span: Span) -> R<bool> {
    capture_shape(ty, work, limits, span, true)
}

fn callback_owned_capture(ty: &Type, work: &mut usize, limits: &Limits, span: Span) -> R<bool> {
    capture_shape(ty, work, limits, span, false)
}

fn capture_shape(
    ty: &Type,
    work: &mut usize,
    limits: &Limits,
    span: Span,
    ordinary_inline: bool,
) -> R<bool> {
    let cost = inference::type_snapshot_work(ty, span, limits)?;
    *work = work.checked_sub(cost.saturating_mul(8)).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "Store capture type budget exceeded",
            Some(span),
        )
    })?;
    // The complete snapshot above prepays this new structural walk. Existing
    // capture families retain their prior predicate and exact fee.
    fn inline_owner(ty: &Type) -> bool {
        match ty {
            Type::Managed(payload) => matches!(payload.as_ref(), Type::OwnedStore { element }
                if inline_element(element) && supported_element(element)),
            Type::Package { fields, .. } => fields.iter().any(|(_, field)| inline_owner(field)),
            Type::Tuple(fields) => fields.iter().any(inline_owner),
            Type::Enum { variants, .. } => variants
                .iter()
                .any(|(_, fields)| fields.iter().any(inline_owner)),
            Type::ErrorUnion { error, result } => inline_owner(error) || inline_owner(result),
            _ => false,
        }
    }
    if ordinary_inline && inline_owner(ty) {
        return Ok(owned_shapes::value(ty));
    }
    fn store(ty: &Type) -> bool {
        matches!(ty, Type::Managed(payload)
            if matches!(payload.as_ref(), Type::OwnedStore { element }
                if (legacy_element(element) || product_element(element)) && leaf_content(element)))
    }
    fn leaf_content(ty: &Type) -> bool {
        match ty {
            Type::Closure(_)
            | Type::FunctionPointer { .. }
            | Type::ForeignCallback { .. }
            | Type::Address { .. }
            | Type::Desc(_)
            | Type::OwnedStore { .. } => false,
            Type::Tuple(fields) => fields.iter().all(leaf_content),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.iter().all(|(_, field)| leaf_content(field))
            }
            Type::Array { element, .. } | Type::Managed(element) => leaf_content(element),
            Type::Enum { variants, .. } => variants
                .iter()
                .all(|(_, fields)| fields.iter().all(leaf_content)),
            Type::ErrorUnion { error, result } => leaf_content(error) && leaf_content(result),
            _ => true,
        }
    }
    Ok(match ty {
        Type::Managed(_) => store(ty),
        Type::Package { fields, .. } => {
            fields.iter().any(|(_, field)| store(field))
                && fields.iter().all(|(_, field)| {
                    field.is_copy_value()
                        || store(field)
                        || matches!(field, Type::Managed(payload)
                            if **payload == Type::ByteBuffer
                                || (matches!(payload.as_ref(), Type::Record { .. })
                                    && payload.is_copy_value()))
                })
        }
        _ => false,
    })
}

pub(super) fn supported_element(ty: &Type) -> bool {
    legacy_element(ty)
        || callback_element(ty, |ty| matches!(ty, Type::Closure(_)))
        || product_element(ty)
        || pod_element(ty)
}

pub(super) fn scalar_element(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}

/// Shape selector for existing owning-composition exclusions. Qualification
/// still requires the complete element and actual carrier checks below.
pub(super) fn inline_element(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Record { .. } | Type::Tuple(_) | Type::Array { .. }
    ) || scalar_element(ty)
}

/// This private predicate is called only after a complete type snapshot has
/// bounded the input and reserved its traversal. It grants no capture profile.
pub(super) fn pod_element(ty: &Type) -> bool {
    fn bounded(n: Option<u64>) -> Option<u64> {
        n.filter(|n| *n <= i64::MAX as u64)
    }
    fn align(n: u64, a: u64) -> Option<u64> {
        bounded(n.checked_add(a - 1)).map(|n| n / a * a)
    }
    fn fields<'a>(ts: impl IntoIterator<Item = &'a Type>) -> Option<(u64, u64)> {
        let (mut size, mut maximum) = (0u64, 1u64);
        for t in ts {
            let (bytes, alignment) = layout(t)?;
            size = bounded(align(size, alignment)?.checked_add(bytes))?;
            maximum = maximum.max(alignment);
        }
        Some((align(size, maximum)?, maximum))
    }
    fn layout(ty: &Type) -> Option<(u64, u64)> {
        Some(match ty {
            Type::Unit => (0, 1),
            Type::Bool | Type::U8 => (1, 1),
            Type::U16 => (2, 2),
            Type::I32 | Type::F32 => (4, 4),
            Type::I64 | Type::U64 | Type::Usize | Type::F64 => (8, 8),
            Type::Record { fields: ts, .. } => fields(ts.iter().map(|(_, t)| t))?,
            Type::Tuple(ts) => fields(ts)?,
            Type::Array { element, length } => {
                // Validate the element even when no runtime item is present.
                let (size, alignment) = layout(element)?;
                (bounded(size.checked_mul(u64::from(*length)))?, alignment)
            }
            _ => return None,
        })
    }
    inline_element(ty) && layout(ty).is_some_and(|(size, _)| size > 0)
}

/// Extra work is exclusive to the inline Record/scalar family. Legacy Package receipts
/// keep their exact charges. Call before the private POD predicate or new copy.
pub(super) fn charge_pod(ty: &Type, limits: &Limits, span: Span, remaining: &mut usize) -> R<()> {
    if inline_element(ty) {
        inference::charge_type_snapshot(ty, span, limits, remaining, 4)?;
        if matches!(ty, Type::Tuple(_) | Type::Array { .. }) {
            *remaining = remaining.checked_sub(96).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "POD Store identity budget exceeded",
                    Some(span),
                )
            })?;
        }
    }
    Ok(())
}

/// Container qualification is explicit. Owning captures add their own complete
/// hidden-type check; arrays and general error payloads remain separate gates.
pub(super) fn product_element(ty: &Type) -> bool {
    packages::product::nested(ty) && packages::product::supported(ty) && has_storage(ty)
}

/// These two exact wrappers are the existing push/pop protocol, not arbitrary
/// Product error unions. Every path still consumes one actual active tag.
pub(super) fn product_protocol(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if (**error == Type::Unit && product_element(result))
            || (**result == Type::Unit && product_element(error)))
}

/// This checks only the exact finite shell. Concrete capture definitions are
/// checked after all functions have been materialized, including unused types.
pub(super) fn callback_element(ty: &Type, accepts: impl Fn(&Type) -> bool) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    let mut count = 0usize;
    let mut owns = false;
    for (_, field) in fields {
        if accepts(field) {
            count += 1;
        } else if matches!(field, Type::Managed(payload) if **payload == Type::ByteBuffer
            || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))
        {
            owns = true;
        } else if !field.is_copy_value() {
            return false;
        }
    }
    owns && (1..=2).contains(&count) && has_storage(ty)
}

fn check_callback_captures(
    element: &Type,
    closures: &[ClosureDefinition],
    work: &mut usize,
    limits: &Limits,
    span: Span,
) -> R<()> {
    let exhausted = || {
        Diagnostic::limit(
            Phase::Types,
            "Store callback capture budget exceeded",
            Some(span),
        )
    };
    let Type::Package { fields, .. } = element else {
        unreachable!("finite callback element")
    };
    *work = work.checked_sub(fields.len()).ok_or_else(exhausted)?;
    let mut pending = fields
        .iter()
        .filter_map(|(_, ty)| {
            if let Type::Closure(id) = ty {
                Some((*id, 0usize))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if pending.len() == 2 && pending[0].0 != pending[1].0 {
        *work = work.checked_sub(2).ok_or_else(exhausted)?;
        for (id, _) in &pending {
            let definition = closures
                .get(id.0 as usize)
                .filter(|definition| definition.id == *id)
                .ok_or_else(|| {
                    error(
                        "E-CLOSURE-INFERENCE",
                        span,
                        "Store callback definition is absent",
                    )
                })?;
            let cost = inference::type_snapshot_work(&definition.result, span, limits)?;
            *work = work.checked_sub(cost).ok_or_else(exhausted)?;
            if !definition.result.is_copy_value() {
                return Err(unsupported(
                    span,
                    "two distinct Store callback types require Copy results",
                ));
            }
        }
    }
    let mut has_owned_store = false;
    while let Some((id, depth)) = pending.pop() {
        *work = work.checked_sub(1).ok_or_else(exhausted)?;
        if depth > limits.max_call_depth {
            return Err(exhausted());
        }
        let definition = closures
            .get(id.0 as usize)
            .filter(|d| d.id == id)
            .ok_or_else(|| {
                error(
                    "E-CLOSURE-INFERENCE",
                    span,
                    "Store callback definition is absent",
                )
            })?;
        *work = work
            .checked_sub(definition.captures.len())
            .ok_or_else(exhausted)?;
        for capture in &definition.captures {
            let cost = inference::type_snapshot_work(capture, span, limits)?;
            *work = work.checked_sub(cost).ok_or_else(exhausted)?;
            match capture {
                Type::Closure(child) => pending.push((*child, depth + 1)),
                Type::Managed(payload)
                    if **payload == Type::ByteBuffer
                        || (matches!(payload.as_ref(), Type::Record { .. })
                            && payload.is_copy_value()) => {}
                // A captured Package may have zero payload bytes. Its real
                // environment retains the existing shell/field live flags;
                // only the complete Store element must have nonzero layout.
                Type::Package { .. } if legacy_fields(capture) => {}
                Type::Package { .. } if packages::product::nested(capture) => {
                    packages::product::validate(capture, work, limits.max_call_depth, span)?;
                }
                Type::Managed(_) | Type::Package { .. }
                    if callback_owned_capture(capture, work, limits, span)? =>
                {
                    has_owned_store = true;
                }
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        span,
                        "Store callbacks require self-contained finite captures and qualified owned leaf Store captures",
                    ));
                }
            }
        }
    }
    if has_owned_store {
        // This additional graph walk is exclusive to the new capture profile.
        // A sibling old lending environment is not subject to its ancestor's
        // result restriction unless that sibling itself owns a Store child.
        *work = work.checked_sub(fields.len()).ok_or_else(exhausted)?;
        let mut profiles = BTreeMap::new();
        for (_, field) in fields {
            if let Type::Closure(id) = field {
                owned_capture_profile(*id, closures, &mut profiles, work, limits, 0, span)?;
            }
        }
    }
    Ok(())
}

fn owned_capture_profile(
    id: ClosureId,
    closures: &[ClosureDefinition],
    profiles: &mut BTreeMap<ClosureId, Option<bool>>,
    work: &mut usize,
    limits: &Limits,
    depth: usize,
    span: Span,
) -> R<bool> {
    let exhausted = || {
        Diagnostic::limit(
            Phase::Types,
            "Store capture graph budget exceeded",
            Some(span),
        )
    };
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    let lookup = profiles.len().checked_ilog2().map_or(1, |n| n as usize + 2);
    *work = work
        .checked_sub(lookup.saturating_mul(3).saturating_add(1))
        .ok_or_else(exhausted)?;
    if let Some(profile) = profiles.get(&id) {
        return profile.ok_or_else(|| unsupported(span, "cyclic owned Store capture graph"));
    }
    profiles.insert(id, None);
    let definition = closures
        .get(id.0 as usize)
        .filter(|definition| definition.id == id)
        .ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                span,
                "Store callback definition is absent",
            )
        })?;
    *work = work
        .checked_sub(definition.captures.len())
        .ok_or_else(exhausted)?;
    let mut owns = false;
    for capture in &definition.captures {
        match capture {
            Type::Closure(child) => {
                owns |= owned_capture_profile(
                    *child,
                    closures,
                    profiles,
                    work,
                    limits,
                    depth + 1,
                    span,
                )?;
            }
            Type::Managed(_) => owns |= is_carrier(capture),
            Type::Package { fields, .. } => {
                *work = work.checked_sub(fields.len()).ok_or_else(exhausted)?;
                owns |= fields.iter().any(|(_, field)| is_carrier(field));
            }
            _ => {}
        }
    }
    if owns {
        let cost = inference::type_snapshot_work(&definition.result, span, limits)?;
        *work = work.checked_sub(cost).ok_or_else(exhausted)?;
        if !definition.result.is_copy_value() {
            return Err(unsupported(
                span,
                "owned Store capture environments require Copy results",
            ));
        }
    }
    // Descendant insertions may have increased the tree since the first lookup.
    let lookup = profiles.len().checked_ilog2().map_or(1, |n| n as usize + 2);
    *work = work.checked_sub(lookup).ok_or_else(exhausted)?;
    profiles.insert(id, Some(owns));
    Ok(owns)
}

fn has_storage(ty: &Type) -> bool {
    match ty {
        Type::Unit => false,
        Type::Managed(payload) => has_storage(payload),
        Type::Tuple(fields) => fields.iter().any(has_storage),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, t)| has_storage(t))
        }
        Type::Array { element, length } => *length > 0 && has_storage(element),
        // Enum and error representations include a tag even for Unit payloads.
        _ => true,
    }
}

pub(super) fn primitive(expression: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(name) = &expression.kind else {
        return None;
    };
    matches!(
        name.as_str(),
        "store_new"
            | "store_reserve"
            | "store_push"
            | "store_pop"
            | "store_len"
            | "store_capacity"
            | "store_at"
            | "store_at_mut"
    )
    .then_some(name)
}

/// Recognize a direct primitive result without cloning its Package type.
pub(super) fn borrow_expression(expression: &CoreExpr) -> bool {
    matches!(&expression.kind, CoreExprKind::Call { callee, .. }
        if matches!(primitive(callee), Some("store_at" | "store_at_mut")))
}

pub(super) fn result_type(name: &str, element: &Type) -> Type {
    match name {
        "store_new" => Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        })),
        "store_reserve" => Type::Bool,
        "store_at" | "store_at_mut" => Type::Address {
            mutable: name == "store_at_mut",
            inner: Box::new(element.clone()),
        },
        "store_push" => Type::ErrorUnion {
            error: Box::new(element.clone()),
            result: Box::new(Type::Unit),
        },
        "store_pop" => Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(element.clone()),
        },
        "store_len" | "store_capacity" => Type::I64,
        _ => unreachable!("known Store primitive"),
    }
}

pub(super) fn definition_name(element: &Type) -> String {
    if matches!(element, Type::Tuple(_)) {
        return format!("@builtin:OwnedStore<tuple:{}>", element.fingerprint());
    }
    if matches!(element, Type::Array { .. }) {
        return format!("@builtin:OwnedStore<array:{}>", element.fingerprint());
    }
    let name = match element {
        Type::Package { name, .. } | Type::Record { name, .. } => name.as_str(),
        Type::Bool => "bool",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::F32 => "f32",
        Type::F64 => "f64",
        _ => unreachable!("checked Store element"),
    };
    format!("@builtin:OwnedStore<{name}>")
}

// Aggregate resolution already bounds each complete nominal tree. This
// selector keeps the old Tuple/Enum-only requests off the new Store scan; the
// selected full subtree is charged before any new pending entries are grown.
pub(super) fn contains_inline_carrier(ty: &Type) -> bool {
    match ty {
        Type::Managed(payload) => matches!(payload.as_ref(), Type::OwnedStore { element }
            if inline_element(element) && supported_element(element)),
        Type::Package { fields, .. } => fields
            .iter()
            .any(|(_, child)| contains_inline_carrier(child)),
        Type::Tuple(fields) => fields.iter().any(contains_inline_carrier),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(contains_inline_carrier)),
        Type::ErrorUnion { error, result } => {
            contains_inline_carrier(error) || contains_inline_carrier(result)
        }
        _ => false,
    }
}

pub(super) fn add_definitions(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    closures: &[ClosureDefinition],
    bound_elements: &[(Type, Span)],
    limits: &Limits,
    remaining: &mut usize,
) -> R<()> {
    let mut work = limits.max_analysis_steps;
    let exhausted = || {
        Diagnostic::limit(
            Phase::Types,
            "owned Store definition scan exceeded budget",
            None,
        )
    };
    let mut found = BTreeMap::<String, Type>::new();
    for function in functions {
        let roots = function
            .values
            .len()
            .checked_add(function.locals.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(exhausted)?;
        work = work.checked_sub(roots).ok_or_else(exhausted)?;
        for ty in function
            .values
            .iter()
            .chain(function.locals.iter().map(|l| &l.ty))
            .chain(std::iter::once(&function.return_type))
        {
            let mut pending = vec![ty];
            while let Some(ty) = pending.pop() {
                if let Some(element) = element(ty) {
                    charge_pod(element, limits, function.span, remaining)?;
                    let cost = inference::type_snapshot_work(element, function.span, limits)?;
                    work = work
                        .checked_sub(cost.saturating_add(2))
                        .ok_or_else(exhausted)?;
                    if !legacy_element(element)
                        && !product_element(element)
                        && !pod_element(element)
                    {
                        if !callback_element(element, |ty| matches!(ty, Type::Closure(_))) {
                            return Err(unsupported(
                                function.span,
                                "Store elements require finite POD Records or supported owning Packages",
                            ));
                        }
                        check_callback_captures(
                            element,
                            closures,
                            &mut work,
                            limits,
                            function.span,
                        )?;
                    }
                    if inline_element(element) {
                        // Cover the canonical name copy and ordered-map key
                        // comparisons before constructing or looking up it.
                        *remaining = remaining
                            .checked_sub(cost.saturating_mul(found.len().saturating_add(2)))
                            .ok_or_else(exhausted)?;
                    }
                    let name = definition_name(element);
                    if let Some(old) = found.get(&name) {
                        if inline_element(element) {
                            inference::charge_type_snapshot(
                                old,
                                function.span,
                                limits,
                                remaining,
                                1,
                            )?;
                            *remaining = remaining.checked_sub(cost).ok_or_else(exhausted)?;
                        }
                        if old != element {
                            return Err(error(
                                "E-OWNED-STORE-TYPE",
                                function.span,
                                "one Store nominal element has conflicting layouts",
                            ));
                        }
                    } else {
                        found.insert(name, element.clone());
                    }
                    // Its exact element predicate excludes another Store.
                    continue;
                }
                // A Store may only be mentioned by a complete State parameter
                // or result, with no direct Store SSA value in this module.
                // Charge children before growing the bounded worklist.
                match ty {
                    Type::Package { fields, .. } => {
                        work = work.checked_sub(fields.len()).ok_or_else(exhausted)?;
                        pending.extend(fields.iter().map(|(_, field)| field));
                    }
                    Type::Tuple(fields) if contains_inline_carrier(ty) => {
                        inference::charge_type_snapshot(ty, function.span, limits, remaining, 2)?;
                        work = work.checked_sub(fields.len()).ok_or_else(exhausted)?;
                        pending.extend(fields);
                    }
                    Type::Enum { variants, .. } if contains_inline_carrier(ty) => {
                        inference::charge_type_snapshot(ty, function.span, limits, remaining, 2)?;
                        let children = variants
                            .iter()
                            .try_fold(0usize, |sum, (_, fields)| sum.checked_add(fields.len()))
                            .ok_or_else(exhausted)?;
                        work = work.checked_sub(children).ok_or_else(exhausted)?;
                        pending.extend(variants.iter().flat_map(|(_, fields)| fields));
                    }
                    Type::Array { element, .. }
                    | Type::Opaque {
                        hidden: element, ..
                    } => {
                        work = work.checked_sub(1).ok_or_else(exhausted)?;
                        pending.push(element);
                    }
                    Type::ErrorUnion { error, result } => {
                        work = work.checked_sub(2).ok_or_else(exhausted)?;
                        pending.extend([error.as_ref(), result.as_ref()]);
                    }
                    _ => {}
                }
            }
        }
    }
    // A complete ordinary StoreElement binding is an untrusted qualification
    // constraint even when its body only transfers T and never makes a Store.
    // Reuse the same canonical definition, with no value or ownership event.
    for (element, span) in bound_elements {
        charge_pod(element, limits, *span, remaining)?;
        let cost = inference::type_snapshot_work(element, *span, limits)?;
        work = work
            .checked_sub(cost.saturating_mul(2).saturating_add(2))
            .ok_or_else(exhausted)?;
        if !legacy_element(element) && !product_element(element) && !pod_element(element) {
            if !callback_element(element, |ty| matches!(ty, Type::Closure(_))) {
                return Err(unsupported(
                    *span,
                    "StoreElement bindings require finite POD Records or supported owning Packages",
                ));
            }
            check_callback_captures(element, closures, &mut work, limits, *span)?;
        }
        if inline_element(element) {
            *remaining = remaining
                .checked_sub(cost.saturating_mul(found.len().saturating_add(2)))
                .ok_or_else(exhausted)?;
        }
        let name = definition_name(element);
        work = work
            .checked_sub(found.len().saturating_mul(name.len().saturating_add(1)))
            .ok_or_else(exhausted)?;
        if let Some(old) = found.get(&name) {
            if inline_element(element) {
                inference::charge_type_snapshot(old, *span, limits, remaining, 1)?;
                *remaining = remaining.checked_sub(cost).ok_or_else(exhausted)?;
            }
            let old_cost = inference::type_snapshot_work(old, *span, limits)?;
            work = work
                .checked_sub(cost.saturating_add(old_cost))
                .ok_or_else(exhausted)?;
            if old != element {
                return Err(error(
                    "E-OWNED-STORE-TYPE",
                    *span,
                    "one Store nominal element has conflicting layouts",
                ));
            }
        } else {
            work = work
                .checked_sub(cost.saturating_add(1))
                .ok_or_else(exhausted)?;
            found.insert(name, element.clone());
        }
    }
    for (name, element) in found {
        definitions.push(ResourceDefinition {
            name,
            payload: Type::OwnedStore {
                element: Box::new(element),
            },
            teardown: None,
        });
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn dynamic_store_call(
        &mut self,
        name: &str,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let arity = match name {
            "store_new" => 0,
            "store_reserve" | "store_push" | "store_at" | "store_at_mut" => 2,
            _ => 1,
        };
        if arguments.len() != arity {
            return Err(error(
                "E-CALL-ARITY",
                span,
                format!("{name} requires {arity} arguments"),
            ));
        }
        let (kind, ty) = if name == "store_new" {
            let element = expected.and_then(element).ok_or_else(|| error("E-OWNED-STORE-TYPE", span, "store_new requires a concrete Store<Package> annotation or owning result context"))?;
            if inline_element(element) {
                self.closures.snapshot_type(element, 4, 0, span)?;
            }
            let work = inference::type_snapshot_work(element, span, self.limits)?;
            self.closures.snapshot(work.saturating_mul(4), span)?;
            if !supported_element(element) {
                return Err(unsupported(
                    span,
                    "Store elements require nonzero complete direct-Managed Packages or finite POD Records",
                ));
            }
            (
                InstructionKind::OwnedStoreNew {
                    element: element.clone(),
                },
                result_type(name, element),
            )
        } else {
            // Capture the descriptor before RHS evaluation, then recheck its
            // liveness at the primitive in independent resource analysis.
            let (input, actual) = self.expr(&arguments[0], None)?;
            let (store, actual) = self.resource_descriptor(input, actual, arguments[0].span)?;
            let element = element(&actual).ok_or_else(|| {
                error(
                    "E-OWNED-STORE-TYPE",
                    arguments[0].span,
                    "Store primitive requires its real resource description",
                )
            })?;
            if inline_element(element) {
                self.closures.snapshot_type(element, 4, 0, span)?;
            }
            let work = inference::type_snapshot_work(element, span, self.limits)?;
            self.closures.snapshot(work.saturating_mul(4), span)?;
            let element = element.clone();
            if !supported_element(&element) {
                return Err(unsupported(
                    span,
                    "Store elements require nonzero complete direct-Managed Packages or finite POD Records",
                ));
            }
            let kind = match name {
                "store_reserve" => {
                    let (capacity, actual) = self.expr(&arguments[1], Some(&Type::I64))?;
                    self.same(&actual, &Type::I64, arguments[1].span)?;
                    InstructionKind::OwnedStoreReserve { store, capacity }
                }
                "store_push" => {
                    let (value, actual) = self.transfer_expr(&arguments[1], Some(&element))?;
                    self.same(&actual, &element, arguments[1].span)?;
                    InstructionKind::OwnedStorePush { store, value }
                }
                "store_at" | "store_at_mut" => {
                    if name == "store_at_mut" && packages::product::nested(&element) {
                        return Err(unsupported(
                            span,
                            "Product Store elements currently expose only shared windows",
                        ));
                    }
                    let (index, actual) = self.expr(&arguments[1], Some(&Type::I64))?;
                    self.same(&actual, &Type::I64, arguments[1].span)?;
                    if name == "store_at_mut" {
                        InstructionKind::OwnedStoreBorrowMut { store, index }
                    } else {
                        InstructionKind::OwnedStoreBorrow { store, index }
                    }
                }
                "store_pop" => InstructionKind::OwnedStorePop { store },
                "store_len" => InstructionKind::OwnedStoreLen { store },
                "store_capacity" => InstructionKind::OwnedStoreCapacity { store },
                _ => unreachable!("resolved Store primitive"),
            };
            (kind, result_type(name, &element))
        };
        if let Some(expected) = expected
            && !(name == "store_new"
                && matches!((&ty, expected), (Type::Managed(a), Type::Desc(b)) if a == b))
        {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(kind, ty.clone(), span)?;
        Ok((value, ty))
    }
}

#[cfg(test)]
mod pod_tests {
    use super::*;

    fn record(field: Type) -> Type {
        Type::Record {
            name: "Vertex".into(),
            fields: vec![("value".into(), field)],
        }
    }

    #[test]
    fn pod_store_requires_complete_nonzero_numeric_records() {
        let numeric = record(Type::Tuple(vec![
            Type::F32,
            Type::F64,
            Type::U64,
            Type::Usize,
            Type::Array {
                element: Box::new(Type::Unit),
                length: 0,
            },
        ]));
        assert!(pod_element(&numeric));
        assert!(supported_element(&numeric));
        assert_eq!(definition_name(&numeric), "@builtin:OwnedStore<Vertex>");
        for field in [
            Type::Unit,
            Type::ForeignHandle,
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(Type::I32),
            },
            Type::Array {
                element: Box::new(Type::ForeignHandle),
                length: 0,
            },
            Type::Array {
                element: Box::new(Type::Array {
                    element: Box::new(Type::U64),
                    length: u32::MAX,
                }),
                length: u32::MAX,
            },
        ] {
            assert!(!pod_element(&record(field)));
        }
        assert!(pod_element(&Type::I32));
        assert!(pod_element(&Type::Tuple(vec![Type::I32])));
        assert!(!pod_element(&Type::Tuple(vec![Type::Unit])));
        assert!(!pod_element(&Type::Array {
            element: Box::new(Type::I32),
            length: 0
        }));
        assert!(!pod_element(&Type::Array {
            element: Box::new(Type::Array {
                element: Box::new(Type::U64),
                length: u32::MAX,
            }),
            length: u32::MAX,
        }));
        let tuple = Type::Tuple(vec![Type::I32, Type::I32]);
        let array = Type::Array {
            element: Box::new(Type::I32),
            length: 2,
        };
        assert!(pod_element(&tuple) && pod_element(&array));
        assert_ne!(definition_name(&tuple), definition_name(&array));
        assert_eq!(
            definition_name(&tuple),
            format!("@builtin:OwnedStore<tuple:{}>", tuple.fingerprint())
        );
        assert_eq!(
            definition_name(&array),
            format!("@builtin:OwnedStore<array:{}>", array.fingerprint())
        );
    }

    #[test]
    fn scalar_store_keeps_exact_identity_and_owning_boundary() {
        for (element, name) in [
            (Type::Bool, "bool"),
            (Type::U8, "u8"),
            (Type::U16, "u16"),
            (Type::I32, "i32"),
            (Type::I64, "i64"),
            (Type::U64, "u64"),
            (Type::Usize, "usize"),
            (Type::F32, "f32"),
            (Type::F64, "f64"),
        ] {
            assert!(supported_element(&element));
            assert_eq!(
                definition_name(&element),
                format!("@builtin:OwnedStore<{name}>")
            );
            let head = Type::Managed(Box::new(Type::OwnedStore {
                element: Box::new(element.clone()),
            }));
            assert!(resources::supported(&head));
            assert!(owned_shapes::value(&Type::Package {
                name: "Outer".into(),
                fields: vec![("items".into(), head)],
            }));
        }
        assert_ne!(definition_name(&Type::U64), definition_name(&Type::Usize));
        for forbidden in [
            Type::Unit,
            Type::Tuple(vec![Type::Unit]),
            Type::ForeignHandle,
        ] {
            assert!(!supported_element(&forbidden));
        }
    }

    #[test]
    fn ordinary_capture_does_not_grant_callback_store_element_qualification() {
        let head = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(Type::I32),
        }));
        let parent = Type::Package {
            name: "Owner".into(),
            fields: vec![("items".into(), head.clone())],
        };
        let limits = Limits::default();
        let span = Span::default();
        for ty in [&head, &parent] {
            let mut ordinary = limits.max_analysis_steps;
            let mut callback = limits.max_analysis_steps;
            assert!(owned_capture(ty, &mut ordinary, &limits, span).unwrap());
            assert!(!callback_owned_capture(ty, &mut callback, &limits, span).unwrap());
            assert_eq!(ordinary, callback);
        }
    }

    #[test]
    fn pod_qualification_work_has_exact_failure_and_fresh_retry() {
        let ty = record(Type::Array {
            element: Box::new(Type::F32),
            length: 5,
        });
        let limits = Limits::default();
        let span = Span::new(zil_base::FileId(0), 0, 1);
        let mut remaining = limits.max_analysis_steps;
        charge_pod(&ty, &limits, span, &mut remaining).unwrap();
        let cost = limits.max_analysis_steps - remaining;
        assert!(cost > 0);
        for allowance in [cost, cost - 1, cost] {
            let mut remaining = allowance;
            let result = charge_pod(&ty, &limits, span, &mut remaining);
            assert_eq!(result.is_ok(), allowance == cost);
        }
    }
    #[test]
    fn composite_root_registration_has_exact_fresh_budget() {
        let limits = Limits::default();
        let span = Span::new(zil_base::FileId(0), 0, 1);
        for element in [
            Type::Tuple(vec![Type::I32, Type::Bool]),
            Type::Array {
                element: Box::new(Type::I32),
                length: 2,
            },
        ] {
            let bound = [(element.clone(), span)];
            let measure = |allowance| {
                let mut remaining = allowance;
                let mut definitions = Vec::new();
                let result =
                    add_definitions(&[], &mut definitions, &[], &bound, &limits, &mut remaining);
                (result, allowance - remaining, definitions)
            };
            let (result, cost, definitions) = measure(limits.max_analysis_steps);
            result.unwrap();
            assert_eq!(definitions[0].name, definition_name(&element));
            assert!(cost > 0);
            assert!(measure(cost).0.is_ok());
            assert_eq!(
                measure(cost - 1).0.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert!(measure(cost).0.is_ok());
        }
    }
    #[test]
    fn pod_definition_registration_borrows_current_request_work() {
        let ty = record(Type::Tuple(vec![Type::F32, Type::U64]));
        let limits = Limits::default();
        let span = Span::new(zil_base::FileId(0), 0, 1);
        let bound = [(ty.clone(), span)];
        let mut remaining = limits.max_analysis_steps;
        let mut definitions = Vec::new();
        add_definitions(&[], &mut definitions, &[], &bound, &limits, &mut remaining).unwrap();
        let cost = limits.max_analysis_steps - remaining;
        assert!(cost > 0);
        assert_eq!(
            definitions[0].payload,
            Type::OwnedStore {
                element: Box::new(ty)
            }
        );
        for allowance in [cost, cost - 1, cost] {
            let mut remaining = allowance;
            let mut definitions = Vec::new();
            let result =
                add_definitions(&[], &mut definitions, &[], &bound, &limits, &mut remaining);
            assert_eq!(result.is_ok(), allowance == cost);
            if let Err(d) = result {
                assert_eq!(d.category, zil_base::ResultCategory::ResourceLimit);
            }
        }
    }
    #[test]
    fn pod_store_enters_ordinary_ownership_but_not_container_qualification() {
        let head = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(record(Type::F32)),
        }));
        let bag = Type::Package {
            name: "Bag".into(),
            fields: vec![("items".into(), head.clone())],
        };
        assert!(!packages::supported(&bag));
        assert!(packages::ordinary(&bag));
        assert!(packages::product::owned_supported(&bag));
        assert!(owned_shapes::value(&bag));
        assert!(owned_shapes::sum(&Type::Tuple(vec![head.clone()])));
        assert!(!packages::owned_array_supported(&Type::Array {
            element: Box::new(bag.clone()),
            length: 1,
        }));
        assert!(resources::supported(&head));
        let created = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(head.clone()),
        };
        assert!(errors::supported(&created));
        assert!(errors::supported(&Type::ErrorUnion {
            error: Box::new(head.clone()),
            result: Box::new(Type::Unit),
        }));
        let hidden_bag = Type::Package {
            name: "Hidden".into(),
            fields: vec![("items".into(), created)],
        };
        assert!(owned_shapes::value(&hidden_bag));
        let owned = Type::Package {
            name: "Owned".into(),
            fields: vec![("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
        };
        let old_head = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(owned),
        }));
        let old_bag = Type::Package {
            name: "OldBag".into(),
            fields: vec![("items".into(), old_head)],
        };
        assert!(packages::supported(&old_bag));
        assert!(packages::product::owned_supported(&old_bag));
        assert!(owned_shapes::value(&old_bag));
    }
}
