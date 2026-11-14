//! Frozen genuine Source→Core→Typed probe outputs. These are incomplete,
//! untrusted producer proposals, not successful Types/Analyze receipts.
use super::*;
const IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-missing-call-sites.wire");
const LOCAL:&[u8]=include_bytes!("fixtures/local-closure-missing-child-instance.wire");
#[test]
fn actual_source_identity_cannot_invent_its_missing_original_call_site() {
    let limits=Limits::default();
    // Historical bytes keep their original schema and remain nonexecutable;
    // current-schema source/identity attacks are checked below.
    assert!(zil_ir::wire::decode_module_candidate(IDENTITY,&limits).is_err());
    // Preserve the historical bytes above. The closest current attack keeps
    // the genuine original WithBody, actual Call and signature fixed.
    let (mut module,_)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    check_program(&module,&mut Budget::new(&limits)).unwrap();
    module.semantic.as_mut().unwrap().sites.clear();
    let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
    assert_eq!(error.code,"E-VERIFY-SEMANTIC");
    assert!(error.message.contains("actual Call has no original semantic specialization site"),"{error:?}");
}
#[test]
fn actual_default_borrowing_closure_requires_its_real_child_instance() {
    let limits=Limits::default();
    assert!(zil_ir::wire::decode_module_candidate(LOCAL,&limits).is_err());
    let (mut module,_)=zil_ir::wire::decode_module_candidate(COMPLETE_LOCAL,&limits).unwrap();
    check_program(&module,&mut Budget::new(&limits)).unwrap();
    let program=module.semantic.as_mut().unwrap();
    program.instances.retain(|instance|instance.environment.is_none());
    let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
    assert_eq!(error.code,"E-VERIFY-SEMANTIC");
    assert!(error.message.contains("actual Closure entry has no complete semantic child instance"),"{error:?}");
}

// This only exercises Verify's independent lexical/SSA sub-proof. The real
// producer still reports its own proof gap, and the empty plan is not Analyze.
const COMPLETE_IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-complete-head.wire");
#[test]
fn source_identity_actual_generic_call_reaches_the_complete_semantic_subproof() {
    let limits=Limits::default();
    let (module,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    check_program(&module,&mut Budget::new(&limits)).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
    let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
    check_program(&decoded,&mut Budget::new(&limits)).unwrap();
}
#[test]
fn source_identity_keeps_the_original_full_actual_type_and_site_on_both_routes() {
    let limits=Limits::default();
    let (original,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    check_program(&original,&mut Budget::new(&limits)).unwrap();
    for attack in 0..4 {
        let mut module=original.clone();
        let program=module.semantic.as_mut().unwrap();
        match attack {
            0=>{
                let identity=program.instances.iter_mut().find(|instance|!instance.actuals.is_empty()).unwrap();
                identity.actuals[0]=SemanticActual::Type(SemanticTypeValue::Resolved{ty:Type::U32,life:empty_life(),classes:vec![]});
            }
            1=>{
                let actuals=program.sites.iter_mut().find_map(|site|match &mut site.action{SemanticSiteAction::BodyCall{actuals}=>Some(actuals),_=>None}).expect("real WithBody original call");
                actuals[0]=SemanticActual::Type(SemanticTypeValue::Resolved{ty:Type::U32,life:empty_life(),classes:vec![]});
            }
            2=>program.sites.clear(),
            3=>{
                let identity=program.instances.iter_mut().find(|instance|!instance.actuals.is_empty()).unwrap();
                identity.actuals[0]=SemanticActual::CoreRecipe{scope:identity.id,recipe:Box::new(SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)))};
            }
            _=>unreachable!(),
        }
        let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","attack {attack}: {error:?}");
        let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
        let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        let error=check_program(&decoded,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","wire attack {attack}: {error:?}");
    }
}

// A genuine WithBody Source/Types output. The embedded empty plan supplies no
// Analyze evidence: these tests exercise the independent semantic kernel only.
const COMPLETE_LOCAL:&[u8]=include_bytes!("fixtures/local-closure-complete-head.wire");
// Genuine current-schema Source probe, byte-identical source to the driver
// phantom fixture. Its plan is untrusted; this exercises only this kernel.
const COMPLETE_PHANTOM:&[u8]=include_bytes!("fixtures/phantom-closure-complete-creator.wire");
#[test]
fn phantom_constructor_class_equations_require_complete_original_creator_worlds(){
    let limits=Limits::default();
    let(original,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_PHANTOM,&limits).unwrap();
    for wire in [false,true]{
        let bytes=zil_ir::wire::encode_module_candidate(&original,&plan,&limits).unwrap();
        let(module,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(original.clone(),plan.clone())};
        let mut budget=Budget::new(&limits);
        check_program(&module,&mut budget).unwrap();
        let function=budget.semantic_constructor_classes.keys().find_map(|key|{
            let function=&module.functions[key.function.0 as usize];
            function.blocks.iter().any(|block|block.instructions.iter().any(|row|matches!(row.kind,InstructionKind::MakeClosure{..}))).then_some(key.function)
        }).expect("genuine phantom creator has a checked logical input image");
        let function=&module.functions[function.0 as usize];
        let reachable=vec![true;function.blocks.len()];
        crate::class_flow::check(&module,function,&reachable,&mut budget).unwrap();
        // The raw physical aggregate alone never grounds its phantom class.
        // Removing the private proof must restore the old strict rejection.
        budget.semantic_constructor_classes.clear();
        let error=crate::class_flow::check(&module,function,&reachable,&mut budget).unwrap_err();
        assert!(error.message.contains("captured Fixed has no independently established source class"),"{error:?}");
    }
}
#[test]
fn qualified_copy_capture_admission_keeps_phantom_domain_and_rejects_physical_targets(){
    let limits=Limits::default();
    let(original,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_PHANTOM,&limits).unwrap();
    for attack in 0..4{
        let mut module=original.clone();
        if attack!=0{
            let capture=&mut module.closures[0].captures[0];
            let Type::BorrowedStorage{storage,..}=capture else{panic!("genuine qualified phantom capture")};
            let Type::Record{fields,..}=storage.as_mut()else{panic!("genuine nominal data")};
            fields[0].1=match attack{1=>Type::Address{mutable:false,inner:Box::new(Type::I32)},2=>Type::Desc(Box::new(Type::I32)),3=>Type::Managed(Box::new(Type::I32)),_=>unreachable!()};
        }
        for wire in [false,true]{
            let decoded;let candidate=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
            let definition=&candidate.closures[0];let mut b=Budget::new(&limits);
            assert_eq!(crate::stored_callables::inline_qualified_capture(candidate,&definition.captures[0],&mut b).unwrap(),attack==0,"attack {attack}, wire {wire}");
            if attack==0{
                assert!(matches!(&definition.captures[0],Type::BorrowedStorage{life,..}if life.binder_count==1));
                crate::stored_callables::captured_profile(candidate,definition,&mut b).unwrap();
                let relation=definition.storage_life.as_ref().unwrap();
                crate::signature_life::closure_relation(candidate,&definition.parameters,&definition.captures,definition.kind,crate::stored_callables::life_result(definition),relation,None).unwrap();
                let factory=candidate.functions.iter().find(|function|function.return_type==Type::Closure(definition.id)&&function.environment.is_none()).expect("genuine borrowed-input phantom factory");
                assert!(factory.storage_life.is_some());
                assert!(factory.callable_return_contract.as_ref().unwrap().life.is_some());
                crate::retained::check_claim(candidate,factory,&limits,&mut b).unwrap();
                assert!(b.returned_environment_sources);
                assert!(crate::stored_partition::mixed(candidate,&mut b).unwrap().is_none(),"all members of the genuine qualified factory/caller/entry component require the same complete stored proof");
            }
        }
    }
}
#[test]
fn nominal_pointee_and_outer_life_keep_distinct_intrinsics_with_equal_images(){
    let limits=Limits::default();let(module,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_PHANTOM,&limits).unwrap();
    let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
    let program=module.semantic.as_ref().unwrap();
    let child=program.instances.iter().find(|row|row.environment.is_some()).unwrap();
    let parent=program.instances.iter().find(|row|Some(row.id)==child.parent).unwrap();
    let scope=ScopeKey::Instance(parent.id);let a=ClassKey::Formal{scope,ordinal:0};
    let env=EvalEnv{lexical:vec![LexicalFrame{definition:parent.definition,key:scope,actuals:vec![CheckedActual::Life(a.clone())],environment:vec![],self_type:None}],assumptions:vec![scope],images:vec![],bounds:vec![],invocations:vec![],at:None};
    let nominal=&module.life_instances[0].ty;let Type::BorrowedStorage{life,..}=nominal else{panic!("real phantom nominal qualification")};
    let value=context.checked_value(&module,nominal,life,vec![a.clone()],&mut b).unwrap();
    let mut active=ActiveQueries::new();
    let address=dynamic_witness::semantic_external::construct(&context,&env,dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(a.clone())},std::slice::from_ref(&value),&mut active,&mut b).unwrap();
    assert_eq!(address.life.binder_count,2);
    assert_eq!(address.classes,vec![a.clone(),a.clone()]);
    let outer=address.life.slots.iter().find(|row|row.path.is_empty()).unwrap().binder as usize;
    let pointee=address.life.retained.iter().find(|row|row.path==[LifeTypeProjection::Pointee]&&row.class==0).unwrap().binder as usize;
    assert_ne!(outer,pointee);assert!(!context.same_origin(&env,&address.origins[outer],&address.origins[pointee],&mut b).unwrap());
    for attack in 0..3{
        let mut claim=SemanticTypeValue::Resolved{ty:address.ty.clone(),life:address.life.clone(),classes:vec![SemanticClassExpr::Formal{scope:SemanticScopeRef::Ancestor(0),ordinal:0};2]};
        let SemanticTypeValue::Resolved{life,classes,..}=&mut claim else{unreachable!()};
        if attack==1{life.binder_count=1;for row in &mut life.slots{row.binder=0;}for row in &mut life.retained{row.binder=0;}for row in &mut life.nominal_arguments{row.binder=0;}classes.truncate(1);}
        if attack==2{for row in &mut life.slots{row.binder=1-row.binder;}for row in &mut life.retained{row.binder=1-row.binder;}for row in &mut life.nominal_arguments{row.binder=1-row.binder;}}
        for wire in [false,true]{
            // This is a transport carrier for the type-value sub-proof only;
            // the inserted Self claim is not an executable source/ABI ticket.
            let decoded;let proposed=if wire{let mut carrier=module.clone();carrier.semantic.as_mut().unwrap().instances[0].self_type=Some(claim.clone());let bytes=zil_ir::wire::encode_module_candidate(&carrier,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0.semantic.unwrap().instances.remove(0).self_type.unwrap();&decoded}else{&claim};
            let result=context.claim(&module,proposed,&address,&env,&mut Budget::new(&limits));
            if attack==0{result.unwrap();}else{assert_eq!(result.unwrap_err().category,zil_base::ResultCategory::SafeReject,"origin attack {attack}, wire {wire}");}
        }
    }
    // Two direct outer occurrences of the same original Life are fresh
    // structural rows and may share one class; a nominal κ was not erased.
    let scalar=CheckedType{ty:Type::I32,life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
    let borrow=dynamic_witness::semantic_external::construct(&context,&env,dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(a)},&[scalar],&mut active,&mut b).unwrap();
    let tuple=dynamic_witness::semantic_external::construct(&context,&env,dynamic_witness::semantic_external::Constructor::Tuple,&[borrow.clone(),borrow],&mut active,&mut b).unwrap();
    assert_eq!(tuple.life.binder_count,1);assert_eq!(tuple.life.slots.len(),2);
}
#[test]
fn actual_phantom_capture_keeps_the_complete_logical_domain_and_exact_creator(){
    let limits=Limits::default();
    let(original,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_PHANTOM,&limits).unwrap();
    for attack in 0..5{
        let mut module=original.clone();
        match attack{
            0=>{},
            1=>{
                let child=module.semantic.as_mut().unwrap().instances.iter_mut().find(|row|row.environment.is_some()).unwrap();
                let SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal),..}=&mut child.header_embedding[0].actual else{panic!("real phantom environment class")};*ordinal+=1;
            },
            2=>{
                let child=module.semantic.as_mut().unwrap().instances.iter_mut().find(|row|row.environment.is_some()).unwrap();
                let SemanticActual::Type(SemanticTypeValue::Resolved{life,classes,..})=&mut child.environment.as_mut().unwrap().actuals[0]else{panic!("real complete phantom type")};
                life.binder_count=0;life.nominal_arguments.clear();classes.clear();
            },
            3=>{
                let program=module.semantic.as_ref().unwrap();
                let creator=program.instances.iter().find(|row|program.sites.iter().any(|site|site.instance==row.id&&matches!(site.action,SemanticSiteAction::Closure{..}))).unwrap();
                let creator_id=creator.id;
                let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{unreachable!()};
                let f=&mut module.functions[function.0 as usize];
                let (block,creation)=f.blocks.iter().enumerate().find_map(|(block,row)|row.instructions.iter().position(|row|matches!(row.kind,InstructionKind::MakeClosure{..})).map(|creation|(block,creation))).unwrap();
                let InstructionKind::MakeClosure{captures,..}=&f.blocks[block].instructions[creation].kind else{unreachable!()};let capture=captures[0];
                let at=f.blocks[block].instructions.iter().position(|row|row.result==Some(capture)).unwrap();
                let InstructionKind::Load(source)=f.blocks[block].instructions[at].kind else{panic!("real phantom capture load")};
                let extra=LocalId(f.locals.len() as u32);let mut local=f.locals[source.0 as usize].clone();local.id=extra;local.name="other_phantom_binding".into();f.locals.push(local);
                let stored=f.blocks[block].instructions[..at].iter().find_map(|row|match row.kind{InstructionKind::Store{local,value}if local==source=>Some(value),_=>None}).unwrap();
                let span=f.blocks[block].instructions[at].span;
                f.blocks[block].instructions[at].kind=InstructionKind::Load(extra);
                f.blocks[block].instructions.splice(at..at,[Instruction{result:None,kind:InstructionKind::StorageLive(extra),span},Instruction{result:None,kind:InstructionKind::Store{local:extra,value:stored},span}]);
                let block=f.blocks[block].id;
                for site in &mut module.semantic.as_mut().unwrap().sites{if site.instance==creator_id{if let SemanticSiteAddress::Typed{block:actual,instruction,..}=&mut site.at{if *actual==block&&*instruction>=at as u32{*instruction+=2;}}}}
            },
            4=>{module.closures[0].storage_life.as_mut().unwrap().fixed_binder_count=0;},
            _=>unreachable!(),
        }
        for wire in [false,true]{
            let decoded;let candidate=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
            let result=check_program(candidate,&mut Budget::new(&limits));
            if attack==0{result.unwrap();}else{let error=result.unwrap_err();assert_eq!(error.category,zil_base::ResultCategory::SafeReject,"phantom attack {attack}, wire {wire}: {error:?}");}
        }
    }
}
#[test]
fn shared_typed_generic_entry_checks_each_original_world_instead_of_the_first(){
    let limits=Limits::default();let(mut module,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    let program=module.semantic.as_mut().unwrap();
    let mut extra=program.instances.iter().find(|row|!row.actuals.is_empty()).unwrap().clone();let previous=extra.id;
    extra.id.index=program.instances.iter().map(|row|row.id.index).max().unwrap()+1;
    let copied=program.sites.iter().filter(|row|row.instance==previous).cloned().map(|mut row|{row.instance=extra.id;row}).collect::<Vec<_>>();
    let world=extra.id;program.instances.push(extra);program.sites.extend(copied);
    for attack in [false,true]{
        let mut module=module.clone();
        if attack{let extra=module.semantic.as_mut().unwrap().instances.iter_mut().find(|row|row.id==world).unwrap();extra.actuals[0]=SemanticActual::Type(SemanticTypeValue::Resolved{ty:Type::U32,life:empty_life(),classes:vec![]});}
        for wire in [false,true]{let decoded;let candidate=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};let result=check_program(candidate,&mut Budget::new(&limits));if attack{let error=result.unwrap_err();assert_eq!(error.category,zil_base::ResultCategory::SafeReject,"mismatched shared world, wire {wire}: {error:?}");}else{result.unwrap();}}
    }
}
#[test]
fn actual_default_capture_has_its_exact_creator_boundary_on_both_routes() {
    let limits=Limits::default();
    let (module,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_LOCAL,&limits).unwrap();
    assert!(matches!(module.closures[0].captures.as_slice(),[Type::Address{mutable:false,..}]));
    assert_eq!(module.closures[0].storage_life.as_ref().unwrap().fixed_binder_count,1);
    check_program(&module,&mut Budget::new(&limits)).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
    let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
    check_program(&decoded,&mut Budget::new(&limits)).unwrap();
}
#[test]
fn actual_default_capture_cannot_substitute_another_head_or_previous_reborrow() {
    let limits=Limits::default();
    let (original,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_LOCAL,&limits).unwrap();
    for attack in 0..6 {
        let mut module=original.clone();
        match attack {
            0=>{
                // Same machine type, distinct initialized local and value. The
                // original free binder remains bias, not this extra occupant.
                let f=&mut module.functions[0];let extra=LocalId(f.locals.len() as u32);
                let mut local=f.locals[0].clone();local.name="other_same_type".into();local.id=extra;f.locals.push(local);
                let value=ValueId(f.values.len() as u32);f.values.push(Type::I32);
                let block=&mut f.blocks[0];
                let at=block.instructions.iter().position(|row|matches!(row.kind,InstructionKind::BorrowLocal{..})).unwrap();
                let span=block.instructions[at].span;
                let InstructionKind::BorrowLocal{local}=&mut block.instructions[at].kind else{unreachable!()};*local=extra;
                block.instructions.splice(at..at,[
                    Instruction{result:Some(value),kind:InstructionKind::Constant(Constant::I32(99)),span},
                    Instruction{result:None,kind:InstructionKind::StorageLive(extra),span},
                    Instruction{result:None,kind:InstructionKind::Store{local:extra,value},span},
                ]);
                let creator=module.semantic.as_ref().unwrap().instances.iter().find(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(FunctionId(0))))).unwrap().id;
                for site in &mut module.semantic.as_mut().unwrap().sites {
                    if site.instance==creator {if let SemanticSiteAddress::Typed{instruction,..}=&mut site.at {if *instruction>=at as u32{*instruction+=3;}}}
                }
            }
            1=>{
                let program=module.semantic.as_mut().unwrap();
                let claim=program.sites.iter_mut().find_map(|site|match &mut site.action{SemanticSiteAction::DeclareLocal{expected,..}if matches!(expected,SemanticTypeValue::Resolved{ty:Type::Closure(_),..})=>Some(expected),_=>None}).unwrap();
                let SemanticTypeValue::Resolved{classes,..}=claim else{unreachable!()};
                let SemanticClassExpr::OuterOccurrence{operand:SemanticOperand::Value(value),..}=&mut classes[0] else{panic!("actual captured head occurrence")};
                *value=ValueId(1); // the parent reborrow is not this capture V2
            }
            2=>{
                let program=module.semantic.as_mut().unwrap();
                let child=program.instances.iter_mut().find(|row|row.environment.is_some()).unwrap();
                child.parent=None;
            }
            3=>{
                let definition=&mut module.closures[0];
                definition.storage_life.as_mut().unwrap().fixed_binder_count=0;
            }
            4=>{
                let child=module.semantic.as_mut().unwrap().instances.iter_mut().find(|row|row.environment.is_some()).unwrap();
                child.header_embedding[0].actual=SemanticClassExpr::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal:999};
            }
            5=>{
                let program=module.semantic.as_mut().unwrap();
                let creator=program.sites.iter_mut().find(|site|matches!(site.action,SemanticSiteAction::Closure{..})).unwrap();
                let SemanticSiteAction::Closure{result,..}=&mut creator.action else{unreachable!()};*result=SemanticOperand::Value(ValueId(4));
            }
            _=>unreachable!(),
        }
        let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","attack {attack}: {error:?}");
        if attack==0{assert!(error.message.contains("differs from its original free-value binding"),"{error:?}");}
        if attack==1{assert!(error.message.contains("site complete type/class claim differs"),"{error:?}");}
        let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
        let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        let error=check_program(&decoded,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","wire attack {attack}: {error:?}");
    }
}
#[test]
fn repeated_closure_identity_does_not_select_the_first_creator_image() {
    let limits=Limits::default();
    let (mut module,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_LOCAL,&limits).unwrap();
    let f=&mut module.functions[0];
    let block=&mut f.blocks[0];
    let ordinal=block.instructions.iter().position(|row|row.result==Some(ValueId(4))).unwrap();
    assert!(matches!(block.instructions[ordinal].kind,InstructionKind::Load(_)));
    block.instructions[ordinal].kind=InstructionKind::MakeClosure{closure:ClosureId(0),captures:vec![ValueId(2)]};
    let program=module.semantic.as_mut().unwrap();
    let mut creation=program.sites.iter().find(|site|matches!(site.action,SemanticSiteAction::Closure{..})).unwrap().clone();
    creation.at=SemanticSiteAddress::Typed{block:BlockId(0),instruction:ordinal as u32,edge:SemanticSiteEdge::After};
    let SemanticSiteAction::Closure{result,..}=&mut creation.action else{unreachable!()};*result=SemanticOperand::Value(ValueId(4));
    program.sites.push(creation);
    for wire in [false,true] {
        let decoded;
        let candidate=if wire {let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
        let error=check_program(candidate,&mut Budget::new(&limits)).unwrap_err();
        assert!(error.message.contains("several unresolved actual creation images"),"{error:?}");
    }
}
#[test]
fn a_reborrow_preserves_two_distinct_inner_origins_with_equal_outside_images() {
    let limits=Limits::default();
    let (module,_)=zil_ir::wire::decode_module_candidate(COMPLETE_LOCAL,&limits).unwrap();
    let mut budget=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut budget).unwrap();
    let scope=ScopeKey::Instance(module.semantic.as_ref().unwrap().instances[0].id);
    let key=ClassKey::Implicit{scope,ordinal:0};
    let origin=|class|IntrinsicOrigin::Existing{scope,operand:OperandKey::Value(ValueId(0)),path:vec![LifeTypeProjection::Pointee],class};
    let old=ClassKey::Outer{scope,operand:OperandKey::Value(ValueId(1)),path:vec![],mode:StoredBorrowMode::SharedAddress};
    let parent=CheckedType{
        ty:Type::Address{mutable:false,inner:Box::new(Type::I32)},
        // The transfer's domain bookkeeping is tested independently of a
        // real rich pointee admission: no opaque/type permission is issued.
        life:StorageLifeShape{binder_count:3,slots:vec![StorageLifeSlot{path:vec![],binder:2,mode:StoredBorrowMode::SharedAddress}],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class:0,binder:0},StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class:1,binder:1}],nominal_arguments:vec![]},
        classes:vec![key.clone(),key,old.clone()],origins:vec![origin(0),origin(1),IntrinsicOrigin::DerivedClass{source:old}],statics:vec![],
    };
    let child=context.reborrow_value(&parent,scope,ValueId(2),false,&mut budget).unwrap();
    assert_eq!(child.life.binder_count,3);
    let a=child.life.retained[0].binder as usize;let c=child.life.retained[1].binder as usize;
    assert_eq!(child.origins[a],origin(0));assert_eq!(child.origins[c],origin(1));assert_ne!(a,c);
    assert_eq!(child.classes[child.life.slots[0].binder as usize],ClassKey::Outer{scope,operand:OperandKey::Value(ValueId(2)),path:vec![],mode:StoredBorrowMode::SharedAddress});
}

// These controls exercise the frozen original-scheme sub-proof, not CT
// execution. Actual Source/context/VM acceptance is tested by the driver.
fn frozen_scheme_control()->(TypedModule,CandidatePlan) {
    let limits=Limits::default();
    let (mut module,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    let owner=module.module_id;
    let id=SemanticDefinitionId{module:owner,index:900};
    let declaration=LifeDeclarationId{module:owner,declaration:zil_base::NodeId(900),compiler:false};
    let mut identity=module.semantic.as_ref().unwrap().definitions[0].identity.clone();
    identity.declaration_path=vec!["frozen-scheme-control".into(),"P".into()];
    let formal=||SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0});
    let callable=SemanticRequirementRecipe::Callable{ty:formal(),kind:CallableKind::Fn,signature:Box::new(SemanticSignatureRecipe{bound_lifetimes:0,invocation_lifetimes:0,parameters:vec![SemanticParameterRecipe{ty:SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound{depth:0,index:0}),inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))},mode:PassingMode::Move}],result:SemanticTypeRecipe::Atom(Type::Unit),result_mode:PassingMode::Move,call_kind:CallKind::Direct})};
    let definition=SemanticDefinition{id,identity:identity.clone(),parent:None,kind:SemanticDefinitionKind::Nominal,parameters:vec![SemanticFormal::Type{requirements:vec![SemanticRequirementRecipe::Structural{ty:formal(),bound:DynStructuralBound::Value}]},SemanticFormal::Lifetime],implicit_lifetimes:vec![],requirements:vec![SemanticRequirementRecipe::ForAll{lifetimes:1,requirements:vec![SemanticRequirementRecipe::ForAll{lifetimes:0,requirements:vec![callable]}]}],environment:vec![],body:SemanticDefinitionBody::Nominal{declaration,kind:LifeNominalKind::Record,body:SemanticNominalBody::Product{fields:vec![SemanticFieldRecipe{name:"field".into(),ty:formal()}]}}};
    module.life_declarations.push(LifeNominalDeclaration{id:declaration,identity,kind:LifeNominalKind::Record,parameters:vec![LifeParameterKind::Type,LifeParameterKind::Lifetime]});
    let mut trait_definition=definition.clone();trait_definition.id.index=901;trait_definition.identity.declaration_path=vec!["frozen-scheme-control".into(),"Trait".into()];
    trait_definition.kind=SemanticDefinitionKind::Scope;trait_definition.parameters=vec![SemanticFormal::Lifetime,SemanticFormal::Type{requirements:vec![]},SemanticFormal::Const{carrier:SemanticTypeRecipe::Atom(Type::Usize)}];trait_definition.requirements=vec![];
    trait_definition.body=SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:LifeDeclarationId{module:owner,declaration:zil_base::NodeId(901),compiler:false}});
    module.semantic.as_mut().unwrap().definitions.extend([definition,trait_definition]);
    (module,plan)
}
fn frozen_schemes(module:&TypedModule,limits:&Limits)->Check<()> {
    let mut budget=Budget::new(limits);let context=CheckedContext::catalogue(module,&mut budget)?;let mut active=ActiveQueries::new();
    let mut original=original::CheckedOriginal::check(module,&context,&mut active,&mut budget)?;
    original.frozen_schemes(module,&context,&mut active,&mut budget)
}
#[test]
fn frozen_unused_nominal_and_scope_keep_complete_original_formals_and_quantifiers() {
    let limits=Limits::default();let(module,plan)=frozen_scheme_control();
    frozen_schemes(&module,&limits).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();let(decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
    frozen_schemes(&decoded,&limits).unwrap();
}
#[test]
fn frozen_unused_original_scheme_cannot_hide_bad_kind_scope_carrier_or_bound_depth() {
    let limits=Limits::default();let(original,plan)=frozen_scheme_control();frozen_schemes(&original,&limits).unwrap();
    for attack in 0..7 {
        let mut module=original.clone();
        let owner=module.module_id;
        let definitions=&mut module.semantic.as_mut().unwrap().definitions;
        let n=definitions.len();
        match attack {
            0=>{let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&mut definitions[n-2].body else{unreachable!()};fields[0].ty=SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:1});},
            1=>{let SemanticRequirementRecipe::ForAll{lifetimes,..}=&mut definitions[n-2].requirements[0] else{unreachable!()};*lifetimes=0;},
            2=>{definitions[n-1].body=SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl{declaration:LifeDeclarationId{module:owner,declaration:zil_base::NodeId(901),compiler:false},self_type:SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0})});},
            3=>{
                definitions[n-2].parameters.push(SemanticFormal::Const{carrier:SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Static,inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))}});
                module.life_declarations.last_mut().unwrap().parameters.push(LifeParameterKind::Const(Type::Address{mutable:false,inner:Box::new(Type::I32)}));
            },
            4|6=>{definitions[n-2].requirements.push(SemanticRequirementRecipe::Trait{ty:SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0}),application:SemanticTraitRecipe{declaration:LifeDeclarationId{module:owner,declaration:zil_base::NodeId(if attack==4{999}else{901}),compiler:false},arguments:if attack==4{vec![]}else{vec![SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)),SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)),SemanticActualRecipe::Const{carrier:SemanticTypeRecipe::Atom(Type::Usize),value:SemanticConstRecipe::Value(Constant::Usize(0))}]} }});},
            5=>{let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&mut definitions[n-2].body else{unreachable!()};fields[0].ty=SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal:999}),inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))};},
            _=>unreachable!(),
        }
        let error=frozen_schemes(&module,&limits).unwrap_err();assert_eq!(error.code,"E-VERIFY-SEMANTIC","attack {attack}: {error:?}");
        let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();let(decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        let error=frozen_schemes(&decoded,&limits).unwrap_err();assert_eq!(error.code,"E-VERIFY-SEMANTIC","wire attack {attack}: {error:?}");
    }
}

// The original functions below are deliberately uninstantiated. Their bodies
// must be checked in the original universal domain, before actual execution or
// a concrete caller can hide an effectful branch.
fn original_effect_control(kind:CallKind,yields:bool,in_defer:bool)->(TypedModule,CandidatePlan){
    let limits=Limits::default();let(mut module,plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    let mut definition=module.semantic.as_ref().unwrap().definitions[0].clone();
    definition.id.index=950;definition.parent=None;definition.kind=SemanticDefinitionKind::Function;
    definition.identity.declaration_path=vec!["original_effect_control".into()];definition.parameters.clear();definition.requirements.clear();definition.environment.clear();definition.implicit_lifetimes.clear();
    let span=module.functions[0].blocks[0].instructions[0].span;
    let expression=CoreExpr{id:zil_base::NodeId(952),span,kind:CoreExprKind::Call{callee:Box::new(CoreExpr{id:zil_base::NodeId(953),span,kind:CoreExprKind::Name("task_yield".into())}),arguments:vec![]}};
    let call=CoreStatement{id:zil_base::NodeId(951),span,kind:CoreStatementKind::Expression(expression)};
    let body=CoreBlock{span,statements:if !yields{vec![]}else if in_defer{vec![CoreStatement{id:zil_base::NodeId(954),span,kind:CoreStatementKind::Defer(CoreBlock{span,statements:vec![call]})}]}else{vec![call]}};
    definition.body=SemanticDefinitionBody::FunctionWithBody{
        signature:SemanticSignatureRecipe{bound_lifetimes:0,invocation_lifetimes:0,parameters:vec![],result:SemanticTypeRecipe::Atom(Type::Unit),result_mode:PassingMode::Move,call_kind:kind},
        sites:if yields{vec![SemanticSiteRecipe{id:SemanticSiteId(950),occurrence_path:vec![950],kind:SemanticSiteRecipeKind::OriginalCall}]}else{vec![]},
        parameters:vec![],receiver_parameter:None,
        site_nodes:if yields{vec![SemanticOriginalSiteNode{site:SemanticSiteId(950),node:SemanticOriginalNode::Expression(zil_base::NodeId(952))}]}else{vec![]},
        names:vec![],protocol_contracts:vec![],foreign:None,resume_bound:TaskResumeBound::Local,body:SemanticOriginalFunctionBody::Defined(body),
    };
    module.semantic.as_mut().unwrap().definitions.push(definition);(module,plan)
}
#[test]
fn unused_original_effects_keep_fixed_conventions_and_include_deferred_execution(){
    let limits=Limits::default();
    for(kind,yields,in_defer,accept)in[(CallKind::Direct,false,false,true),(CallKind::Suspend,false,false,true),(CallKind::Suspend,true,false,true),(CallKind::Direct,true,false,false),(CallKind::Direct,true,true,false)]{
        let(module,plan)=original_effect_control(kind,yields,in_defer);
        for route in 0..2{
            let decoded=if route==0{module.clone()}else{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0};
            let result=check_program(&decoded,&mut Budget::new(&limits));
            if accept{result.unwrap();}else{let error=result.unwrap_err();assert!(error.message.contains("original body may suspend beyond its fixed Direct"),"{error:?}");}
        }
    }
}
#[test]
fn deleting_a_real_name_binding_cannot_promote_a_declaration_to_an_intrinsic(){
    let limits=Limits::default();let(mut module,plan)=original_effect_control(CallKind::Suspend,true,false);
    let program=module.semantic.as_mut().unwrap();
    let mut shadow=program.definitions.last().unwrap().clone();shadow.id.index=960;shadow.identity.declaration_path=vec!["task_yield".into()];
    let SemanticDefinitionBody::FunctionWithBody{signature,sites,site_nodes,body,..}=&mut shadow.body else{unreachable!()};
    signature.call_kind=CallKind::Direct;sites.clear();site_nodes.clear();*body=SemanticOriginalFunctionBody::Defined(CoreBlock{statements:vec![],span:module.functions[0].blocks[0].instructions[0].span});
    program.definitions.push(shadow);
    for route in 0..2{
        let decoded=if route==0{module.clone()}else{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0};
        let error=check_program(&decoded,&mut Budget::new(&limits)).unwrap_err();assert!(error.message.contains("unresolved real declaration shadow"),"{error:?}");
    }
}
