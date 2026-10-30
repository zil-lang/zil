//! Producer-side prefix resource admission; Verify rebuilds it independently.
use zil_ir::{InstructionKind, Type, ValueId};

use super::{Budget, CompileResult, Diagnostic, Phase, TypedModule, callable_fields};

/// The caller completes bounded raw type preflight before invoking this
/// producer-only admission. No Types result is used as a proof here.
pub(super) fn product_element(element: &Type, budget: &mut Budget) -> CompileResult<()> {
    super::resources::product::admit(element, budget)?;
    budget.type_work(element, true, "Product Store nonzero layout")?;
    fn storage(ty: &Type) -> bool {
        match ty {
            Type::Unit => false,
            Type::Managed(payload) => storage(payload),
            Type::Tuple(fields) => fields.iter().any(storage),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.iter().any(|(_, ty)| storage(ty))
            }
            Type::Array { element, length } => *length > 0 && storage(element),
            _ => true,
        }
    }
    if !storage(element) {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "Product Store element requires nonzero actual inline storage",
            None,
        )]);
    }
    Ok(())
}

pub(super) fn product_protocol_payload(ty: &Type) -> Option<&Type> {
    let Type::ErrorUnion { error, result } = ty else {
        return None;
    };
    if **error == Type::Unit && super::resources::product::nested(result) {
        Some(result)
    } else if **result == Type::Unit && super::resources::product::nested(error) {
        Some(error)
    } else {
        None
    }
}

pub(super) fn product_carrier(
    module: &TypedModule,
    element: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    product_element(element, budget)?;
    let Type::Package { name, .. } = element else {
        unreachable!("independently admitted Product")
    };
    budget.spend(
        name.len()
            .saturating_add("@builtin:OwnedStore<".len())
            .saturating_add(">".len()),
        "Product Store canonical identity",
    )?;
    let expected = format!("@builtin:OwnedStore<{name}>");
    for definition in &module.resources {
        budget.spend(
            expected
                .len()
                .saturating_add(definition.name.len())
                .saturating_add(1),
            "Product Store definition lookup",
        )?;
        if definition.name != expected {
            continue;
        }
        budget.type_work(&definition.payload, true, "Product Store exact definition")?;
        budget.type_work(element, true, "Product Store exact element")?;
        if definition.teardown.is_none()
            && matches!(&definition.payload, Type::OwnedStore { element: actual } if actual.as_ref() == element)
        {
            return Ok(());
        }
    }
    Err(super::bad(
        "Product Store lacks its exact canonical element definition",
    ))
}

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

pub(super) fn is_owned_carrier(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }))
}

fn capture_error(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, message, None)]
}

/// The inner captured Item keeps the old leaf shape, even though an ordinary
/// outer Store may now contain callbacks. Inspect its complete actual tree,
/// including types hidden inside otherwise Copy aggregates.
fn leaf_element(element: &Type, budget: &mut Budget) -> CompileResult<()> {
    const WORK: &str = "owned Store capture leaf qualification";
    let Type::Package { fields, .. } = element else {
        return Err(capture_error(
            "owned Store capture requires a complete leaf Item Package",
        ));
    };
    budget.spend(1, WORK)?;
    let mut pending = vec![(element, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        budget.spend(1, WORK)?;
        if depth > budget.helper_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "owned Store capture type depth exceeded",
                None,
            )]);
        }
        let mut push = |child| -> CompileResult<()> {
            budget.spend(pending.len().saturating_add(1), WORK)?;
            pending.push((child, depth + 1));
            Ok(())
        };
        match ty {
            Type::Closure(_)
            | Type::FunctionPointer { .. }
            | Type::ForeignCallback { .. }
            | Type::Address { .. }
            | Type::Desc(_)
            | Type::OwnedStore { .. }
            | Type::Opaque { .. }
            | Type::Named(_)
            | Type::Unsupported(_)
            | Type::Slice { .. } => {
                return Err(capture_error(
                    "captured Store Item cannot contain callbacks, addresses, descriptions, nested Store or unresolved types",
                ));
            }
            Type::Package { fields, .. } if depth == 0 => {
                for (_, child) in fields {
                    push(child)?;
                }
            }
            Type::Package { .. } => {
                return Err(capture_error(
                    "captured Store Item cannot contain owning aggregates",
                ));
            }
            Type::Managed(child) if depth == 1 => push(child)?,
            Type::Managed(_) => {
                return Err(capture_error(
                    "captured Store Item ownership must be a direct field",
                ));
            }
            Type::Tuple(fields) => {
                for child in fields {
                    push(child)?;
                }
            }
            Type::Record { fields, .. } => {
                for (_, child) in fields {
                    push(child)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for child in fields {
                        push(child)?;
                    }
                }
            }
            Type::Array { element, .. } => push(element)?,
            Type::ErrorUnion { error, result } => {
                push(error)?;
                push(result)?;
            }
            _ => {}
        }
    }
    // The structural walk above rejects opaque/unbounded hidden trees before
    // the existing recursive comparison/Copy reservation can inspect them.
    budget.type_work(element, true, WORK)?;
    let mut managed = false;
    for (_, field) in fields {
        match field {
            Type::Managed(inner)
                if **inner == Type::ByteBuffer
                    || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()) =>
            {
                managed = true
            }
            ty if ty.is_copy_value() => {}
            _ => {
                return Err(capture_error(
                    "captured Store Item requires finite Copy fields and direct supported Managed resources",
                ));
            }
        }
    }
    if !managed {
        return Err(capture_error(
            "captured Store Item requires at least one direct Managed field",
        ));
    }
    Ok(())
}

pub(super) fn owned_capture(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    const WORK: &str = "owned Store capture canonical definition";
    let Type::Managed(payload) = ty else {
        return Err(capture_error(
            "owned Store capture requires the actual Managed carrier",
        ));
    };
    let Type::OwnedStore { element } = payload.as_ref() else {
        return Err(capture_error(
            "owned Store capture requires the actual Store payload",
        ));
    };
    leaf_element(element, budget)?;
    let Type::Package { name, .. } = element.as_ref() else {
        unreachable!("qualified leaf")
    };
    budget.spend(name.len().saturating_add(32), WORK)?;
    let canonical = format!("@builtin:OwnedStore<{name}>");
    budget.spend(module.resources.len(), WORK)?;
    let mut found = 0;
    for resource in &module.resources {
        budget.spend(resource.name.len().saturating_add(canonical.len()), WORK)?;
        let related_payload = if let Type::OwnedStore { element: actual } = &resource.payload
            && let Type::Package {
                name: actual_name, ..
            } = actual.as_ref()
        {
            budget.spend(actual_name.len().saturating_add(name.len()), WORK)?;
            actual_name == name
        } else {
            false
        };
        if resource.name != canonical && !related_payload {
            continue;
        }
        let Type::OwnedStore { element: actual } = &resource.payload else {
            return Err(super::bad(
                "owned Store capture canonical name has a different payload",
            ));
        };
        leaf_element(actual, budget)?;
        budget.type_work(&resource.payload, false, WORK)?;
        if resource.name != canonical
            || resource.payload != **payload
            || resource.teardown.is_some()
        {
            return Err(super::bad(
                "owned Store capture requires its exact canonical payload and no user teardown",
            ));
        }
        found += 1;
    }
    if found != 1 {
        return Err(super::bad(
            "owned Store capture requires exactly one canonical resource definition",
        ));
    }
    Ok(())
}

pub(super) fn captured_package(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    let Type::Package { fields, .. } = ty else {
        return Err(super::bad("expected captured Package"));
    };
    budget.type_work(ty, true, "owned Store capture Package")?;
    let mut store = false;
    for (_, field) in fields {
        if is_owned_carrier(field) {
            owned_capture(module, field, budget)?;
            store = true;
        } else if !(field.is_copy_value()
            || matches!(field, Type::Managed(inner) if **inner == Type::ByteBuffer
                || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value())))
        {
            return Err(capture_error(
                "captured Store Package requires direct owned Store, supported Managed and finite Copy fields",
            ));
        }
    }
    if !store {
        return Err(capture_error("unsupported captured Package shape"));
    }
    Ok(())
}

/// Run only after the paid type walk actually sees an owned Store capture.
/// Private profile bits constrain results; actual MakeClosure values and the
/// ordinary resource pass still establish ownership and distinct live roots.
pub(super) fn owned_capture_profiles(
    module: &TypedModule,
    roots: &[zil_ir::ClosureId],
    budget: &mut Budget,
) -> CompileResult<Vec<bool>> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }
    const WORK: &str = "owned Store capture graph";
    let count = module.closures.len();
    budget.product(count, 5, 1, WORK)?;
    let mut parents = vec![Vec::new(); count];
    let mut remaining = vec![0usize; count];
    let mut depth = vec![1usize; count];
    let mut profile = vec![false; count];
    let mut ready = Vec::with_capacity(count);
    for (index, definition) in module.closures.iter().enumerate() {
        if definition.id.0 as usize != index {
            return Err(super::bad(
                "owned Store capture closure identity differs from its table position",
            ));
        }
        budget.spend(definition.captures.len(), WORK)?;
        for capture in &definition.captures {
            if let Type::Closure(child) = capture {
                let incoming = parents
                    .get_mut(child.0 as usize)
                    .ok_or_else(|| super::bad("owned Store child environment is absent"))?;
                budget.spend(incoming.len().saturating_add(1), WORK)?;
                incoming.push(index);
                remaining[index] += 1;
            }
        }
        if remaining[index] == 0 {
            ready.push(index);
        }
    }
    budget.spend(roots.len(), WORK)?;
    for root in roots {
        profile[root.0 as usize] = true;
    }
    let mut position = 0;
    while position < ready.len() {
        let child = ready[position];
        position += 1;
        budget.spend(parents[child].len().saturating_add(1), WORK)?;
        for &parent in &parents[child] {
            profile[parent] |= profile[child];
            depth[parent] = depth[parent].max(depth[child].saturating_add(1));
            if depth[parent] > budget.helper_depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Analyze,
                    "owned Store capture graph depth exceeded",
                    None,
                )]);
            }
            remaining[parent] -= 1;
            if remaining[parent] == 0 {
                ready.push(parent);
            }
        }
    }
    if ready.len() != count {
        return Err(super::bad(
            "owned Store capture graph contains an owning cycle",
        ));
    }
    for (definition, &new_profile) in module.closures.iter().zip(&profile) {
        if !new_profile {
            continue;
        }
        budget.type_work(&definition.result, true, "owned Store capture result")?;
        if !definition.result.is_copy_value()
            || definition.kind == zil_ir::CallableKind::FunctionPointer
        {
            return Err(capture_error(
                "owned Store capture and its owning Closure parents require finite Copy or Unit results",
            ));
        }
        let function = module
            .functions
            .get(definition.function.0 as usize)
            .ok_or_else(|| super::bad("owned Store environment entry is absent"))?;
        budget.type_work(
            &function.return_type,
            true,
            "owned Store actual entry result",
        )?;
        if function.environment != Some(definition.id)
            || function.foreign.is_some()
            || function.return_type != definition.result
        {
            return Err(super::bad(
                "owned Store environment must match its actual entry and result",
            ));
        }
        budget.spend(1, "owned Store self-contained root")?;
        callable_fields::self_contained(module, definition.id, budget)?;
    }
    budget.spend(module.functions.len(), "owned Store MakeClosure functions")?;
    for function in &module.functions {
        budget.spend(function.blocks.len(), "owned Store MakeClosure blocks")?;
        for block in &function.blocks {
            budget.spend(
                block.instructions.len(),
                "owned Store MakeClosure instructions",
            )?;
            for instruction in &block.instructions {
                let InstructionKind::MakeClosure { closure, captures } = &instruction.kind else {
                    continue;
                };
                if !profile.get(closure.0 as usize).copied().unwrap_or(false) {
                    continue;
                }
                let expected = &module.closures[closure.0 as usize].captures;
                if expected.len() != captures.len() {
                    return Err(super::bad("owned Store MakeClosure capture count mismatch"));
                }
                budget.spend(captures.len(), "owned Store MakeClosure actual operands")?;
                for (value, expected) in captures.iter().zip(expected) {
                    let actual = function
                        .values
                        .get(value.0 as usize)
                        .ok_or_else(|| super::bad("owned Store MakeClosure operand is absent"))?;
                    budget.type_work(actual, false, "owned Store MakeClosure type comparison")?;
                    if actual != expected {
                        return Err(super::bad(
                            "owned Store MakeClosure actual capture type differs from the environment",
                        ));
                    }
                }
            }
        }
    }
    Ok(profile)
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
