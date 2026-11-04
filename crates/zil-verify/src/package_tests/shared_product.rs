//! Independently built synchronous shared Product parameter candidates.
//! No producer/type/analyzer solver supplies these roots or Hold rows.
use super::*;
use zil_base::ResultCategory;

fn address() -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(package()),
    }
}

// 0: borrowed parameter, 1: owning parameter's local storage, 2: ordinary local.
fn fixture(mode: u8) -> (TypedModule, CandidatePlan) {
    let mut render = Body::new(Type::I32);
    let panel = if mode == 0 {
        render.parameter(address())
    } else if mode == 1 {
        render.parameter(package())
    } else {
        render.package_local()
    };
    let body = render.storage(Type::Closure(ClosureId(0)));
    let root = if mode == 0 {
        HoldRoot::Parameter(0)
    } else {
        HoldRoot::Local(panel)
    };
    let input = render.value(
        address(),
        if mode == 0 {
            InstructionKind::Load(panel)
        } else {
            InstructionKind::BorrowLocal { local: panel }
        },
    );
    let borrowed = render.value(
        address(),
        InstructionKind::ReborrowAddress {
            address: input,
            mutable: false,
        },
    );
    let closure = render.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![borrowed],
        },
    );
    render.op(InstructionKind::Store {
        local: body,
        value: closure,
    });
    let result = render.value(
        Type::I32,
        InstructionKind::CallClosureShared {
            callee: body,
            arguments: vec![],
        },
    );
    if mode != 0 {
        render.op(InstructionKind::DropPackage { local: panel });
    }
    let mut module = module(render.finish(Some(result), CleanupExitKind::Return));
    let mut body = Body::new(Type::I32);
    body.0.id = FunctionId(1);
    body.0.name = "read_panel".into();
    body.0.environment = Some(ClosureId(0));
    let captured = body.value(address(), InstructionKind::ReadCapture { index: 0 });
    let field = body.value(
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        InstructionKind::AddressField {
            address: captured,
            index: 2,
        },
    );
    let result = body.value(Type::I32, InstructionKind::ReadAddress { address: field });
    module
        .functions
        .push(body.finish(Some(result), CleanupExitKind::Return));
    module.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::Fn,
        captures: vec![address()],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    let mut plan = resource_tests::plan(&module);
    for value in [input, borrowed, closure] {
        plan.functions[0].holds[value.0 as usize].roots = vec![root];
    }
    for value in [captured, field] {
        plan.functions[1].holds[value.0 as usize].roots = vec![HoldRoot::Capture(0)];
    }
    (module, plan)
}

fn both(module: TypedModule, plan: CandidatePlan) -> [CompileResult<VerifiedModule>; 2] {
    let target = module.target.clone();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    [
        verify_module(module, plan, &target, &limits),
        verify_module_bytes(&bytes, &target, &limits),
    ]
}

#[test]
fn shared_product_parameter_keeps_its_actual_root_and_local_controls() {
    for mode in 0..3 {
        let (module, plan) = fixture(mode);
        for result in both(module, plan) {
            result.unwrap();
        }
    }
    // The pointer parameter's activation storage is not its caller-owned
    // target. Destroying that storage after capture must not kill the target.
    let (mut module, plan) = fixture(0);
    let block = &mut module.functions[0].blocks[0];
    let dead = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::StorageDead(LocalId(0))))
        .unwrap();
    let dead = block.instructions.remove(dead);
    let invoke = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    block.instructions.insert(invoke, dead);
    let mut plan = plan;
    plan.module_fingerprint = module.fingerprint();
    for result in both(module, plan) {
        result.unwrap();
    }

    let (module, mut plan) = fixture(0);
    for hold in &mut plan.functions[0].holds {
        if !hold.roots.is_empty() {
            hold.roots = vec![HoldRoot::Local(LocalId(0))];
        }
    }
    for result in both(module, plan) {
        let ds = result.unwrap_err();
        assert!(ds.iter().any(|d| d.code == "E-VERIFY-HOLD"), "{ds:?}");
    }
}

fn store_payload() -> Type {
    Type::OwnedStore {
        element: Box::new(package()),
    }
}
fn store_desc() -> Type {
    Type::Desc(Box::new(store_payload()))
}

// All executable operations, parent roots and candidate Holds are written
// here. The only negative mutation selects the borrowed Store as write target.
fn stores(same: bool, after_read: bool, indirect: bool) -> (TypedModule, CandidatePlan) {
    let (mut module, _) = fixture(0);
    let render = &mut module.functions[0];
    render.locals.push(LocalDecl {
        id: LocalId(2),
        name: "other".into(),
        ty: store_desc(),
        mutable: false,
        span: span(),
    });
    render.parameters.push(LocalId(2));
    let desc = ValueId(render.values.len() as u32);
    render.values.push(store_desc());
    let capacity = ValueId(render.values.len() as u32);
    render.values.push(Type::I64);
    let result = ValueId(render.values.len() as u32);
    render.values.push(Type::Bool);
    let operations = vec![
        ins(Some(desc), InstructionKind::Load(LocalId(2))),
        ins(Some(capacity), InstructionKind::Constant(Constant::I64(2))),
        ins(
            Some(result),
            if indirect {
                InstructionKind::Call {
                    function: FunctionId(3),
                    arguments: vec![desc, capacity],
                }
            } else {
                InstructionKind::OwnedStoreReserve {
                    store: desc,
                    capacity,
                }
            },
        ),
    ];
    let index = render.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    render.blocks[0].instructions.splice(
        index + usize::from(after_read)..index + usize::from(after_read),
        operations,
    );
    // Explicit parameter storage lifetime is still independent of its target.
    let end = render.blocks[0].instructions.len() - 1;
    render.blocks[0]
        .instructions
        .insert(end, ins(None, InstructionKind::StorageDead(LocalId(2))));

    let owned = Type::Managed(Box::new(store_payload()));
    let mut caller = Body::new(Type::I32);
    caller.0.id = FunctionId(2);
    caller.0.name = "caller".into();
    let mut stores = vec![];
    let mut descriptions = vec![];
    for _ in 0..2 {
        let local = caller.storage(owned.clone());
        let value = caller.value(
            owned.clone(),
            InstructionKind::OwnedStoreNew { element: package() },
        );
        caller.op(InstructionKind::Store { local, value });
        descriptions.push(caller.value(store_desc(), InstructionKind::DescribeLocal { local }));
        stores.push(local);
    }
    let index = caller.value(Type::I64, InstructionKind::Constant(Constant::I64(0)));
    let view = caller.value(
        address(),
        InstructionKind::OwnedStoreBorrow {
            store: descriptions[0],
            index,
        },
    );
    let result = caller.value(
        Type::I32,
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![view, descriptions[usize::from(!same)]],
        },
    );
    for local in stores.into_iter().rev() {
        caller.op(InstructionKind::DropManaged { local });
    }
    module
        .functions
        .push(caller.finish(Some(result), CleanupExitKind::Return));
    if indirect {
        let mut writer = Body::new(Type::Bool);
        writer.0.id = FunctionId(3);
        writer.0.name = "writer".into();
        let input = writer.parameter(store_desc());
        let cap = writer.parameter(Type::I64);
        let desc = writer.value(store_desc(), InstructionKind::Load(input));
        let capacity = writer.value(Type::I64, InstructionKind::Load(cap));
        let result = writer.value(
            Type::Bool,
            InstructionKind::OwnedStoreReserve {
                store: desc,
                capacity,
            },
        );
        module
            .functions
            .push(writer.finish(Some(result), CleanupExitKind::Return));
    }
    module.resources.push(ResourceDefinition {
        name: dynamic_store::definition_name(&package()),
        payload: store_payload(),
        teardown: None,
    });
    let mut plan = resource_tests::plan(&module);
    for hold in plan.functions[0].holds.iter_mut().take(3) {
        hold.roots = vec![HoldRoot::Parameter(0)];
    }
    for hold in plan.functions[1].holds.iter_mut().take(2) {
        hold.roots = vec![HoldRoot::Capture(0)];
    }
    // Store descriptions retain their historical parent Hold rows; these
    // claims are checked against the independently reconstructed identities.
    plan.functions[0].holds[desc.0 as usize].roots = vec![HoldRoot::Parameter(1)];
    for (local, value) in descriptions.iter().enumerate() {
        plan.functions[2].holds[value.0 as usize].roots =
            vec![HoldRoot::Local(LocalId(local as u32))];
    }
    if indirect {
        plan.functions[3].holds[0].roots = vec![HoldRoot::Parameter(0)];
    }
    plan.functions[2].holds[view.0 as usize].roots = vec![HoldRoot::Local(LocalId(0))];
    (module, plan)
}

#[test]
fn shared_product_parameter_call_window_uses_actual_store_and_transitive_may_writes() {
    for after_read in [false, true] {
        for indirect in [false, true] {
            let (module, plan) = stores(false, after_read, indirect);
            for result in both(module, plan) {
                result.unwrap();
            }
            let (module, plan) = stores(true, after_read, indirect);
            for result in both(module, plan) {
                let ds = result.unwrap_err();
                assert!(ds.iter().any(|d| d.code == "E-BORROW-CONFLICT"), "{ds:?}");
            }
        }
    }
}

#[test]
fn shared_product_parameter_distinguishes_owned_package_store_fields() {
    for case in 0..4 {
        let same = case == 1;
        let (mut module, _) = stores(false, false, false);
        let outer = Type::Package {
            name: "Stores".into(),
            fields: vec![
                ("a".into(), Type::Managed(Box::new(store_payload()))),
                ("b".into(), Type::Managed(Box::new(store_payload()))),
            ],
        };
        let mut caller = Body::new(Type::I32);
        caller.0.id = FunctionId(2);
        caller.0.name = "caller".into();
        let owner = caller.parameter(outer.clone());
        let moved = (case >= 2).then(|| caller.storage(outer.clone()));
        let first = caller.value(
            store_desc(),
            InstructionKind::DescribePackageField {
                local: owner,
                index: 0,
            },
        );
        let second = caller.value(
            store_desc(),
            InstructionKind::DescribePackageField {
                local: owner,
                index: u32::from(!same),
            },
        );
        let index = caller.value(Type::I64, InstructionKind::Constant(Constant::I64(0)));
        let view = caller.value(
            address(),
            InstructionKind::OwnedStoreBorrow {
                store: first,
                index,
            },
        );
        if case == 2 {
            let value = caller.value(outer.clone(), InstructionKind::TakeLocal { local: owner });
            caller.op(InstructionKind::Store {
                local: moved.unwrap(),
                value,
            });
        }
        let result = caller.value(
            Type::I32,
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![view, second],
            },
        );
        if case == 3 {
            let value = caller.value(outer, InstructionKind::TakeLocal { local: owner });
            caller.op(InstructionKind::Store {
                local: moved.unwrap(),
                value,
            });
        }
        caller.op(InstructionKind::DropPackage {
            local: moved.unwrap_or(owner),
        });
        module.functions[2] = caller.finish(Some(result), CleanupExitKind::Return);
        let mut plan = resource_tests::plan(&module);
        for hold in plan.functions[0].holds.iter_mut().take(3) {
            hold.roots = vec![HoldRoot::Parameter(0)];
        }
        for hold in plan.functions[1].holds.iter_mut().take(2) {
            hold.roots = vec![HoldRoot::Capture(0)];
        }
        plan.functions[0].holds[4].roots = vec![HoldRoot::Parameter(1)];
        for value in [first, second, view] {
            plan.functions[2].holds[value.0 as usize].roots = vec![HoldRoot::Local(owner)];
        }
        for result in both(module, plan) {
            if same || case == 2 {
                let ds = result.unwrap_err();
                assert!(ds.iter().any(|d| d.code == "E-BORROW-CONFLICT"), "{ds:?}");
            } else {
                result.unwrap();
            }
        }
    }
}

#[test]
fn shared_product_parameter_does_not_open_outer_capture_or_direct_store_capture() {
    let (mut module, mut plan) = fixture(0);
    module.functions[0].environment = Some(ClosureId(1));
    module.functions[0].parameters.clear();
    module.functions[0].blocks[0]
        .instructions
        .insert(1, ins(None, InstructionKind::StorageLive(LocalId(0))));
    plan.functions[0].blocks[0].initialized_in.clear();
    module.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(0),
        kind: CallableKind::Fn,
        captures: vec![address()],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    let first = module.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| i.result == Some(ValueId(0)))
        .unwrap();
    first.kind = InstructionKind::ReadCapture { index: 0 };
    for hold in plan.functions[0].holds.iter_mut().take(3) {
        hold.roots = vec![HoldRoot::Capture(0)];
    }
    plan.module_fingerprint = module.fingerprint();
    for result in both(module, plan) {
        let ds = result.unwrap_err();
        assert!(
            ds.iter().any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("Product origins")),
            "{ds:?}"
        );
    }

    let (mut module, mut plan) = stores(false, false, false);
    let caller = &mut module.functions[2];
    let call = caller.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(0),
                    ..
                }
            )
        })
        .unwrap();
    let InstructionKind::Call { ref arguments, .. } = caller.blocks[0].instructions[call].kind
    else {
        unreachable!()
    };
    let view = arguments[0];
    let result = caller.blocks[0].instructions[call].result;
    let env = ValueId(caller.values.len() as u32);
    caller.values.push(Type::Closure(ClosureId(0)));
    caller.locals.push(LocalDecl {
        id: LocalId(2),
        name: "direct".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: false,
        span: span(),
    });
    caller.blocks[0].instructions.splice(
        call..=call,
        [
            ins(None, InstructionKind::StorageLive(LocalId(2))),
            ins(
                Some(env),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![view],
                },
            ),
            ins(
                None,
                InstructionKind::Store {
                    local: LocalId(2),
                    value: env,
                },
            ),
            ins(
                result,
                InstructionKind::CallClosureShared {
                    callee: LocalId(2),
                    arguments: vec![],
                },
            ),
            ins(None, InstructionKind::StorageDead(LocalId(2))),
        ],
    );
    plan.functions[2].holds.push(ValueHold {
        value: env,
        roots: vec![HoldRoot::Local(LocalId(0))],
    });
    plan.module_fingerprint = module.fingerprint();
    for result in both(module, plan) {
        let ds = result.unwrap_err();
        assert!(
            ds.iter().any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("ordinary local Product origins")),
            "{ds:?}"
        );
    }
}

#[test]
fn shared_product_parameter_and_legacy_controls_have_exact_fresh_budgets() {
    let mut fixtures: Vec<_> = (0..3)
        .map(|mode| {
            let (module, plan) = fixture(mode);
            (
                format!("mode={mode}"),
                module,
                plan,
                Some([4770, 5184, 7037][mode as usize]),
            )
        })
        .collect();
    for indirect in [false, true] {
        let (module, plan) = stores(false, true, indirect);
        fixtures.push((
            format!("Store indirect={indirect}"),
            module,
            plan,
            Some(if indirect { 18987 } else { 16195 }),
        ));
    }
    let mut observed = Vec::new();
    let mut recorded = Vec::new();
    for (label, module, plan, expected) in fixtures {
        let defaults = Limits::default();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &defaults).unwrap();
        let mut minima = vec![];
        for wire in [false, true] {
            let run = |n| {
                let limits = Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                };
                let result = if wire {
                    verify_module_bytes(&bytes, &module.target, &limits)
                } else {
                    verify_module(module.clone(), plan.clone(), &module.target, &limits)
                };
                match result {
                    Ok(_) => true,
                    Err(ds) => {
                        assert!(
                            ds.iter()
                                .all(|d| d.category == ResultCategory::ResourceLimit),
                            "{ds:?}"
                        );
                        false
                    }
                }
            };
            let (mut lo, mut hi) = (0, defaults.max_analysis_steps);
            assert!(run(hi));
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if run(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            assert!(run(hi));
            assert!(!run(hi - 1));
            assert!(run(hi));
            eprintln!("shared Product {label} wire={wire} N={hi} N-1=ResourceLimit freshN=Pass");
            minima.push(hi);
        }
        assert_eq!(minima[0], minima[1]);
        // Original controls were measured against d3b7b29b. HRTB's actual
        // closure capture-root preservation charges Store false/true +1/+2;
        // these exact public candidates and N-1/fresh checks remain unchanged.
        if let Some(expected) = expected {
            observed.push((label.clone(), minima[0]));
            recorded.push((label, expected));
        }
    }
    assert_eq!(observed, recorded, "current actual raw/Wire minima");
}
