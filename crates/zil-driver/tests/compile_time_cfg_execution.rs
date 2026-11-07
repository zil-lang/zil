//! Real source -> pure CT CFG -> independent verification -> VM -> native tests.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CtProgram, InstructionKind, Type};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-ct-aggregate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "ct-program.zil", text)
}
fn run(path: &Path, expected: i64) {
    let out = Command::new(path).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        expected.to_string()
    );
    assert!(out.stderr.is_empty());
}
fn native(text: &str, expected: i64) {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let core = fs::read_to_string(&report.artifacts["ct-core"]).unwrap();
    assert!(!core.contains("CompileTime"), "CT did not finish: {core}");
    run(&report.artifacts["executable"], expected);
    let optimized = temp.0.join("optimized");
    let out = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    run(&optimized, expected);
}
fn rejection(text: &str, options: CompileOptions, category: ResultCategory, code: &str) {
    let report = compile_source(&source(text), &options);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}

fn raw(text: &str) -> CtProgram {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    CtProgram {
        entry: module
            .functions
            .iter()
            .find(|f| f.name == "result")
            .unwrap()
            .id,
        module,
        plan,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_values + limits.max_locals,
    }
}
#[test]
fn aggregate_return_materializes_records_arrays_and_tuples() {
    native(include_str!("../../../examples/impl034/main.zil"), 345045);
}
#[test]
fn nested_aggregates_functions_generic_instances_and_empty_data() {
    native(
        "struct Empty {} struct Boxed { x: (bool, [i64; 2]); empty: Empty; unit: (); } fn identity<T>(x: T) -> T { return x } fn edit(value: Boxed) -> Boxed { var changed=value; changed.x.1[1]=9; return identity(changed) } fn result()->i64 { let value=#(edit(Boxed { x: (true, [5000000000,7]),empty:Empty{},unit:() })); let a:[i32;0]=#([]); if value.x.0 {return value.x.1[0]+value.x.1[1]} return 0; }",
        5000000009,
    );
    native(
        "fn result()->i32 { let tuple=#((true,[3,7])); let x:i32=#([5,9][1]); if tuple.0 { return x } return 0; }",
        9,
    );
}
#[test]
fn ct_bounds_and_runtime_names_are_diagnosed_before_native() {
    for index in ["-1", "2"] {
        rejection(
            &format!("fn result()->i32 {{ return #([4,7][{index}]) }}"),
            CompileOptions::default(),
            ResultCategory::SafeReject,
            "E-CT-BOUNDS",
        );
    }
    rejection(
        "fn result()->i32 { let runtime=3; let xs=#([runtime,2]);return xs[0]; }",
        CompileOptions::default(),
        ResultCategory::SafeReject,
        "E-CT-RUNTIME",
    );
}
#[test]
fn aggregate_wire_shape_copy_steps_and_expanded_cell_quotas() {
    let raw = raw("fn result()->[i64;3] { var x:[i64;3]=[5,7,9]; x[1]=8;return x; }");
    let limits = Limits::default();
    let target = TargetProfile::default();
    let bytes = zil_ir::wire::encode_ct_program(&raw, &limits).unwrap();
    let verified = zil_verify::verify_ct_program_bytes(&bytes, &target, &limits).unwrap();
    let value = zil_ct::evaluate_program(&verified).unwrap();
    assert_eq!(
        value.value,
        zil_ct::CompileTimeValue::Aggregate(
            [5, 8, 9]
                .into_iter()
                .map(|v| zil_ct::CompileTimeValue::Scalar(zil_ir::Constant::I64(v)))
                .collect()
        )
    );
    let mut exact = raw.clone();
    exact.max_steps = value.steps;
    exact.max_live_slots = value.peak_live_slots;
    assert_eq!(
        zil_ct::evaluate_program(
            &zil_verify::verify_ct_program(exact.clone(), &target, &limits).unwrap()
        )
        .unwrap(),
        value
    );
    exact.max_steps -= 1;
    assert_eq!(
        zil_ct::evaluate_program(&zil_verify::verify_ct_program(exact, &target, &limits).unwrap())
            .unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    let mut short = raw.clone();
    short.max_live_slots = value.peak_live_slots - 1;
    assert_eq!(
        zil_ct::evaluate_program(&zil_verify::verify_ct_program(short, &target, &limits).unwrap())
            .unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
    let mut forged = raw;
    let f = &mut forged.module.functions[0];
    let value = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::MakeAggregate { .. }))
        .unwrap()
        .result
        .unwrap();
    f.values[value.0 as usize] = Type::Array {
        element: Box::new(Type::Bool),
        length: 3,
    };
    forged.plan.module_fingerprint = forged.module.fingerprint();
    assert!(zil_verify::verify_ct_program(forged, &target, &limits).is_err());
}

#[test]
fn aggregate_argument_context_keeps_i64_literals_and_record_names() {
    native(
        "fn first(xs:[i64;2])->i64{return xs[0]} fn result()->i64 {return first(#([5000000000,7]))}",
        5000000000,
    );
    native(
        "struct Pair { a:i64; b:i64 } fn sum(p:Pair)->i64{return p.a+p.b} fn result()->i64{return sum(#(Pair{a:5000000000,b:7}))}",
        5000000007,
    );
}

#[test]
fn review_inferred_ct_results_preserve_zero_length_array_element_types() {
    let cases = [
        "fn empty()->[i64;0]=>[];fn result()->i64{let a=#(empty());return 7}",
        "fn empty()->([i64;0],bool)=>([],true);fn result()->i64{let p=#(empty());if p.1{return 7}return 0}",
    ];
    let reports: Vec<_> = cases
        .iter()
        .map(|text| compile_source(&source(text), &CompileOptions::default()))
        .collect();
    assert!(
        reports.iter().all(|r| r.status == ResultCategory::Success),
        "{:?}",
        reports.iter().map(|r| r.to_json()).collect::<Vec<_>>()
    );
    for text in cases {
        native(text, 7)
    }
}
#[test]
fn review_cross_module_record_result_keeps_declaring_nominal_identity() {
    use zil_driver::{ProjectSources, compile_project};
    let make = |ct: bool, public_field: bool| ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                if ct {
                    "import model;fn result()->i64{let p=#(model.make());return p.x}"
                } else {
                    "import model;fn result()->i64{let p=model.make();return p.x}"
                },
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "model.zil",
                if public_field {
                    "pub struct Point{pub x:i64;} pub fn make()->Point=>Point{x:5000000000}"
                } else {
                    "pub struct Point{x:i64;} pub fn make()->Point=>Point{x:5000000000}"
                },
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["model".into()], ModuleId(1))]),
    };
    let ordinary = compile_project(&make(false, true), &CompileOptions::default());
    assert_eq!(
        ordinary.status,
        ResultCategory::Success,
        "{}",
        ordinary.to_json()
    );
    let compile_time = compile_project(&make(true, true), &CompileOptions::default());
    assert_eq!(
        compile_time.status,
        ResultCategory::Success,
        "{}",
        compile_time.to_json()
    );
    for ct in [false, true] {
        let private = compile_project(&make(ct, false), &CompileOptions::default());
        assert_eq!(
            private.status,
            ResultCategory::SafeReject,
            "{}",
            private.to_json()
        );
        assert_eq!(private.phase, Phase::Types, "{}", private.to_json());
        assert_eq!(
            private.diagnostics[0].code,
            "E-FIELD-PRIVATE",
            "{}",
            private.to_json()
        );
        assert!(!private.artifacts.contains_key("executable"));
    }
}
#[test]
fn review_empty_unit_nested_projection_and_signed_limits_execute() {
    native(
        "struct Z{unit:();xs:[();0];} fn value()->(Z,[i64;2])=>(Z{unit:(),xs:[]},[-9223372036854775808,9223372036854775807]);fn result()->i64{let p=#(value());let unit=p.0.unit;return p.1[0]+p.1[1]}",
        -1,
    );
    for text in [
        "fn result()->i32{let run=1;let x=#(([2,3],run));return x.0[0]}",
        "fn result()->i32{let run=1;return #([2,3][run])}",
        "struct P{x:i32;} fn result()->i32{let run=1;let p=#(P{x:run});return p.x}",
    ] {
        rejection(
            text,
            CompileOptions::default(),
            ResultCategory::SafeReject,
            "E-CT-RUNTIME",
        );
    }
}
#[test]
fn review_expanded_return_and_call_frames_enforce_exact_quotas() {
    let raw = raw(
        "fn copy(xs:[(i64,bool);2])->[(i64,bool);2]=>xs;fn result()->[(i64,bool);2]{var xs:[(i64,bool);2]=[(1,true),(2,false)];for i in 0..3{xs=copy(xs)}return xs}",
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let checked = zil_verify::verify_ct_program(raw.clone(), &target, &limits).unwrap();
    let observed = zil_ct::evaluate_program(&checked).unwrap();
    assert_eq!(observed.peak_call_depth, 2);
    for shortage in [false, true] {
        let mut exact = raw.clone();
        exact.max_steps = observed.steps;
        exact.max_live_slots = observed.peak_live_slots - usize::from(shortage);
        let checked = zil_verify::verify_ct_program(exact, &target, &limits).unwrap();
        if shortage {
            assert_eq!(
                zil_ct::evaluate_program(&checked).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            )
        } else {
            assert_eq!(zil_ct::evaluate_program(&checked).unwrap(), observed)
        }
    }
}

#[test]
fn review_materialized_children_receive_fresh_noncolliding_node_ids() {
    fn ids(core: &str) -> Vec<u32> {
        core.split("id: NodeId(")
            .skip(1)
            .map(|part| {
                part.trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap()
            })
            .collect()
    }
    let temp = Temp::new();
    let report = compile_source(
        &source(
            "struct P{x:[i64;2];} fn make()->P=>P{x:[1,2]};fn result()->i64{let a=#(make());let b:(bool,[i64;2])=#((true,[3,4]));let c:[i64;2]=#([5,6]);return a.x[0]+b.1[0]+c[0]}",
        ),
        &CompileOptions {
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let original = ids(&fs::read_to_string(&report.artifacts["core"]).unwrap());
    let output = ids(&fs::read_to_string(&report.artifacts["ct-core"]).unwrap());
    let original_max = *original.iter().max().unwrap();
    let mut unique = std::collections::BTreeSet::new();
    for id in &output {
        assert!(unique.insert(*id), "duplicate output node {id}")
    }
    let fresh: Vec<_> = output
        .iter()
        .copied()
        .filter(|id| !original.contains(id))
        .collect();
    assert!(fresh.len() >= 8, "{fresh:?}");
    assert!(fresh.iter().all(|id| *id > original_max), "{fresh:?}");
}

#[test]
fn raw_ascribed_expression_does_not_grant_type_or_shape_authority() {
    let input = source("fn result()->bool{return true}");
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let mut core = zil_expand::expand(&input, &ast).unwrap();
    let zil_ir::CoreStatementKind::Return(Some(value)) =
        &mut core.functions[0].body.statements[0].kind
    else {
        panic!()
    };
    let old = value.clone();
    value.kind = zil_ir::CoreExprKind::Ascribed {
        ty: Type::I64,
        value: Box::new(old),
    };
    let limits = Limits::default();
    let target = TargetProfile::default();
    assert_eq!(
        zil_types::check(&core, &target, &limits).unwrap_err()[0].code,
        "E-TYPE-MISMATCH"
    );
    let zil_ir::CoreStatementKind::Return(Some(value)) =
        &mut core.functions[0].body.statements[0].kind
    else {
        panic!()
    };
    let zil_ir::CoreExprKind::Ascribed { ty, .. } = &mut value.kind else {
        panic!()
    };
    *ty = (0..20).fold(Type::I64, |inner, _| Type::Tuple(vec![inner]));
    let limits = Limits {
        max_call_depth: 5,
        ..Limits::default()
    };
    assert_eq!(
        zil_types::check(&core, &target, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
