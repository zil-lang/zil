//! Real mixed State instances retain data identities even for phantom parameters.
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
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:Fn()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn run<T:Copy,G:Fn()->i32>(state:State<T,G>)->T{state.callback();return state.value}
fn factory()->impl Fn()->i32{return move()=>1}
fn main()->i32{let a=run(make(factory(),A{n:3}));let b=run(make(factory(),B{n:5}));
let again:Alias=run(make(factory(),A{n:7}));let small=run(make(factory(),i32(11)));let wide=run(make(factory(),i64(13)));
return a.n+b.n+again.n+small+i32(wide)-39}
"#;
const PHANTOM: &str = r#"
struct A{n:i32}struct B{n:i32}type Alias=A;
package State<T,F>{manages bytes:ByteBuffer;callback:F}
fn make<T:Copy,F:Fn()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback}}
fn run<T:Copy,G:Fn()->i32>(state:State<T,G>)->i32{return state.callback()}
fn factory()->impl Fn()->i32{return move()=>1}
fn main()->i32{let a=run(make(factory(),A{n:3}));let b=run(make(factory(),B{n:5}));
let same:Alias=A{n:7};let again=run(make(factory(),same));let small=run(make(factory(),i32(11)));let wide=run(make(factory(),i64(13)));
return a+b+again+small+wide-5}
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
            "raw197-{label}-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "raw197.zil", text)
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

fn runs(module: &TypedModule) -> Vec<&zil_ir::TypedFunction> {
    module
        .functions
        .iter()
        .filter(|f| f.name.contains("$generic_run_"))
        .collect()
}
fn state_type(function: &zil_ir::TypedFunction) -> &Type {
    &function.locals[function.parameters[0].0 as usize].ty
}
fn data_type(function: &zil_ir::TypedFunction) -> &Type {
    let Type::Package { fields, .. } = state_type(function) else {
        panic!("actual State required")
    };
    &fields.iter().find(|(name, _)| name == "value").unwrap().1
}
fn redirect(module: &mut TypedModule, from: FunctionId, to: FunctionId) {
    let mut count = 0;
    for f in &mut module.functions {
        for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::Call { function, .. } = &mut instruction.kind
                && *function == from
            {
                *function = to;
                count += 1;
            }
        }
    }
    assert!(count > 0, "must mutate an actual static source call");
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
fn complete_state_data_families_are_distinct_and_aliases_reuse_one_instance() {
    let (_, module, _) = baseline(EXTRA);
    let helpers = runs(&module);
    assert_eq!(helpers.len(), 4, "{helpers:#?}");
    assert_eq!(
        helpers
            .iter()
            .map(|f| state_type(f))
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    let ids = helpers
        .iter()
        .map(|f| {
            let Type::Package { fields, .. } = state_type(f) else {
                unreachable!()
            };
            let Type::Closure(id) = fields
                .iter()
                .find(|(name, _)| name == "callback")
                .unwrap()
                .1
            else {
                panic!("actual callback")
            };
            id
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        1,
        "same factory means same concrete ClosureId, not same T"
    );
    let records = helpers
        .iter()
        .map(|f| data_type(f))
        .filter(|t| matches!(t, Type::Record { .. }))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0], records[1]);
    let (Type::Record { fields: a, .. }, Type::Record { fields: b, .. }) = (records[0], records[1])
    else {
        unreachable!()
    };
    assert_eq!(
        a, b,
        "same layout still needs different nominal data identity"
    );
}

#[test]
fn raw_and_wire_reject_same_callback_wrong_scalar_state_instance() {
    let (_, mut module, plan) = baseline(EXTRA);
    let ids = runs(&module)
        .iter()
        .filter_map(|f| matches!(data_type(f), Type::I32 | Type::I64).then_some(f.id))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    redirect(&mut module, ids[0], ids[1]);
    rejected("wrong-scalar-state", module, plan, "E-VERIFY-CALL", false);
}

#[test]
fn raw_and_wire_reject_same_layout_nominal_state_instance() {
    let (_, mut module, plan) = baseline(EXTRA);
    let ids = runs(&module)
        .iter()
        .filter_map(|f| matches!(data_type(f), Type::Record { .. }).then_some(f.id))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    redirect(&mut module, ids[0], ids[1]);
    rejected("wrong-record-state", module, plan, "E-VERIFY-CALL", false);
}

#[test]
fn phantom_data_has_distinct_actual_nominal_types_and_retargeting_is_rejected() {
    let (_, mut module, plan) = baseline(PHANTOM);
    let helpers = runs(&module);
    assert_eq!(helpers.len(), 4);
    let states = helpers.iter().map(|f| state_type(f)).collect::<Vec<_>>();
    assert_eq!(states.iter().collect::<BTreeSet<_>>().len(), 4);
    let fields = states
        .iter()
        .map(|t| {
            let Type::Package { fields, .. } = t else {
                unreachable!()
            };
            fields
        })
        .collect::<Vec<_>>();
    assert!(
        fields.windows(2).all(|pair| pair[0] == pair[1]),
        "all runtime layouts and actual closure IDs are equal"
    );
    let ids = helpers.iter().map(|f| f.id).collect::<Vec<_>>();
    redirect(&mut module, ids[0], ids[1]);
    rejected("wrong-phantom-state", module, plan, "E-VERIFY-CALL", false);
}

#[test]
fn raw_and_wire_reject_actual_factory_returning_a_different_data_state() {
    let (_, mut module, plan) = baseline(EXTRA);
    let factories = module
        .functions
        .iter()
        .filter(|f| f.name.contains("$generic_make_"))
        .collect::<Vec<_>>();
    let first=factories.iter().find(|f|matches!(&f.return_type,Type::Package{fields,..}if fields.iter().any(|(n,t)|n=="value"&&*t==Type::I32))).unwrap().id;
    let second=factories.iter().find(|f|matches!(&f.return_type,Type::Package{fields,..}if fields.iter().any(|(n,t)|n=="value"&&*t==Type::I64))).unwrap().id;
    redirect(&mut module, first, second);
    rejected("wrong-factory-state", module, plan, "E-VERIFY-CALL", false);
}
#[test]
fn mixed_state_family_keys_pay_work_nodes_depth_and_functions_with_fresh_retry() {
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
