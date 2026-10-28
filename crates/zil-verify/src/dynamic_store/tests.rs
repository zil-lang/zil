//! Raw layout/qualification boundary controls. These do not claim source or
//! native coverage; the driver and non-author suites exercise that full path.
use super::*;
use zil_base::ModuleId;

fn zero() -> Type {
    Type::Record {
        name: "Zero".into(),
        fields: vec![],
    }
}

fn module(captures: Vec<Type>) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"201 raw Store layout controls"),
        target: TargetProfile::host_llvm(),
        functions: vec![],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            kind: CallableKind::Fn,
            captures,
            parameters: vec![],
            result: Type::I32,
            lending_captures: vec![],
        }],
        resources: vec![
            ResourceDefinition {
                name: "Zero".into(),
                payload: zero(),
                teardown: None,
            },
            ResourceDefinition {
                name: byte_buffers::NAME.into(),
                payload: Type::ByteBuffer,
                teardown: None,
            },
        ],
        unsupported: vec![],
    }
}

fn element(callbacks: &[u32]) -> Type {
    let mut fields = vec![("token".into(), Type::Managed(Box::new(zero())))];
    fields.extend(
        callbacks
            .iter()
            .enumerate()
            .map(|(index, id)| (format!("callback{index}"), Type::Closure(ClosureId(*id)))),
    );
    Type::Package {
        name: "Item".into(),
        fields,
    }
}

fn admit(module: &TypedModule, element: &Type, limits: &Limits) -> Check<()> {
    check_element_with_limits(
        module,
        element,
        None,
        Some(limits),
        &mut Budget::new(limits),
    )
}

#[test]
fn concrete_capture_alignment_and_appended_live_flags_determine_stride() {
    let mut m = module(vec![
        Type::U8,
        Type::I64,
        Type::Managed(Box::new(Type::ByteBuffer)),
    ]);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        captures: vec![Type::Closure(ClosureId(0)), Type::U16],
        ..m.closures[0].clone()
    });
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    let layouts = closure_layouts(&m, ClosureId(1), Some(&limits), &mut budget).unwrap();
    // The first environment ends at byte 40 before its owned-buffer flag;
    // the flag requires tail padding to 48. The containing environment has
    // a separate child flag and rounds 48 + 2 + 1 to 56, not 50 or 48.
    assert_eq!(layouts[0], Some((48, 8)));
    assert_eq!(layouts[1], Some((56, 8)));
    assert_eq!(
        layout(&element(&[1, 1]), Some(&layouts), &mut budget).unwrap(),
        (112, 8)
    );
    admit(&m, &element(&[1, 1]), &limits).unwrap();
}

#[test]
fn actual_zero_stride_rejects_after_the_manages_package_requirement() {
    let limits = Limits::default();
    let empty = module(vec![]);
    let ty = element(&[0]);
    packages::check_type_for(&empty, &ty, None, &mut Budget::new(&limits)).unwrap();
    let error = admit(&empty, &ty, &limits).unwrap_err();
    assert!(error.message.contains("nonzero actual inline layout"));
    // Same nominal shell and zero-sized Managed field, but a real i64
    // environment supplies a valid nonzero stride.
    admit(&module(vec![Type::I64]), &ty, &limits).unwrap();
    let unit = module(vec![Type::Unit]);
    let mut budget = Budget::new(&limits);
    let layouts = closure_layouts(&unit, ClosureId(0), Some(&limits), &mut budget).unwrap();
    assert_eq!(layouts[0], Some((1, 1)));
    admit(&unit, &ty, &limits).unwrap();
}

#[test]
fn one_type_binding_keeps_both_fields_and_rejects_extra_or_heterogeneous_fields() {
    let mut m = module(vec![Type::I64]);
    let limits = Limits::default();
    admit(&m, &element(&[0]), &limits).unwrap();
    admit(&m, &element(&[0, 0]), &limits).unwrap();
    assert!(admit(&m, &element(&[0, 0, 0]), &limits).is_err());
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        ..m.closures[0].clone()
    });
    assert!(admit(&m, &element(&[0, 1]), &limits).is_err());
}

#[test]
fn direct_and_package_hidden_store_capture_do_not_enter_the_layout_graph() {
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element(&[0])),
    }));
    let limits = Limits::default();
    for capture in [
        store.clone(),
        Type::Package {
            name: "Bag".into(),
            fields: vec![("items".into(), store)],
        },
    ] {
        let error = admit(&module(vec![capture]), &element(&[0]), &limits).unwrap_err();
        assert!(error.message.contains("finite owning environment profile"));
    }
    let error = admit(
        &module(vec![Type::Desc(Box::new(Type::ByteBuffer))]),
        &element(&[0]),
        &limits,
    )
    .unwrap_err();
    assert!(error.message.contains("self-contained environment"));
}

#[test]
fn raw_cycles_and_missing_closure_ids_reject_before_recursive_layout() {
    let limits = Limits::default();
    let mut m = module(vec![Type::Closure(ClosureId(1))]);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        captures: vec![Type::Closure(ClosureId(0))],
        ..m.closures[0].clone()
    });
    let error = admit(&m, &element(&[0]), &limits).unwrap_err();
    assert_eq!(error.code, "E-VERIFY-CLOSURE-LAYOUT");
    let error = admit(&m, &element(&[99]), &limits).unwrap_err();
    assert_eq!(error.code, "E-VERIFY-CLOSURE");
}

#[test]
fn early_resource_admission_bounds_capture_shape_and_layout_arithmetic() {
    let mut nested = Type::I64;
    for _ in 0..8 {
        nested = Type::Tuple(vec![nested]);
    }
    let limits = Limits {
        max_call_depth: 3,
        ..Limits::default()
    };
    let error = admit(&module(vec![nested]), &element(&[0]), &limits).unwrap_err();
    assert!(error.message.contains("aggregate type nesting"));

    let huge = Type::Array {
        element: Box::new(Type::Array {
            element: Box::new(Type::I64),
            length: u32::MAX,
        }),
        length: u32::MAX,
    };
    let limits = Limits {
        max_analysis_steps: usize::MAX,
        ..Limits::default()
    };
    let error = admit(&module(vec![huge]), &element(&[0]), &limits).unwrap_err();
    assert!(
        error
            .message
            .contains("layout exceeds target address range")
    );
}
