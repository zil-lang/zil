//! Hand-built TypedIR admission and responsibility controls. These are not
//! source-lowering, independent Verify, or native-execution evidence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn ins(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
fn function(id: u32) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("probe{id}"),
        foreign: None,
        environment: None,
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
fn local(index: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(index),
        name: format!("p{index}"),
        ty,
        mutable: true,
        span: span(),
    }
}
fn element(fields: usize) -> Type {
    Type::Package {
        name: "State<i32,F0>".into(),
        fields: std::iter::once(("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))))
            .chain(
                (0..fields).map(|index| (format!("callback{index}"), Type::Closure(ClosureId(0)))),
            )
            .chain(std::iter::once(("value".into(), Type::I32)))
            .collect(),
    }
}
fn fixture(element: Type, environments: usize) -> TypedModule {
    let store = Type::OwnedStore {
        element: Box::new(element),
    };
    let mut probe = function(0);
    probe.parameters = vec![LocalId(0)];
    probe.locals = vec![local(0, Type::Managed(Box::new(store.clone())))];
    probe.blocks[0].instructions = vec![
        ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        ins(None, InstructionKind::DropManaged { local: LocalId(0) }),
        ins(None, InstructionKind::StorageDead(LocalId(0))),
        ins(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    let mut functions = vec![probe];
    let mut closures = vec![];
    for index in 0..environments {
        let id = ClosureId(index as u32);
        let mut body = function(index as u32 + 1);
        body.environment = Some(id);
        closures.push(ClosureDefinition {
            id,
            function: body.id,
            captures: vec![],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        });
        functions.push(body);
    }
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"raw callback Store boundary"),
        target: TargetProfile::host_llvm(),
        functions,
        closures,
        resources: vec![ResourceDefinition {
            name: "@builtin:OwnedStore<State<i32,F0>>".into(),
            payload: store,
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn record(module: &TypedModule, result: &str) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let path = PathBuf::from(root).join(format!(
        "raw201-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("result.txt"), result).unwrap();
}
fn accepted(module: &TypedModule) -> CandidatePlan {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    result.unwrap()
}
fn rejected(module: &TypedModule, category: ResultCategory, message: &str) {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == category
            && d.message.contains(message)),
        "{errors:?}"
    );
}
fn minimum(module: &TypedModule) -> usize {
    let expected = accepted(module);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        match analyze(
            module,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert_eq!(errors.len(), 1, "{errors:?}");
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                assert_eq!(errors[0].phase, Phase::Analyze);
                low = mid + 1;
            }
        }
    }
    let exact = analyze(
        module,
        &Limits {
            max_analysis_steps: low,
            ..Limits::default()
        },
    )
    .unwrap();
    let below = analyze(
        module,
        &Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(below.len(), 1);
    assert_eq!(below[0].category, ResultCategory::ResourceLimit);
    assert_eq!(below[0].phase, Phase::Analyze);
    assert_eq!(exact, expected);
    assert_eq!(analyze(module, &Limits::default()).unwrap(), expected);
    record(
        module,
        &format!("exact N={low}; N-1={below:#?}; fresh plan equals exact and initial"),
    );
    low
}

#[test]
fn old_noncallback_store_budget_is_unchanged() {
    let raw = fixture(element(0), 0);
    let required = minimum(&raw);
    println!("old noncallback Store exact budget: {required}");
    // Measured at cbaf40b9 before any 201 production edit, with the same
    // target and byte-identical input. This is specifically Analyze's bound.
    assert_eq!(required, 561);
}

#[test]
fn finite_one_and_two_callback_elements_accept_each_owned_call_mode() {
    for fields in [1, 2] {
        for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let mut raw = fixture(element(fields), 1);
            raw.closures[0].kind = kind;
            accepted(&raw);
        }
    }
    let mut captured = fixture(element(2), 1);
    captured.closures[0].captures = vec![
        Type::Tuple(vec![Type::I32; 4]),
        Type::Managed(Box::new(Type::ByteBuffer)),
    ];
    accepted(&captured);
}

#[test]
fn unused_parameter_and_resource_only_callback_elements_are_checked() {
    let mut external = fixture(element(1), 1);
    external.closures[0].captures = vec![Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    }];
    rejected(
        &external,
        ResultCategory::Unsupported,
        "self-contained environment",
    );
    // No Store in any function type or instruction: only the sealed raw
    // resource definition still references this invalid callback element.
    external.functions[0] = function(0);
    rejected(
        &external,
        ResultCategory::Unsupported,
        "self-contained environment",
    );
    external.closures[0].captures.clear();
    accepted(&external);
}

#[test]
fn two_distinct_self_contained_copy_result_callbacks_are_accepted_since_203() {
    // This is the original 201 heterogeneous-root input. The explicit 203
    // profile now admits its two direct, self-contained Fn/Unit roots.
    let mut different = element(2);
    let Type::Package { fields, .. } = &mut different else {
        unreachable!()
    };
    fields[2].1 = Type::Closure(ClosureId(1));
    accepted(&fixture(different, 2));
}

#[test]
fn fields_require_direct_finite_shape_and_complete_resources() {
    rejected(
        &fixture(element(3), 1),
        ResultCategory::Unsupported,
        "one or two direct self-contained callbacks",
    );
    rejected(
        &fixture(element(1), 0),
        ResultCategory::SafeReject,
        "environment is absent",
    );

    let mut no_resource = element(1);
    let Type::Package { fields, .. } = &mut no_resource else {
        unreachable!()
    };
    fields.remove(0);
    rejected(
        &fixture(no_resource, 1),
        ResultCategory::Unsupported,
        "one or two direct self-contained callbacks",
    );

    for hidden in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(0))),
        },
        Type::Array {
            element: Box::new(Type::Closure(ClosureId(0))),
            length: 2,
        },
        Type::Package {
            name: "Nested".into(),
            fields: vec![
                ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("callback".into(), Type::Closure(ClosureId(0))),
            ],
        },
        Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element(1)),
        })),
    ] {
        let mut bad = element(0);
        let Type::Package { fields, .. } = &mut bad else {
            unreachable!()
        };
        fields.push(("hidden".into(), hidden));
        rejected(
            &fixture(bad, 1),
            ResultCategory::Unsupported,
            "one or two direct self-contained callbacks",
        );
    }
}

#[test]
fn direct_and_package_store_captures_stay_closed() {
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element(0)),
    }));
    for capture in [
        Type::Desc(Box::new(Type::ByteBuffer)),
        store.clone(),
        Type::Package {
            name: "StoreOwner".into(),
            fields: vec![("items".into(), store)],
        },
        element(1),
    ] {
        let mut raw = fixture(element(1), 1);
        raw.closures[0].captures = vec![capture];
        rejected(
            &raw,
            ResultCategory::Unsupported,
            "self-contained environment",
        );
    }
    let mut through_child = fixture(element(1), 2);
    through_child.closures[0].captures = vec![Type::Closure(ClosureId(1))];
    through_child.closures[1].captures = vec![Type::Desc(Box::new(Type::ByteBuffer))];
    rejected(
        &through_child,
        ResultCategory::Unsupported,
        "self-contained environment",
    );
    through_child.closures[1].captures = vec![Type::I32];
    accepted(&through_child);
}

fn new_store_return(element: Type, environments: usize, error_union: bool) -> TypedModule {
    let mut raw = fixture(element.clone(), environments);
    let owned = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element.clone()),
    }));
    let f = &mut raw.functions[0];
    f.values = vec![owned.clone()];
    f.blocks[0].instructions.insert(
        1,
        ins(Some(ValueId(0)), InstructionKind::OwnedStoreNew { element }),
    );
    let result = if error_union {
        let union = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(owned),
        };
        f.values.push(union.clone());
        f.blocks[0].instructions.insert(
            2,
            ins(
                Some(ValueId(1)),
                InstructionKind::MakeErrorUnion {
                    is_error: false,
                    value: ValueId(0),
                },
            ),
        );
        f.return_type = union;
        ValueId(1)
    } else {
        f.return_type = owned;
        ValueId(0)
    };
    f.blocks[0].terminator = Terminator::Return(Some(result));
    raw
}

#[test]
fn store_return_error_payload_and_raw_new_metadata_are_independently_walked() {
    for error_union in [false, true] {
        accepted(&new_store_return(element(1), 1, error_union));
    }
    let baseline = new_store_return(element(0), 1, false);
    accepted(&baseline);
    let mut attacked = baseline.clone();
    // All parameter, return, SSA and resource types remain the old ordinary
    // Store. Only the constructor metadata contains the unsupported callback
    // shape. Analyze must see it before Verify's separate exact-type check.
    attacked.functions[0].blocks[0].instructions[1].kind = InstructionKind::OwnedStoreNew {
        element: element(3),
    };
    rejected(
        &attacked,
        ResultCategory::Unsupported,
        "one or two direct self-contained callbacks",
    );
    accepted(&baseline);
}

fn window(mutable: bool, kind: CallableKind, lending: bool) -> TypedModule {
    let mut raw = fixture(element(1), 1);
    raw.closures[0].kind = kind;
    let desc = Type::Desc(Box::new(Type::OwnedStore {
        element: Box::new(element(1)),
    }));
    let address = |inner| Type::Address {
        mutable,
        inner: Box::new(inner),
    };
    let mut call_result = Type::Unit;
    if lending {
        call_result = Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        };
        raw.closures[0].captures = vec![Type::Managed(Box::new(Type::ByteBuffer))];
        raw.closures[0].result = call_result.clone();
        raw.closures[0].lending_captures = vec![0];
        let body = &mut raw.functions[1];
        body.return_type = call_result.clone();
        body.values = vec![Type::Desc(Box::new(Type::ByteBuffer)), call_result.clone()];
        body.blocks[0].instructions = vec![
            ins(
                Some(ValueId(0)),
                InstructionKind::DescribeCapture { index: 0 },
            ),
            ins(
                Some(ValueId(1)),
                InstructionKind::ByteBufferBorrow {
                    buffer: ValueId(0),
                    mutable: false,
                },
            ),
        ];
        body.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    }
    let f = &mut raw.functions[0];
    f.values = vec![
        desc,
        Type::I32,
        address(element(1)),
        address(Type::Closure(ClosureId(0))),
        address(Type::Closure(ClosureId(0))),
    ];
    let mut operations = vec![
        ins(
            Some(ValueId(0)),
            InstructionKind::DescribeLocal { local: LocalId(0) },
        ),
        ins(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(0)),
        ),
        ins(
            Some(ValueId(2)),
            if mutable {
                InstructionKind::OwnedStoreBorrowMut {
                    store: ValueId(0),
                    index: ValueId(1),
                }
            } else {
                InstructionKind::OwnedStoreBorrow {
                    store: ValueId(0),
                    index: ValueId(1),
                }
            },
        ),
        ins(
            Some(ValueId(3)),
            InstructionKind::AddressField {
                address: ValueId(2),
                index: 1,
            },
        ),
        ins(
            Some(ValueId(4)),
            InstructionKind::ReborrowAddress {
                address: ValueId(3),
                mutable,
            },
        ),
        ins(
            lending.then_some(ValueId(5)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(4),
                kind,
                arguments: vec![],
            },
        ),
    ];
    if lending {
        f.values.push(call_result);
    }
    f.blocks[0].instructions.splice(1..1, operations.drain(..));
    raw
}

#[test]
fn callback_windows_and_lending_results_retain_the_parent_store_hold() {
    for (mutable, kind, lending) in [
        (false, CallableKind::Fn, false),
        (true, CallableKind::FnMut, false),
        (false, CallableKind::Fn, true),
        (true, CallableKind::FnMut, true),
    ] {
        let raw = window(mutable, kind, lending);
        let plan = accepted(&raw);
        let holds = &plan.functions[0].holds;
        for hold in &holds[2..if lending { 6 } else { 5 }] {
            assert_eq!(hold.roots, vec![HoldRoot::Local(LocalId(0))], "{plan:#?}");
        }
    }
}

#[test]
fn borrowed_call_cannot_consume_or_upgrade_shared_permissions() {
    let shared = window(false, CallableKind::Fn, false);
    accepted(&shared);
    for kind in [CallableKind::FnOnce, CallableKind::FnMut] {
        let mut attacked = shared.clone();
        for instruction in &mut attacked.functions[0].blocks[0].instructions {
            if let InstructionKind::CallClosureBorrowed { kind: actual, .. } = &mut instruction.kind
            {
                *actual = kind;
            }
        }
        rejected(
            &attacked,
            ResultCategory::SafeReject,
            "exclusive FnMut address",
        );
    }
}

#[test]
fn callback_store_work_has_exact_replayable_bounds_and_capture_width_cost() {
    let mut narrow = fixture(element(2), 1);
    narrow.closures[0].captures = vec![Type::Tuple(vec![Type::I32; 2])];
    let mut wide = narrow.clone();
    wide.closures[0].captures = vec![Type::Tuple(vec![Type::I32; 64])];
    let budgets = [
        minimum(&fixture(element(1), 1)),
        minimum(&narrow),
        minimum(&wide),
        minimum(&window(true, CallableKind::FnMut, true)),
    ];
    println!("callback Store Analyze exact budgets: {budgets:?}");
    assert!(
        budgets[2] - budgets[1] >= 4 * (64 - 2),
        "each Store occurrence must pay actual capture work"
    );
}
