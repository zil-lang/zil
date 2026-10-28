//! Producer-side prefix resource admission; Verify rebuilds it independently.
use zil_ir::{InstructionKind, Type, ValueId};

use super::{Budget, CompileResult, Diagnostic, Phase, TypedModule, callable_fields};

/// The structural preflight records only Store elements that actually contain
/// a concrete Closure. Ordinary Store inputs therefore keep their old work
/// bound. This pass reads the raw element and environment tables; no Types
/// classification or public candidate claim grants admission.
pub(super) fn callback_element(
    module: &TypedModule,
    element: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    const CONTEXT: &str = "callback Store element admission";
    let unsupported = || {
        vec![Diagnostic::unsupported(
            Phase::Analyze,
            "callback Store requires a complete State with one or two direct self-contained callbacks, finite Copy fields and direct Managed resources",
            None,
        )]
    };
    // Preflight has bounded every type and capture tree before this recursive
    // reservation. Reserve both the field scan and recursive Copy predicates.
    budget.type_work(element, true, CONTEXT)?;
    let Type::Package { fields, .. } = element else {
        return Err(unsupported());
    };
    budget.spend(fields.len().saturating_add(1), CONTEXT)?;
    let (mut callback, mut second, mut count, mut managed) = (None, None, 0usize, false);
    for (_, field) in fields {
        match field {
            Type::Closure(id) => {
                if count == 2 {
                    return Err(unsupported());
                }
                match callback {
                    None => callback = Some(*id),
                    Some(first) if first != *id => second = Some(*id),
                    _ => {}
                }
                count += 1;
            }
            Type::Managed(inner)
                if **inner == Type::ByteBuffer
                    || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()) =>
            {
                managed = true;
            }
            other if other.is_copy_value() => {}
            _ => return Err(unsupported()),
        }
    }
    let Some(callback) = callback.filter(|_| managed) else {
        return Err(unsupported());
    };
    // Reserve the initial pending entry before the existing independent
    // capture-graph checker allocates it. Each actual capture is then charged
    // there before inspecting or enqueueing its type.
    if let Some(other) = second {
        // Only the new heterogeneous profile adds result constraints. The
        // existing same-environment lending path keeps its original work.
        for id in [callback, other] {
            budget.spend(1, "heterogeneous Store callback result")?;
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| super::bad("callback Store environment is absent"))?;
            budget.type_work(
                &definition.result,
                true,
                "heterogeneous Store result Copy predicate",
            )?;
            if !definition.result.is_copy_value() {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "heterogeneous callback Store requires Copy results for both actual environments",
                    None,
                )]);
            }
        }
    }
    budget.spend(1, "callback Store environment root")?;
    callable_fields::self_contained(module, callback, budget)?;
    if let Some(other) = second {
        budget.spend(1, "callback Store second environment root")?;
        callable_fields::self_contained(module, other, budget)?;
    }
    Ok(())
}

pub(super) fn is_carrier(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }))
}

pub(super) fn access(kind: &InstructionKind) -> Option<(ValueId, bool)> {
    match *kind {
        InstructionKind::OwnedStoreReserve { store, .. }
        | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => Some((store, true)),
        InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store }
        | InstructionKind::OwnedStoreBorrow { store, .. } => Some((store, false)),
        _ => None,
    }
}

#[cfg(test)]
mod tests203 {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, TargetProfile};
    use zil_ir::{CallableKind, ClosureDefinition, ClosureId, FunctionId};

    // These call the producer's qualification gate directly. The independent
    // driver suite also submits complete modules to Analyze and Verify.
    fn fixture() -> (TypedModule, Type) {
        let closures = (0..2)
            .map(|id| ClosureDefinition {
                id: ClosureId(id),
                function: FunctionId(id),
                kind: CallableKind::Fn,
                captures: vec![Type::Managed(Box::new(Type::ByteBuffer))],
                parameters: vec![],
                result: Type::I32,
                lending_captures: vec![],
            })
            .collect();
        let module = TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"203 producer raw qualification"),
            target: TargetProfile::host_llvm(),
            functions: vec![],
            closures,
            resources: vec![],
            unsupported: vec![],
        };
        let element = Type::Package {
            name: "State".into(),
            fields: vec![
                ("token".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("first".into(), Type::Closure(ClosureId(0))),
                ("second".into(), Type::Closure(ClosureId(1))),
            ],
        };
        (module, element)
    }
    fn check(module: &TypedModule, element: &Type, limits: &Limits) -> CompileResult<()> {
        callback_element(module, element, &mut Budget::new(limits))
    }

    #[test]
    fn impl203_producer_qualifies_each_actual_root_and_each_result() {
        let (m, element) = fixture();
        let limits = Limits::default();
        check(&m, &element, &limits).unwrap();
        for root in [0, 1] {
            let mut invalid = m.clone();
            invalid.closures[root].result = Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            };
            let errors = check(&invalid, &element, &limits).unwrap_err();
            assert!(errors[0].message.contains("Copy results for both"));
        }
        let store = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        }));
        for capture in [
            Type::Desc(Box::new(Type::ByteBuffer)),
            store.clone(),
            Type::Package {
                name: "NestedStore".into(),
                fields: vec![("items".into(), store)],
            },
        ] {
            let mut invalid = m.clone();
            invalid.closures[1].captures = vec![capture];
            let errors = check(&invalid, &element, &limits).unwrap_err();
            assert_eq!(errors[0].category, ResultCategory::Unsupported);
        }
    }

    #[test]
    fn impl203_producer_two_root_work_has_exact_and_fresh_boundaries() {
        let (m, element) = fixture();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            };
            if check(&m, &element, &limits).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0);
        let exact = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        for _ in 0..2 {
            check(&m, &element, &exact).unwrap();
        }
        let errors = check(
            &m,
            &element,
            &Limits {
                max_analysis_steps: low - 1,
                ..Limits::default()
            },
        )
        .unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        println!("impl203 producer two-root qualification work={low}");
    }
}
