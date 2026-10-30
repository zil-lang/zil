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

/// Admission for an actually owned capture, not for an arbitrary Store
/// element. The inner element deliberately uses the original leaf profile;
/// allowing callback elements here would open recursive owning storage.
pub(super) fn owned_capture(ty: &Type, work: &mut usize, limits: &Limits, span: Span) -> R<bool> {
    let cost = inference::type_snapshot_work(ty, span, limits)?;
    *work = work.checked_sub(cost.saturating_mul(8)).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "Store capture type budget exceeded",
            Some(span),
        )
    })?;
    fn store(ty: &Type) -> bool {
        matches!(ty, Type::Managed(payload)
            if matches!(payload.as_ref(), Type::OwnedStore { element }
                if legacy_element(element) && leaf_content(element)))
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
}

/// Container qualification is explicit. It does not widen captured Stores,
/// owning arrays or general error payloads through the ordinary Product gate.
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
                    if owned_capture(capture, work, limits, span)? =>
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
    let Type::Package { name, .. } = element else {
        unreachable!("checked Store element")
    };
    format!("@builtin:OwnedStore<{name}>")
}

pub(super) fn add_definitions(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    closures: &[ClosureDefinition],
    bound_elements: &[(Type, Span)],
    limits: &Limits,
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
                    let cost = inference::type_snapshot_work(element, function.span, limits)?;
                    work = work
                        .checked_sub(cost.saturating_add(2))
                        .ok_or_else(exhausted)?;
                    if !legacy_element(element) && !product_element(element) {
                        if !callback_element(element, |ty| matches!(ty, Type::Closure(_))) {
                            return Err(unsupported(
                                function.span,
                                "Store elements require finite direct-Managed Packages with at most two self-contained callbacks",
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
                    let name = definition_name(element);
                    if let Some(old) = found.get(&name) {
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
        let cost = inference::type_snapshot_work(element, *span, limits)?;
        work = work
            .checked_sub(cost.saturating_mul(2).saturating_add(2))
            .ok_or_else(exhausted)?;
        if !legacy_element(element) && !product_element(element) {
            if !callback_element(element, |ty| matches!(ty, Type::Closure(_))) {
                return Err(unsupported(
                    *span,
                    "StoreElement bindings require finite direct-Managed Packages with at most two self-contained callbacks",
                ));
            }
            check_callback_captures(element, closures, &mut work, limits, *span)?;
        }
        let name = definition_name(element);
        work = work
            .checked_sub(found.len().saturating_mul(name.len().saturating_add(1)))
            .ok_or_else(exhausted)?;
        if let Some(old) = found.get(&name) {
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
            let work = inference::type_snapshot_work(element, span, self.limits)?;
            self.closures.snapshot(work.saturating_mul(4), span)?;
            if !supported_element(element) {
                return Err(unsupported(
                    span,
                    "Store elements require nonzero complete direct-Managed Packages",
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
            let work = inference::type_snapshot_work(element, span, self.limits)?;
            self.closures.snapshot(work.saturating_mul(4), span)?;
            let element = element.clone();
            if !supported_element(&element) {
                return Err(unsupported(
                    span,
                    "Store elements require nonzero complete direct-Managed Packages",
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
