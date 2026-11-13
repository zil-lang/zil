//! Callable outer relations do not equate complete type values. Fixed image
//! agreement below is intentionally unavailable to query keys or constructors.
use super::*;

pub(super) struct FixedRelationMatch {
    pub position: StoragePosition,
    pub actual: requests::Intrinsic,
    pub required: requests::Intrinsic,
    pub lexical: StorageBinder,
}
pub(super) struct Obligation {
    pub closure: ClosureId,
    pub function: FunctionId,
    pub kind: CallableKind,
    pub signature: signatures::Resolved,
    pub expected_header: requests::Header,
    pub actual_header: StorageLifeContract,
    pub rigid: Vec<(StorageBinder, requests::Intrinsic, StorageBinder)>,
    pub fixed_relations: Vec<FixedRelationMatch>,
}
fn role(actual: CallableKind, required: CallableKind) -> bool {
    matches!(
        (actual, required),
        (
            CallableKind::Fn,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) | (
            CallableKind::FnMut,
            CallableKind::FnMut | CallableKind::FnOnce
        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}
fn input(base: StorageBase) -> bool {
    !matches!(base, StorageBase::Result)
}
fn outer(position: &StoragePosition) -> bool {
    position
        .path
        .iter()
        .all(|step| matches!(step, ContractProjection::Place(_)))
}

/// Structural interpretation only. All captures (including unused classes and
/// Static occurrences) remain in actual_header. Actual role, result sources,
/// head consumption, loans and effects must still be proved from the entry CFG.
pub(super) fn check(
    module: &TypedModule,
    receiver: &terms::Value,
    kind: CallableKind,
    signature: &signatures::Resolved,
    scope: DynInstantiationScope,
    fixed_relations_allowed: bool,
    b: &mut Budget,
) -> Check<Obligation> {
    let Type::Closure(id) = receiver.raw.ty else {
        return Err(bad(
            "callable premise requires a complete actual closure definition",
        ));
    };
    values::check(module, &receiver.raw, scope, b)?;
    b.spend(module.closures.len().saturating_add(module.functions.len()))?;
    let definition = module
        .closures
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| bad("callable actual definition is absent"))?;
    if !role(definition.kind, kind) {
        return Err(bad(
            "actual closure does not implement the required callable role",
        ));
    }
    let actual = module
        .functions
        .get(definition.function.0 as usize)
        .filter(|f| f.id == definition.function && f.environment == Some(id))
        .ok_or_else(|| bad("callable exact entry/environment identity is absent"))?;
    if actual.foreign.is_some() {
        return Err(bad(
            "closure callable entry must have an actual executable environment body",
        ));
    }
    if signature.call_kind != actual.call_kind
        || signature.result_mode != actual.result_mode
        || signature.parameters.len() != definition.parameters.len()
        || signature.result.raw.ty != definition.result
    {
        return Err(bad(
            "callable entry changes its full result, role or ordered signature",
        ));
    }
    for (expected, actual) in signature.parameters.iter().zip(&definition.parameters) {
        dynamic_metadata::recipe(&expected.raw, b)?;
        dynamic_metadata::ty(actual, b)?;
        if expected.raw.ty != *actual {
            return Err(bad("callable parameter changes its complete semantic type"));
        }
    }
    let actual_header = definition
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("callable complete environment relation is Unknown"))?;
    if receiver.terms.len() != actual_header.fixed_binder_count as usize {
        return Err(bad(
            "callable omits free Fixed classes from its whole actual type",
        ));
    }
    let expected = headers::expected(module, &signature.parameters, &signature.result, b)?;
    contracts::charge(module, &signature.contract, b)?;
    for operand in &signature.operands {
        dynamic_metadata::recipe(&operand.value, b)?;
    }
    for parameter in signature
        .parameters
        .iter()
        .chain(std::iter::once(&signature.result))
    {
        dynamic_metadata::recipe(&parameter.raw, b)?;
        b.spend(parameter.terms.len())?;
    }
    let mut proof = Obligation {
        closure: id,
        function: actual.id,
        kind,
        signature: signature.clone(),
        expected_header: expected.clone(),
        actual_header: actual_header.clone(),
        rigid: vec![],
        fixed_relations: vec![],
    };
    // Inputs establish rigid relations first. Results cannot seed a fresh
    // quantified class, and equal Context images never seed such a relation.
    for is_input in [true, false] {
        for row in &expected.classes {
            if input(row.position.base) != is_input {
                continue;
            }
            b.spend(
                actual_header
                    .classes
                    .len()
                    .saturating_add(expected.origins.len()),
            )?;
            let mut rows = actual_header
                .classes
                .iter()
                .filter(|a| a.position == row.position);
            let actual = rows
                .next()
                .ok_or_else(|| bad("callable loses an entire child class occurrence"))?;
            if rows.next().is_some() {
                return Err(bad("callable repeats an entire child class occurrence"));
            }
            let origin = expected
                .origins
                .iter()
                .find_map(|origin| match origin {
                    requests::Occurrence::Class {
                        position, origin, ..
                    } if *position == row.position => Some(*origin),
                    _ => None,
                })
                .ok_or_else(|| bad("callable class lacks its original intrinsic origin"))?;
            relation(
                receiver,
                actual.binder,
                origin,
                row.binder,
                is_input,
                &mut proof,
                b,
            )?;
        }
        for row in &expected.bindings {
            if input(row.position.base) != is_input {
                continue;
            }
            b.spend(
                actual_header
                    .bindings
                    .len()
                    .saturating_add(actual_header.static_bindings.len())
                    .saturating_add(expected.origins.len()),
            )?;
            if actual_header.static_bindings.contains(&row.position) {
                if is_input {
                    return Err(bad(
                        "callable cannot narrow an ordinary input to program-static storage",
                    ));
                }
                // Provenance stays pending. Static is never converted to a
                // Fixed class or admitted here as an executable certificate.
                continue;
            }
            let mut rows = actual_header
                .bindings
                .iter()
                .filter(|a| a.position == row.position);
            let actual = rows
                .next()
                .ok_or_else(|| bad("callable loses an outer borrow occurrence"))?;
            if rows.next().is_some() {
                return Err(bad("callable repeats an outer borrow occurrence"));
            }
            let required = expected
                .origins
                .iter()
                .find_map(|origin| match origin {
                    requests::Occurrence::Borrow {
                        position, origin, ..
                    } if *position == row.position => Some(*origin),
                    _ => None,
                })
                .ok_or_else(|| bad("callable borrow lacks its original scoped relation"))?;
            if let StorageBinder::Fixed(class) = actual.binder {
                let term = receiver.term(class, b)?;
                if fixed_relations_allowed
                    && outer(&row.position)
                    && matches!((term.image, row.binder),
                    (StorageBinder::Fixed(a), StorageBinder::Fixed(z)) if a == z)
                {
                    proof.fixed_relations.push(FixedRelationMatch {
                        position: row.position.clone(),
                        actual: headers::symbol(term),
                        required,
                        lexical: row.binder,
                    });
                    continue;
                }
            }
            relation(
                receiver,
                actual.binder,
                required,
                row.binder,
                is_input,
                &mut proof,
                b,
            )?;
        }
    }
    let public_base = |base| matches!(base, StorageBase::Parameter(_) | StorageBase::Result);
    if actual_header
        .classes
        .iter()
        .filter(|r| public_base(r.position.base))
        .map(|r| &r.position)
        .ne(expected.classes.iter().map(|r| &r.position))
        || actual_header
            .bindings
            .iter()
            .filter(|r| public_base(r.position.base))
            .map(|r| &r.position)
            .ne(expected
                .bindings
                .iter()
                .filter(|r| !actual_header.static_bindings.contains(&r.position))
                .map(|r| &r.position))
        || actual_header
            .static_bindings
            .iter()
            .filter(|p| public_base(p.base))
            .any(|p| !expected.bindings.iter().any(|r| r.position == *p))
    {
        return Err(bad(
            "callable P/R directory has missing, extra or reordered occurrences",
        ));
    }
    Ok(proof)
}
fn relation(
    receiver: &terms::Value,
    actual: StorageBinder,
    required: requests::Intrinsic,
    image: StorageBinder,
    is_input: bool,
    proof: &mut Obligation,
    b: &mut Budget,
) -> Check<()> {
    match actual {
        StorageBinder::Fixed(class) => {
            let term = receiver.term(class, b)?;
            if term.image != image || headers::symbol(term) != required {
                return Err(bad(
                    "callable retained/class relation changes an intrinsic origin or rigid class",
                ));
            }
        }
        StorageBinder::Invocation(_) => {
            if !matches!(image, StorageBinder::Invocation(_)) {
                return Err(bad(
                    "callable invocation cannot replace a required persistent Fixed class",
                ));
            }
            b.spend(proof.rigid.len().saturating_add(1))?;
            if proof
                .rigid
                .iter()
                .any(|(from, _, old)| *from == actual && *old != image)
            {
                return Err(bad(
                    "callable merges distinct fresh quantified lifetime images",
                ));
            }
            if is_input {
                if !proof.rigid.contains(&(actual, required, image)) {
                    proof.rigid.push((actual, required, image));
                }
            } else if !proof.rigid.contains(&(actual, required, image)) {
                return Err(bad(
                    "callable result selects a different fresh input occurrence",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn blank(ty: Type) -> terms::Value {
        terms::Value {
            raw: DynTypeValue {
                ty,
                life: StorageLifeShape {
                    binder_count: 0,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                classes: vec![],
            },
            terms: vec![],
        }
    }
    fn term(class: u32, image: StorageBinder, quantified: bool) -> terms::LifeTerm {
        terms::LifeTerm {
            origin: terms::Origin {
                owner: terms::Owner::Function(FunctionId(99)),
                root: if quantified {
                    terms::Root::Quantified(4, class)
                } else {
                    terms::Root::Lexical(image)
                },
                class: 0,
            },
            image,
        }
    }
    fn signature(result: terms::Value) -> signatures::Resolved {
        signatures::Resolved {
            parameters: vec![],
            parameter_modes: vec![],
            result,
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            contract: DynContract {
                requires: Some(vec![]),
                guarantees: Some(vec![]),
                hold: Some(vec![]),
                call: Some(DynCallAccess {
                    reads: vec![],
                    writes: vec![],
                }),
                result: Some(DynResultSource::Value),
                cleanup: Some(DynCleanupContract {
                    environment: false,
                    positions: vec![],
                    access: DynCallAccess {
                        reads: vec![],
                        writes: vec![],
                    },
                    effects: vec![],
                }),
                effects: Some(vec![]),
                resume: Some(DynResumeBound::Local),
                threads: Some(DynThreadRequirements {
                    transfer: false,
                    share: false,
                }),
            },
            operands: vec![],
        }
    }
    fn fixture(captures: u32, inputs: u32) -> (TypedModule, terms::Value) {
        let (mut module, _) = crate::tests::baseline();
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let mut relation = StorageLifeContract {
            fixed_binder_count: captures,
            invocation_binder_count: inputs,
            bindings: vec![],
            static_bindings: vec![],
            classes: vec![],
        };
        for index in 0..inputs {
            relation.bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(index),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(index),
            });
        }
        for index in 0..captures {
            relation.bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Capture(index),
                    path: vec![],
                },
                binder: StorageBinder::Fixed(index),
            });
        }
        relation.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
            binder: if captures > 0 {
                StorageBinder::Fixed(0)
            } else {
                StorageBinder::Invocation(0)
            },
        });
        let span = module.functions[0].span;
        let function = &mut module.functions[0];
        function.environment = Some(ClosureId(0));
        function.parameters = (0..inputs).map(LocalId).collect();
        function.locals = (0..inputs)
            .map(|i| LocalDecl {
                id: LocalId(i),
                name: format!("value{i}"),
                ty: address.clone(),
                mutable: false,
                span,
            })
            .collect();
        function.return_type = address.clone();
        function.storage_life = Some(relation.clone());
        module.closures.push(ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![address.clone(); captures as usize],
            parameters: vec![address.clone(); inputs as usize],
            result: address,
            kind: CallableKind::Fn,
            storage_life: Some(relation),
            lending_captures: vec![],
        });
        let limits = Limits::default();
        let receiver = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::Closure(ClosureId(0)),
                life: StorageLifeShape {
                    binder_count: captures,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
                classes: vec![StorageBinder::Fixed(0); captures as usize],
            },
            terms::Owner::Function(FunctionId(0)),
            terms::Root::DeclarationType(0),
            &mut Budget::new(&limits),
        )
        .unwrap();
        (module, receiver)
    }
    fn run(
        module: &TypedModule,
        receiver: &terms::Value,
        expected: &signatures::Resolved,
        kind: CallableKind,
        fixed: bool,
    ) -> Check<Obligation> {
        let limits = Limits::default();
        let raw = check(
            module,
            receiver,
            kind,
            expected,
            DynInstantiationScope {
                fixed_binder_count: 2,
                invocation_binder_count: 6,
            },
            fixed,
            &mut Budget::new(&limits),
        );
        let plan = CandidatePlan::empty_for(module);
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = check(
            &decoded,
            receiver,
            kind,
            expected,
            DynInstantiationScope {
                fixed_binder_count: 2,
                invocation_binder_count: 6,
            },
            fixed,
            &mut Budget::new(&limits),
        );
        assert_eq!(
            raw.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map(|_| ()).map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn fixed_outer_relation_does_not_erase_extra_captures_or_quantify_them() {
        let (module, receiver) = fixture(2, 0);
        let limits = Limits::default();
        let output = constructors::address(
            &blank(Type::I32),
            term(0, StorageBinder::Fixed(0), false),
            StoredBorrowMode::SharedAddress,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let expected = signature(output);
        let proof = run(&module, &receiver, &expected, CallableKind::FnMut, true).unwrap();
        assert_eq!(proof.fixed_relations.len(), 1);
        assert_eq!(proof.actual_header.fixed_binder_count, 2);
        assert_eq!(receiver.life.binder_count, 2);
        assert_eq!(receiver.terms[0].image, receiver.terms[1].image);
        assert_ne!(receiver.terms[0].origin, receiver.terms[1].origin);
        assert!(run(&module, &receiver, &expected, CallableKind::Fn, false).is_err());
        let mut different_lexical = receiver.clone();
        different_lexical.terms[0].image = StorageBinder::Fixed(1);
        different_lexical.raw.classes[0] = StorageBinder::Fixed(1);
        assert!(run(
            &module,
            &different_lexical,
            &expected,
            CallableKind::Fn,
            true
        )
        .is_err());
        let forall = signature(
            constructors::address(
                &blank(Type::I32),
                term(0, StorageBinder::Invocation(3), true),
                StoredBorrowMode::SharedAddress,
                &mut Budget::new(&limits),
            )
            .unwrap(),
        );
        assert!(run(&module, &receiver, &forall, CallableKind::Fn, true).is_err());
        let mut once = module.clone();
        once.closures[0].kind = CallableKind::FnOnce;
        assert!(run(&once, &receiver, &expected, CallableKind::FnMut, true).is_err());
    }
    #[test]
    fn quantified_inputs_are_rigid_and_the_return_must_select_the_correct_one() {
        let (module, receiver) = fixture(0, 2);
        let limits = Limits::default();
        let parameters = (0..2)
            .map(|index| {
                constructors::address(
                    &blank(Type::I32),
                    term(index, StorageBinder::Invocation(4 + index), true),
                    StoredBorrowMode::SharedAddress,
                    &mut Budget::new(&limits),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let mut expected = signature(parameters[0].clone());
        expected.parameters = parameters;
        expected.parameter_modes = vec![PassingMode::Infer; 2];
        assert!(run(&module, &receiver, &expected, CallableKind::FnOnce, true).is_ok());
        let scope = DynInstantiationScope {
            fixed_binder_count: 2,
            invocation_binder_count: 6,
        };
        let mut measured = Budget::new(&limits);
        check(
            &module,
            &receiver,
            CallableKind::FnOnce,
            &expected,
            scope,
            true,
            &mut measured,
        )
        .unwrap();
        let cost = limits.max_analysis_steps - measured.remaining;
        let plan = CandidatePlan::empty_for(&module);
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let mut wire_cost = Budget::new(&limits);
        check(
            &decoded,
            &receiver,
            CallableKind::FnOnce,
            &expected,
            scope,
            true,
            &mut wire_cost,
        )
        .unwrap();
        assert_eq!(cost, limits.max_analysis_steps - wire_cost.remaining);
        let mut short = limits.clone();
        short.max_analysis_steps = cost - 1;
        for candidate in [&module, &decoded] {
            let error = check(
                candidate,
                &receiver,
                CallableKind::FnOnce,
                &expected,
                scope,
                true,
                &mut Budget::new(&short),
            )
            .err()
            .unwrap();
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        }
        let mut exact = limits.clone();
        exact.max_analysis_steps = cost;
        assert!(check(
            &module,
            &receiver,
            CallableKind::FnOnce,
            &expected,
            scope,
            true,
            &mut Budget::new(&exact)
        )
        .is_ok());

        let mut wrong_return = module.clone();
        wrong_return.closures[0]
            .storage_life
            .as_mut()
            .unwrap()
            .bindings[2]
            .binder = StorageBinder::Invocation(1);
        wrong_return.functions[0].storage_life = wrong_return.closures[0].storage_life.clone();
        assert!(run(&wrong_return, &receiver, &expected, CallableKind::Fn, true).is_err());
        let mut coalesced = module.clone();
        let relation = coalesced.closures[0].storage_life.as_mut().unwrap();
        relation.invocation_binder_count = 1;
        relation.bindings[1].binder = StorageBinder::Invocation(0);
        coalesced.functions[0].storage_life = coalesced.closures[0].storage_life.clone();
        assert!(run(&coalesced, &receiver, &expected, CallableKind::Fn, true).is_err());
    }
}
