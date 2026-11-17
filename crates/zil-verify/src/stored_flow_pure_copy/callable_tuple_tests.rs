//! Actual transport components; these fixtures do not supply callback body,
//! static literal, resource phase or Source obligations for a whole module.
use super::*;

fn fixture() -> TypedModule {
    let (mut m, _) = crate::tests::baseline();
    let pointer = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let task = Type::Managed(Box::new(Type::Task {
        result: Box::new(Type::I32),
        retained_life: RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }));
    let header = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        classes: vec![],
        static_bindings: vec![StoragePosition {
            base: StorageBase::Capture(1),
            path: vec![],
        }],
    };
    m.functions[0].environment = Some(ClosureId(0));
    m.functions[0].storage_life = Some(header.clone());
    m.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(0),
        kind: CallableKind::FnMut,
        parameters: vec![],
        result: Type::I32,
        captures: vec![task, pointer],
        storage_life: Some(header),
        lending_captures: vec![],
    });
    let tuple = Type::Tuple(vec![Type::Closure(ClosureId(0))]);
    m.functions[0].locals[0].ty = tuple.clone();
    let mut local = m.functions[0].locals[0].clone();
    local.id = LocalId(1);
    local.ty = Type::Closure(ClosureId(0));
    m.functions[0].locals.push(local);
    m.functions[0].values = vec![Type::Closure(ClosureId(0)), tuple];
    m
}
fn budget(m: &TypedModule) -> Budget {
    let mut b = Budget::new(&Limits::default());
    b.sums = true;
    b.function_count = m.functions.len();
    b
}
fn permission() -> Permission {
    Permission {
        sources: vec![Source {
            base: HoldRoot::Static,
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
            retained_port: None,
            static_dependency: StaticProvenance::Literal,
        }],
        loans: vec![7, 11],
    }
}
fn instruction(f: &TypedFunction, result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: f.span,
    }
}

#[test]
fn opaque_callable_tuple_moves_actual_static_fields_and_all_parents_raw_wire() {
    let m = fixture();
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&m, &CandidatePlan::empty_for(&m), &limits).unwrap();
    let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    for m in [&m, &wire] {
        let f = &m.functions[0];
        let mut b = budget(m);
        assert!(
            !complete_owned_value(m, &f.locals[0].ty, &mut b).unwrap(),
            "callback with a Task/pointer is not target-free"
        );
        let make = instruction(
            f,
            Some(1),
            InstructionKind::MakeOwnedTuple {
                fields: vec![ValueId(0)],
            },
        );
        let unpack = instruction(
            f,
            None,
            InstructionKind::UnpackTuple {
                local: LocalId(0),
                fields: vec![LocalId(1)],
            },
        );
        let take = instruction(
            f,
            Some(0),
            InstructionKind::TakeTupleField {
                local: LocalId(0),
                path: vec![PlaceProjection::Field(0)],
            },
        );
        let drop = instruction(f, None, InstructionKind::DropTuple { local: LocalId(0) });
        for i in [&make, &unpack, &take, &drop] {
            assert!(ordinary_owned_tuple(m, f, i, &mut b).unwrap());
        }
        let p = permission();
        let mut input = Payload::default();
        add_leaf(&mut input, &[PlaceProjection::Field(1)], &p, &mut b).unwrap();
        let mut tuple = Payload::default();
        let mut state = Frame {
            locals: vec![Payload::default(); 2],
            ..Frame::default()
        };
        ordinary_tuple_transfer(f, &make, &mut state, &[input], &mut tuple, &mut b).unwrap();
        assert_eq!(
            leaf(
                &tuple,
                &[PlaceProjection::Field(0), PlaceProjection::Field(1)],
                &mut b
            )
            .unwrap(),
            Some(&p)
        );
        state.locals[0] = tuple;
        ordinary_tuple_transfer(f, &unpack, &mut state, &[], &mut Payload::default(), &mut b)
            .unwrap();
        assert_eq!(
            leaf(&state.locals[1], &[PlaceProjection::Field(1)], &mut b).unwrap(),
            Some(&p)
        );
        assert!(state.locals[0].leaves.is_empty());
        let mut wrong = m.clone();
        wrong.closures[0].storage_life = None;
        assert!(ordinary_owned_tuple(&wrong, f, &unpack, &mut budget(&wrong)).is_err());
        let mut unknown = Payload::default();
        unknown
            .task_ports
            .push((vec![ContractProjection::RetainedPort(0)], p.clone()));
        assert!(
            physical_tuple_payload(&unknown, &mut b).is_err(),
            "logical facts are never silently discarded"
        );
    }
}

#[test]
fn plain_failed_creation_returns_the_actual_callback_capture_permissions() {
    let mut m = fixture();
    let tuple = m.functions[0].values[1].clone();
    let error = Type::Package {
        name: format!("@std.task:v1:TaskCreateFailure:{}", tuple.fingerprint()),
        fields: vec![
            (
                "reason".into(),
                Type::Enum {
                    name: "@std.task:v1:TaskCreateError".into(),
                    variants: vec![("Allocation".into(), vec![])],
                },
            ),
            ("inputs".into(), tuple),
        ],
    };
    m.functions[0].values.push(Type::ErrorUnion {
        error: Box::new(error),
        result: Box::new(Type::Unit),
    });
    let f = &m.functions[0];
    let i = instruction(
        f,
        Some(2),
        InstructionKind::TaskCreateOwned {
            worker: FunctionId(0),
            arguments: vec![ValueId(0)],
            port_bindings: vec![],
        },
    );
    let mut b = budget(&m);
    let p = permission();
    let mut callback = Payload::default();
    add_leaf(&mut callback, &[PlaceProjection::Field(1)], &p, &mut b).unwrap();
    let mut result = Payload::default();
    task_failure::create(f, &i, &[ValueId(0)], &[callback], &mut result, &mut b).unwrap();
    assert_eq!(
        leaf(
            &result,
            &[
                PlaceProjection::ErrorArm(true),
                PlaceProjection::Field(1),
                PlaceProjection::Field(0),
                PlaceProjection::Field(1)
            ],
            &mut b
        )
        .unwrap(),
        Some(&p)
    );
    assert!(result.address.sources.is_empty());
    // Follow the actual allocation-error route all the way back to its
    // callback local, with the original parents and Static category intact.
    let mut failure_function = f.clone();
    failure_function.locals[0].ty = f.values[2].clone();
    let mut state = Frame {
        locals: vec![result, Payload::default()],
        ..Frame::default()
    };
    b.task_failure_carriers = true;
    let take_error = instruction(
        &failure_function,
        Some(0),
        InstructionKind::TakeVariantField {
            local: LocalId(0),
            path: vec![PlaceProjection::ErrorArm(true)],
        },
    );
    let mut failed = task_failure::wrapper(&failure_function, &take_error, &mut state, &[], &mut b)
        .unwrap()
        .unwrap();
    assert!(state.locals[0].leaves.is_empty());
    let inputs = take_package_field_leaves(&mut failed, 1, &mut b).unwrap();
    state.locals[0] = inputs;
    let unpack = instruction(
        f,
        None,
        InstructionKind::UnpackTuple {
            local: LocalId(0),
            fields: vec![LocalId(1)],
        },
    );
    ordinary_tuple_transfer(f, &unpack, &mut state, &[], &mut Payload::default(), &mut b).unwrap();
    assert_eq!(
        leaf(&state.locals[1], &[PlaceProjection::Field(1)], &mut b).unwrap(),
        Some(&p)
    );
    assert!(state.locals[0].leaves.is_empty());
    let mut wrong = m.clone();
    let Type::ErrorUnion { error, .. } = &mut wrong.functions[0].values[2] else {
        unreachable!()
    };
    let Type::Package { fields, .. } = error.as_mut() else {
        unreachable!()
    };
    fields[1].1 = Type::Tuple(vec![]);
    assert!(
        task_failure::create(
            &wrong.functions[0],
            &i,
            &[ValueId(0)],
            &[Payload::default()],
            &mut Payload::default(),
            &mut b
        )
        .is_err()
    );
}
