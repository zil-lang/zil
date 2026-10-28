//! Actual source instances retain data identity; raw/Wire claims cannot hide a wrong instance.
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
    json_string,
};
use zil_ir::{CandidatePlan, CoreModule, FunctionId, InstructionKind as I, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const EXTRA: &str = r#"
struct A{n:i32}struct B{n:i32}type Alias=A;
fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}
fn main()->i32{let f=move()=>1;let a=keep(&f,A{n:3});let b=keep(&f,B{n:5});
let again:Alias=keep(&f,A{n:7});let small=keep(&f,i32(11));let wide=keep(&f,i64(13));
return a.n+b.n+again.n+small+i32(wide)-39}
"#;

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "raw194-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, saved.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "raw194.zil", text)
}
fn baseline(text: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    let case = Case::new("baseline");
    fs::write(case.0.join("source.zil"), text).unwrap();
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(case.0.join("candidate.wire"), bytes).unwrap();
    (core, module, plan)
}
fn helper(module: &TypedModule, wanted: &Type) -> FunctionId {
    module
        .functions
        .iter()
        .find(|f| {
            f.callable_contract.is_some() && f.locals[f.parameters[1].0 as usize].ty == *wanted
        })
        .unwrap()
        .id
}
fn data_types(module: &TypedModule) -> Vec<Type> {
    module
        .functions
        .iter()
        .filter(|f| f.callable_contract.is_some())
        .map(|f| f.locals[f.parameters[1].0 as usize].ty.clone())
        .collect()
}
fn redirect(module: &mut TypedModule, from: FunctionId, to: FunctionId) {
    let mut count = 0;
    for function in &mut module.functions {
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::Call { function, .. } = &mut instruction.kind
                && *function == from
            {
                *function = to;
                count += 1;
            }
        }
    }
    assert!(count > 0);
}
fn rejected(
    name: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    code: &str,
    analyze_must_reject: bool,
) {
    // Type-invalid ordinary Copy calls need not be rejected by resource analysis;
    // its candidate remains untrusted. Record that phase separately and require
    // the independent verifier to reject concrete data identity/type forgeries.
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    if analyze_must_reject {
        let errors = analyzed.as_ref().unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject)
        );
    }
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    let results = [
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    let case = Case::new(name);
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(case.0.join("candidate.wire"), bytes).unwrap();
    fs::write(case.0.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    fs::write(case.0.join("verify.txt"), format!("{results:#?}")).unwrap();
    fs::write(case.0.join("result.json"), format!(
        "{{\"mutation\":{},\"analyze_must_reject\":{analyze_must_reject},\"analyze_accepted\":{},\"verify_executed\":2,\"expected_verify_code\":{}}}\n",
        json_string(name), analyzed.is_ok(), json_string(code))).unwrap();
    for errors in results {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{name}: {errors:?}"
        );
    }
}

#[test]
fn same_closure_has_four_data_instances_with_alias_and_repeated_type_reuse() {
    let (core, module, _) = baseline(EXTRA);
    let templates: Vec<_> = core
        .functions
        .iter()
        .filter(|f| !f.callable_parameters.is_empty())
        .collect();
    assert_eq!(templates.len(), 4);
    assert_eq!(
        templates
            .iter()
            .map(|f| f.id)
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    let helpers: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.callable_contract.is_some())
        .collect();
    assert_eq!(helpers.len(), 4);
    assert_eq!(
        helpers.iter().map(|f| f.id).collect::<BTreeSet<_>>().len(),
        4
    );
    assert_eq!(
        helpers
            .iter()
            .map(|f| f.callable_contract.as_ref().unwrap().roles[0].closure)
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
    let types = data_types(&module);
    assert!(types.contains(&Type::I32) && types.contains(&Type::I64));
    let records: Vec<_> = types
        .iter()
        .filter(|t| matches!(t, Type::Record { .. }))
        .collect();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0], records[1]);
    let (Type::Record { fields: first, .. }, Type::Record { fields: second, .. }) =
        (records[0], records[1])
    else {
        unreachable!()
    };
    assert_eq!(
        first, second,
        "same layout does not erase distinct nominal identity"
    );
}

#[test]
fn both_callable_arguments_and_result_are_concretized_before_types() {
    let text = "fn map<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn main()->i32{let a=move(n:i32)=>n+1;let b=move(n:i64)=>n+2;return map(&a,i32(2))+i32(map(&b,i64(2)))-7}";
    let (core, _, _) = baseline(text);
    let templates: Vec<_> = core
        .functions
        .iter()
        .filter(|f| !f.callable_parameters.is_empty())
        .collect();
    assert_eq!(templates.len(), 2);
    for f in templates {
        let bound = f.callable_parameters.first().unwrap();
        assert!(matches!(f.return_type, Type::I32 | Type::I64));
        assert_eq!(bound.parameters, vec![f.return_type.clone()]);
        assert_eq!(bound.result, f.return_type);
        assert_eq!(f.parameters[1].ty, f.return_type);
    }
}

#[test]
fn raw_and_wire_reject_same_environment_wrong_scalar_instance() {
    let (_, mut module, plan) = baseline(EXTRA);
    let from = helper(&module, &Type::I32);
    let to = helper(&module, &Type::I64);
    redirect(&mut module, from, to);
    rejected("wrong-width-instance", module, plan, "E-VERIFY-CALL", false);
}

#[test]
fn raw_and_wire_reject_same_layout_wrong_nominal_instance() {
    let (_, mut module, plan) = baseline(EXTRA);
    let records: Vec<_> = data_types(&module)
        .into_iter()
        .filter(|t| matches!(t, Type::Record { .. }))
        .collect();
    let from = helper(&module, &records[0]);
    let to = helper(&module, &records[1]);
    redirect(&mut module, from, to);
    rejected(
        "wrong-nominal-instance",
        module,
        plan,
        "E-VERIFY-CALL",
        false,
    );
}

#[test]
fn raw_and_wire_reject_forged_result_type_even_with_rebound_module_hash() {
    let (_, mut module, plan) = baseline(EXTRA);
    let target = helper(&module, &Type::I32);
    let mut changed = false;
    for f in &mut module.functions {
        for instruction in f.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(instruction.kind, I::Call { function, .. } if function == target) {
                f.values[instruction.result.unwrap().0 as usize] = Type::I64;
                changed = true;
            }
        }
    }
    assert!(changed);
    rejected("wrong-result-type", module, plan, "E-VERIFY-TYPE", false);
}

#[test]
fn each_data_instance_keeps_an_independently_checked_callback_contract() {
    let (_, module, plan) = baseline(EXTRA);
    for all in [false, true] {
        let mut forged = module.clone();
        let selected = helper(&forged, &Type::I64);
        for f in &mut forged.functions {
            if (all || f.id == selected)
                && let Some(contract) = &mut f.callable_contract
            {
                for trace in contract.traces.as_finite_mut().unwrap() {
                    trace.calls.clear();
                }
            }
        }
        rejected(
            if all {
                "all-instances-omit-call"
            } else {
                "one-instance-omits-call"
            },
            forged,
            plan.clone(),
            "E-VERIFY-CALLABLE-CONTRACT",
            true,
        );
    }
}

#[test]
fn new_instances_pay_work_nodes_depth_and_function_limits_without_budget_changes() {
    let input = source(EXTRA);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let full = zil_expand::expand_with_map_limits(&input, &ast, &Limits::default()).unwrap();
    let case = Case::new("budgets");
    fs::write(case.0.join("source.zil"), EXTRA).unwrap();
    let mut results = Vec::new();
    for dimension in ["work", "nodes", "depth", "functions"] {
        let probe = |value| {
            let mut limits = Limits::default();
            match dimension {
                "work" => limits.max_analysis_steps = value,
                "nodes" => limits.max_nodes = value,
                "depth" => limits.max_call_depth = value,
                _ => limits.max_functions = value,
            }
            zil_expand::expand_with_map_limits(&input, &ast, &limits)
        };
        let defaults = Limits::default();
        let mut high = match dimension {
            "work" => defaults.max_analysis_steps,
            "nodes" => defaults.max_nodes,
            "depth" => defaults.max_call_depth,
            _ => defaults.max_functions,
        };
        let mut low = 0;
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().any(|d| d.phase == Phase::Expand
                            && d.category == ResultCategory::ResourceLimit),
                        "{dimension} {middle}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(probe(high).unwrap().core, full.core);
        let errors = probe(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit)
        );
        results.push(format!(
            "{{\"phase\":\"expand\",\"dimension\":{},\"N\":{high},\"below\":{}}}",
            json_string(dimension),
            json_string(&format!("{errors:?}"))
        ));
    }
    let (_, module, plan) = baseline(EXTRA);
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let probe = |value| -> Result<(), Vec<Diagnostic>> {
            let limits = Limits {
                max_analysis_steps: value,
                ..Limits::default()
            };
            match phase {
                Phase::Types => zil_types::check(&full.core, &module.target, &limits).map(|_| ()),
                Phase::Analyze => zil_analyze::analyze(&module, &limits).map(|_| ()),
                _ => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                        .map(|_| ())
                }
            }
        };
        let mut high = Limits::default().max_analysis_steps;
        let mut low = 0;
        assert!(probe(high).is_ok());
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().any(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        probe(high).unwrap();
        let errors = probe(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
        );
        results.push(format!(
            "{{\"phase\":\"{phase:?}\",\"dimension\":\"work\",\"N\":{high},\"below\":{}}}",
            json_string(&format!("{errors:?}"))
        ));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]\n", results.join(",")),
    )
    .unwrap();
}

#[test]
fn full_copy_instance_keys_still_reject_cycles_and_bound_infinite_growth() {
    for (name, text, category) in [
        (
            "same-instance-cycle",
            "struct Cycle<T>{next:Cycle<T>}fn id<T:Copy>(x:T)->T=>x;fn inspect(x:Cycle<i32>)->i32{let y=id(x);return 0}fn main(){}",
            ResultCategory::SafeReject,
        ),
        (
            "growing-instance",
            "struct Grow<T>{next:Grow<(T,T)>}fn id<T:Copy>(x:T)->T=>x;fn inspect(x:Grow<i32>)->i32{let y=id(x);return 0}fn main(){}",
            ResultCategory::ResourceLimit,
        ),
    ] {
        let input = source(text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let limits = Limits {
            max_analysis_steps: 4096,
            max_call_depth: 20,
            ..Limits::default()
        };
        let errors = zil_expand::expand_with_map_limits(&input, &ast, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.category == category),
            "{name}: {errors:?}"
        );
        let case = Case::new(name);
        fs::write(case.0.join("source.zil"), text).unwrap();
        fs::write(case.0.join("limits.txt"), format!("{limits:#?}")).unwrap();
        fs::write(case.0.join("diagnostics.txt"), format!("{errors:#?}")).unwrap();
    }
}
