//! Copy data specialization preserves one actual callback environment and its obligations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "copy194-{label}-{}-{}",
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
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
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
    }
}
fn compile(
    case: &Case,
    files: &[(&str, &str)],
    optimization: Option<Optimization>,
) -> CompileReport {
    for (name, text) in files {
        fs::write(case.0.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
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
fn accepted(label: &str, text: &str) {
    let case = Case::new(label);
    let report = compile(&case, &[("main", text)], None);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn rejected(label: &str, text: &str, phase: Phase, status: ResultCategory, message: &str) {
    let case = Case::new(label);
    let report = compile(&case, &[("main", text)], None);
    assert_eq!(report.status, status, "{label}: {}", report.to_json());
    assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
    if phase == Phase::Expand {
        assert_eq!(report.verification_runs, 0);
    }
}
fn native(label: &str, files: &[(&str, &str)], stdout: &str) {
    native_outcome(label, files, stdout, 0)
}
fn native_outcome(label: &str, files: &[(&str, &str)], stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, files, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let executable = &report.artifacts["executable"];
        let result = Command::new(executable).output().unwrap();
        fs::write(case.0.join("stdout.log"), &result.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &result.stderr).unwrap();
        fs::write(case.0.join("native.json"), format!(
            "{{\"command\":[{}],\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout\":{},\"expected_stdout\":{}}}\n",
            json_string(&executable.to_string_lossy()), result.status.code().map_or("null".into(), |n| n.to_string()),
            json_string(&String::from_utf8_lossy(&result.stdout)), json_string(stdout))).unwrap();
        assert_eq!(result.status.code(), Some(exit), "{label}: {result:?}");
        assert_eq!(result.stdout, stdout.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty(), "{label}: {result:?}");
    }
}

const DROP: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
"#;

#[test]
fn original_audit_map_and_reordered_parameters_reach_native() {
    native(
        "original-map",
        &[(
            "main",
            "fn apply<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}\n",
        )],
        "",
    );
    native(
        "reordered-map",
        &[(
            "main",
            "fn apply<F:Fn(T)->T,T:Copy>(f:&F,x:T)->T=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}",
        )],
        "",
    );
}

#[test]
fn ordinary_copy_bound_duplicates_only_its_declared_structural_values() {
    native(
        "ordinary-copy",
        &[(
            "main",
            "fn duplicate<T:Copy>(x:T)->(T,T)=>(x,x);fn main()->i32{let p=duplicate((7,true));return p.0.0+p.1.0-14}",
        )],
        "",
    );
    native(
        "symbolic-record-copy",
        &[(
            "main",
            "struct Box<T>{value:T}fn twice<T:Copy>(x:Box<T>)->(Box<T>,Box<T>)=>(x,x);fn main()->i32{let p=twice(Box::<i32>{value:7});return p.0.value+p.1.value-14}",
        )],
        "",
    );
    accepted("unit-copy", "fn id<T:Copy>(x:T)->T=>x;fn main(){id(())}");
}

#[test]
fn finite_nested_same_nominal_is_not_a_recursive_layout() {
    for (label, declaration) in [
        ("nested-box-control", "fn id<T>(x:T)->T=>x;"),
        ("nested-box-copy", "fn id<T:Copy>(x:T)->T=>x;"),
    ] {
        let text = format!(
            "struct Box<T>{{value:T}}{declaration}fn main()->i32{{let x=Box::<Box<i32>>{{value:Box::<i32>{{value:7}}}};let y=id(x);return y.value.value-7}}"
        );
        native(label, &[("main", &text)], "");
    }
    native(
        "nested-box-duplicate",
        &[(
            "main",
            "struct Box<T>{value:T}fn duplicate<T:Copy>(x:T)->(T,T)=>(x,x);fn main()->i32{let x=Box::<Box<i32>>{value:Box::<i32>{value:7}};let y=duplicate(x);return y.0.value.value+y.1.value.value-14}",
        )],
        "",
    );
    native(
        "nested-box-callable-extra",
        &[(
            "main",
            "struct Box<T>{value:T}fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}fn main()->i32{let f=move()=>1;let x=Box::<Box<i32>>{value:Box::<i32>{value:7}};let y=keep(&f,x);return y.value.value-7}",
        )],
        "",
    );
}

#[test]
fn copy_nested_instance_fix_does_not_change_the_store_element_profile() {
    rejected(
        "store-nominal-rule-unchanged",
        "struct Box<T>{value:T}resource R{code:i32}package Item{manages r:R;data:Box<Box<i32>>}fn size<T:StoreElement>(x:T)->i32=>0;fn main()->i32{let x=Item{r:R{code:65},data:Box::<Box<i32>>{value:Box::<i32>{value:7}}};return size(x)}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "StoreElement requires a finite nonrecursive layout",
    );
}

#[test]
fn same_closure_extra_data_does_not_erase_width_or_nominal_identity() {
    native(
        "same-env-multiple-data",
        &[(
            "main",
            r#"
struct A{n:i32}struct B{n:i32}type Alias=A;
fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}
fn main()->i32{let f=move()=>1;let a=keep(&f,A{n:3});let b=keep(&f,B{n:5});
let again:Alias=keep(&f,A{n:7});let small=keep(&f,i32(11));let wide=keep(&f,i64(13));
return a.n+b.n+again.n+small+i32(wide)-39}
"#,
        )],
        "",
    );
}

#[test]
fn record_enum_tuple_array_error_and_function_pointer_are_real_copy_shapes() {
    let cases = [
        (
            "record",
            "struct R{n:i64}fn main()->i32{let f=move(x:R)=>R{n:x.n+1};return i32(map(&f,R{n:6}).n)-7}",
        ),
        (
            "enum",
            "enum E{A(i32),B}fn main()->i32{let f=move(x:E)=>x;let v=map(&f,E.A(7));match v{E.A(n)=>{return n-7},E.B()=>{return 1}}}",
        ),
        (
            "tuple",
            "fn main()->i32{let f=move(x:(i32,bool))=>x;let v=map(&f,(7,true));return v.0-7}",
        ),
        (
            "array",
            "fn main()->i32{let f=move(x:[i32;2])=>x;let v=map(&f,[3,4]);return v[0]+v[1]-7}",
        ),
        (
            "error-value",
            "fn success()->bool!i32=>7;fn main()->bool!i32{let f=move(x:bool!i32)=>x;let v=map(&f,success());return v?-7}",
        ),
        (
            "function-pointer",
            "fn plus(x:i32)->i32=>x+1;fn main()->i32{let p:fn(i32)->i32=plus;let f=move(x:fn(i32)->i32)=>x;let v=map(&f,p);return v(6)-7}",
        ),
    ];
    for (label, body) in cases {
        let source = format!("fn map<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);{body}");
        native(label, &[("main", &source)], "");
    }
}

#[test]
fn nested_symbolic_copy_constraints_and_complete_callable_bound_are_substituted() {
    native(
        "nested-renamed-types",
        &[(
            "main",
            r#"
fn leaf<U:Copy,G:Fn(U)->U>(g:&G,x:U)->U=>g(x)
fn outer<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>leaf(f,x)
fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,6)-7}
"#,
        )],
        "",
    );
    rejected(
        "nested-mismatch",
        "fn leaf<U:Copy,G:Fn(U)->U>(g:&G,x:U)->U=>g(x);fn bad<T:Copy,F:Fn(i32)->i32>(f:&F,x:T)->T=>leaf(f,x);fn main(){}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "type",
    );
    rejected(
        "nested-unproven-copy",
        "fn id<U:Copy>(x:U)->U=>x;fn bad<T>(x:T)->T=>id(x);fn main(){}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "Copy",
    );
}

#[test]
fn explicit_result_context_can_select_copy_data_without_guessing_from_closure_layout() {
    native(
        "result-context",
        &[(
            "main",
            "fn read<T:Copy,F:Fn()->T>(f:&F)->T=>f();fn main()->i32{let f=move()=>7;let n:i32=read(&f);return n-7}",
        )],
        "",
    );
    rejected(
        "missing-data-constraint",
        "fn read<T:Copy,F:Fn()->T>(f:&F)->T=>f();fn main(){let f=move()=>7;let n=read(&f)}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "no unique constraint",
    );
}

#[test]
fn same_mode_finite_and_flow_nesting_preserves_copy_loop_state() {
    native(
        "copy-flow",
        &[(
            "main",
            r#"
fn leaf<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x)
fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,n:i32)->T{var out=x;var i=n;while i>0{out=leaf(f,out);i-=1}return out}
fn main()->i32{let f=move(n:i32)=>n+1;return run(&f,1,0)+run(&f,2,1)+run(&f,3,7)-14}
"#,
        )],
        "",
    );
    native(
        "copy-for",
        &[(
            "main",
            r#"
fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,n:i32)->T{var out=x;for i in 0..n{out=f(out)}return out}
fn main()->i32{let f=move(n:i32)=>n+1;return run(&f,3,4)-7}
"#,
        )],
        "",
    );
}

#[test]
fn mutable_and_consuming_environments_keep_their_real_cleanup() {
    let mutable = format!(
        "{DROP}fn apply<T:Copy,F:FnMut(T)->T>(f:&mut F,x:T)->T=>f(x);fn main()->i32{{let r=R{{code:65}};var total=0;var f=move(n:i32)=>{{r.code;total+=n;return total}};return apply(&mut f,3)+apply(&mut f,4)-10}}"
    );
    native("fnmut-owned-capture", &[("main", &mutable)], "A");
    let once = format!(
        "{DROP}fn take(give r:R){{}}fn apply<T:Copy,F:FnOnce(T)->T>(f:F,x:T)->T=>f(x);fn main()->i32{{let r=R{{code:65}};let f=move(n:i32)=>{{take(give r);return n}};return apply(f,7)-7}}"
    );
    native("fnonce-consuming", &[("main", &once)], "A");
}

#[test]
fn copy_error_results_and_early_returns_keep_environment_and_local_cleanup_order() {
    for fail in [false, true] {
        let source = format!(
            r#"{DROP}
fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,fail:bool)->bool!T{{let local=R{{code:66}};if fail{{return false}}return f(x)}}
fn main()->bool!i32{{let owner=R{{code:65}};let f=move(n:i32)=>{{owner.code;return n}};return run(&f,7,{fail})?-7}}
"#
        );
        native_outcome(
            "copy-error-cleanup",
            &[("main", &source)],
            "BA",
            i32::from(fail),
        );
    }
}

#[test]
fn explicit_copy_addresses_and_current_capture_conflicts_use_existing_permissions() {
    native(
        "copy-address",
        &[(
            "main",
            "fn apply<T:Copy,F:Fn(&mut T)->i32>(f:&F,x:&mut T)->i32=>f(x);fn main()->i32{let f=move(n:&mut i32)=>{*n+=1;return *n};var n=6;return apply(&f,&mut n)-7}",
        )],
        "",
    );
    let conflict = "fn apply<T:Copy,F:Fn(&T)->i32>(f:&F,x:&T)->i32=>f(x);fn main()->i32{var n=1;var f=(x:&i32)=>{n+=*x;return n};return apply(&f,&n)}";
    rejected(
        "shared-bound-does-not-grant-write",
        conflict,
        Phase::Types,
        ResultCategory::SafeReject,
        "callable",
    );
}

#[test]
fn borrowed_lending_windows_are_not_extended_by_copy_data() {
    native(
        "lending-copy-extra",
        &[(
            "main",
            "fn make()->impl FnMut()->&i32{var n=6;return move()=>{n+=1;return &n}}fn read<T:Copy,F:FnMut()->&i32>(f:&mut F,x:T)->&i32=>f();fn main()->i32{var f=make();let p=read(&mut f,0);let n=*p;let q=read(&mut f,i64(0));return n+*q-15}",
        )],
        "",
    );
    rejected(
        "lending-copy-live-window",
        "fn make()->impl FnMut()->&i32{var n=6;return move()=>{n+=1;return &n}}fn read<T:Copy,F:FnMut()->&i32>(f:&mut F,x:T)->&i32=>f();fn main()->i32{var f=make();let p=read(&mut f,0);let q=read(&mut f,i64(0));return *p+*q}",
        Phase::Verify,
        ResultCategory::SafeReject,
        "active address window",
    );
}

#[test]
fn copy_bound_does_not_admit_other_categories_or_resource_descriptions() {
    let cases = [
        ("address", "fn main(){let n=7;id(&n)}"),
        ("mutable-address", "fn main(){var n=7;id(&mut n)}"),
        ("slice", "fn main(){let n=[7,8];id(&n[..])}"),
        (
            "resource-desc",
            "resource R{n:i32}fn main(){let r=R{n:7};id(r)}",
        ),
        (
            "package",
            "resource R{n:i32}package P{manages r:R}fn main(){id(P{r:R{n:7}})}",
        ),
        ("copy-only-package", "package P{n:i32}fn main(){id(P{n:7})}"),
    ];
    for (name, body) in cases {
        rejected(
            name,
            &format!("fn id<T:Copy>(x:T)->T=>x;{body}"),
            Phase::Expand,
            ResultCategory::SafeReject,
            "Copy",
        );
    }
}

#[test]
fn closure_environment_is_not_inferred_to_be_a_copy_code_pointer() {
    rejected(
        "local-closure-data",
        "fn id<T:Copy>(x:T)->T=>x;fn main()->i32{let n=7;let f=move()=>n;let g=id(f);return g()-7}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "cannot be inferred",
    );
    rejected(
        "opaque-closure-data",
        "fn id<T:Copy>(x:T)->T=>x;fn make()->impl Fn()->i32{let n=7;return move()=>n}fn main()->i32{let f=make();let g=id(f);return g()-7}",
        Phase::Expand,
        ResultCategory::Unsupported,
        "additional type resolution",
    );
    native(
        "explicit-code-pointer-data",
        &[(
            "main",
            "fn id<T:Copy>(x:T)->T=>x;fn main()->i32{let f:fn()->i32=()=>7;let g=id(f);return g()-7}",
        )],
        "",
    );
}

#[test]
fn uninstantiated_templates_do_not_gain_numeric_member_or_forged_capabilities() {
    for (name, source, status, message) in [
        (
            "numeric",
            "fn bad<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>x+1;fn main(){}",
            ResultCategory::Unsupported,
            "operation on an unconstrained",
        ),
        (
            "member",
            "fn bad<T:Copy>(x:T)->i32=>x.n;fn main(){}",
            ResultCategory::Unsupported,
            "operation on an unconstrained",
        ),
        (
            "method",
            "fn bad<T:Copy>(x:T)->i32=>x.foo();fn main(){}",
            ResultCategory::SafeReject,
            "bound",
        ),
        (
            "duplicate-unbound",
            "fn bad<T>(x:T)->(T,T)=>(x,x);fn main(){}",
            ResultCategory::SafeReject,
            "used after move",
        ),
        (
            "fake-impl",
            "resource R{n:i32}package P{manages r:R}impl Copy for P{}fn main(){}",
            ResultCategory::SafeReject,
            "cannot be implemented",
        ),
        (
            "fake-trait",
            "trait Copy{}fn main(){}",
            ResultCategory::SafeReject,
            "reserved",
        ),
    ] {
        rejected(name, source, Phase::Expand, status, message);
    }
}

#[test]
fn newly_opened_two_roles_keep_original_definitions_and_exercise_actual_calls() {
    // The original unused definitions are kept byte-for-byte. Their success
    // alone is not runtime evidence, so actual invocation controls follow.
    accepted(
        "two-role",
        "fn twice<F:Fn()->i32>(a:&F,b:&F)->i32=>a()+b();fn main(){}",
    );
    accepted(
        "two-callable",
        "fn both<F:Fn()->i32,G:Fn()->i32>(f:&F,g:&G)->i32=>f()+g();fn main(){}",
    );
    native(
        "two-role-invoked",
        &[(
            "main",
            "fn twice<F:Fn()->i32>(a:&F,b:&F)->i32=>a()+b();fn make(n:i32)->impl Fn()->i32{return move()=>n};fn main()->i32{let a=make(3);let b=make(4);return twice(&a,&b)-7}",
        )],
        "",
    );
    native(
        "two-callable-invoked",
        &[(
            "main",
            "fn both<F:Fn()->i32,G:Fn()->i32>(f:&F,g:&G)->i32=>f()+g();fn main()->i32{let f=move()=>3;let g=move()=>4;return both(&f,&g)-7}",
        )],
        "",
    );
}

#[test]
fn unbounded_data_mixed_modes_and_recursion_remain_closed() {
    for (name, text, message) in [
        (
            "unbounded",
            "fn map<T,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn main(){}",
            "direct Copy",
        ),
        (
            "mixed-mode",
            "fn inner<F:FnMut()->i32>(f:&mut F)->i32=>f();fn outer<F:Fn()->i32>(f:&mut F)->i32=>inner(f);fn main(){}",
            "same declared callable kind",
        ),
        (
            "recursive",
            "fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>run(f,x);fn main(){}",
            "recursive callable helper",
        ),
    ] {
        rejected(
            name,
            text,
            Phase::Expand,
            ResultCategory::Unsupported,
            message,
        );
    }
}

#[test]
fn four_module_instantiation_retains_definition_and_nominal_types() {
    native(
        "four-modules",
        &[
            (
                "main",
                "import model\nimport callbacks\nimport api\nfn main()->i32{let f=callbacks.make();let a=api.keep(&f,model.A{n:3});let b=api.keep(&f,model.B{n:4});return a.n+b.n-7}",
            ),
            ("model", "pub struct A{pub n:i32}pub struct B{pub n:i32}"),
            (
                "callbacks",
                "pub fn make()->impl Fn()->i32{return move()=>1}",
            ),
            (
                "api",
                "pub fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}",
            ),
        ],
        "",
    );
}
