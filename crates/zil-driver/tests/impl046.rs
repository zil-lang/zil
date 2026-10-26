//! Actual source integration for related return-origin propagation. Solver
//! equivalence and deterministic work counts live in each solver's own tests.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{FunctionId, HoldRoot, InstructionKind, TypedModule};

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "frontier.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &parsed).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn check(text: &str) -> zil_driver::CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "frontier.zil", text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    )
}
fn chain(n: usize) -> String {
    let mut text = String::new();
    for i in 0..n {
        text += &format!("fn f{i}(a: &i32, b: &i32) -> &i32 {{ return ");
        text += &if i + 1 == n {
            "b".into()
        } else {
            format!("f{}(a, b)", i + 1)
        };
        text += "; }\n";
    }
    text
}

#[test]
fn source_reverse_chain_exports_exact_origins_and_roundtrips_wire() {
    let mut source = chain(96);
    source += "fn result()->i32 {var x=40;var y=2;let p=f0(&x,&y);x=100;return *p+x;}";
    let module = typed(&source);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    for function in &module.functions[..96] {
        let returned = match function.blocks.last().unwrap().terminator {
            zil_ir::Terminator::Return(Some(value)) => value,
            _ => panic!("expected actual source return"),
        };
        assert_eq!(
            plan.functions[function.id.0 as usize].holds[returned.0 as usize].roots,
            vec![HoldRoot::Parameter(1)]
        );
    }
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let verified = zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    assert_eq!(verified.plan(), &plan);
    assert_eq!(verified.module(), &module);
    assert_eq!(check(&source).status, ResultCategory::Success);
}

#[test]
fn recursive_parameter_permutation_reaches_all_grounded_roots() {
    let source = "fn a(x:&i32,y:&i32,n:i32)->&i32 {if n==0{return x;}return b(y,x,n-1);} fn b(x:&i32,y:&i32,n:i32)->&i32 {return a(x,y,n);} fn result()->i32 {let x=3;let y=7;return *a(&x,&y,4);}";
    let module = typed(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert_eq!(check(source).status, ResultCategory::Success);
    let call = module.functions[1]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result
        .unwrap();
    assert_eq!(
        plan.functions[1].holds[call.0 as usize].roots,
        vec![HoldRoot::Parameter(0), HoldRoot::Parameter(1)]
    );
    for source in [
        "fn a(x:&i32)->&i32{return b(x);}fn b(x:&i32)->&i32{return a(x);}fn result()->i32{return 1;}",
        "fn a(x:&i32,y:&i32,n:i32)->&i32 {if n==0{return x;}return a(y,x,n-1);}fn result()->i32{var x=1;var y=2;let p=a(&x,&y,3);y=8;return *p;}",
    ] {
        let report = check(source);
        assert_eq!(report.status, ResultCategory::SafeReject);
        assert_eq!(report.phase, Phase::Verify);
    }
}

#[test]
fn scalar_consumer_is_recomputed_when_address_callee_grows() {
    // result has no address return summary of its own, but its internal facts
    // must still consume the completed summary of the later identity function.
    let good = "fn result()->i32{var x=40;var y=2;let p=id(&x,&y);x=99;return *p+x;}fn id(a:&i32,b:&i32)->&i32{return b;}";
    assert_eq!(check(good).status, ResultCategory::Success);
    let bad = good.replace("x=99", "y=99");
    let report = check(&bad);
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
}

#[test]
fn forged_root_and_out_of_range_call_cannot_manufacture_authority() {
    let module =
        typed("fn f(a:&i32,b:&i32)->&i32{return g(a,b);}fn g(a:&i32,b:&i32)->&i32{return b;}");
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let call = module.functions[0].blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result
        .unwrap();
    plan.functions[0].holds[call.0 as usize].roots = vec![HoldRoot::Parameter(0)];
    let error =
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-HOLD");
    let mut changed = module.clone();
    for instruction in &mut changed.functions[0].blocks[0].instructions {
        if let InstructionKind::Call { function, .. } = &mut instruction.kind {
            *function = FunctionId(1000);
        }
    }
    assert!(zil_analyze::analyze(&changed, &limits).is_err());
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    plan.module_fingerprint = changed.fingerprint();
    assert!(zil_verify::verify_module(changed, plan, &module.target, &limits).is_err());
}

#[test]
fn budgets_still_fail_closed_and_scalar_only_programs_work() {
    let module = typed(&chain(32));
    let low = Limits {
        max_analysis_steps: 1,
        ..Limits::default()
    };
    assert_eq!(
        zil_analyze::analyze(&module, &low).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert_eq!(
        zil_verify::verify_module(module.clone(), plan, &module.target, &low).unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    assert_eq!(
        check("fn result()->i32{var n=0;for i in 0..9 {n+=i;}return n;}").status,
        ResultCategory::Success
    );
}

#[test]
fn unreachable_bad_call_is_not_a_summary_edge_but_is_still_structurally_rejected() {
    let mut module = typed("fn id(a:&i32)->&i32{return a;}");
    let function = &mut module.functions[0];
    let result = zil_ir::ValueId(function.values.len() as u32);
    function.values.push(function.return_type.clone());
    function.blocks.push(zil_ir::BasicBlock {
        id: zil_ir::BlockId(function.blocks.len() as u32),
        instructions: vec![zil_ir::Instruction {
            result: Some(result),
            kind: InstructionKind::Call {
                function: FunctionId(999),
                arguments: vec![zil_ir::ValueId(0)],
            },
            span: function.span,
        }],
        terminator: zil_ir::Terminator::Return(Some(result)),
    });
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(plan.functions[0].holds[result.0 as usize].roots.is_empty());
    let error = zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
        .unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-CALL");
}
