use super::*;
#[path = "../../../../../tests/language-readiness/terminal/340/reservation_protocol.rs"]
mod fixture;
fn with_caller(fresh:bool)->TypedModule {
    let mut m=fixture::module();
    let address=m.functions[0].locals[0].ty.clone();
    let resource=m.functions[0].locals[1].ty.clone();
    let Type::Address{inner:package,..}=&address else{unreachable!()};
    let package=package.as_ref().clone();
    let mut relay=m.functions[0].clone();
    relay.id=FunctionId(1);relay.name="relay".into();
    relay.blocks[0].instructions=vec![
        fixture::instruction(Some(ValueId(0)),InstructionKind::Load(LocalId(0))),
        fixture::instruction(Some(ValueId(1)),InstructionKind::TakeLocal{local:LocalId(1)}),
        fixture::instruction(None,InstructionKind::Call{function:FunctionId(0),arguments:vec![ValueId(0),ValueId(1)]}),
    ];
    m.functions.push(relay);
    let mut caller=m.functions[0].clone();
    caller.id=FunctionId(2);caller.name="create_result".into();
    caller.return_type=resource.clone();caller.result_mode=PassingMode::Create;
    caller.parameters=if fresh{vec![]}else{vec![LocalId(0)]};
    caller.locals[0].ty=resource.clone();
    caller.locals[1].ty=package.clone();
    caller.values=vec![Type::I32,resource.clone(),package,address,resource.clone(),resource];
    caller.blocks[0].instructions=vec![
        fixture::instruction(Some(ValueId(0)),InstructionKind::Constant(Constant::I32(65))),
        fixture::instruction(Some(ValueId(1)),InstructionKind::MakeManaged{fields:vec![ValueId(0)]}),
        fixture::instruction(Some(ValueId(2)),InstructionKind::MakePackage{fields:vec![ValueId(1),ValueId(0)]}),
        fixture::instruction(None,InstructionKind::Store{local:LocalId(1),value:ValueId(2)}),
        fixture::instruction(Some(ValueId(3)),InstructionKind::BorrowLocalMut{local:LocalId(1)}),
        fixture::instruction(Some(ValueId(4)),if fresh{InstructionKind::MakeManaged{fields:vec![ValueId(0)]}}else{InstructionKind::TakeLocal{local:LocalId(0)}}),
        fixture::instruction(None,InstructionKind::Call{function:FunctionId(1),arguments:vec![ValueId(3),ValueId(4)]}),
        fixture::instruction(Some(ValueId(5)),InstructionKind::TakePackageField{local:LocalId(1),index:0}),
        fixture::instruction(None,InstructionKind::DropPackage{local:LocalId(1)}),
    ];
    caller.blocks[0].terminator=Terminator::Return(Some(ValueId(5)));
    m.functions.push(caller);m
}
#[test]
fn replacement_transfer_updates_actual_field_birth_through_two_calls() {
    for fresh in [false,true] {
        let module=with_caller(fresh);
        let graphs=vec![(vec![vec![]],vec![true]);3];
        let limits=Limits::default();let mut b=Budget::new(&limits);
        b.head_descriptions=true;
        let program=Program::new(&module,&graphs,&mut b).unwrap();
        let effects=infer(&module,&graphs,&program,&mut b).unwrap();
        let target=Target{root:origins::Root::Parameter(0),path:vec![PlaceProjection::Field(0)],mutable:true};
        assert_eq!(effects[0].replacements[&target],BTreeSet::from([Birth::Argument(1)]));
        assert_eq!(effects[1].replacements[&target],BTreeSet::from([Birth::Argument(1)]));
        let result=analyze_function_program(&module,&module.functions[2],&[true],&effects,true,&mut b,None,Some(&program));
        if fresh {result.unwrap();} else {assert!(result.unwrap_err().iter().any(|d|d.message.contains("create result forwards an incoming responsibility")));}
    }
}
