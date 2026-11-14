//! Frozen genuine Source→Core→Typed probe outputs. These are incomplete,
//! untrusted producer proposals, not successful Types/Analyze receipts.
use super::*;
const IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-missing-call-sites.wire");
const LOCAL:&[u8]=include_bytes!("fixtures/local-closure-missing-child-instance.wire");
#[test]
fn actual_source_identity_cannot_invent_its_missing_original_call_site() {
    let limits=Limits::default();
    let (historical,_)=zil_ir::wire::decode_module_candidate(IDENTITY,&limits).unwrap();
    let old_error=check_program(&historical,&mut Budget::new(&limits)).unwrap_err();
    assert!(old_error.message.contains("original definition body disagrees with its declared kind"),"{old_error:?}");
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
    let (historical,_)=zil_ir::wire::decode_module_candidate(LOCAL,&limits).unwrap();
    let old_error=check_program(&historical,&mut Budget::new(&limits)).unwrap_err();
    assert!(old_error.message.contains("original definition body disagrees with its declared kind"),"{old_error:?}");
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
const COMPLETE_IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-original-body.wire");
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
const COMPLETE_LOCAL:&[u8]=include_bytes!("fixtures/local-closure-complete-creator.wire");
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
        names:vec![],protocol_contracts:vec![],body:SemanticOriginalFunctionBody::Defined(body),
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
