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

/// Keep this first profile independent of future Package composition widening.
pub(super) fn supported_element(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| t.is_copy_value()
            || matches!(t, Type::Managed(payload) if **payload == Type::ByteBuffer
                || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))))
        && has_storage(ty)
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
    )
    .then_some(name)
}

/// Recognize a direct primitive result without cloning its Package type.
pub(super) fn borrow_expression(expression: &CoreExpr) -> bool {
    matches!(&expression.kind, CoreExprKind::Call { callee, .. }
        if primitive(callee) == Some("store_at"))
}

pub(super) fn result_type(name: &str, element: &Type) -> Type {
    match name {
        "store_new" => Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        })),
        "store_reserve" => Type::Bool,
        "store_at" => Type::Address {
            mutable: false,
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
                    if !supported_element(element) {
                        return Err(unsupported(
                            function.span,
                            "Store elements require nonzero complete direct-Managed Packages",
                        ));
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
            "store_reserve" | "store_push" | "store_at" => 2,
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
                "store_at" => {
                    let (index, actual) = self.expr(&arguments[1], Some(&Type::I64))?;
                    self.same(&actual, &Type::I64, arguments[1].span)?;
                    InstructionKind::OwnedStoreBorrow { store, index }
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
