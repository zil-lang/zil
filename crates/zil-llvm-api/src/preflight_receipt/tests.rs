//! Raw machine-input proof binding and fresh-preflight differential checks.
//! These tests do not invoke LLVM or claim native object validation.
use super::*;
use crate::model::*;

fn fixture() -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "diagnostic-only-original".into(),
        items: vec![
            Item::NamedType("Pair".into(), vec![Ty::Int(32), Ty::Int(32)]),
            Item::Global(Global {
                name: "data".into(),
                linkage: Linkage::External,
                ty: Ty::Int(32),
                initializer: Some(Val::Int(1)),
                constant: true,
                unnamed_addr: false,
                alignment: 4,
            }),
            Item::Function(Function {
                name: "callee".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), vec![Param::unnamed(Ty::Int(32))]),
                attrs: Vec::new(),
                blocks: None,
            }),
            Item::Function(Function {
                name: "main".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Int(32), Vec::new()),
                attrs: Vec::new(),
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![
                            Inst::new(
                                Some("answer".into()),
                                Op::Call {
                                    result: Ty::Int(32),
                                    result_attrs: Vec::new(),
                                    callee: Val::Global("callee".into()),
                                    args: vec![Arg::new(Ty::Int(32), Val::Int(42))],
                                    vararg: None,
                                },
                            ),
                            Inst::new(
                                None,
                                Op::Return(Some(Arg::new(
                                    Ty::Int(32),
                                    Val::Local("answer".into()),
                                ))),
                            ),
                        ],
                    }]
                    .into(),
                ),
            }),
        ]
        .into(),
    }
}

fn function_mut(module: &mut Module, index: usize) -> &mut Function {
    let Item::Function(function) = &mut module.items[index] else {
        panic!("fixture function")
    };
    function
}

fn instructions_mut(module: &mut Module) -> &mut Vec<Inst> {
    &mut function_mut(module, 3).blocks.as_mut().unwrap()[0].instructions
}

fn global_mut(module: &mut Module) -> &mut Global {
    let Item::Global(global) = &mut module.items[1] else {
        panic!("fixture global")
    };
    global
}

#[test]
fn exact_immutable_inputs_reuse_and_receipt_rebinds_independently_assembled_items() {
    let module = fixture();
    let (first, reused) = validate_module_with_receipt(&module, None).unwrap();
    assert!(!reused);
    let (warm, reused) = validate_module_with_receipt(&module, Some(&first)).unwrap();
    assert!(reused);
    assert!(warm.items.ptr_eq(&module.items));
    let mut rebuilt = fixture();
    assert_eq!(rebuilt, module);
    assert!(!rebuilt.items.ptr_eq(&first.items));
    let (current, reused) = validate_module_with_receipt(&rebuilt, Some(&first)).unwrap();
    assert!(reused);
    assert!(current.items.ptr_eq(&rebuilt.items));
    assert!(!current.items.ptr_eq(&first.items));
    // A retained proof prevents COW mutation from changing its checked bytes.
    instructions_mut(&mut rebuilt)[1].op = Op::Return(None);
    assert!(!current.items.ptr_eq(&rebuilt.items));
    let fresh = validate_module(&rebuilt).unwrap_err();
    assert_eq!(
        validate_module_with_receipt(&rebuilt, Some(&current)).unwrap_err(),
        fresh
    );
    assert!(
        validate_module_with_receipt(&module, Some(&first))
            .unwrap()
            .1
    );
}

#[test]
fn diagnostic_fingerprint_is_not_a_preflight_input_or_an_authority() {
    let mut module = fixture();
    let (receipt, _) = validate_module_with_receipt(&module, None).unwrap();
    for fingerprint in ["", "forged-external-key", "another-project-epoch"] {
        module.fingerprint = fingerprint.into();
        validate_module(&module).unwrap();
        assert!(
            validate_module_with_receipt(&module, Some(&receipt))
                .unwrap()
                .1
        );
    }
    // Keeping or forging that annotation cannot rescue an invalid current body.
    instructions_mut(&mut module)[1].op = Op::Branch("absent".into());
    let expected = validate_module(&module).unwrap_err();
    for fingerprint in ["diagnostic-only-original", "", "new-key"] {
        module.fingerprint = fingerprint.into();
        assert_eq!(validate_module(&module).unwrap_err(), expected);
        assert_eq!(
            validate_module_with_receipt(&module, Some(&receipt)).unwrap_err(),
            expected
        );
    }
}

#[test]
fn changed_supported_target_and_valid_bodies_receive_fresh_proofs() {
    let module = fixture();
    let (original, _) = validate_module_with_receipt(&module, None).unwrap();
    let mut windows = module.clone();
    windows.triple = "x86_64-pc-windows-msvc".into();
    let (windows_receipt, reused) =
        validate_module_with_receipt(&windows, Some(&original)).unwrap();
    assert!(!reused);
    assert_eq!(windows_receipt.triple, windows.triple);
    assert!(
        validate_module_with_receipt(&windows, Some(&windows_receipt))
            .unwrap()
            .1
    );
    let mut edited = module.clone();
    let Op::Call { args, .. } = &mut instructions_mut(&mut edited)[0].op else {
        unreachable!()
    };
    args[0].value = Val::Int(43);
    validate_module(&edited).unwrap();
    let (edited_receipt, reused) = validate_module_with_receipt(&edited, Some(&original)).unwrap();
    assert!(!reused);
    assert!(
        validate_module_with_receipt(&edited, Some(&edited_receipt))
            .unwrap()
            .1
    );
    // A historical receipt for different valid content is not a current hit.
    assert!(
        !validate_module_with_receipt(&module, Some(&edited_receipt))
            .unwrap()
            .1
    );
}

#[test]
fn complete_item_inventory_order_and_non_body_fields_are_bound() {
    let module = fixture();
    let (receipt, _) = validate_module_with_receipt(&module, None).unwrap();
    let mut reordered = module.clone();
    reordered.items.swap(0, 1);
    validate_module(&reordered).unwrap();
    assert!(
        !validate_module_with_receipt(&reordered, Some(&receipt))
            .unwrap()
            .1
    );
    let mut changed = module.clone();
    global_mut(&mut changed).unnamed_addr = true;
    validate_module(&changed).unwrap();
    assert!(
        !validate_module_with_receipt(&changed, Some(&receipt))
            .unwrap()
            .1
    );
    let mut added = module.clone();
    added
        .items
        .push(Item::NamedType("Extra".into(), vec![Ty::Int(64)]));
    validate_module(&added).unwrap();
    assert!(
        !validate_module_with_receipt(&added, Some(&receipt))
            .unwrap()
            .1
    );
}

#[test]
fn invalid_current_inputs_preserve_fresh_first_errors_and_do_not_issue_receipts() {
    let original = fixture();
    let (receipt, _) = validate_module_with_receipt(&original, None).unwrap();
    for attack in 0..13 {
        let mut module = original.clone();
        match attack {
            0 => {
                module.triple = "unsupported".into();
                // The target error must remain ahead of this symbol error.
                module.items.push(module.items[2].clone());
            }
            1 => module.items.push(module.items[2].clone()),
            2 => function_mut(&mut module, 2).signature.params[0].ty = Ty::Int(64),
            3 => function_mut(&mut module, 2).signature.params[0]
                .attrs
                .push(Attr::NoInline),
            4 => instructions_mut(&mut module)[1].op = Op::Branch("absent".into()),
            5 => {
                instructions_mut(&mut module)[1].op =
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("undefined".into()))))
            }
            6 => instructions_mut(&mut module).insert(0, Inst::new(None, Op::Return(None))),
            7 => global_mut(&mut module).initializer = Some(Val::Invalid("raw mutation".into())),
            8 => module.items[0] = Item::NamedType("Pair".into(), vec![Ty::Named("Pair".into())]),
            9 => function_mut(&mut module, 3).signature.variadic = true,
            10 => function_mut(&mut module, 2).linkage = Linkage::Private,
            11 => function_mut(&mut module, 2)
                .signature
                .result_attrs
                .push(Attr::ZeroExt),
            12 => global_mut(&mut module).alignment = 3,
            _ => unreachable!(),
        }
        let fresh = validate_module(&module).expect_err("invalid fixture mutation");
        if attack == 0 {
            assert_eq!(fresh.message, "machine module target is not supported");
        }
        assert_eq!(
            validate_module_with_receipt(&module, Some(&receipt)).unwrap_err(),
            fresh,
            "current mutation {attack} must preserve the fresh first error"
        );
        assert!(
            validate_module_with_receipt(&original, Some(&receipt))
                .unwrap()
                .1
        );
    }
}

#[test]
fn fixed_type_and_constant_depth_limits_remain_exact_on_reuse_and_fallback() {
    for constants in [false, true] {
        let mut module = fixture();
        if constants {
            let mut value = Val::Null;
            for _ in 0..128 {
                value = Val::Gep {
                    ty: Ty::Int(8),
                    ptr: Box::new(value),
                    indices: vec![Arg::new(Ty::Int(64), Val::Int(0))],
                };
            }
            let global = global_mut(&mut module);
            global.ty = Ty::Ptr;
            global.initializer = Some(value);
        } else {
            let mut ty = Ty::Int(8);
            for _ in 0..128 {
                ty = Ty::array(1, ty);
            }
            let global = global_mut(&mut module);
            global.ty = ty;
            global.initializer = Some(Val::Zero);
        }
        validate_module(&module).unwrap();
        let (receipt, reused) = validate_module_with_receipt(&module, None).unwrap();
        assert!(!reused);
        assert!(
            validate_module_with_receipt(&module, Some(&receipt))
                .unwrap()
                .1
        );
        let global = global_mut(&mut module);
        if constants {
            global.initializer = Some(Val::Gep {
                ty: Ty::Int(8),
                ptr: Box::new(global.initializer.take().unwrap()),
                indices: vec![Arg::new(Ty::Int(64), Val::Int(0))],
            });
        } else {
            global.ty = Ty::array(1, global.ty.clone());
        }
        let expected = validate_module(&module).unwrap_err();
        assert!(expected.message.contains("nesting exceeds supported limit"));
        assert_eq!(
            validate_module_with_receipt(&module, Some(&receipt)).unwrap_err(),
            expected
        );
    }
}

#[test]
fn collecting_preflight_issues_schedule_and_receipt_from_exactly_one_check() {
    let module = fixture();
    let before = preflight::validation_count();
    let (schedule, receipt) = validate_for_builder(&module).unwrap();
    assert_eq!(preflight::validation_count(), before + 1);
    assert_eq!(schedule.functions["main"], vec![(0, 0), (0, 1)]);
    assert!(schedule.functions["callee"].is_empty());
    let (_, reused) = validate_module_with_receipt(&module, Some(&receipt)).unwrap();
    assert!(reused);
    assert_eq!(preflight::validation_count(), before + 1);

    let mut invalid = module.clone();
    instructions_mut(&mut invalid)[1].op = Op::Branch("missing".into());
    let expected = preflight::validate(&invalid).unwrap_err();
    assert_eq!(validate_for_builder(&invalid).unwrap_err(), expected);
    assert_eq!(preflight::validation_count(), before + 3);
    assert!(
        validate_module_with_receipt(&module, Some(&receipt))
            .unwrap()
            .1
    );
    assert_eq!(preflight::validation_count(), before + 3);
}

#[cfg(not(feature = "native"))]
#[test]
fn receipt_emission_entry_without_native_returns_no_object_or_receipt() {
    let module = fixture();
    let config = crate::Config {
        triple: &module.triple,
        optimization: crate::Optimization::O0,
    };
    let before = preflight::validation_count();
    let error = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::Disabled,
        &|| false,
    )
    .unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Unavailable);
    assert_eq!(
        error,
        crate::compile_module(&module, &config, &|| false).unwrap_err()
    );
    assert_eq!(preflight::validation_count(), before);
}

#[cfg(feature = "native")]
#[test]
fn successful_native_emission_signs_one_preflight_and_first_hit_reuses_it() {
    let module = fixture();
    let config = crate::Config {
        triple: &module.triple,
        optimization: crate::Optimization::O0,
    };
    let before = preflight::validation_count();
    let (object, receipt) = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::Disabled,
        &|| false,
    )
    .unwrap();
    assert_eq!(preflight::validation_count(), before + 1);
    assert!(object.bytes.starts_with(b"\x7fELF"));
    assert!(object.receipt.verify_before_after);
    assert_eq!(object.receipt.input_kind, "builder");
    assert_eq!(object.receipt.parse_ns, 0);
    assert_eq!(object.receipt.triple, config.triple);
    assert!(
        validate_module_with_receipt(&module, Some(&receipt))
            .unwrap()
            .1
    );
    assert_eq!(preflight::validation_count(), before + 1);
    let legacy = crate::compile_module(&module, &config, &|| false).unwrap();
    assert_eq!(preflight::validation_count(), before + 2);
    assert_eq!(object.bytes, legacy.bytes);
    assert_eq!(object.receipt.pipeline, legacy.receipt.pipeline);
    assert_eq!(object.receipt.data_layout, legacy.receipt.data_layout);
}

#[cfg(feature = "native")]
#[test]
fn native_config_and_post_preflight_cancellation_fail_without_a_receipt() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut module = fixture();
    let config = crate::Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: crate::Optimization::O0,
    };
    let before = preflight::validation_count();
    module.triple = "x86_64-pc-windows-msvc".into();
    let error = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::Disabled,
        &|| false,
    )
    .unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Target);
    assert_eq!(
        error,
        crate::compile_module(&module, &config, &|| false).unwrap_err()
    );
    assert_eq!(preflight::validation_count(), before);

    module.triple = config.triple.into();
    let error = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::AfterOptimization,
        &|| false,
    )
    .unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Optimize);
    assert_eq!(preflight::validation_count(), before);
    let checks = AtomicUsize::new(0);
    let error = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::Disabled,
        &|| checks.fetch_add(1, Ordering::SeqCst) == 1,
    )
    .unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Cancelled);
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(preflight::validation_count(), before + 1);
    // A later successful request still executes its own complete preflight.
    let (_, receipt) = crate::compile_module_with_preflight_receipt(
        &module,
        &config,
        crate::FunctionMerging::Disabled,
        &|| false,
    )
    .unwrap();
    assert_eq!(preflight::validation_count(), before + 2);
    assert!(
        validate_module_with_receipt(&module, Some(&receipt))
            .unwrap()
            .1
    );
    assert_eq!(preflight::validation_count(), before + 2);
}
