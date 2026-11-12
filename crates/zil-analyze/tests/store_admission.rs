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
        retained_return_sources: None,
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        id: FunctionId(id),
        name: format!("probe{id}"),
        foreign: None,
        environment: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
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
            storage_life: None,
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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
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
fn two_distinct_self_contained_copy_result_callbacks_are_accepted() {
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
        // 223 admits finite Product Store shapes before checking their actual
        // children. Preserve this original malformed Nested+Closure input:
        // its child is now rejected by Product qualification. The other hidden
        // Address/Array/Store neighbors retain their original boundary.
        let (category, message) = if matches!(&hidden, Type::Package { .. }) {
            (
                ResultCategory::SafeReject,
                "ordinary Product has an unsupported owning or borrowed child",
            )
        } else {
            (
                ResultCategory::Unsupported,
                "one or two direct self-contained callbacks",
            )
        };
        let mut bad = element(0);
        let Type::Package { fields, .. } = &mut bad else {
            unreachable!()
        };
        fields.push(("hidden".into(), hidden));
        rejected(&fixture(bad, 1), category, message);
    }
}

#[test]
fn direct_and_package_store_captures_stay_closed() {
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element(0)),
    }));
    // Preserve all original raw attacks. The captured leaf element(0) and
    // the outer callback element(1) share the nominal name State<i32,F0>,
    // but only the latter has a canonical definition here. 204 admission
    // therefore rejects its callback-bearing payload at the earlier leaf
    // boundary; do not add a new definition or turn these into positives.
    for (capture, message) in [
        (
            Type::Desc(Box::new(Type::ByteBuffer)),
            "self-contained environment",
        ),
        (
            store.clone(),
            "captured Store Item cannot contain callbacks, addresses, descriptions, nested Store or unresolved types",
        ),
        (
            Type::Package {
                name: "StoreOwner".into(),
                fields: vec![("items".into(), store)],
            },
            "captured Store Item cannot contain callbacks, addresses, descriptions, nested Store or unresolved types",
        ),
        (
            element(1),
            "captured Store Package requires direct owned Store, supported Managed and finite Copy fields",
        ),
    ] {
        let mut raw = fixture(element(1), 1);
        raw.closures[0].captures = vec![capture];
        rejected(&raw, ResultCategory::Unsupported, message);
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

fn pod_element() -> Type {
    Type::Record {
        name: "Point".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}

fn pod_fixture(element: Type) -> TypedModule {
    let mut raw = fixture(element, 0);
    raw.resources[0].name = "@builtin:OwnedStore<Point>".into();
    raw
}

fn scalar_name(element: &Type) -> &'static str {
    match element {
        Type::Bool => "bool",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::F32 => "f32",
        Type::F64 => "f64",
        _ => panic!("unsupported scalar test input"),
    }
}

fn scalar_fixture(element: Type) -> TypedModule {
    let mut raw = fixture(element.clone(), 0);
    raw.resources[0].name = format!("@builtin:OwnedStore<{}>", scalar_name(&element));
    raw
}

fn scalar_window(element: Type, mutable: bool) -> TypedModule {
    let mut raw = scalar_fixture(element.clone());
    let f = &mut raw.functions[0];
    f.values = vec![
        Type::Desc(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        })),
        Type::I64,
        Type::Address {
            mutable,
            inner: Box::new(element.clone()),
        },
        element,
        Type::Bool,
    ];
    f.blocks[0].instructions.splice(
        1..1,
        [
            ins(
                Some(ValueId(0)),
                InstructionKind::DescribeLocal { local: LocalId(0) },
            ),
            ins(
                Some(ValueId(1)),
                InstructionKind::Constant(Constant::I64(0)),
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
                InstructionKind::ReadAddress {
                    address: ValueId(2),
                },
            ),
            ins(
                Some(ValueId(4)),
                InstructionKind::OwnedStoreReserve {
                    store: ValueId(0),
                    capacity: ValueId(1),
                },
            ),
        ],
    );
    raw
}

#[test]
fn scalar_store_all_nine_complete_types_and_both_windows() {
    let mut bounds = Vec::new();
    for element in [
        Type::Bool,
        Type::U8,
        Type::U16,
        Type::I32,
        Type::I64,
        Type::U64,
        Type::Usize,
        Type::F32,
        Type::F64,
    ] {
        accepted(&scalar_fixture(element.clone()));
        for mutable in [false, true] {
            let raw = scalar_window(element.clone(), mutable);
            let plan = accepted(&raw);
            assert_eq!(
                plan.functions[0].holds[2].roots,
                vec![HoldRoot::Local(LocalId(0))]
            );
            bounds.push((scalar_name(&element), mutable, minimum(&raw)));
            let mut push = raw.clone();
            push.functions[0].values[4] = Type::ErrorUnion {
                error: Box::new(element.clone()),
                result: Box::new(Type::Unit),
            };
            push.functions[0].blocks[0].instructions[5].kind = InstructionKind::OwnedStorePush {
                store: ValueId(0),
                value: ValueId(3),
            };
            accepted(&push);
            let mut pop = raw;
            pop.functions[0].values[4] = Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(element.clone()),
            };
            pop.functions[0].blocks[0].instructions[5].kind =
                InstructionKind::OwnedStorePop { store: ValueId(0) };
            accepted(&pop);
        }
    }
    println!("Analyze scalar shared/mutable exact N, N-1 and fresh: {bounds:?}");
}

#[test]
fn scalar_store_rejects_nearby_type_and_canonical_forgery() {
    for forbidden in [
        Type::Unit,
        Type::Tuple(vec![Type::I32]),
        Type::Array {
            element: Box::new(Type::I32),
            length: 1,
        },
        Type::ForeignHandle,
    ] {
        let mut raw = scalar_fixture(Type::I32);
        raw.functions.clear();
        raw.resources[0].payload = Type::OwnedStore {
            element: Box::new(forbidden),
        };
        assert!(analyze(&raw, &Limits::default()).is_err());
    }
    let mut wrong_name = scalar_fixture(Type::U64);
    wrong_name.resources[0].name = "@builtin:OwnedStore<usize>".into();
    rejected(
        &wrong_name,
        ResultCategory::SafeReject,
        "canonical definition",
    );
    let mut wrong_payload = scalar_fixture(Type::U64);
    wrong_payload.resources[0].payload = Type::OwnedStore {
        element: Box::new(Type::Usize),
    };
    rejected(
        &wrong_payload,
        ResultCategory::SafeReject,
        "canonical definition",
    );
    let mut wrong_index = scalar_window(Type::I32, true);
    wrong_index.functions[0].values[1] = Type::I32;
    rejected(
        &wrong_index,
        ResultCategory::SafeReject,
        "exact complete operands and result",
    );
    let mut wrong_result = scalar_window(Type::U64, true);
    wrong_result.functions[0].values[2] = Type::Address {
        mutable: true,
        inner: Box::new(Type::Usize),
    };
    rejected(
        &wrong_result,
        ResultCategory::SafeReject,
        "exact complete operands and result",
    );
    let mut wrong_carrier = scalar_window(Type::I32, false);
    wrong_carrier.functions[0].values[0] = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(Type::I32),
    }));
    rejected(
        &wrong_carrier,
        ResultCategory::SafeReject,
        "real description",
    );
}

#[test]
fn scalar_mutable_window_uses_actual_whole_store_root() {
    let mut same = scalar_window(Type::I32, true);
    let f = &mut same.functions[0];
    f.values.extend([
        Type::I64,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
    ]);
    f.blocks[0].instructions.insert(
        4,
        ins(
            Some(ValueId(5)),
            InstructionKind::Constant(Constant::I64(1)),
        ),
    );
    let second = ins(
        Some(ValueId(6)),
        InstructionKind::OwnedStoreBorrow {
            store: ValueId(0),
            index: ValueId(5),
        },
    );
    f.blocks[0].instructions.insert(5, second.clone());
    rejected(
        &same,
        ResultCategory::SafeReject,
        "actual borrowed resource access conflicts",
    );
    let f = &mut same.functions[0];
    f.blocks[0].instructions.remove(5);
    f.blocks[0].instructions.insert(6, second);
    accepted(&same);

    let mut other = scalar_window(Type::I32, true);
    let f = &mut other.functions[0];
    f.parameters.push(LocalId(1));
    f.locals.push(local(1, f.locals[0].ty.clone()));
    f.values.push(f.values[0].clone());
    f.blocks[0].instructions.insert(
        4,
        ins(
            Some(ValueId(5)),
            InstructionKind::DescribeLocal { local: LocalId(1) },
        ),
    );
    f.blocks[0].instructions[6].kind = InstructionKind::OwnedStoreReserve {
        store: ValueId(5),
        capacity: ValueId(1),
    };
    let reserve = f.blocks[0].instructions.remove(6);
    f.blocks[0].instructions.insert(5, reserve);
    f.blocks[0].instructions.splice(
        7..7,
        [
            ins(None, InstructionKind::DropManaged { local: LocalId(1) }),
            ins(None, InstructionKind::StorageDead(LocalId(1))),
        ],
    );
    accepted(&other);
}

#[test]
fn scalar_store_qualifies_ordinary_owning_fields_but_not_record_or_array_children() {
    let baseline = scalar_fixture(Type::I32);
    let store = Type::Managed(Box::new(baseline.resources[0].payload.clone()));
    for owner in [
        Type::Package {
            name: "Bag".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Tuple(vec![store.clone()]),
        Type::Enum {
            name: "Owner".into(),
            variants: vec![("Item".into(), vec![store.clone()])],
        },
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(store.clone()),
        },
    ] {
        let mut raw = baseline.clone();
        raw.functions[0].locals.push(local(1, owner));
        accepted(&raw);
    }
    for owner in [
        Type::Record {
            name: "Bag".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Array {
            element: Box::new(store.clone()),
            length: 1,
        },
        Type::Package {
            name: "Bag".into(),
            fields: vec![(
                "items".into(),
                Type::Array {
                    element: Box::new(store),
                    length: 1,
                },
            )],
        },
    ] {
        let mut raw = baseline.clone();
        raw.functions[0].locals.push(local(1, owner));
        rejected(
            &raw,
            ResultCategory::Unsupported,
            "does not qualify Record or Array children",
        );
    }
}

#[test]
fn pod_store_admits_complete_numeric_trees_without_element_debts() {
    let mut element = pod_element();
    let Type::Record { fields, .. } = &mut element else {
        unreachable!()
    };
    fields.push((
        "all".into(),
        Type::Tuple(vec![
            Type::Unit,
            Type::Bool,
            Type::U8,
            Type::U16,
            Type::I32,
            Type::I64,
            Type::U64,
            Type::Usize,
            Type::F32,
            Type::F64,
            Type::Record {
                name: "Empty".into(),
                fields: vec![],
            },
            Type::Array {
                element: Box::new(Type::U16),
                length: 3,
            },
        ]),
    ));
    accepted(&pod_fixture(element));
    let raw = pod_fixture(pod_element());
    let required = minimum(&raw);
    println!("POD Store Analyze N={required}; N-1 ResourceLimit; fresh identical");
}

#[test]
fn pod_store_rebuilds_unused_definitions_full_identity_and_all_leaves() {
    let baseline = pod_fixture(pod_element());
    for kind in 0..4 {
        let mut raw = baseline.clone();
        // Remove every function: sealed but unused definitions still qualify.
        raw.functions.clear();
        match kind {
            0 => raw.resources[0].name = "wrong".into(),
            1 => raw.resources.push(raw.resources[0].clone()),
            2 => raw.resources[0].teardown = Some(FunctionId(0)),
            3 => {
                raw.functions = baseline.functions.clone();
                let Type::OwnedStore { element } = &mut raw.resources[0].payload else {
                    unreachable!()
                };
                let Type::Record { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                fields[0].1 = Type::I64;
            }
            _ => unreachable!(),
        }
        assert!(analyze(&raw, &Limits::default()).is_err(), "kind={kind}");
    }
    for forbidden in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::ByteBuffer)),
        Type::Managed(Box::new(Type::ByteBuffer)),
        Type::ForeignHandle,
        Type::CWideString,
        Type::Closure(ClosureId(0)),
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::I32),
        },
        Type::Enum {
            name: "E".into(),
            variants: vec![("A".into(), vec![])],
        },
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::I32),
        },
        Type::OwnedStore {
            element: Box::new(pod_element()),
        },
    ] {
        let mut element = pod_element();
        let Type::Record { fields, .. } = &mut element else {
            unreachable!()
        };
        fields.push((
            "hidden".into(),
            Type::Array {
                element: Box::new(forbidden),
                length: 0,
            },
        ));
        let mut raw = pod_fixture(element);
        raw.functions.clear();
        assert!(analyze(&raw, &Limits::default()).is_err(), "{raw:#?}");
    }
    for fields in [
        vec![],
        vec![("zero".into(), Type::Unit)],
        vec![("".into(), Type::I32)],
        vec![("n".into(), Type::I32), ("n".into(), Type::I64)],
    ] {
        let mut raw = pod_fixture(Type::Record {
            name: "Point".into(),
            fields,
        });
        raw.functions.clear();
        assert!(analyze(&raw, &Limits::default()).is_err(), "{raw:#?}");
    }
    accepted(&baseline);
}

#[test]
fn pod_store_does_not_qualify_an_unused_owning_resource_head() {
    let mut raw = pod_fixture(pod_element());
    let store = Type::Managed(Box::new(raw.resources[0].payload.clone()));
    let hidden_record = Type::Record {
        name: "Hidden".into(),
        fields: vec![("items".into(), store.clone())],
    };
    raw.resources.push(ResourceDefinition {
        name: "Hidden".into(),
        payload: hidden_record,
        teardown: None,
    });
    rejected(
        &raw,
        ResultCategory::Unsupported,
        "does not qualify Record or Array children",
    );

    raw.resources.pop();
    raw.resources.push(ResourceDefinition {
        name: "HiddenPackage".into(),
        payload: Type::Package {
            name: "HiddenPackage".into(),
            fields: vec![("items".into(), store)],
        },
        teardown: None,
    });
    rejected(
        &raw,
        ResultCategory::Unsupported,
        "does not grant owning aggregate field or payload qualification",
    );
}

#[test]
fn pod_store_qualifies_unused_ordinary_owning_aggregate_fields_and_payloads() {
    let baseline = pod_fixture(pod_element());
    let store = Type::Managed(Box::new(baseline.resources[0].payload.clone()));
    let bag = Type::Package {
        name: "Bag".into(),
        fields: vec![(
            "items".into(),
            Type::Managed(Box::new(baseline.resources[0].payload.clone())),
        )],
    };
    for owner in [
        bag.clone(),
        Type::Package {
            name: "Outer".into(),
            fields: vec![("bag".into(), bag), ("tag".into(), Type::I32)],
        },
        Type::Tuple(vec![store.clone()]),
        Type::Enum {
            name: "Owner".into(),
            variants: vec![("Item".into(), vec![store.clone()])],
        },
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(store.clone()),
        },
        Type::ErrorUnion {
            error: Box::new(store.clone()),
            result: Box::new(Type::Unit),
        },
    ] {
        let mut raw = baseline.clone();
        raw.functions[0].locals.push(local(1, owner));
        accepted(&raw);
    }
    for owner in [
        Type::Record {
            name: "Hidden".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Array {
            element: Box::new(store.clone()),
            length: 1,
        },
        Type::Package {
            name: "NestedArray".into(),
            fields: vec![(
                "items".into(),
                Type::Array {
                    element: Box::new(store),
                    length: 0,
                },
            )],
        },
    ] {
        let mut raw = baseline.clone();
        raw.functions[0].locals.push(local(1, owner));
        rejected(
            &raw,
            ResultCategory::Unsupported,
            "does not qualify Record or Array children",
        );
    }
    // Preserve the old flat Product field family with an owned Package element.
    let mut old = fixture(element(0), 0);
    old.functions[0].locals.push(local(
        1,
        Type::Package {
            name: "OldBag".into(),
            fields: vec![(
                "items".into(),
                Type::Managed(Box::new(old.resources[0].payload.clone())),
            )],
        },
    ));
    accepted(&old);
    accepted(&baseline);
}

fn pod_window() -> TypedModule {
    let element = pod_element();
    let mut raw = pod_fixture(element.clone());
    let f = &mut raw.functions[0];
    f.values = vec![
        Type::Desc(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        })),
        Type::I64,
        Type::Address {
            mutable: false,
            inner: Box::new(element.clone()),
        },
        element,
        Type::Bool,
    ];
    f.blocks[0].instructions.splice(
        1..1,
        [
            ins(
                Some(ValueId(0)),
                InstructionKind::DescribeLocal { local: LocalId(0) },
            ),
            ins(
                Some(ValueId(1)),
                InstructionKind::Constant(Constant::I64(0)),
            ),
            ins(
                Some(ValueId(2)),
                InstructionKind::OwnedStoreBorrow {
                    store: ValueId(0),
                    index: ValueId(1),
                },
            ),
            ins(
                Some(ValueId(3)),
                InstructionKind::ReadAddress {
                    address: ValueId(2),
                },
            ),
            ins(
                Some(ValueId(4)),
                InstructionKind::OwnedStoreReserve {
                    store: ValueId(0),
                    capacity: ValueId(1),
                },
            ),
        ],
    );
    raw
}

fn pod_mutable_window() -> TypedModule {
    let mut raw = pod_window();
    let f = &mut raw.functions[0];
    f.values[2] = Type::Address {
        mutable: true,
        inner: Box::new(pod_element()),
    };
    f.blocks[0].instructions[3].kind = InstructionKind::OwnedStoreBorrowMut {
        store: ValueId(0),
        index: ValueId(1),
    };
    raw
}

#[test]
fn pod_store_windows_keep_actual_roots_last_use_and_structural_conflicts() {
    let baseline = pod_window();
    let plan = accepted(&baseline);
    assert_eq!(
        plan.functions[0].holds[2].roots,
        vec![HoldRoot::Local(LocalId(0))]
    );
    let required = minimum(&baseline);
    println!("POD view Analyze N={required}; N-1 ResourceLimit; fresh identical");
    for kind in 0..3 {
        let mut raw = baseline.clone();
        let f = &mut raw.functions[0];
        let write = &mut f.blocks[0].instructions[5];
        match kind {
            0 => {}
            1 => {
                f.values[4] = Type::ErrorUnion {
                    error: Box::new(Type::Unit),
                    result: Box::new(pod_element()),
                };
                write.kind = InstructionKind::OwnedStorePop { store: ValueId(0) };
            }
            2 => {
                // Push a separate Copy snapshot while the original view lives.
                f.values.push(pod_element());
                f.blocks[0].instructions.insert(
                    4,
                    ins(
                        Some(ValueId(5)),
                        InstructionKind::ReadAddress {
                            address: ValueId(2),
                        },
                    ),
                );
                f.values[4] = Type::ErrorUnion {
                    error: Box::new(pod_element()),
                    result: Box::new(Type::Unit),
                };
                f.blocks[0].instructions[6].kind = InstructionKind::OwnedStorePush {
                    store: ValueId(0),
                    value: ValueId(5),
                };
            }
            _ => unreachable!(),
        }
        accepted(&raw);
        let f = &mut raw.functions[0];
        // Move structural mutation immediately before the final view read.
        let position = f.blocks[0]
            .instructions
            .iter()
            .position(|i| {
                matches!(
                    i.kind,
                    InstructionKind::OwnedStoreReserve { .. }
                        | InstructionKind::OwnedStorePop { .. }
                        | InstructionKind::OwnedStorePush { .. }
                )
            })
            .unwrap();
        let mutation = f.blocks[0].instructions.remove(position);
        let before = f.blocks[0]
            .instructions
            .iter()
            .position(|i| i.result == Some(ValueId(3)))
            .unwrap();
        f.blocks[0].instructions.insert(before, mutation);
        assert!(
            analyze(&raw, &Limits::default()).is_err(),
            "kind={kind}: {raw:#?}"
        );
    }
}

#[test]
fn pod_store_rejects_forged_mutable_windows_and_protocol_types() {
    let baseline = pod_window();
    for kind in 0..5 {
        let mut raw = baseline.clone();
        let f = &mut raw.functions[0];
        match kind {
            0 => {
                f.values[2] = Type::Address {
                    mutable: true,
                    inner: Box::new(Type::Record {
                        name: "Point".into(),
                        fields: vec![("n".into(), Type::I64)],
                    }),
                };
                f.blocks[0].instructions[3].kind = InstructionKind::OwnedStoreBorrowMut {
                    store: ValueId(0),
                    index: ValueId(1),
                };
            }
            1 => f.values[1] = Type::I32,
            2 => {
                f.values[2] = Type::Address {
                    mutable: true,
                    inner: Box::new(pod_element()),
                }
            }
            3 => f.values[4] = Type::I64,
            4 => f.blocks[0].instructions[5].result = None,
            _ => unreachable!(),
        }
        assert!(analyze(&raw, &Limits::default()).is_err(), "kind={kind}");
    }
    accepted(&baseline);
    let mut wrong_index = pod_mutable_window();
    wrong_index.functions[0].values[1] = Type::I32;
    rejected(
        &wrong_index,
        ResultCategory::SafeReject,
        "exact complete operands and result",
    );
    let mut wrong_result = pod_mutable_window();
    wrong_result.functions[0].values[2] = Type::Address {
        mutable: false,
        inner: Box::new(pod_element()),
    };
    rejected(
        &wrong_result,
        ResultCategory::SafeReject,
        "exact complete operands and result",
    );
    accepted(&pod_mutable_window());
}

#[test]
fn pod_mutable_field_write_keeps_the_actual_store_parent() {
    let mut raw = pod_mutable_window();
    let f = &mut raw.functions[0];
    f.values.extend([
        Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        },
        Type::I32,
    ]);
    f.blocks[0].instructions.splice(
        4..4,
        [
            ins(
                Some(ValueId(5)),
                InstructionKind::AddressField {
                    address: ValueId(2),
                    index: 0,
                },
            ),
            ins(
                Some(ValueId(6)),
                InstructionKind::Constant(Constant::I32(42)),
            ),
            ins(
                None,
                InstructionKind::WriteAddress {
                    address: ValueId(5),
                    value: ValueId(6),
                },
            ),
        ],
    );
    let plan = accepted(&raw);
    for value in [2, 5] {
        assert_eq!(
            plan.functions[0].holds[value].roots,
            vec![HoldRoot::Local(LocalId(0))]
        );
    }
    let required = minimum(&raw);
    println!("Analyze mutable field N={required}; N-1 ResourceLimit; fresh identical");
}

#[test]
fn pod_mutable_window_conflicts_with_every_same_store_index() {
    for mutable in [false, true] {
        let mut raw = pod_mutable_window();
        let f = &mut raw.functions[0];
        f.values.extend([
            Type::I64,
            Type::Address {
                mutable,
                inner: Box::new(pod_element()),
            },
        ]);
        let second = ins(
            Some(ValueId(6)),
            if mutable {
                InstructionKind::OwnedStoreBorrowMut {
                    store: ValueId(0),
                    index: ValueId(5),
                }
            } else {
                InstructionKind::OwnedStoreBorrow {
                    store: ValueId(0),
                    index: ValueId(5),
                }
            },
        );
        f.blocks[0].instructions.insert(
            4,
            ins(
                Some(ValueId(5)),
                InstructionKind::Constant(Constant::I64(1)),
            ),
        );
        f.blocks[0].instructions.insert(5, second.clone());
        rejected(
            &raw,
            ResultCategory::SafeReject,
            "actual borrowed resource access conflicts",
        );
        // The first result has its last use before the next borrow.
        let f = &mut raw.functions[0];
        f.blocks[0].instructions.remove(5);
        f.blocks[0].instructions.insert(6, second);
        accepted(&raw);
    }
}

#[test]
fn pod_mutable_window_allows_a_distinct_actual_store() {
    let mut raw = pod_mutable_window();
    let f = &mut raw.functions[0];
    f.parameters.push(LocalId(1));
    f.locals.push(local(1, f.locals[0].ty.clone()));
    f.values.push(f.values[0].clone());
    f.blocks[0].instructions.insert(
        4,
        ins(
            Some(ValueId(5)),
            InstructionKind::DescribeLocal { local: LocalId(1) },
        ),
    );
    f.blocks[0].instructions[6].kind = InstructionKind::OwnedStoreReserve {
        store: ValueId(5),
        capacity: ValueId(1),
    };
    let reserve = f.blocks[0].instructions.remove(6);
    f.blocks[0].instructions.insert(5, reserve);
    f.blocks[0].instructions.splice(
        7..7,
        [
            ins(None, InstructionKind::DropManaged { local: LocalId(1) }),
            ins(None, InstructionKind::StorageDead(LocalId(1))),
        ],
    );
    accepted(&raw);
}

#[test]
fn pod_mutable_window_blocks_structural_changes_until_last_use() {
    for kind in 0..3 {
        let mut raw = pod_mutable_window();
        let f = &mut raw.functions[0];
        match kind {
            0 => {}
            1 => {
                f.values[4] = Type::ErrorUnion {
                    error: Box::new(Type::Unit),
                    result: Box::new(pod_element()),
                };
                f.blocks[0].instructions[5].kind =
                    InstructionKind::OwnedStorePop { store: ValueId(0) };
            }
            2 => {
                f.values.push(pod_element());
                f.blocks[0].instructions.insert(
                    4,
                    ins(
                        Some(ValueId(5)),
                        InstructionKind::ReadAddress {
                            address: ValueId(2),
                        },
                    ),
                );
                f.values[4] = Type::ErrorUnion {
                    error: Box::new(pod_element()),
                    result: Box::new(Type::Unit),
                };
                f.blocks[0].instructions[6].kind = InstructionKind::OwnedStorePush {
                    store: ValueId(0),
                    value: ValueId(5),
                };
            }
            _ => unreachable!(),
        }
        accepted(&raw);
        let f = &mut raw.functions[0];
        let position = f.blocks[0]
            .instructions
            .iter()
            .position(|instruction| {
                matches!(
                    instruction.kind,
                    InstructionKind::OwnedStoreReserve { .. }
                        | InstructionKind::OwnedStorePop { .. }
                        | InstructionKind::OwnedStorePush { .. }
                )
            })
            .unwrap();
        let mutation = f.blocks[0].instructions.remove(position);
        let final_read = f.blocks[0]
            .instructions
            .iter()
            .position(|instruction| instruction.result == Some(ValueId(3)))
            .unwrap();
        f.blocks[0].instructions.insert(final_read, mutation);
        rejected(&raw, ResultCategory::SafeReject, "window");
    }
}

#[test]
fn pod_mutable_helper_reborrow_ends_at_the_call_last_use() {
    let mut raw = pod_mutable_window();
    let address = raw.functions[0].values[2].clone();
    let mut helper = function(1);
    helper.parameters.push(LocalId(0));
    helper.locals.push(local(0, address.clone()));
    helper.values = vec![
        address,
        Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        },
        Type::I32,
    ];
    helper.blocks[0].instructions = vec![
        ins(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        ins(
            Some(ValueId(1)),
            InstructionKind::AddressField {
                address: ValueId(0),
                index: 0,
            },
        ),
        ins(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(42)),
        ),
        ins(
            None,
            InstructionKind::WriteAddress {
                address: ValueId(1),
                value: ValueId(2),
            },
        ),
    ];
    let f = &mut raw.functions[0];
    f.blocks[0].instructions.insert(
        5,
        ins(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(2)],
            },
        ),
    );
    raw.functions.push(helper);
    accepted(&raw);
    let f = &mut raw.functions[0];
    f.blocks[0].instructions.swap(5, 6);
    rejected(&raw, ResultCategory::SafeReject, "window");
}

#[test]
fn pod_store_checks_zero_byte_unit_numeric_overflow_and_constructor_metadata() {
    for leaf in [Type::Unit, Type::U64] {
        let mut huge = leaf.clone();
        for _ in 0..4 {
            huge = Type::Array {
                element: Box::new(huge),
                length: 65536,
            };
        }
        let mut element = pod_element();
        let Type::Record { fields, .. } = &mut element else {
            unreachable!()
        };
        fields.push(("padding".into(), huge));
        let mut raw = pod_fixture(element);
        raw.functions.clear();
        if leaf == Type::Unit {
            accepted(&raw);
        } else {
            rejected(
                &raw,
                ResultCategory::ResourceLimit,
                "layout exceeds the target address range",
            );
        }
    }

    let mut raw = pod_fixture(pod_element());
    let f = &mut raw.functions[0];
    f.values.push(f.locals[0].ty.clone());
    // Only the instruction metadata claims this invalid Point; all value and
    // resource table occurrences remain the original valid Point definition.
    f.blocks[0].instructions.insert(
        1,
        ins(
            Some(ValueId(0)),
            InstructionKind::OwnedStoreNew {
                element: Type::Record {
                    name: "Point".into(),
                    fields: vec![("n".into(), Type::I64)],
                },
            },
        ),
    );
    rejected(
        &raw,
        ResultCategory::SafeReject,
        "exact canonical definition",
    );
}

#[test]
fn pod_store_distinct_owners_do_not_merge_window_roots() {
    let mut raw = pod_window();
    let f = &mut raw.functions[0];
    f.parameters.push(LocalId(1));
    f.locals.push(local(1, f.locals[0].ty.clone()));
    f.values.push(f.values[0].clone());
    f.blocks[0].instructions.insert(
        4,
        ins(
            Some(ValueId(5)),
            InstructionKind::DescribeLocal { local: LocalId(1) },
        ),
    );
    f.blocks[0].instructions[6].kind = InstructionKind::OwnedStoreReserve {
        store: ValueId(5),
        capacity: ValueId(1),
    };
    let mutation = f.blocks[0].instructions.remove(6);
    f.blocks[0].instructions.insert(5, mutation);
    f.blocks[0].instructions.splice(
        7..7,
        [
            ins(None, InstructionKind::DropManaged { local: LocalId(1) }),
            ins(None, InstructionKind::StorageDead(LocalId(1))),
        ],
    );
    accepted(&raw);
}

#[test]
fn pod_store_helper_distinct_roots_and_conservative_same_call_limit() {
    let mut raw = pod_window();
    let address = raw.functions[0].values[2].clone();
    let desc = raw.functions[0].values[0].clone();
    let mut helper = function(1);
    helper.parameters = vec![LocalId(0), LocalId(1)];
    helper.locals = vec![local(0, address.clone()), local(1, desc.clone())];
    helper.values = vec![address, desc, Type::I64, Type::Bool, pod_element()];
    helper.blocks[0].instructions = vec![
        ins(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        ins(Some(ValueId(1)), InstructionKind::Load(LocalId(1))),
        ins(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I64(0)),
        ),
        ins(
            Some(ValueId(4)),
            InstructionKind::ReadAddress {
                address: ValueId(0),
            },
        ),
        ins(
            Some(ValueId(3)),
            InstructionKind::OwnedStoreReserve {
                store: ValueId(1),
                capacity: ValueId(2),
            },
        ),
    ];
    let f = &mut raw.functions[0];
    f.blocks[0].instructions.drain(4..6);
    f.blocks[0].instructions.insert(
        4,
        ins(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(2), ValueId(0)],
            },
        ),
    );
    raw.functions.push(helper);
    // Keep the original nearest conservative rejection: this callee reads the
    // view before reserve, but the existing summary covers its complete call.
    rejected(
        &raw,
        ResultCategory::SafeReject,
        "actual borrowed resource access conflicts",
    );
    let f = &mut raw.functions[0];
    f.parameters.push(LocalId(1));
    f.locals.push(local(1, f.locals[0].ty.clone()));
    f.values.push(f.values[0].clone());
    f.blocks[0].instructions.insert(
        4,
        ins(
            Some(ValueId(5)),
            InstructionKind::DescribeLocal { local: LocalId(1) },
        ),
    );
    f.blocks[0].instructions[5].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(2), ValueId(5)],
    };
    f.blocks[0].instructions.splice(
        6..6,
        [
            ins(None, InstructionKind::DropManaged { local: LocalId(1) }),
            ins(None, InstructionKind::StorageDead(LocalId(1))),
        ],
    );
    accepted(&raw);
    raw.functions[1].blocks[0].instructions.swap(3, 4);
    accepted(&raw);
    raw.functions[0].blocks[0].instructions[5].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(2), ValueId(0)],
    };
    assert!(
        analyze(&raw, &Limits::default()).is_err(),
        "same Store helper must reject a live read after mutation"
    );
}

#[test]
fn pod_cfg_order_matches_legacy_transfer_on_diamonds_and_backedges() {
    for looping in [false, true] {
        for invalid in [false, true] {
            let mut legacy = fixture(element(0), 0);
            let f = &mut legacy.functions[0];
            let desc = match &f.locals[0].ty {
                Type::Managed(payload) => Type::Desc(payload.clone()),
                _ => unreachable!(),
            };
            let cleanup = f.blocks[0].instructions[1..].to_vec();
            f.values = vec![Type::Bool, desc];
            f.blocks = vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![
                        ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
                        ins(
                            Some(ValueId(0)),
                            InstructionKind::Constant(Constant::Bool(true)),
                        ),
                    ],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: if invalid {
                        vec![ins(
                            None,
                            InstructionKind::DropManaged { local: LocalId(0) },
                        )]
                    } else {
                        vec![]
                    },
                    terminator: if looping {
                        Terminator::Branch {
                            condition: ValueId(0),
                            then_block: BlockId(1),
                            else_block: BlockId(3),
                        }
                    } else {
                        Terminator::Jump(BlockId(3))
                    },
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![],
                    terminator: Terminator::Jump(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(3),
                    instructions: std::iter::once(ins(
                        Some(ValueId(1)),
                        InstructionKind::DescribeLocal { local: LocalId(0) },
                    ))
                    .chain(cleanup)
                    .collect(),
                    terminator: Terminator::Return(None),
                },
            ];
            let mut ordered = legacy.clone();
            // An unused, fully checked POD definition selects the new private
            // scheduling lane while all executable IR stays byte-identical.
            ordered
                .resources
                .push(pod_fixture(pod_element()).resources.remove(0));
            let a = analyze(&legacy, &Limits::default());
            let b = analyze(&ordered, &Limits::default());
            if invalid {
                let a = a.expect_err("legacy rejects moved/cleaned incoming responsibility");
                let b = b.expect_err("ordered rejects the same incoming responsibility");
                assert_eq!(a[0].category, b[0].category, "loop={looping}");
            } else {
                assert_eq!(a.unwrap().functions, b.unwrap().functions, "loop={looping}");
            }
        }
    }
}
