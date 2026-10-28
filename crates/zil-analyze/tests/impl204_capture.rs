//! Hand-built TypedIR / Wire producer controls, not source or machine evidence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn function(index: usize) -> TypedFunction {
    TypedFunction {
        id: FunctionId(index as u32),
        name: format!("entry{index}"),
        foreign: None,
        environment: Some(ClosureId(index as u32)),
        callable_return_contract: None,
        callable_contract: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn leaf() -> Type {
    Type::Package {
        name: "Leaf204".into(),
        fields: vec![
            ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("n".into(), Type::I32),
        ],
    }
}
fn store(element: Type) -> Type {
    Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element),
    }))
}
fn resource(element: Type) -> ResourceDefinition {
    let Type::Package { name, .. } = &element else {
        panic!("test Item")
    };
    ResourceDefinition {
        name: format!("@builtin:OwnedStore<{name}>"),
        payload: Type::OwnedStore {
            element: Box::new(element),
        },
        teardown: None,
    }
}
fn fixture(element: Type) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"204 raw unused Store capture"),
        target: TargetProfile::host_llvm(),
        functions: vec![function(0)],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            kind: CallableKind::Fn,
            captures: vec![store(element.clone())],
            parameters: vec![],
            result: Type::Unit,
            lending_captures: vec![],
        }],
        resources: vec![resource(element)],
        unsupported: vec![],
    }
}
fn record(label: &str, module: &TypedModule, result: &str, wire: Option<&[u8]>) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let path = PathBuf::from(root).join(format!(
        "raw204-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("result.txt"), result).unwrap();
    if let Some(wire) = wire {
        fs::write(path.join("candidate.wire"), wire).unwrap();
    }
}
fn both(label: &str, module: &TypedModule, expected: Option<(ResultCategory, &str)>) {
    let limits = Limits::default();
    let candidate = CandidatePlan::empty_for(module);
    let bytes = zil_ir::wire::encode_module_candidate(module, &candidate, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(&decoded, module);
    for (route, input) in [("raw", module), ("wire", &decoded)] {
        let result = zil_analyze::analyze(input, &limits);
        record(
            &format!("{label}-{route}"),
            input,
            &format!("{result:#?}"),
            Some(&bytes),
        );
        if let Some((category, message)) = expected {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Analyze
                    && d.category == category
                    && d.message.contains(message)),
                "{label}/{route}: {errors:?}"
            );
        } else {
            result.unwrap();
        }
    }
}
fn add_parent(module: &mut TypedModule) {
    let index = module.closures.len();
    module.functions.push(function(index));
    module.closures.push(ClosureDefinition {
        id: ClosureId(index as u32),
        function: FunctionId(index as u32),
        kind: CallableKind::Fn,
        captures: vec![Type::Closure(ClosureId(index as u32 - 1))],
        parameters: vec![],
        result: Type::Unit,
        lending_captures: vec![],
    });
}

fn bind_package_view(module: &mut TypedModule) {
    let entry = &mut module.functions[0];
    entry.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "captured_bag_view".into(),
        ty: module.closures[0].captures[0].clone(),
        mutable: true,
        span: span(),
    }];
    entry.blocks[0].instructions = [
        InstructionKind::ScopeEnter { scope: ScopeId(0) },
        InstructionKind::BindPackageCapture {
            local: LocalId(0),
            index: 0,
        },
        InstructionKind::EndPackageCapture {
            local: LocalId(0),
            index: 0,
        },
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    ]
    .into_iter()
    .map(|kind| Instruction {
        result: None,
        kind,
        span: span(),
    })
    .collect();
}

#[test]
fn unused_direct_package_and_child_metadata_qualify_each_actual_mode() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut module = fixture(leaf());
        module.closures[0].kind = kind;
        both("unused-direct", &module, None);
        module.closures[0].captures = vec![Type::Package {
            name: "Bag".into(),
            fields: vec![
                ("items".into(), store(leaf())),
                ("side".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("copied".into(), Type::Tuple(vec![Type::I32, Type::Bool])),
            ],
        }];
        bind_package_view(&mut module);
        both("unused-package", &module, None);
        add_parent(&mut module);
        add_parent(&mut module);
        both("child-depth-three", &module, None);
    }
}

#[test]
fn new_leaf_restriction_does_not_close_captured_package_copy_companions_or_outer_state() {
    let mut module = fixture(leaf());
    module.closures[0].captures = vec![Type::Package {
        name: "Bag".into(),
        fields: vec![
            ("items".into(), store(leaf())),
            (
                "callback_pointer".into(),
                Type::FunctionPointer {
                    parameters: vec![],
                    result: Box::new(Type::Unit),
                },
            ),
        ],
    }];
    bind_package_view(&mut module);
    let outer = Type::Package {
        name: "OuterState".into(),
        fields: vec![
            ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("callback".into(), Type::Closure(ClosureId(0))),
        ],
    };
    module.resources.push(resource(outer));
    both("outer-state-and-copy-companion", &module, None);
}

#[test]
fn hidden_callbacks_and_borrowed_or_nested_owned_items_stay_closed() {
    let forbidden = [
        Type::Closure(ClosureId(0)),
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::Unit),
        },
        Type::ForeignCallback {
            parameters: vec![],
            result: Box::new(Type::Unit),
        },
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::ByteBuffer)),
        Type::OwnedStore {
            element: Box::new(leaf()),
        },
    ];
    for ty in forbidden {
        for wrap in 0..6 {
            let hidden = match wrap {
                0 => ty.clone(),
                1 => Type::Tuple(vec![ty.clone()]),
                2 => Type::Array {
                    element: Box::new(ty.clone()),
                    length: 1,
                },
                3 => Type::Record {
                    name: "Hidden".into(),
                    fields: vec![("value".into(), ty.clone())],
                },
                4 => Type::Enum {
                    name: "Hidden".into(),
                    variants: vec![("Value".into(), vec![ty.clone()])],
                },
                _ => Type::Managed(Box::new(Type::Record {
                    name: "UserHead".into(),
                    fields: vec![("value".into(), ty.clone())],
                })),
            };
            let Type::Package { name, mut fields } = leaf() else {
                unreachable!()
            };
            fields.push(("hidden".into(), hidden));
            let module = fixture(Type::Package { name, fields });
            // Some non-Copy outer shapes close earlier in the ordinary TypedIR
            // boundary; every case must be an explicit Analyze Unsupported.
            both(
                "forbidden-inner-tree",
                &module,
                Some((ResultCategory::Unsupported, "")),
            );
        }
    }
}

#[test]
fn canonical_definition_is_exact_unique_and_has_no_user_teardown() {
    for attack in 0..5 {
        let mut module = fixture(leaf());
        match attack {
            0 => module.resources.clear(),
            1 => module.resources.push(module.resources[0].clone()),
            2 => module.resources[0].name = "alias".into(),
            3 => module.resources[0].teardown = Some(FunctionId(0)),
            _ => {
                let Type::OwnedStore { element } = &mut module.resources[0].payload else {
                    unreachable!()
                };
                let Type::Package { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                fields[1].1 = Type::I64;
            }
        }
        both(
            "canonical-attack",
            &module,
            Some((ResultCategory::SafeReject, "canonical")),
        );
    }
}

#[test]
fn copy_result_restriction_propagates_to_real_owned_children_and_entry_matches() {
    let mut module = fixture(leaf());
    add_parent(&mut module);
    module.closures[1].result = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    module.closures[1].lending_captures = vec![0];
    module.functions[1].return_type = module.closures[1].result.clone();
    both(
        "child-lending",
        &module,
        Some((ResultCategory::Unsupported, "Copy or Unit")),
    );
    let mut module = fixture(leaf());
    module.functions[0].environment = None;
    both(
        "wrong-entry-owner",
        &module,
        Some((ResultCategory::SafeReject, "actual entry")),
    );
    let mut module = fixture(leaf());
    module.functions[0].return_type = Type::I32;
    both(
        "wrong-entry-result",
        &module,
        Some((ResultCategory::SafeReject, "actual entry")),
    );
}

#[test]
fn owned_capture_graph_cycles_and_depth_fail_closed() {
    let mut module = fixture(leaf());
    add_parent(&mut module);
    module.closures[0]
        .captures
        .push(Type::Closure(ClosureId(1)));
    both(
        "cycle",
        &module,
        Some((ResultCategory::SafeReject, "owning cycle")),
    );
    let mut module = fixture(leaf());
    for _ in 0..9 {
        add_parent(&mut module);
    }
    let result = zil_analyze::analyze(
        &module,
        &Limits {
            max_call_depth: 8,
            ..Limits::default()
        },
    );
    record("depth", &module, &format!("{result:#?}"), None);
    let errors = result.unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert!(
        errors[0].message.contains("capture graph depth"),
        "{errors:?}"
    );
}

#[test]
fn new_qualification_has_exact_and_fresh_work_boundaries() {
    let mut module = fixture(leaf());
    add_parent(&mut module);
    let expected = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                low = middle + 1;
            }
        }
    }
    for steps in [low, low - 1, low] {
        let result = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        );
        record(
            &format!("budget-{steps}"),
            &module,
            &format!("{result:#?}"),
            None,
        );
        if steps == low {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(
                result.unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
        }
    }
    assert_eq!(
        zil_analyze::analyze(&module, &Limits::default()).unwrap(),
        expected
    );
    println!(
        "204 raw owned Store + child Analyze exact N={low}, N-1 ResourceLimit, fresh plan equal"
    );
}
