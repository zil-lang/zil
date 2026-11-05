//! Independent Source-derived G3.3.4 controls. Raw/Wire mutations below are
//! adversarial candidates derived after genuine Source succeeds, not Source
//! producer authority or model-only positive evidence.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CoreClosureHeaderPosition, CoreModule, InstructionKind, LifeParameterKind, StorageBinder, TypedModule};

fn source(text: &str) -> SourceUnit { SourceUnit::new(FileId(0), ModuleId(0), "review-g334.zil", text) }
fn core(text: &str) -> CoreModule {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand_with_map_limits_target_context(
        &unit, &syntax, &Limits::default(), &TargetProfile::host_llvm(),
        SourceIdentityContext::ClosedSnapshot { root: unit.module_id, aliases: &Default::default(), declarations: DeclarationPlacement::Original },
    ).unwrap().core
}
fn typed(text: &str) -> TypedModule { zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap() }
const SIMPLE: &str = include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/fn-shared-input.zil");
const REVERSE: &str = include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/reverse-generics-good.zil");
const GOOD: &str = include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/actual-caller-good.zil");
const WRONG: &str = include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/actual-caller-wrong-class.zil");

#[test]
fn review_g334_source_bound_selection_retains_original_full_ordinals_and_role() {
    let a = core(include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/unused-declared-bound-a.zil"));
    let b = core(include_str!("../../../tests/language-readiness/terminal/342/fixed-helper-bridge/unused-declared-bound-b.zil"));
    let select = |c: &CoreModule| c.functions.iter().find(|f| f.callable_parameters.iter().any(|r| r.source_life.is_some())).unwrap().clone();
    let (af, bf) = (select(&a), select(&b));
    assert_eq!(af.storage_life, bf.storage_life);
    let (ar, br) = (&af.callable_parameters[0], &bf.callable_parameters[0]);
    assert_eq!(ar.parameters, br.parameters); assert_eq!(ar.result, br.result); assert_eq!(ar.life, br.life);
    let (ac, bc) = (ar.source_life.as_ref().unwrap(), br.source_life.as_ref().unwrap());
    assert_eq!(ac.parameters, [LifeParameterKind::Lifetime, LifeParameterKind::Lifetime, LifeParameterKind::Type]);
    assert_eq!((ar.index, ar.generic_index, ac.generic_parameter, ac.bound_ordinal), (0, 0, 2, 0));
    assert_eq!((ac.named_fixed[0].parameter, bc.named_fixed[0].parameter), (0, 1));
    assert_eq!(ac.inputs, bc.inputs);
    // The Source provenance identity is recorded, not used as runtime class authority.
    let reverse = select(&core(REVERSE));
    let role = &reverse.callable_parameters[0]; let context = role.source_life.as_ref().unwrap();
    assert_eq!(context.parameters, [LifeParameterKind::Lifetime, LifeParameterKind::Type, LifeParameterKind::Lifetime]);
    assert_eq!((role.index, role.generic_index, context.generic_parameter, context.named_fixed[0].parameter), (0, 0, 1, 2));
    assert!(context.inputs.iter().any(|a| a.parameter == 1 && a.source_parameter == Some(0) && a.header_binder == StorageBinder::Fixed(0) && matches!(a.position, CoreClosureHeaderPosition::Class(_))));
    assert!(context.inputs.iter().any(|a| a.parameter == 2 && a.source_parameter == Some(2) && a.header_binder == StorageBinder::Fixed(1) && matches!(a.position, CoreClosureHeaderPosition::Class(_))));
    let reordered = SIMPLE.replace("(f:&F,x:&Tag<'s>)", "(x:&Tag<'s>,f:&F)").replace("helper(&f,seed)", "helper(seed,&f)");
    assert_ne!(reordered, SIMPLE);
    let reordered_core = core(&reordered); let reordered_helper = select(&reordered_core);
    assert_eq!(reordered_helper.callable_parameters[0].index, 1);
    // Runtime scalar parameters create no borrow/class anchors. Their count
    // must not be compared to the anchor inventory cardinality.
    let scalars = SIMPLE.replace("(f:&F,x:&Tag<'s>)", "(a:i32,f:&F,x:&Tag<'s>,b:i32)")
        .replace("return f(x)", "return f(x)+a+b").replace("helper(&f,seed)", "helper(0,&f,seed,0)");
    assert_ne!(scalars, SIMPLE);
    for input in [SIMPLE, REVERSE, reordered.as_str(), scalars.as_str()] {
        let module = typed(input); let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &Limits::default()).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    }
}

#[test]
fn review_g334_actual_source_wrong_data_class_is_rejected_by_the_kernel() {
    let options = zil_driver::CompileOptions::default();
    // The declared bound remains an obligation even when the callee ignores F.
    for (good_text, wrong_text) in [
        (GOOD.to_owned(), WRONG.to_owned()),
        (GOOD.replace("return helper(&f,left)", "let first=helper(&f,left);return helper(&f,left)"), GOOD.replace("return helper(&f,left)", "let first=helper(&f,left);return helper(&f,right)")),
        (GOOD.replace("return f(x)", "return 42"), WRONG.replace("return f(x)", "return 42")),
        (GOOD.replace("return f(x)", "return #(42)"), WRONG.replace("return f(x)", "return #(42)")),
    ] {
        let control = zil_driver::compile_source(&source(&good_text), &options);
        assert_eq!(control.status, ResultCategory::Success, "{}", control.to_json());
        assert_eq!(control.verification_runs, 1);
        let wrong = zil_driver::compile_source(&source(&wrong_text), &options);
        assert_eq!(wrong.status, ResultCategory::SafeReject, "{}", wrong.to_json());
        assert_eq!(wrong.phase, Phase::Analyze, "{}", wrong.to_json());
        assert!(wrong.diagnostics.iter().any(|d| d.message.contains("retained classes")), "{}", wrong.to_json());
        assert_eq!(wrong.verification_runs, 0);
    }
}

#[test]
fn review_g334_actual_source_typed_raw_wire_need_independent_class_and_capture_proofs() {
    let limits = Limits::default(); let module = typed(GOOD);
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let helper_index = module.functions.iter().position(|f| f.callable_contract.as_ref().is_some_and(|c| c.roles.iter().any(|r| r.life.is_some()))).unwrap();
    let helper_id = module.functions[helper_index].id;
    for attack in 0..3 {
        let mut bad = module.clone();
        match attack {
            0 => {
                let caller = bad.functions.iter_mut().find(|f| f.locals.iter().any(|l| l.name == "right") && f.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i.kind, InstructionKind::Call { function, .. } if function == helper_id))).unwrap();
                let right = caller.locals.iter().find(|l| l.name == "right").unwrap().id;
                let replacement = caller.blocks.iter().flat_map(|b| &b.instructions).find_map(|i| matches!(i.kind, InstructionKind::Load(local) if local == right).then_some(i.result).flatten()).unwrap();
                let instruction = caller.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(i.kind, InstructionKind::Call { function, .. } if function == helper_id)).unwrap();
                let InstructionKind::Call { arguments, .. } = &mut instruction.kind else { unreachable!() };
                assert_eq!(caller.values[arguments[1].0 as usize], caller.values[replacement.0 as usize]);
                assert_ne!(arguments[1], replacement); arguments[1] = replacement;
            }
            1 => {
                let header = bad.functions[helper_index].storage_life.as_mut().unwrap();
                assert_eq!(header.fixed_binder_count, 2);
                let data = header.classes.iter_mut().find(|c| c.position.base == zil_ir::StorageBase::Parameter(1)).unwrap();
                assert_eq!(data.binder, StorageBinder::Fixed(0)); data.binder = StorageBinder::Fixed(1);
            }
            _ => {
                let life = bad.functions[helper_index].callable_contract.as_mut().unwrap().roles[0].life.as_mut().unwrap();
                life.classes[0].binder = StorageBinder::Fixed(1);
            }
        }
        let analyze = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(analyze.iter().all(|d| d.category != ResultCategory::ResourceLimit), "attack={attack}: {analyze:?}");
        let mut forged = plan.clone(); forged.module_fingerprint = bad.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&bad, &forged, &limits).unwrap();
        for result in [zil_verify::verify_module(bad.clone(), forged, &bad.target, &limits), zil_verify::verify_module_bytes(&wire, &bad.target, &limits)] {
            let errors = result.unwrap_err();
            assert!(errors.iter().all(|d| d.phase == Phase::Verify && d.category != ResultCategory::ResourceLimit && !d.message.to_ascii_lowercase().contains("fingerprint")), "attack={attack}: {errors:?}");
            eprintln!("review G3.3.4 Source-derived adversarial raw/Wire attack={attack}: {}", errors[0].message);
        }
    }
}

#[test]
fn review_g334_phantom_data_without_physical_life_slot_is_not_a_helper_bridge() {
    let text = include_str!("/workspace/.zil-env/review-G3.3.4-tmp/phantom-data-frontier.zil");
    let report = zil_driver::compile_source(&source(text), &zil_driver::CompileOptions::default());
    eprintln!("review G3.3.4 phantom actual Source report: {}", report.to_json());
    assert_eq!(report.status, ResultCategory::Unsupported, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn review_g334_phantom_unused_keeps_none_but_raw_some_needs_physical_slot() {
    let text = include_str!("/workspace/.zil-env/review-G3.3.4-tmp/phantom-data-frontier.zil");
    let unused = format!("{}fn main()->i32{{return 0}}", text.split("fn apply").next().unwrap());
    let mut phantom = core(&unused);
    assert!(phantom.functions.iter().flat_map(|f| &f.callable_parameters).all(|r| r.source_life.is_none()), "outside-profile unused phantom must retain old None lane");
    let report = zil_driver::compile_source(&source(&unused), &zil_driver::CompileOptions::default());
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    let real = core(SIMPLE);
    let role = real.functions.iter().flat_map(|f| &f.callable_parameters).find(|r| r.source_life.is_some()).unwrap();
    let target = phantom.functions.iter_mut().flat_map(|f| &mut f.callable_parameters).next().unwrap();
    // A raw candidate attaches an otherwise coherent Some inverse to the
    // genuine Phantom P/R and genuine phantom data header. No AST authority
    // or prior kernel plan is being synthesized here.
    target.source_life = role.source_life.clone(); target.life = role.life.clone();
    let errors = zil_types::check(&phantom, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(errors.iter().any(|d| d.category == ResultCategory::Unsupported && (d.message.contains("physical") || d.message.contains("stored") || d.message.contains("qualified"))), "{errors:?}");
    eprintln!("review G3.3.4 actual phantom Core with attached Some rejects: {errors:?}");
}

#[test]
fn review_g334_physical_local_wrong_class_remains_a_real_kernel_obligation() {
    let text = "struct Tag<life s>{p:&'s i64}\nfn helper<life s,F:Fn(&Tag<'s>)->i32>(f:&F,x:&Tag<'s>)->i32{return f(x)}\nfn apply<life a,life b>(seed:&Tag<'a>,other:&'b i64)->i32{let f=(p:&Tag<'a>)->i32=>i32(*p.p+*seed.p);let wrong:Tag<'b>=Tag::<'b>{p:other};return helper(&f,&wrong)}\nfn setup<life a,life b>(x:&'a i64,y:&'b i64)->i32{let seed:Tag<'a>=Tag::<'a>{p:x};return apply(&seed,y)}fn main()->i32{let x:i64=21;let y:i64=21;return setup(&x,&y)}";
    let wrong = zil_driver::compile_source(&source(text), &zil_driver::CompileOptions::default());
    assert_eq!(wrong.status, ResultCategory::SafeReject, "{}", wrong.to_json());
    assert_eq!(wrong.phase, Phase::Analyze, "{}", wrong.to_json());
    assert!(wrong.diagnostics.iter().any(|d| d.message.contains("class")), "{}", wrong.to_json());
    let good_text = text.replace("helper(&f,&wrong)", "helper(&f,seed)");
    let good = zil_driver::compile_source(&source(&good_text), &zil_driver::CompileOptions::default());
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    assert_eq!(good.verification_runs, 1);
}
