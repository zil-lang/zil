//! Actual source inputs distinguish callable type identity from runtime roles.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CallableKind, CallableTraces, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "roles196-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(
    case: &Case,
    files: &[(&str, &str)],
    optimization: Option<Optimization>,
) -> CompileReport {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                fs::write(case.0.join(format!("{name}.zil")), text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn accepted(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, &[("main", source)], None);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn native(label: &str, files: &[(&str, &str)], stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, files, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(case.0.join("stdout.log"), &output.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &output.stderr).unwrap();
        fs::write(case.0.join("native.txt"),format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n",output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}");
        assert!(output.stderr.is_empty());
    }
}
fn typed(source: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "roles196.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    module
}
const ORIGINALS: [(&str, &str); 12] = [
    (
        "00",
        include_str!("impl196_fixtures/00_one_role_control.zil"),
    ),
    (
        "01",
        include_str!("impl196_fixtures/01_same_factory_two_instances_direct.zil"),
    ),
    (
        "02",
        include_str!("impl196_fixtures/02_same_f_shared_distinct.zil"),
    ),
    (
        "03",
        include_str!("impl196_fixtures/03_same_f_shared_same_actual.zil"),
    ),
    (
        "04",
        include_str!("impl196_fixtures/04_same_f_mut_distinct.zil"),
    ),
    (
        "05",
        include_str!("impl196_fixtures/05_same_f_mut_same_actual.zil"),
    ),
    (
        "06",
        include_str!("impl196_fixtures/06_f_g_composition.zil"),
    ),
    (
        "07",
        include_str!("impl196_fixtures/07_f_g_mixed_modes.zil"),
    ),
    (
        "08",
        include_str!("impl196_fixtures/08_same_f_once_distinct.zil"),
    ),
    (
        "09",
        include_str!("impl196_fixtures/09_same_f_once_same_actual.zil"),
    ),
    (
        "10",
        include_str!("impl196_fixtures/10_nested_role_permutation.zil"),
    ),
    (
        "11",
        include_str!("impl196_fixtures/11_second_role_desc_alias.zil"),
    ),
];
#[test]
fn original_positive_probes_reach_verify_unchanged() {
    for (index, (label, source)) in ORIGINALS.iter().enumerate() {
        if ![5, 11].contains(&index) {
            accepted(label, source);
        }
    }
}
#[test]
fn original_exclusive_alias_and_second_role_write_probes_reach_security_checks() {
    for index in [5, 11] {
        let (label, source) = ORIGINALS[index];
        let case = Case::new(label);
        let report = compile(&case, &[("main", source)], None);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Analyze | Phase::Verify),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn original_positive_probes_execute_o0_o2() {
    for index in [0, 1, 2, 3, 4, 6, 7, 8, 9, 10] {
        let (label, source) = ORIGINALS[index];
        native(
            label,
            &[("main", source)],
            "",
            if index == 9 { 2 } else { 0 },
        );
    }
}
#[test]
fn child_static_calls_substitute_actual_parameter_roles() {
    let module = typed(ORIGINALS[10].1);
    let traces: Vec<_> = module
        .functions
        .iter()
        .filter_map(|f| f.callable_contract.as_ref())
        .filter_map(|c| c.traces.as_finite())
        .flat_map(|paths| paths.iter())
        .map(|p| p.calls.iter().map(|e| e.parameter).collect::<Vec<_>>())
        .collect();
    assert!(traces.contains(&vec![0, 1]));
    assert!(traces.contains(&vec![0, 1, 1, 0]), "{traces:?}");
    assert!(
        module
            .functions
            .iter()
            .filter_map(|f| f.callable_contract.as_ref())
            .all(|c| c.roles.len() == 2 && c.roles[0].closure == c.roles[1].closure)
    );
}
#[test]
fn branch_correlation_preserves_order_and_exclusive_paths() {
    for (body, wanted) in [
        (
            "if flag{return a()}else{return b()}",
            vec![vec![0], vec![1]],
        ),
        (
            "if flag{let x=a();return x+b()}else{let x=b();return x+a()}",
            vec![vec![0, 1], vec![1, 0]],
        ),
    ] {
        let source = format!(
            "fn make(n:i32)->impl Fn()->i32{{return move()=>n}}fn choose<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32{{{body}}}fn main()->i32{{let a=make(1);let b=make(2);return choose(&a,&b,true)}}"
        );
        let module = typed(&source);
        let contract = module
            .functions
            .iter()
            .find_map(|f| f.callable_contract.as_ref())
            .unwrap();
        let actual: Vec<_> = contract
            .traces
            .as_finite()
            .unwrap()
            .iter()
            .map(|p| p.calls.iter().map(|e| e.parameter).collect::<Vec<_>>())
            .collect();
        assert_eq!(actual, wanted);
    }
}
#[test]
fn declaration_order_is_independent_of_runtime_parameter_position() {
    let source = "fn combine<G:Fn(i32)->i32,F:Fn(i32)->i32>(unused:i32,first:&F,second:&G,n:i32)->i32{return second(first(n))}fn main()->i32{let f=move(x:i32)=>x+1;let g=move(x:i32)=>x*2;return combine(0,&f,&g,3)-8}";
    native("decl-order", &[("main", source)], "", 0);
    let module = typed(source);
    let c = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    assert_eq!(
        c.roles.iter().map(|r| r.parameter).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        c.traces.as_finite().unwrap()[0]
            .calls
            .iter()
            .map(|e| e.parameter)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_ne!(c.roles[0].closure, c.roles[1].closure);
}
#[test]
fn same_generic_requires_actual_type_identity_even_with_equal_signatures() {
    let source = "fn pair<F:Fn()->i32>(a:&F,b:&F)->i32{return a()+b()}fn main()->i32{let a=move()=>1;let b=move()=>2;return pair(&a,&b)}";
    let case = Case::new("same-f-wrong-id");
    let report = compile(&case, &[("main", source)], None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TYPE-MISMATCH")
    );
}
#[test]
fn same_f_and_f_g_keep_the_copy_data_specialization() {
    let files = [
        (
            "main",
            r#"import helpers;fn make()->impl Fn(i32)->i32{return move(x:i32)=>x+1}fn main()->i32{let a=make();let b=make();let data:i64=7;let out=helpers.same(&a,&b,data);let g=move(x:i32)=>x+2;let other:i32=5;let again=helpers.mixed(&a,&g,other);if out!=data{return 1};return again-other}"#,
        ),
        (
            "helpers",
            r#"pub fn same<T:Copy,F:Fn(i32)->i32>(a:&F,b:&F,value:T)->T{a(1);b(2);return value}pub fn mixed<G:Fn(i32)->i32,T:Copy,F:Fn(i32)->i32>(a:&F,b:&G,value:T)->T{a(1);b(2);return value}"#,
        ),
    ];
    native("copy-data", &files, "", 0);
}
#[test]
fn multi_role_runtime_loop_has_joint_flow() {
    let source = "fn make(n:i32)->impl FnMut()->i32{var value=n;return move()=>{value+=1;return value}}fn run<F:FnMut()->i32>(a:&mut F,b:&mut F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=a()*10+b();i+=1};return sum}fn main()->i32{var a=make(1);var b=make(5);return run(&mut a,&mut b,3)-111}";
    native("runtime-flow", &[("main", source)], "", 0);
    let module = typed(source);
    let c = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    let CallableTraces::Flow(flow) = &c.traces else {
        panic!("expected actual cyclic graph")
    };
    assert!(
        flow.nodes
            .iter()
            .any(|node| node.calls.iter().map(|e| e.parameter).collect::<Vec<_>>() == vec![0, 1])
    );
    assert!(c.roles.iter().all(|r| r.kind == CallableKind::FnMut));
}
#[test]
fn nested_helper_second_role_writes_the_actual_owned_environment() {
    native(
        "nested-owned-write",
        &[(
            "main",
            include_str!("impl196_fixtures/12_nested_second_role_owned_write.zil"),
        )],
        "",
        0,
    );
}
#[test]
fn nested_helper_keeps_actual_identity_signature_and_permission_checks() {
    let cases = [
        (
            "nested-same-f-different-id",
            "fn pair<F:Fn()->i32>(a:&F,b:&F)->i32{return a()+b()}fn make()->impl Fn()->i32{return move()=>{let a=move()=>1;let b=move()=>2;return pair(&a,&b)}}fn main()->i32{let f=make();return f()}",
        ),
        (
            "nested-second-signature",
            "fn pair<F:Fn()->i32,G:Fn()->i32>(a:&F,b:&G)->i32{return a()+b()}fn make()->impl Fn()->i32{return move()=>{let a=move()=>1;let b=move()=>true;return pair(&a,&b)}}fn main()->i32{let f=make();return f()}",
        ),
        (
            "nested-second-permission",
            "fn pair<F:Fn()->i32,G:FnMut()->i32>(a:&F,b:&mut G)->i32{return a()+b()}fn make()->impl Fn()->i32{return move()=>{let a=move()=>1;var count=0;var b=()=>{count+=1;return count};return pair(&a,&b)}}fn main()->i32{let f=make();return f()}",
        ),
    ];
    for (label, source) in cases {
        let case = Case::new(label);
        let report = compile(&case, &[("main", source)], None);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types);
    }
}
#[test]
fn each_owned_role_cleans_its_own_uncalled_environment() {
    let source = r#"extern "C" fn putchar(n:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn consume(give r:R)->i32{return r.code}fn make(give r:R)->impl FnOnce()->i32{return move()=>consume(give r)}fn choose<F:FnOnce()->i32>(a:F,b:F,flag:bool)->i32{if flag{return a()};return b()}fn main()->i32{let a=make(R{code:65});let b=make(R{code:66});return choose(a,b,true)-65}"#;
    native("owned-choice", &[("main", source)], "AB", 0);
}
#[test]
fn cyclic_helper_with_any_once_role_stays_unsupported() {
    let source = "fn run<F:FnOnce()->i32,G:Fn()->i32>(once:F,repeat:&G,n:i32)->i32{var i=0;while i<n{repeat();i+=1};return 0}fn main()->i32{let f=move()=>1;let g=move()=>2;return run(f,&g,0)}";
    let case = Case::new("cyclic-once");
    let report = compile(&case, &[("main", source)], None);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cyclic FnOnce"))
    );
}
#[test]
fn unused_bad_templates_and_extra_roles_do_not_gain_capabilities() {
    for source in [
        "fn bad<T:Copy,F:Fn()->i32,G:Fn()->i32>(a:&F,b:&G,value:T)->T{return value+value}fn main()->i32{return 0}",
        "fn bad<F:Fn()->i32>(a:&F,b:&F,c:&F)->i32{return a()+b()+c()}fn main()->i32{return 0}",
        "fn bad<F:Fn()->i32>(a:F,b:&F)->i32{return a()+b()}fn main()->i32{return 0}",
    ] {
        let case = Case::new("closed");
        let report = compile(&case, &[("main", source)], None);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn actual_non_copy_once_environment_cannot_fill_two_owned_roles() {
    let source = "resource R{n:i32}fn consume(give r:R)->i32{return r.n}fn make(give r:R)->impl FnOnce()->i32{return move()=>consume(give r)}fn pair<F:FnOnce()->i32>(a:F,b:F)->i32{return a()+b()}fn main()->i32{let f=make(R{n:3});return pair(f,f)}";
    let case = Case::new("owned-same-actual");
    let report = compile(&case, &[("main", source)], None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
