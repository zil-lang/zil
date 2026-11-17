//! The Product ledger must retain an opaque callback debt. These component
//! controls do not replace actual callback body/source/phase verification.
use super::*;

fn budget() -> Budget {
    let mut b = Budget::new(&Limits::default());
    b.sums = true;
    b
}
fn module() -> TypedModule {
    let (mut m, _) = crate::tests::baseline();
    let header = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![],
        classes: vec![],
    };
    m.functions[0].environment = Some(ClosureId(0));
    m.functions[0].storage_life = Some(header.clone());
    m.closures.push(ClosureDefinition {
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![Type::I32],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::FnMut,
        storage_life: Some(header),
        lending_captures: vec![],
    });
    m
}
fn failure() -> Type {
    Type::Package {
        name: "failure_product".into(),
        fields: vec![
            ("reason".into(), Type::I32),
            (
                "inputs".into(),
                Type::Tuple(vec![Type::Closure(ClosureId(0))]),
            ),
        ],
    }
}
fn roundtrip(m: &TypedModule) -> TypedModule {
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(m, &CandidatePlan::empty_for(m), &limits).unwrap();
    zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0
}

#[test]
fn opaque_callable_product_requires_the_actual_complete_entry_raw_wire() {
    let m = module();
    for m in [&m, &roundtrip(&m)] {
        assert!(needs_shape(&failure(), &mut budget()).unwrap());
        assert!(owned(&Type::Tuple(vec![Type::Closure(ClosureId(0))])));
        check_type_for(m, &failure(), None, &mut budget()).unwrap();
        packages::check_capture_type(m, &failure(), None, &mut budget()).unwrap();
        for attack in 0..5 {
            let mut wrong = m.clone();
            match attack {
                0 => wrong.closures[0].storage_life = None,
                1 => wrong.functions[0].storage_life = None,
                2 => wrong.functions[0].environment = None,
                3 => wrong.closures[0].result = Type::Bool,
                _ => wrong.closures[0].id = ClosureId(4),
            }
            for wrong in [&wrong, &roundtrip(&wrong)] {
                assert!(
                    check_type_for(wrong, &failure(), None, &mut budget()).is_err(),
                    "attack {attack}"
                );
            }
        }
    }
}

#[test]
fn error_input_callback_debt_survives_selection_and_exact_partial_take() {
    let ty = Type::ErrorUnion {
        error: Box::new(failure()),
        result: Box::new(Type::I32),
    };
    let mut b = budget();
    let mut value = content(&ty, Some(2), &mut vec![], false, &mut b).unwrap();
    assert!(!value.no_cleanup_debt(&mut b).unwrap());
    let route = [
        PlaceProjection::ErrorArm(true),
        PlaceProjection::Field(1),
        PlaceProjection::Field(0),
    ];
    let callback = value.take_place(&route, true, &mut b).unwrap();
    let Content::Leaf(sources) = &callback else {
        panic!("callback ownership erased")
    };
    assert_eq!(
        sources,
        &BTreeSet::from([Source::Input(2, vec![1, 0, 0, 0, 1, 0, 0])])
    );
    assert!(!callback.no_cleanup_debt(&mut b).unwrap());
    assert!(value.no_cleanup_debt(&mut b).unwrap());
    assert!(value.take_place(&route, true, &mut b).is_err());
    assert!(
        value
            .take_place(&[PlaceProjection::ErrorArm(true)], true, &mut b)
            .is_err()
    );
    assert!(
        !Content::Copy
            .tags_unknown(&Type::Closure(ClosureId(0)), &mut b)
            .unwrap()
    );
}

#[test]
fn callable_product_transfer_is_affine_even_for_zero_cleanup_capture_layout() {
    let mut m = module();
    m.functions[0].locals[0].ty = failure();
    let instruction = |kind| Instruction {
        result: Some(ValueId(0)),
        kind,
        span: m.functions[0].span,
    };
    let take = instruction(InstructionKind::TakePackageField {
        local: LocalId(0),
        index: 1,
    });
    assert_eq!(
        packages::check_instruction(&m, &m.functions[0], &take, &mut budget()).unwrap(),
        Some(Type::Tuple(vec![Type::Closure(ClosureId(0))]))
    );
    let read = instruction(InstructionKind::ReadPackageField {
        local: LocalId(0),
        index: 1,
    });
    assert!(packages::check_instruction(&m, &m.functions[0], &read, &mut budget()).is_err());
}

#[test]
fn zero_class_static_tuple_callback_keeps_its_real_static_directory_raw_wire() {
    let mut m = module();
    let address = Type::Address {
        mutable: false,
        inner: Box::new(Type::Slice {
            element: Box::new(Type::U8),
        }),
    };
    m.closures[0].captures = vec![Type::Tuple(vec![address.clone(), Type::I32])];
    m.closures[0].result = address.clone();
    m.functions[0].return_type = address;
    let positions = vec![
        StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        },
        StoragePosition {
            base: StorageBase::Result,
            path: vec![],
        },
    ];
    m.closures[0].storage_life.as_mut().unwrap().static_bindings = positions.clone();
    m.functions[0]
        .storage_life
        .as_mut()
        .unwrap()
        .static_bindings = positions;
    for m in [&m, &roundtrip(&m)] {
        let (count, pending) =
            dynamic_witness::intrinsic_domain(m, &Type::Closure(ClosureId(0)), &mut budget())
                .unwrap();
        assert_eq!(count, 0);
        assert!(pending, "metadata never grounds a static value");
        check_type_for(m, &failure(), None, &mut budget()).unwrap();
        for index in 0..2 {
            let mut wrong = m.clone();
            wrong.closures[0]
                .storage_life
                .as_mut()
                .unwrap()
                .static_bindings
                .remove(index);
            wrong.functions[0].storage_life = wrong.closures[0].storage_life.clone();
            assert!(check_type_for(&wrong, &failure(), None, &mut budget()).is_err());
        }
    }
}

#[test]
fn callable_product_does_not_erase_an_intrinsic_capture_class_raw_wire() {
    let mut m = module();
    m.closures[0].captures = vec![Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    }];
    let c = m.closures[0].storage_life.as_mut().unwrap();
    c.fixed_binder_count = 1;
    c.bindings = vec![StorageLifeBinding {
        position: StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![],
        },
        binder: StorageBinder::Fixed(0),
    }];
    m.functions[0].storage_life = m.closures[0].storage_life.clone();
    for m in [&m, &roundtrip(&m)] {
        assert_eq!(
            dynamic_witness::intrinsic_domain(m, &Type::Closure(ClosureId(0)), &mut budget())
                .unwrap()
                .0,
            1
        );
        assert!(check_type_for(m, &failure(), None, &mut budget()).is_err());
    }
}

#[test]
fn opaque_callback_cleanup_uses_actual_captures_and_never_task_result() {
    let mut m = module();
    let registration = Type::Managed(Box::new(Type::AsyncRegistration {
        environment: Box::new(Type::Closure(ClosureId(0))),
    }));
    m.closures[0].captures = vec![Type::Tuple(vec![registration.clone()])];
    let mut b = budget();
    let mut value = content(&failure(), None, &mut vec![], false, &mut b).unwrap();
    assert!(value.live_async(&m, &failure(), &mut b).unwrap());
    assert!(value.live_task(&failure(), &mut b).unwrap());
    let taken = value
        .take_place(&[PlaceProjection::Field(1)], true, &mut b)
        .unwrap();
    assert!(!value.live_async(&m, &failure(), &mut b).unwrap());
    assert!(
        taken
            .live_async(&m, &Type::Tuple(vec![Type::Closure(ClosureId(0))]), &mut b)
            .unwrap()
    );
    m.closures[0].captures = vec![Type::Managed(Box::new(Type::Task {
        result: Box::new(registration),
        retained_life: RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }))];
    assert!(
        !closure_live_async(&m, ClosureId(0), &mut b).unwrap(),
        "R is not the hidden Task frame"
    );
}

#[test]
fn callback_array_is_not_empty_cleanup_until_its_actual_element_is_moved() {
    let mut b=budget();let ty=Type::Array {element:Box::new(Type::Closure(ClosureId(0))),length:1};
    let mut cell=content(&ty,Some(3),&mut vec![],false,&mut b).unwrap();
    assert!(!cell.no_cleanup_debt(&mut b).unwrap());assert!(cell.live_task(&ty,&mut b).unwrap());
    let callback=cell.take_place(&[PlaceProjection::Field(0)],true,&mut b).unwrap();
    assert_eq!(callback,Content::Leaf(BTreeSet::from([Source::Input(3,vec![0,0])])));
    assert!(cell.no_cleanup_debt(&mut b).unwrap());assert!(!cell.live_task(&ty,&mut b).unwrap());
    assert!(!callback.no_cleanup_debt(&mut b).unwrap());assert_eq!(callback.fresh(),2);
    assert!(cell.take_place(&[PlaceProjection::Field(0)],true,&mut b).is_err());
}

#[test]
fn allocation_failure_reason_and_affine_inputs_keep_the_complete_content_shape_raw_wire(){
    let mut m=module();
    let reason=Type::Enum{name:"@std.task:v1:TaskCreateError".into(),variants:vec![("Allocation".into(),vec![])]};
    let inputs=Type::Tuple(vec![Type::Closure(ClosureId(0))]);
    let error=Type::Package{name:format!("@std.task:v1:TaskCreateFailure:{}",inputs.fingerprint()),fields:vec![("reason".into(),reason.clone()),("inputs".into(),inputs)]};
    let ty=Type::ErrorUnion{error:Box::new(error.clone()),result:Box::new(Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:0,ports:vec![],retained:vec![],nominal_arguments:vec![]}})))};
    m.functions[0].values=vec![ty];
    for m in [&m,&roundtrip(&m)]{
        let ty=&m.functions[0].values[0];let mut b=budget();
        let source=Source::Input(7,vec![0,1]);
        let actual=allocation_content(ty,Some(Content::Product(vec![Content::Leaf(BTreeSet::from([source.clone()]))])),&mut b).unwrap();
        let mut fixed=content(ty,None,&mut vec![],true,&mut b).unwrap();fixed.merge(&actual,&mut b).unwrap();
        let Content::Sum(arms,_)=&fixed else{unreachable!()};
        let Content::Product(fields)=&arms[&0][0]else{unreachable!()};
        assert!(matches!(&fields[0],Content::Sum(tags,_)if tags.contains_key(&0)),"reason keeps its actual Enum tag domain");
        let callback=fixed.take_place(&[PlaceProjection::ErrorArm(true),PlaceProjection::Field(1),PlaceProjection::Field(0)],true,&mut b).unwrap();
        assert_eq!(callback,Content::Leaf(BTreeSet::from([source])));assert!(!callback.no_cleanup_debt(&mut b).unwrap());
        assert!(fixed.take_place(&[PlaceProjection::ErrorArm(true),PlaceProjection::Field(1),PlaceProjection::Field(0)],true,&mut b).is_err());
        let Type::ErrorUnion{result:owner,..}=ty else{unreachable!()};
        let copy_inputs=Type::Tuple(vec![Type::I32]);
        let record=Type::Record{name:format!("@std.task:v1:TaskCreateFailure:{}",copy_inputs.fingerprint()),fields:vec![("reason".into(),reason.clone()),("inputs".into(),copy_inputs)]};
        let copy_outcome=Type::ErrorUnion{error:Box::new(record),result:owner.clone()};
        let actual=allocation_content(&copy_outcome,Some(Content::Product(vec![Content::Copy])),&mut b).unwrap();
        let Content::Sum(arms,_)=actual else{unreachable!()};assert_eq!(arms[&0],vec![Content::Copy]);
        assert!(allocation_content(&copy_outcome,Some(Content::Product(vec![Content::Leaf(BTreeSet::from([Source::Fresh]))])),&mut b).is_err(),"Record cannot erase live affine input debt");
        let plain=Type::ErrorUnion{error:Box::new(reason.clone()),result:owner.clone()};
        let mut default=content(&plain,None,&mut vec![],false,&mut b).unwrap();
        default.merge(&allocation_content(&plain,None,&mut b).unwrap(),&mut b).unwrap();
        assert!(Content::Product(vec![Content::Copy]).merge(&Content::Product(vec![]),&mut b).is_err());
    }
}
