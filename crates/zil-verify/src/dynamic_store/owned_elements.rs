//! Finite owning element qualification, independently reconstructed from types
//! and actual resource definitions. Store headers stop inline layout recursion;
//! they never stop qualification of their actual element or confer a loan.
use super::*;

pub(super) fn candidate(element: &Type, b: &mut Budget) -> Check<bool> {
    if matches!(element, Type::Managed(_)) {
        return Ok(true);
    }
    let Type::Package { fields, .. } = element else {
        return Ok(false);
    };
    b.spend(fields.len().saturating_add(1))?;
    let mut pending: Vec<&Type> = fields.iter().map(|(_, child)| child).collect();
    while let Some(child) = pending.pop() {
        b.spend(1)?;
        match child {
            Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
                return Ok(true);
            }
            Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            _ => {}
        }
    }
    Ok(false)
}

// Resource definitions are visited before all later resource payloads have
// received their ordinary shape bound. Bound an actual matching definition
// before the existing complete equality/canonical admission can inspect it.
fn bound_heads(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    let mut pending = vec![ty];
    while let Some(child) = pending.pop() {
        b.spend(1)?;
        match child {
            Type::Managed(payload) => {
                let name = match payload.as_ref() {
                    Type::ByteBuffer => "@builtin:ByteBuffer".to_owned(),
                    Type::Record { name, .. } => {
                        b.spend(name.len())?;
                        name.clone()
                    }
                    Type::OwnedStore { element } => {
                        resources::charge_payload(element, b)?;
                        pod_name_work(element, b)?;
                        definition_name(element)
                    }
                    _ => {
                        return Err(unsupported(
                            "direct Store owners require ByteBuffer, a finite user resource or a checked Store",
                            None,
                        ));
                    }
                };
                b.spend(
                    module
                        .resources
                        .len()
                        .saturating_mul(name.len().saturating_add(1)),
                )?;
                let limits = b.limits.clone();
                for definition in &module.resources {
                    if definition.name == name {
                        check_type_shape(&definition.payload, &module.target, &limits, b)?;
                    }
                }
            }
            Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    element: &Type,
    span: Option<Span>,
    b: &mut Budget,
) -> Check<()> {
    target(&module.target, span)?;
    bound_heads(module, element, b)?;
    match element {
        Type::Managed(payload)
            if matches!(
                payload.as_ref(),
                Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }
            ) =>
        {
            resources::check_type_for(module, element, span, b)?;
        }
        Type::Package { .. } => {
            packages::check_product_type(module, element, span, b, true)?;
        }
        _ => {
            return Err(unsupported(
                "Store element is not a complete supported owner",
                span,
            ));
        }
    }
    let (bytes, _) = layout(element, None, b)?;
    if bytes == 0 {
        return Err(unsupported(
            "owning Store element requires nonzero actual inline layout",
            span,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(element: Type) -> Type {
        Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element),
        }))
    }
    fn resource(element: &Type) -> ResourceDefinition {
        ResourceDefinition {
            name: definition_name(element),
            payload: Type::OwnedStore {
                element: Box::new(element.clone()),
            },
            teardown: None,
        }
    }
    fn ticket(name: &str) -> Type {
        Type::Record {
            name: name.into(),
            fields: vec![("tag".into(), Type::I64)],
        }
    }
    fn request(element: Type) -> (TypedModule, CandidatePlan) {
        let (mut module, plan) = super::super::tests::scoped_pod_fixture(element);
        module.resources.extend([
            ResourceDefinition {
                name: byte_buffers::NAME.into(),
                payload: Type::ByteBuffer,
                teardown: None,
            },
            ResourceDefinition {
                name: "TicketA".into(),
                payload: ticket("TicketA"),
                teardown: None,
            },
            ResourceDefinition {
                name: "TicketB".into(),
                payload: ticket("TicketB"),
                teardown: None,
            },
            resource(&Type::I32),
        ]);
        (module, plan)
    }

    fn rejected(result: CompileResult<VerifiedModule>) {
        let errors = result.unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|e| e.category != zil_base::ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }

    #[test]
    fn complete_direct_and_package_owner_elements_require_every_actual_resource_raw_wire() {
        let user = Type::Managed(Box::new(ticket("TicketA")));
        let drain = Type::Package {
            name: "Drain<i32>".into(),
            fields: vec![
                ("values".into(), store(Type::I32)),
                ("next".into(), Type::I64),
            ],
        };
        let wrapper = Type::Package {
            name: "Nested<Drain<i32>>".into(),
            fields: vec![
                ("child".into(), drain.clone()),
                ("mark".into(), user.clone()),
            ],
        };
        for element in [
            Type::Managed(Box::new(Type::ByteBuffer)),
            user,
            store(Type::I32),
            drain,
            wrapper,
        ] {
            let (module, plan) = request(element);
            for result in super::super::tests::pod_both(&module, &plan, &Limits::default()) {
                result.unwrap();
            }
            for mode in 0..3 {
                let mut changed = module.clone();
                let outer = &mut changed.resources[0];
                match mode {
                    0 => outer.name.push_str("forged"),
                    1 => outer.teardown = Some(FunctionId(0)),
                    _ => {
                        outer.payload = Type::OwnedStore {
                            element: Box::new(Type::Managed(Box::new(ticket("TicketB")))),
                        };
                    }
                }
                for result in super::super::tests::pod_both(&changed, &plan, &Limits::default()) {
                    rejected(result);
                }
            }
        }
        let (module, plan) = request(store(Type::I32));
        for mode in 0..3 {
            let mut changed = module.clone();
            let inner = changed
                .resources
                .iter()
                .position(|r| r.name == definition_name(&Type::I32))
                .unwrap();
            match mode {
                0 => {
                    changed.resources.remove(inner);
                }
                1 => {
                    changed.resources[inner].payload = Type::OwnedStore {
                        element: Box::new(Type::U32),
                    };
                }
                _ => {
                    let duplicate = changed.resources[inner].clone();
                    changed.resources.push(duplicate);
                }
            }
            for result in super::super::tests::pod_both(&changed, &plan, &Limits::default()) {
                rejected(result);
            }
        }
        // A weak description is not an owning element, including unused roots.
        let (mut module, plan) = request(Type::Managed(Box::new(ticket("TicketA"))));
        module.resources[0].payload = Type::OwnedStore {
            element: Box::new(Type::Desc(Box::new(ticket("TicketA")))),
        };
        for result in super::super::tests::pod_both(&module, &plan, &Limits::default()) {
            rejected(result);
        }
    }

    #[test]
    fn nested_owner_requests_keep_exact_fresh_raw_and_wire_budget_boundaries() {
        let (module, mut plan) = request(store(Type::I32));
        plan.module_fingerprint = module.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let run = |encoded: bool, n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if encoded {
                verify_module_bytes(&wire, &module.target, &limits)
            } else {
                verify_module(module.clone(), plan.clone(), &module.target, &limits)
            }
        };
        for encoded in [false, true] {
            let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
            assert!(run(encoded, high).is_ok());
            while low + 1 < high {
                let middle = (low + high) / 2;
                match run(encoded, middle) {
                    Ok(_) => high = middle,
                    Err(errors) => {
                        assert!(
                            errors
                                .iter()
                                .all(|e| e.category == zil_base::ResultCategory::ResourceLimit),
                            "{errors:?}"
                        );
                        low = middle;
                    }
                }
            }
            run(encoded, high).unwrap();
            assert!(
                run(encoded, high - 1)
                    .unwrap_err()
                    .iter()
                    .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
            );
            run(encoded, high).unwrap();
            eprintln!(
                "V nested owning Store {} N={high}, N-1/fresh checked",
                if encoded { "Wire" } else { "raw" }
            );
        }
    }
    fn direct_buffer_push() -> (TypedModule, CandidatePlan) {
        let element = Type::Managed(Box::new(Type::ByteBuffer));
        let outcome = Type::ErrorUnion {
            error: Box::new(element.clone()),
            result: Box::new(Type::Unit),
        };
        let (mut module, mut plan) = request(element.clone());
        let function = &mut module.functions[0];
        let span = function.span;
        function.locals.push(LocalDecl {
            id: LocalId(1),
            name: "returned_owner".into(),
            ty: outcome.clone(),
            mutable: true,
            span,
        });
        function.values.extend([
            Type::Desc(Box::new(Type::OwnedStore { element: Box::new(element.clone()) })),
            element,
            outcome,
        ]);
        let body = &mut function.blocks[0].instructions;
        let at = body.iter().position(|i| matches!(i.kind, InstructionKind::DropManaged {..})).unwrap();
        let instruction = |result, kind| Instruction {result, kind, span};
        body.splice(at..at, [
            instruction(None, InstructionKind::StorageLive(LocalId(1))),
            instruction(Some(ValueId(2)), InstructionKind::DescribeLocal {local: LocalId(0)}),
            instruction(Some(ValueId(3)), InstructionKind::ByteBufferNew),
            instruction(Some(ValueId(4)), InstructionKind::OwnedStorePush {store: ValueId(2), value: ValueId(3)}),
            instruction(None, InstructionKind::Store {local: LocalId(1), value: ValueId(4)}),
            instruction(None, InstructionKind::DropErrorUnion {local: LocalId(1)}),
            instruction(None, InstructionKind::StorageDead(LocalId(1))),
        ]);
        plan.functions[0].holds.extend([
            ValueHold {value:ValueId(2), roots:vec![HoldRoot::Local(LocalId(0))]},
            ValueHold {value:ValueId(3), roots:vec![]},
            ValueHold {value:ValueId(4), roots:vec![]},
        ]);
        (module, plan)
    }

    #[test]
    fn direct_owned_push_failure_keeps_exact_owner_debt_raw_wire() {
        let (module, plan) = direct_buffer_push();
        for result in super::super::tests::pod_both(&module, &plan, &Limits::default()) {
            result.unwrap();
        }
        for attack in 0..4 {
            let mut changed = module.clone();
            let span = changed.functions[0].span;
            let body = &mut changed.functions[0].blocks[0].instructions;
            let at = body.iter().position(|i| matches!(i.kind, InstructionKind::DropErrorUnion {..})).unwrap();
            match attack {
                0 => {body.remove(at);}
                1 => {let duplicate=body[at].clone(); body.insert(at,duplicate);}
                2 => {
                    // Same payload layout does not turn an owned failure into Copy.
                    changed.functions[0].values[4] = Type::ErrorUnion {
                        error: Box::new(Type::ByteBuffer), result: Box::new(Type::Unit),
                    };
                }
                _ => {
                    // Returning the owner on failure must not leave the input live.
                    body.insert(at, Instruction {
                        result: None,
                        kind: InstructionKind::Store {local:LocalId(0),value:ValueId(3)},
                        span,
                    });
                }
            }
            for result in super::super::tests::pod_both(&changed, &plan, &Limits::default()) {
                rejected(result);
            }
        }
    }

}
