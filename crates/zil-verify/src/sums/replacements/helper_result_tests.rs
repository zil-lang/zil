//! Private result-shape/cost components; no Source or public Verify claim.
use super::*;

#[test]
fn qualified_replacement_helpers_consume_the_whole_header_and_keep_affine_exit_debts() {
    let mut module=crate::place_replacements::fixture::module();
    let Type::Address{inner,..}=&module.functions[0].locals[0].ty else{unreachable!()};
    let endpoint=inner.as_ref().clone();
    let f=&mut module.functions[0];
    f.locals[1].ty=endpoint.clone();f.values[1]=endpoint.clone();
    let InstructionKind::ReservePlaceAddress{path,..}=&mut f.blocks[0].instructions[1].kind else{unreachable!()};path.clear();
    f.storage_life=Some(StorageLifeContract{fixed_binder_count:0,invocation_binder_count:1,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Parameter(0),path:vec![]},binder:StorageBinder::Invocation(0)}],static_bindings:vec![],classes:vec![]});
    let limits=Limits::default();
    assert!(package_helper(&module,&module.functions[0],&endpoint,Some(0),&mut Budget::new(&limits)).unwrap());
    for attack in 0..5{
        let mut wrong=module.clone();let f=&mut wrong.functions[0];let c=f.storage_life.as_mut().unwrap();
        match attack{0=>c.bindings[0].binder=StorageBinder::Invocation(1),1=>c.bindings.clear(),2=>c.fixed_binder_count=1,3=>c.bindings[0].position.base=StorageBase::Result,_=>c.static_bindings.push(StoragePosition{base:StorageBase::Result,path:vec![]})}
        assert!(package_helper(&wrong,&wrong.functions[0],&endpoint,Some(0),&mut Budget::new(&limits)).is_err(),"header attack {attack}");
    }
    let mut lending=module.clone();let f=&mut lending.functions[0];
    f.return_type=f.locals[0].ty.clone();
    f.storage_life.as_mut().unwrap().bindings.push(StorageLifeBinding{position:StoragePosition{base:StorageBase::Result,path:vec![]},binder:StorageBinder::Invocation(0)});
    assert!(!package_helper(&lending,&lending.functions[0],&endpoint,Some(0),&mut Budget::new(&limits)).unwrap(),"a valid borrowing header grants no owning-result admission");
    for abort in [false,true]{
        let mut request=module.clone();
        if abort{
            request.functions[0].blocks[0].instructions[2].kind=InstructionKind::AbortPlaceReservation{reservation:zil_ir::ReservationId(71)};
            request.functions[0].blocks[0].instructions[2].result=None;
            request.functions[0].blocks[0].instructions[3].kind=InstructionKind::DropPackage{local:LocalId(1)};
        }
        let bytes=zil_ir::wire::encode_module_candidate(&request,&CandidatePlan::empty_for(&request),&limits).unwrap();
        let(decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        for request in [&request,&decoded]{
            let graph=FunctionGraph{successors:vec![vec![]],predecessors:vec![vec![]],reachable:vec![true],package_views:Default::default(),role_data_uses:vec![]};
            let run=|module:&TypedModule|{
                let mut b=Budget::new(&limits);b.replacements=true;b.sums=true;b.head_descriptions=true;b.function_count=1;b.sum_functions=vec![true];
                crate::resources::check_module(module,std::slice::from_ref(&graph),&mut b)
            };
            run(request).unwrap();
            let mut unsettled=request.clone();unsettled.functions[0].blocks[0].instructions.remove(if abort{2}else{3});
            assert!(run(&unsettled).is_err(),"unsettled exit (abort={abort})");
            let mut twice=request.clone();let operation=twice.functions[0].blocks[0].instructions[if abort{2}else{3}].clone();twice.functions[0].blocks[0].instructions.push(operation);
            assert!(run(&twice).is_err(),"duplicate activation consumption (abort={abort})");
        }
    }
}

#[test]
fn finite_helper_result_recurses_without_borrowed_owned_or_ffi_escape() {
    let defaults = Limits::default();
    let nested = |payload| Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "Fault".into(),
            variants: vec![("Bad".into(), vec![])],
        }),
        result: Box::new(Type::Record {
            name: "Reply".into(),
            fields: vec![("data".into(), Type::Tuple(vec![Type::Unit, payload]))],
        }),
    };
    assert!(
        helper_copy_result(
            &nested(Type::Array {
                element: Box::new(Type::I32),
                length: 2
            }),
            &mut Budget::new(&defaults)
        )
        .unwrap()
    );
    for payload in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::Record {
            name: "R".into(),
            fields: vec![],
        })),
        Type::Managed(Box::new(Type::Record {
            name: "R".into(),
            fields: vec![],
        })),
        Type::ForeignHandle,
        Type::CWideString,
        Type::FunctionPointer {
            parameters: vec![Type::I32],
            result: Box::new(Type::I32),
        },
    ] {
        assert!(!helper_copy_result(&nested(payload), &mut Budget::new(&defaults)).unwrap());
    }
    let empty = Limits {
        max_analysis_steps: 0,
        ..defaults
    };
    assert!(helper_copy_result(&Type::Unit, &mut Budget::new(&empty)).unwrap());
    assert_eq!(
        helper_copy_result(&Type::I32, &mut Budget::new(&empty))
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
}
