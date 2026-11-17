//! Current Content, tag and initialization determine cleanup obligations.
//! These private transfer controls do not supply a synchronous worker proof.
use super::*;

fn model(error_tag:i32)->TypedModule{
    let(mut module,_)=crate::tests::baseline();let f=&mut module.functions[0];let span=f.span;
    let task=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:0,ports:vec![],retained:vec![],nominal_arguments:vec![]}}));
    let outcome=Type::ErrorUnion{error:Box::new(Type::Enum{name:"Failure".into(),variants:vec![("Bad".into(),vec![])]}),result:Box::new(task)};
    f.parameters=vec![LocalId(0)];f.locals=vec![LocalDecl{id:LocalId(0),name:"from_helper_or_input".into(),ty:outcome,mutable:true,span}];f.values=vec![Type::I32,Type::I32,Type::Bool];f.return_type=Type::Unit;f.storage_life=None;
    let op=|result:Option<u32>,kind|Instruction{result:result.map(ValueId),kind,span};
    f.blocks=vec![BasicBlock{id:BlockId(0),instructions:vec![op(Some(0),InstructionKind::PlaceTagLocal{local:LocalId(0),path:vec![]}),op(Some(1),InstructionKind::Constant(Constant::I32(error_tag))),op(Some(2),InstructionKind::Binary{op:BinaryOp::Equal,left:ValueId(0),right:ValueId(1)})],terminator:Terminator::Branch{condition:ValueId(2),then_block:BlockId(1),else_block:BlockId(2)}},BasicBlock{id:BlockId(1),instructions:vec![op(None,InstructionKind::DropErrorUnion{local:LocalId(0)})],terminator:Terminator::Return(None)},BasicBlock{id:BlockId(2),instructions:vec![op(None,InstructionKind::DropErrorUnion{local:LocalId(0)})],terminator:Terminator::Return(None)}];f.entry=BlockId(0);module
}
fn solve(module:&TypedModule)->Check<Vec<Vec<Bridge>>>{
    let f=&module.functions[0];let mut budget=Budget::new(&Limits::default());budget.sums=true;
    let graph=FunctionGraph{successors:vec![vec![1,2],vec![],vec![]],predecessors:vec![vec![],vec![0],vec![0]],reachable:vec![true;3],package_views:Default::default(),role_data_uses:vec![]};
    run(module,f,&graph,&[Content::Copy],true,&mut budget).map(|(_,rows,_)|rows)
}
fn wire(module:&TypedModule)->TypedModule{
    let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(module,&CandidatePlan::empty_for(module),&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0
}
#[test]
fn selected_error_cleanup_keeps_actual_task_debt_without_a_local_constructor(){
    for tag in [0,1]{let module=model(tag);let decoded=wire(&module);
        for module in [&module,&decoded]{
            assert!(module.functions[0].blocks.iter().flat_map(|block|&block.instructions).all(|row|!matches!(row.kind,InstructionKind::TaskCreate{..}|InstructionKind::TaskCreateOwned{..})),"the cleanup fact cannot depend on a constructor opcode in its current function");
            let rows=solve(module).unwrap();assert_eq!(rows[1][0].task_cleanup,Some(tag==0));assert_eq!(rows[2][0].task_cleanup,Some(tag==1),"only the real selected Copy error has no Task; the success arm still needs its original full cleanup proof");
        }
    }
}
#[test]
fn missing_or_stale_error_observation_never_erases_a_possible_task_owner(){
    let original=model(1);
    for stale in [false,true]{let mut module=original.clone();let f=&mut module.functions[0];let span=f.span;
        if stale{
            let mut incoming=f.locals[0].clone();incoming.id=LocalId(1);f.locals.push(incoming);f.parameters.push(LocalId(1));f.values.push(f.locals[0].ty.clone());
            f.blocks[0].instructions.extend([Instruction{result:None,kind:InstructionKind::DropErrorUnion{local:LocalId(0)},span},Instruction{result:Some(ValueId(3)),kind:InstructionKind::TakeLocal{local:LocalId(1)},span},Instruction{result:None,kind:InstructionKind::Store{local:LocalId(0),value:ValueId(3)},span}]);
        }else{f.blocks[0].instructions[2].kind=InstructionKind::Constant(Constant::Bool(true));}
        let decoded=wire(&module);for module in [&module,&decoded]{let rows=solve(module).unwrap();assert_eq!(rows[1][0].task_cleanup,Some(true),"old/no tag cannot constrain a new occupant; stale={stale}");assert_eq!(rows[2][0].task_cleanup,Some(true));}
    }
}
