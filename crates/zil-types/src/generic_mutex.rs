//! Source construction of untrusted generic mutex candidates. The two
//! independent kernels establish the actual head, guard and child-loan facts.
use super::*;

pub(super) fn primitive(name: &str) -> bool {
    matches!(
        name,
        "mutex_create" | "mutex_lock" | "mutex_value" | "mutex_unlock"
    )
}

pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.sync:v1:MutexCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

pub(super) fn guard(payload: &Type) -> Type {
    Type::MutexGuard {
        payload: Box::new(payload.clone()),
        retained_life: RetainedLifeShape {
            binder_count: 1,
            ports: vec![RetainedBorrowPort {
                leaf: Type::Mutex {
                    payload: Box::new(payload.clone()),
                },
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }
}

pub(super) fn add_definitions(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    limits: &Limits,
    remaining: &mut usize,
) -> R<()> {
    let mut found = BTreeMap::new();
    for function in functions {
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(&function.values)
        {
            inference::charge_type_snapshot(ty, function.span, limits, remaining, 2)?;
            let mut pending = vec![ty];
            while let Some(ty) = pending.pop() {
                *remaining = remaining.checked_sub(1).ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "mutex definition inventory exceeded",
                        Some(function.span),
                    )
                })?;
                match ty {
                    Type::Mutex { payload } => {
                        let name = format!("@builtin:Mutex<{}>", payload.fingerprint());
                        found.insert(name, ty.clone());
                        pending.push(payload);
                    }
                    Type::Managed(inner)
                    | Type::Desc(inner)
                    | Type::Address { inner, .. }
                    | Type::Slice { element: inner }
                    | Type::OwnedStore { element: inner }
                    | Type::Array { element: inner, .. }
                    | Type::MutexGuard { payload: inner, .. } => pending.push(inner),
                    Type::Tuple(fields) => pending.extend(fields),
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        pending.extend(fields.iter().map(|(_, field)| field))
                    }
                    Type::Enum { variants, .. } => {
                        pending.extend(variants.iter().flat_map(|(_, fields)| fields))
                    }
                    Type::ErrorUnion { error, result } => {
                        pending.extend([error.as_ref(), result.as_ref()])
                    }
                    _ => {}
                }
            }
        }
    }
    for (name, payload) in found {
        definitions.push(ResourceDefinition {
            name,
            payload,
            teardown: None,
        });
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn generic_mutex_call(
        &mut self,
        name: &str,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [argument] = arguments else {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "mutex operation requires one argument",
            ));
        };
        let (kind, ty) = match name {
            "mutex_create" => {
                let (value, payload) = self.transfer_expr(argument, None)?;
                self.closures.snapshot_type(&payload, 4, 0, span)?;
                if !owned_shapes::value(&payload) {
                    return Err(unsupported(
                        span,
                        "mutex payload requires a complete ordinary Copy or owning value",
                    ));
                }
                let ty = Type::ErrorUnion {
                    error: Box::new(create_error()),
                    result: Box::new(Type::Managed(Box::new(Type::Mutex {
                        payload: Box::new(payload),
                    }))),
                };
                (InstructionKind::MutexCreate { value }, ty)
            }
            "mutex_lock" => {
                let (mutex, actual) = self.expr(argument, None)?;
                let (mutex, actual) = self.resource_descriptor(mutex, actual, argument.span)?;
                let Type::Desc(inner) = actual else {
                    unreachable!()
                };
                let Type::Mutex { payload } = inner.as_ref() else {
                    return Err(error(
                        "E-MUTEX-HEAD",
                        argument.span,
                        "lock requires an actual Mutex<T> head",
                    ));
                };
                (InstructionKind::MutexLock { mutex }, guard(payload))
            }
            "mutex_value" => {
                let (guard_value, actual) = self.expr(argument, None)?;
                let Type::Address {
                    mutable: true,
                    inner,
                } = actual
                else {
                    return Err(error(
                        "E-MUTEX-GUARD",
                        argument.span,
                        "value requires a mutable guard borrow",
                    ));
                };
                let Type::MutexGuard { payload, .. } = inner.as_ref() else {
                    return Err(error(
                        "E-MUTEX-GUARD",
                        argument.span,
                        "value requires an actual guard",
                    ));
                };
                (
                    InstructionKind::MutexValue { guard: guard_value },
                    Type::Address {
                        mutable: true,
                        inner: payload.clone(),
                    },
                )
            }
            "mutex_unlock" => {
                let (guard_value, actual) = self.transfer_expr(argument, None)?;
                if !matches!(actual, Type::MutexGuard { .. }) {
                    return Err(error(
                        "E-MUTEX-GUARD",
                        argument.span,
                        "unlock consumes an actual guard",
                    ));
                }
                (
                    InstructionKind::MutexUnlock { guard: guard_value },
                    Type::Unit,
                )
            }
            _ => unreachable!(),
        };
        if let Some(wanted) = expected {
            self.same(&ty, wanted, span)?;
        }
        let value = self.value(kind, ty.clone(), span)?;
        Ok((value, ty))
    }
}
