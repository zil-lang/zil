//! Lexical instantiation identities are checked before any Context projection.
//! An embedding transports equalities; it never rewrites the local type domain.
use super::*;

fn cardinality(scope: DynInstantiationScope) -> CompileResult<usize> {
    usize::try_from(scope.fixed_binder_count)
        .ok()
        .and_then(|fixed| fixed.checked_add(scope.invocation_binder_count as usize))
        .ok_or_else(|| limit("dynamic lexical scope cardinality overflow"))
}
fn ordinal(scope: DynInstantiationScope, binder: StorageBinder) -> CompileResult<usize> {
    match binder {
        StorageBinder::Fixed(i) if i < scope.fixed_binder_count => Ok(i as usize),
        StorageBinder::Invocation(i) if i < scope.invocation_binder_count => {
            Ok(scope.fixed_binder_count as usize + i as usize)
        }
        _ => Err(fail(
            "dynamic embedding binder escapes its actual namespace",
        )),
    }
}

/// Source cardinality is complete, while destination classes may legitimately
/// have no source occurrence (an unused impl lifetime in a runtime header).
fn embedding(
    from: DynInstantiationScope,
    to: DynInstantiationScope,
    rows: &[DynLifeEmbedding],
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(2, WORK)?;
    let count = cardinality(from)?;
    if rows.len() != count || count > b.helper_nodes {
        return Err(fail("dynamic embedding omits or invents a source class"));
    }
    b.spend(count.saturating_mul(2), WORK)?;
    let mut present = vec![false; count];
    for row in rows {
        let index = ordinal(from, row.from)?;
        ordinal(to, row.to)?;
        if std::mem::replace(&mut present[index], true) {
            return Err(fail("dynamic embedding repeats a source class"));
        }
    }
    Ok(())
}
fn actuals(
    module: &TypedModule,
    actuals: &[DynGenericActual],
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(actuals.len(), WORK)?;
    for actual in actuals {
        match actual {
            DynGenericActual::Type(ty) => {
                if !values::known_zero(module, ty, b)? {
                    return Err(fail(
                        "unqualified generic type actual has a persistent domain",
                    ));
                }
            }
            DynGenericActual::QualifiedType(value) => {
                values::check(
                    module,
                    value,
                    scope.fixed_binder_count,
                    Some(scope.invocation_binder_count),
                    b,
                )?;
                if value.life.binder_count == 0 {
                    return Err(fail(
                        "QualifiedType actual invents a zero-domain qualification",
                    ));
                }
            }
            DynGenericActual::Life(binder) => {
                ordinal(scope, *binder)?;
            }
            DynGenericActual::Const(_) => {}
        }
    }
    Ok(())
}
pub(super) fn witness(
    module: &TypedModule,
    witness: &DynImplementationWitness,
    context_fixed: u32,
    b: &mut Budget,
) -> CompileResult<()> {
    if witness.scope.invocation_binder_count != 0 {
        return Err(fail(
            "persistent implementation witness introduces invocation classes",
        ));
    }
    embedding(
        witness.scope,
        DynInstantiationScope {
            fixed_binder_count: context_fixed,
            invocation_binder_count: 0,
        },
        &witness.to_context,
        b,
    )?;
    values::check(
        module,
        &witness.receiver,
        witness.scope.fixed_binder_count,
        None,
        b,
    )?;
    actuals(module, &witness.application.actuals, witness.scope, b)?;
    actuals(module, &witness.actuals, witness.scope, b)?;
    b.spend(witness.associated.len(), WORK)?;
    for resolution in &witness.associated {
        actuals(
            module,
            &resolution.member.application.actuals,
            witness.scope,
            b,
        )?;
        let value = match &resolution.value {
            DynAssociatedValue::Type(value) | DynAssociatedValue::Constant { ty: value, .. } => {
                value
            }
        };
        values::check(module, value, witness.scope.fixed_binder_count, None, b)?;
    }
    Ok(())
}
pub(super) fn instance(
    module: &TypedModule,
    function: &TypedFunction,
    instance: &DynFunctionInstance,
    b: &mut Budget,
) -> CompileResult<()> {
    let header = if let Some(header) = &function.storage_life {
        DynInstantiationScope {
            fixed_binder_count: header.fixed_binder_count,
            invocation_binder_count: header.invocation_binder_count,
        }
    } else {
        // None is not empty: prove the entire actual signature has no domain.
        b.spend(function.parameters.len(), WORK)?;
        for parameter in &function.parameters {
            let local = function
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| fail("dynamic definition's actual parameter is absent"))?;
            if !values::known_zero(module, &local.ty, b)? {
                return Err(fail(
                    "dynamic definition's complete actual header domain is Unknown",
                ));
            }
        }
        if !values::known_zero(module, &function.return_type, b)? {
            return Err(fail("dynamic definition's actual result domain is Unknown"));
        }
        DynInstantiationScope {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
        }
    };
    embedding(header, instance.scope, &instance.header_embedding, b)?;
    values::check(
        module,
        &instance.receiver,
        instance.scope.fixed_binder_count,
        Some(instance.scope.invocation_binder_count),
        b,
    )?;
    actuals(module, &instance.declaration_actuals, instance.scope, b)?;
    actuals(module, &instance.method_actuals, instance.scope, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(fixed: u32, invocation: u32) -> DynInstantiationScope {
        DynInstantiationScope {
            fixed_binder_count: fixed,
            invocation_binder_count: invocation,
        }
    }
    #[test]
    fn unused_lexical_classes_do_not_require_an_invented_header_occurrence() {
        let mut b = Budget::new(&zil_base::Limits::default());
        embedding(scope(0, 0), scope(2, 1), &[], &mut b).unwrap();
        embedding(
            scope(1, 0),
            scope(2, 0),
            &[DynLifeEmbedding {
                from: StorageBinder::Fixed(0),
                to: StorageBinder::Fixed(1),
            }],
            &mut b,
        )
        .unwrap();
    }
    #[test]
    fn complete_header_mapping_preserves_namespace_and_rejects_missing_or_duplicate_rows() {
        let mut b = Budget::new(&zil_base::Limits::default());
        let rows = [
            DynLifeEmbedding {
                from: StorageBinder::Fixed(0),
                to: StorageBinder::Fixed(1),
            },
            DynLifeEmbedding {
                from: StorageBinder::Invocation(0),
                to: StorageBinder::Invocation(2),
            },
        ];
        embedding(scope(1, 1), scope(2, 3), &rows, &mut b).unwrap();
        assert!(embedding(scope(1, 1), scope(2, 3), &rows[..1], &mut b).is_err());
        assert!(embedding(scope(1, 1), scope(2, 3), &[rows[0], rows[0]], &mut b).is_err());
        assert!(embedding(scope(1, 1), scope(2, 2), &rows, &mut b).is_err());
    }
    #[test]
    fn context_projection_can_merge_classes_without_erasing_the_local_domain() {
        let mut b = Budget::new(&zil_base::Limits::default());
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
        embedding(scope(2, 0), scope(1, 0), &rows, &mut b).unwrap();
        assert!(embedding(scope(1, 0), scope(1, 0), &rows, &mut b).is_err());
    }
}
