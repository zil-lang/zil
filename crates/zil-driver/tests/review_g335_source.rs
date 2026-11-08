//! Prepared independent genuine Source controls for the finite G3.3.5 lane.
//! Not executed until an exact implementation/test freeze and fresh review build.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CallableKind, InstructionKind, Type, TypedModule};
fn source(text: &str) -> SourceUnit { SourceUnit::new(FileId(0),ModuleId(0),"review-g335.zil",text) }
const STATEFUL: &str = "struct Tag<life s>{p:&'s i64}\nfn helper<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)}\nfn apply<life s>(seed:&Tag<'s>)->i32{var count=0;var f=move(p:&Tag<'s>)->i32=>{count+=1;return i32(*p.p+*seed.p)+count-1};let first=helper(&mut f,seed);let second=helper(&mut f,seed);if first!=42{return 11}if second!=43{return 12}return 42}\nfn setup<life s>(x:&'s i64)->i32{let seed:Tag<'s>=Tag::<'s>{p:x};return apply(&seed)}fn main()->i32{let x:i64=21;return setup(&x)-42}";
const TWO_CLASSES: &str = "struct Tag<life s>{p:&'s i64}\nfn helper<life s,F:FnMut(&Tag<'s>)->i32>(f:&mut F,x:&Tag<'s>)->i32{return f(x)}\nfn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{let saved=right;var count=0;var f=move(p:&Tag<'a>)->i32=>{count+=1;return i32(*p.p+*left.p)+count-1};let first=helper(&mut f,left);let second=helper(&mut f,left);if first!=42{return 11}if second!=43{return 12}return 42}\nfn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let left:Tag<'a>=Tag::<'a>{p:x};let right:Tag<'b>=Tag::<'b>{p:y};return apply(&left,&right)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)-42}";
fn typed(text:&str)->TypedModule {
    use zil_expand::source_context::{DeclarationPlacement,SourceIdentityContext};
    let s=source(text);let syntax=zil_syntax::parse(&s).into_result().unwrap();let limits=Limits::default();
    let core=zil_expand::expand_with_map_limits_target_context(&s,&syntax,&limits,&TargetProfile::host_llvm(),SourceIdentityContext::ClosedSnapshot{root:s.module_id,aliases:&Default::default(),declarations:DeclarationPlacement::Original}).unwrap().core;
    let role=core.functions.iter().flat_map(|f|&f.callable_parameters).find(|r|r.source_life.is_some()).expect("real selected source-bound context");
    assert_eq!(role.kind,CallableKind::FnMut);assert_eq!((role.index,role.generic_index),(0,0));
    zil_types::check(&core,&TargetProfile::host_llvm(),&limits).unwrap()
}
#[test]
fn review_g335_real_mutating_environment_and_exclusive_helper_are_independent_facts(){
    let m=typed(STATEFUL);let limits=Limits::default();
    let d=m.closures.iter().find(|d|d.kind==CallableKind::FnMut).expect("actual stateful FnMut, not a pure Fn adapter");
    assert!(d.captures.iter().any(|t|*t==Type::I32));
    assert!(d.captures.iter().any(|t|matches!(t,Type::Address{mutable:false,inner} if matches!(inner.as_ref(),Type::BorrowedStorage{..}))));
    let entry=m.functions.iter().find(|f|f.id==d.function).unwrap();
    assert!(entry.blocks.iter().flat_map(|b|&b.instructions).any(|i|matches!(i.kind,InstructionKind::WriteCapture{..})),"real state update must reach Typed instructions");
    let helper=m.functions.iter().find(|f|f.callable_contract.is_some()).unwrap();let role=&helper.callable_contract.as_ref().unwrap().roles[0];
    assert_eq!(role.kind,CallableKind::FnMut);assert_eq!(role.life,d.storage_life);
    let header=helper.storage_life.as_ref().unwrap();
    for class in 0..d.storage_life.as_ref().unwrap().fixed_binder_count {
        assert!(header.classes.iter().any(|r|r.position.base==zil_ir::StorageBase::Parameter(role.parameter) && r.position.path==[zil_ir::LifeTypeProjection::Pointee] && r.position.class==class),"every actual free Fixed must have a real receiver selector");
    }
    let receiver=helper.parameters[role.parameter as usize];
    assert!(matches!(&helper.locals[receiver.0 as usize].ty,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Closure(id) if *id==d.id)));
    assert!(helper.blocks.iter().flat_map(|b|&b.instructions).any(|i|matches!(i.kind,InstructionKind::CallClosureBorrowed{kind:CallableKind::FnMut,..})));
    let plan=zil_analyze::analyze(&m,&limits).unwrap();zil_verify::verify_module(m.clone(),plan.clone(),&m.target,&limits).unwrap();
    let wire=zil_ir::wire::encode_module_candidate(&m,&plan,&limits).unwrap();zil_verify::verify_module_bytes(&wire,&m.target,&limits).unwrap();
}
#[test]
fn review_g335_wrong_class_and_overlapping_exclusive_receiver_are_not_accepted(){
    let wrong=TWO_CLASSES.replace("let second=helper(&mut f,left)","let second=helper(&mut f,right)");
    let report=zil_driver::compile_source(&source(&wrong),&zil_driver::CompileOptions::default());
    assert_eq!(report.status,ResultCategory::SafeReject,"{}",report.to_json());assert_eq!(report.phase,Phase::Analyze,"{}",report.to_json());assert!(report.diagnostics.iter().any(|d|d.message.contains("class")),"{}",report.to_json());assert_eq!(report.verification_runs,0);
    let overlap=STATEFUL.replace("let first=helper(&mut f,seed);let second=helper(&mut f,seed)","let held=&mut f;let first=helper(&mut f,seed);let second=helper(held,seed)");
    assert_ne!(overlap,STATEFUL);
    let report=zil_driver::compile_source(&source(&overlap),&zil_driver::CompileOptions::default());
    assert_eq!(report.status,ResultCategory::SafeReject,"{}",report.to_json());assert_eq!(report.phase,Phase::Analyze,"{}",report.to_json());assert!(report.diagnostics.iter().any(|d|d.code=="E-ANALYZE-IR" && d.message.contains("overlaps an actual stored target/parent loan")),"{}",report.to_json());assert_eq!(report.verification_runs,0);
}
#[test]
fn review_g335_source_derived_shared_receiver_and_downgraded_mode_need_both_kernels(){
    let m=typed(STATEFUL);let limits=Limits::default();let plan=zil_analyze::analyze(&m,&limits).unwrap();
    let d=m.closures.iter().find(|d|d.kind==CallableKind::FnMut).unwrap();let closure=d.id;
    for attack in 0..3 {
        let mut bad=m.clone();
        if attack==0 {
            fn shared(ty:&mut Type,id:zil_ir::ClosureId){if let Type::Address{mutable,inner}=ty && matches!(inner.as_ref(),Type::Closure(actual) if *actual==id){*mutable=false;}}
            for f in &mut bad.functions {
                for l in &mut f.locals{shared(&mut l.ty,closure)}for v in &mut f.values{shared(v,closure)}
                for i in f.blocks.iter_mut().flat_map(|b|&mut b.instructions){
                    if let InstructionKind::BorrowLocalMut{local}=i.kind && f.locals[local.0 as usize].ty==Type::Closure(closure){i.kind=InstructionKind::BorrowLocal{local};}
                    if let InstructionKind::ReborrowAddress{mutable,..}=&mut i.kind && i.result.is_some_and(|v|matches!(&f.values[v.0 as usize],Type::Address{mutable:false,inner} if matches!(inner.as_ref(),Type::Closure(id) if *id==closure))){*mutable=false;}
                }
            }
        } else {
            let helper=bad.functions.iter_mut().find(|f|f.callable_contract.is_some()).unwrap();
            if attack==1{helper.callable_contract.as_mut().unwrap().roles[0].kind=CallableKind::Fn;}
            else{helper.blocks.iter_mut().flat_map(|b|&mut b.instructions).find_map(|i|if let InstructionKind::CallClosureBorrowed{kind,..}=&mut i.kind{Some(kind)}else{None}).map(|kind|*kind=CallableKind::Fn).expect("actual borrowed invocation");}
        }
        let e=zil_analyze::analyze(&bad,&limits).unwrap_err();assert!(e.iter().all(|d|d.category!=ResultCategory::ResourceLimit),"attack={attack} {e:?}");
        let mut forged=plan.clone();forged.module_fingerprint=bad.fingerprint();let wire=zil_ir::wire::encode_module_candidate(&bad,&forged,&limits).unwrap();
        for result in [zil_verify::verify_module(bad.clone(),forged,&bad.target,&limits),zil_verify::verify_module_bytes(&wire,&bad.target,&limits)]{
            let e=result.unwrap_err();assert!(e.iter().all(|d|d.phase==Phase::Verify && d.category!=ResultCategory::ResourceLimit && !d.message.to_ascii_lowercase().contains("fingerprint")),"attack={attack} {e:?}");
            eprintln!("review G335 semantic raw/Wire receiver/mode attack={attack}: {}",e[0].message);
        }
    }
}
#[test]
fn review_g335_stateful_phantom_data_does_not_regain_some_eligibility(){
    use zil_expand::source_context::{DeclarationPlacement,SourceIdentityContext};
    let text="struct Tag<life s>{p:&'s i64}\nstruct Phantom<life s>{n:i32}\nfn helper<life s,F:FnMut(&Phantom<'s>)->i32>(f:&mut F,x:&Phantom<'s>)->i32{return f(x)}\nfn apply<life a,life b>(seed:&Tag<'a>)->i32{var count=0;var f=move(p:&Phantom<'a>)->i32=>{count+=1;return p.n+i32(*seed.p)+count-1};let wrong:Phantom<'b>=Phantom::<'b>{n:21};return helper(&mut f,&wrong)}\nfn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let seed:Tag<'a>=Tag::<'a>{p:x};return apply(&seed)}fn main()->i32{let x:i64=21;let y:i64=0;return setup(&x,&y)}";
    let s=source(text);let syntax=zil_syntax::parse(&s).into_result().unwrap();let mut core=zil_expand::expand_with_map_limits_target_context(&s,&syntax,&Limits::default(),&TargetProfile::host_llvm(),SourceIdentityContext::ClosedSnapshot{root:s.module_id,aliases:&Default::default(),declarations:DeclarationPlacement::Original}).unwrap().core;
    assert!(core.functions.iter().flat_map(|f|&f.callable_parameters).all(|r|r.source_life.is_none()),"FnMut must not reopen G334 phantom qualification");
    let real_source=source(STATEFUL);let real_syntax=zil_syntax::parse(&real_source).into_result().unwrap();
    let real_core=zil_expand::expand_with_map_limits_target_context(&real_source,&real_syntax,&Limits::default(),&TargetProfile::host_llvm(),SourceIdentityContext::ClosedSnapshot{root:real_source.module_id,aliases:&Default::default(),declarations:DeclarationPlacement::Original}).unwrap().core;
    let real_role=real_core.functions.iter().flat_map(|f|&f.callable_parameters).find(|r|r.source_life.is_some()).unwrap();
    let phantom_role=core.functions.iter_mut().flat_map(|f|&mut f.callable_parameters).next().unwrap();
    phantom_role.source_life=real_role.source_life.clone();phantom_role.life=real_role.life.clone();
    let errors=zil_types::check(&core,&TargetProfile::host_llvm(),&Limits::default()).unwrap_err();
    assert!(errors.iter().any(|d|d.phase==Phase::Types && d.category==ResultCategory::Unsupported && d.message.contains("source helper first profile")),"{errors:?}");
    eprintln!("review G335 genuine Phantom Core with attached FnMut Some rejects independently: {errors:?}");
    let report=zil_driver::compile_source(&s,&zil_driver::CompileOptions::default());
    assert_eq!(report.status,ResultCategory::SafeReject,"{}",report.to_json());assert_eq!(report.phase,Phase::Analyze,"{}",report.to_json());assert_eq!(report.verification_runs,0);
}
