//! Lexical, runtime-header and proof-context domains are separate namespaces.
//! These checks validate maps; their original-signature meaning is established
//! by the subsequent complete pattern matching and actual body proof.
use super::*;
use crate::tasks::input_scope::{self, Domain};

pub(super) fn bounded(scope: DynInstantiationScope, b: &mut Budget) -> Check<()> {
    let count = (scope.fixed_binder_count as usize)
        .checked_add(scope.invocation_binder_count as usize)
        .ok_or_else(|| limit("dynamic lexical scope count overflow"))?;
    if count > b.limits.max_nodes {
        return Err(limit("dynamic lexical scope exceeds selected node bound"));
    }
    b.spend(count.saturating_add(1))
}
fn index(scope: DynInstantiationScope, binder: StorageBinder) -> Check<usize> {
    match binder {
        StorageBinder::Fixed(i) if i < scope.fixed_binder_count => Ok(i as usize),
        StorageBinder::Invocation(i) if i < scope.invocation_binder_count => {
            Ok(scope.fixed_binder_count as usize + i as usize)
        }
        _ => Err(bad("dynamic class selects a different lexical scope")),
    }
}
pub(super) fn class(
    scope: DynInstantiationScope,
    binder: StorageBinder,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    index(scope, binder).map(|_| ())
}
/// Covers the source domain once. Images may coincide; equality in an outer
/// Context does not merge or rewrite the source domain or any physical type.
pub(super) fn embedding(
    from: DynInstantiationScope,
    to: DynInstantiationScope,
    rows: &[DynLifeEmbedding],
    b: &mut Budget,
) -> Check<()> {
    bounded(from, b)?;
    bounded(to, b)?;
    b.spend(rows.len())?;
    let length = from.fixed_binder_count as usize + from.invocation_binder_count as usize;
    if rows.len() != length {
        return Err(bad("dynamic scope embedding omits a source class"));
    }
    let mut seen = vec![false; length];
    for row in rows {
        b.spend(1)?;
        let position = index(from, row.from)?;
        index(to, row.to)?;
        if std::mem::replace(&mut seen[position], true) {
            return Err(bad("dynamic scope embedding repeats a source class"));
        }
    }
    Ok(())
}
pub(super) fn value(
    module: &TypedModule,
    value: &DynTypeValue,
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<()> {
    super::values::check(module, value, scope, b).map(|_| ())
}

pub(super) fn actuals(
    module: &TypedModule,
    actuals: &[DynGenericActual],
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<()> {
    b.spend(actuals.len())?;
    for actual in actuals {
        dynamic_metadata::recipe(actual, b)?;
        match actual {
            DynGenericActual::QualifiedType(ty) => value(module, ty, scope, b)?,
            DynGenericActual::Life(binder) => class(scope, *binder, b)?,
            DynGenericActual::Type(ty) => super::values::closed(module, ty, b)?,
            DynGenericActual::Const(_) => {}
        }
    }
    Ok(())
}
fn zero(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    // Actual scope inventories distinguish Unknown from a proved zero. Opaque
    // task domains are checked in full before their explicit count is used.
    match ty {
        Type::Address { .. }
        | Type::Desc(_)
        | Type::Opaque { .. }
        | Type::Named(_)
        | Type::Unsupported(_) => return Ok(false),
        Type::OwnedDyn { interface } => {
            b.spend(module.dyn_interfaces.len())?;
            let mut found = module
                .dyn_interfaces
                .iter()
                .filter(|row| row.id == *interface);
            let Some(actual) = found.next() else {
                return Ok(false);
            };
            if found.next().is_some() {
                return Err(bad("actual dynamic type has an ambiguous interface"));
            }
            dynamic_context::validate(module, actual, b)?;
            return Ok(actual.context.is_none()
                && actual.retained_life
                    == RetainedLifeShape {
                        binder_count: 0,
                        ports: vec![],
                        retained: vec![],
                        nominal_arguments: vec![],
                    });
        }
        Type::Task { .. } => crate::tasks::require_closed_context(ty, None)?,
        _ => {}
    }
    Ok(input_scope::domain(ty, b)? == Domain::Known(0))
}
pub(super) fn function(
    module: &TypedModule,
    f: &TypedFunction,
    instance: &DynFunctionInstance,
    b: &mut Budget,
) -> Check<()> {
    bounded(instance.scope, b)?;
    value(module, &instance.receiver, instance.scope, b)?;
    actuals(module, &instance.declaration_actuals, instance.scope, b)?;
    actuals(module, &instance.method_actuals, instance.scope, b)?;
    let runtime = if let Some(header) = &f.storage_life {
        signature_life::charge(header, b)?;
        DynInstantiationScope {
            fixed_binder_count: header.fixed_binder_count,
            invocation_binder_count: header.invocation_binder_count,
        }
    } else {
        if !module.closures.is_empty() && b.task_input_scopes.is_none() {
            let limits = b.limits.clone();
            b.task_input_scopes = Some(input_scope::build(module, &limits, b)?);
        }
        for local in &f.parameters {
            b.spend(1)?;
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("dynamic actual header parameter local is absent"))?
                .ty;
            if !zero(module, ty, b)? {
                return Err(bad(
                    "dynamic function has no complete runtime header domain",
                ));
            }
        }
        if !zero(module, &f.return_type, b)? {
            return Err(bad(
                "dynamic function result has no complete runtime header domain",
            ));
        }
        DynInstantiationScope {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
        }
    };
    embedding(runtime, instance.scope, &instance.header_embedding, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexical_classes_survive_a_many_to_one_context_image() {
        let lexical = DynInstantiationScope {
            fixed_binder_count: 2,
            invocation_binder_count: 0,
        };
        let context = DynInstantiationScope {
            fixed_binder_count: 1,
            invocation_binder_count: 0,
        };
        let rows = [
            DynLifeEmbedding {
                from: StorageBinder::Fixed(0),
                to: StorageBinder::Fixed(0),
            },
            DynLifeEmbedding {
                from: StorageBinder::Fixed(1),
                to: StorageBinder::Fixed(0),
            },
        ];
        assert!(
            embedding(
                lexical,
                context,
                &rows,
                &mut Budget::new(&Limits::default())
            )
            .is_ok()
        );
        assert!(
            embedding(
                lexical,
                context,
                &rows[..1],
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
        let mut wrong = rows;
        wrong[1].from = StorageBinder::Fixed(0);
        assert!(
            embedding(
                lexical,
                context,
                &wrong,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
    }
    #[test]
    fn runtime_header_need_not_publish_an_unused_lexical_impl_lifetime() {
        let runtime = DynInstantiationScope {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
        };
        let lexical = DynInstantiationScope {
            fixed_binder_count: 2,
            invocation_binder_count: 1,
        };
        let rows = [DynLifeEmbedding {
            from: StorageBinder::Invocation(0),
            to: StorageBinder::Invocation(0),
        }];
        assert!(
            embedding(
                runtime,
                lexical,
                &rows,
                &mut Budget::new(&Limits::default())
            )
            .is_ok()
        );
        let wrong = [DynLifeEmbedding {
            from: StorageBinder::Fixed(0),
            to: StorageBinder::Fixed(0),
        }];
        assert!(
            embedding(
                runtime,
                lexical,
                &wrong,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
    }
}
