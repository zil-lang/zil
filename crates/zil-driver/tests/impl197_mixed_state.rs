//! Mixed owning States retain complete Copy data identity and one concrete callback.
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
            "state197-{label}-{}-{}",
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

const STATE: &str = r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:Fn()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn factory()->impl Fn()->i32{return move()=>1}
fn run<T:Copy,G:Fn()->i32>(state:State<T,G>)->T{state.callback();return state.value}
"#;

#[test]
fn original_corrected_preflight_inputs_reach_verify_unchanged() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/impl197_cases");
    let mut cases = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    cases.sort();
    for folder in cases {
        if !folder.is_dir()
            || folder
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("12-")
        {
            continue;
        }
        let mut names = fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        names.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("zil"));
        names.sort_by_key(|p| (p.file_stem().unwrap() != "main", p.clone()));
        let contents = names
            .iter()
            .map(|path| {
                (
                    path.file_stem().unwrap().to_str().unwrap().to_owned(),
                    fs::read_to_string(path).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let files = contents
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let case = Case::new(folder.file_name().unwrap().to_str().unwrap());
        let report = compile(&case, &files, None);
        if folder
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("09-")
        {
            // The original record declared its type public, but not its field.
            // Opening State must preserve that real cross-module privacy check.
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "E-FIELD-PRIVATE")
            );
            let contents = contents
                .iter()
                .map(|(name, text)| {
                    (
                        name.clone(),
                        if name == "model" {
                            text.replace("n:i32", "pub n:i32")
                        } else {
                            text.clone()
                        },
                    )
                })
                .collect::<Vec<_>>();
            let files = contents
                .iter()
                .map(|(name, text)| (name.as_str(), text.as_str()))
                .collect::<Vec<_>>();
            native("public-field-companion", &files, "");
            continue;
        }
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
}

#[test]
fn original_uninstantiated_borrow_probe_is_only_a_definition_check() {
    // The original preflight 12 never calls run. In 197 its bad body was
    // omitted before Core; 205 now rejects the same bytes during the source
    // definition check. This is not an execution or independent Verify proof.
    let text = include_str!("impl197_cases/12-mixed-held-field-window-negative/main.zil");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "uninstantiated197.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let errors = zil_expand::expand(&source, &ast).unwrap_err();
    assert!(errors.iter().any(|d| d.phase == Phase::Expand
        && d.category == ResultCategory::SafeReject
        && d.code == "E-GENERIC-ARGUMENT"
        && d.message == "State alias outlives a moved or replaced field"));
    rejected(
        "original-unused-window",
        text,
        Phase::Expand,
        ResultCategory::SafeReject,
        "State alias outlives a moved or replaced field",
    );
}

#[test]
fn mixed_state_infers_data_only_from_its_envelope_across_helper_names() {
    native(
        "infer-envelope",
        &[(
            "main",
            &format!(
                "{STATE} struct A{{n:i32}}type Alias=A;fn relay<U:Copy,H:Fn()->i32>(state:State<U,H>)->U=>run(state);fn main()->i32{{let a:Alias=relay(make(factory(),A{{n:7}}));let b=run(make(factory(),i64(11)));return a.n+i32(b)-18}}"
            ),
        )],
        "",
    );
}

#[test]
fn nominal_alias_and_nested_copy_values_keep_complete_state_families() {
    native(
        "nested-data",
        &[(
            "main",
            &format!(
                "{STATE} struct Nest<T>{{value:T}}struct A{{n:i32}}struct B{{n:i32}}type Alias=Nest<Nest<A>>;fn main()->i32{{let a:Alias=run(make(factory(),Nest::<Nest<A>>{{value:Nest::<A>{{value:A{{n:7}}}}}}));let b=run(make(factory(),Nest::<B>{{value:B{{n:11}}}}));return a.value.value.n+b.value.n-18}}"
            ),
        )],
        "",
    );
}

#[test]
fn phantom_data_is_inferred_without_a_field_and_declaration_order_is_irrelevant() {
    native(
        "phantom-envelope",
        &[(
            "main",
            r#"
package State<F,T>{manages bytes:ByteBuffer;callback:F}
fn make<F:Fn()->i32,T:Copy>(callback:F,value:T)->State<F,T>{return State::<F,T>{bytes:bytes_new(),callback:callback}}
fn run<T:Copy,G:Fn()->i32>(state:State<G,T>)->i32{return state.callback()}
fn factory()->impl Fn()->i32{return move()=>7}
fn main()->i32{return run(make(factory(),i32(1)))+run(make(factory(),i64(2)))-14}
"#,
        )],
        "",
    );
}

#[test]
fn same_source_spelling_in_distinct_modules_is_not_the_same_data_type() {
    let model = "pub struct Item{pub n:i32}";
    native(
        "modules",
        &[
            (
                "main",
                "import left\nimport right\nimport state\nfn main()->i32{let a=state.run(state.make(state.factory(),left.Item{n:7}));let b=state.run(state.make(state.factory(),right.Item{n:11}));return a.n+b.n-18}",
            ),
            ("left", model),
            ("right", model),
            (
                "state",
                &STATE
                    .replace("package State", "pub package State")
                    .replace("fn make", "pub fn make")
                    .replace("fn run", "pub fn run")
                    .replace("fn factory", "pub fn factory"),
            ),
        ],
        "",
    );
}

#[test]
fn actual_held_copy_field_window_rejects_and_last_use_then_reborrow_passes() {
    let prefix = r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:FnMut(&mut T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn factory()->impl FnMut(&mut i32)->i32{var count=0;return move(value:&mut i32)=>{count+=1;*value+=1;return *value}}
"#;
    let bad = format!(
        "{prefix}fn run<T:Copy,F:FnMut(&mut T)->T>(state:State<T,F>)->T{{var current=state;let held=&current.value;let callback=&mut current.callback;let value=&mut current.value;callback(value);return *held}}fn main()->i32{{return run(make(factory(),41))-42}}"
    );
    let case = Case::new("actual-held-window");
    let report = compile(&case, &[("main", &bad)], None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-GENERIC-ARGUMENT"
                && d.message == "State alias outlives a moved or replaced field"),
        "{}",
        report.to_json()
    );
    native(
        "last-use-window",
        &[(
            "main",
            &format!(
                "{prefix}fn run<T:Copy,F:FnMut(&mut T)->T>(state:State<T,F>)->T{{var current=state;let held=&current.value;let copied=*held;let callback=&mut current.callback;let value=&mut current.value;return callback(value)}}fn main()->i32{{return run(make(factory(),41))-42}}"
            ),
        )],
        "",
    );
}

#[test]
fn unused_copy_bound_cannot_authorize_arithmetic_fields_or_calls() {
    for body in [
        "return state.value+state.value",
        "return state.value.n",
        "return state.value()",
    ] {
        let text = format!(
            "package State<T,F>{{manages bytes:ByteBuffer;callback:F;value:T}}fn unused<T:Copy,F:Fn()->i32>(state:State<T,F>)->T{{{body}}}fn main(){{}}"
        );
        let case = Case::new("unused-bound");
        let report = compile(&case, &[("main", &text)], None);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn concrete_resource_cannot_hide_in_copy_data_and_state_is_not_copy() {
    let text = format!(
        "{STATE} resource R{{n:i32}}impl Drop for R{{fn drop(self:Self){{}}}}fn main(){{let state=make(factory(),R{{n:1}})}}"
    );
    rejected(
        "resource-data",
        &text,
        Phase::Expand,
        ResultCategory::SafeReject,
        "Copy",
    );
    let text = format!(
        "{STATE} fn duplicate<T:Copy>(x:T)->(T,T)=>(x,x);fn main(){{let state=make(factory(),7);duplicate(state)}}"
    );
    let case = Case::new("state-not-copy");
    let report = compile(&case, &[("main", &text)], None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand);
}

#[test]
fn mixed_state_accepts_one_callable_field_only() {
    let text = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;value:T}fn make<T:Copy,F:Fn()->i32>(a:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),a:a,b:a,value:value}}fn main(){let f=move()=>1;make(f,1)}";
    let case = Case::new("multiple-fields");
    let report = compile(&case, &[("main", text)], None);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand);
}

#[test]
fn aggregate_bound_remains_explicitly_outside_the_finite_profile() {
    let text = "package State<T:Copy,F>{manages bytes:ByteBuffer;callback:F;value:T}fn main(){}";
    rejected(
        "aggregate-bound",
        text,
        Phase::Expand,
        ResultCategory::Unsupported,
        "generic bound",
    );
}

#[test]
fn owning_fnonce_field_consumes_once_and_cleans_the_remaining_state() {
    let prefix = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<T,F>{manages ticket:R;callback:F;value:T}
fn pack<T:Copy,F:FnOnce()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{ticket:R{code:84},callback:callback,value:value}}
fn make()->impl FnOnce()->i32{let r=R{code:65};return move()=>{let value=give r;return value.code}}
"#;
    native(
        "fnonce-cleanup",
        &[(
            "main",
            &format!(
                "{prefix}fn run<T:Copy,G:FnOnce()->i32>(state:State<T,G>)->T{{state.callback();return state.value}}fn main()->i32{{return run(pack(make(),7))-7}}"
            ),
        )],
        "AT",
    );
    let bad = format!(
        "{prefix}fn run<T:Copy,G:FnOnce()->i32>(state:State<T,G>)->T{{state.callback();state.callback();return state.value}}fn main()->i32{{return run(pack(make(),7))-7}}"
    );
    rejected(
        "fnonce-reuse",
        &bad,
        Phase::Expand,
        ResultCategory::SafeReject,
        "callback field used after",
    );
}

#[test]
fn mixed_state_explicit_fnmut_field_borrows_compose_with_continue_and_break() {
    let direct = r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn pack<T:Copy,F:FnMut(&mut T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn spin<T:Copy,G:FnMut(&mut T)->T>(state:State<T,G>,count:i32)->T{
var current=state;var i=0;while i<count{i+=1;if i==2{continue}current.callback(&mut current.value);if i==4{break}}return current.value}
fn factory()->impl FnMut(&mut i32)->i32{var hits=0;return move(value:&mut i32)=>{hits+=1;*value+=1;return *value}}
fn main()->i32{return spin(pack(factory(),10),7)-13}
"#;
    // 199 preserves the real field for direct Fn/FnMut invocation. The
    // original 197 input and the explicit sibling-borrow control both run.
    native("state-loop-direct-place", &[("main", direct)], "");
    let borrowed = direct.replace(
        "current.callback(&mut current.value);",
        "let callback=&mut current.callback;let value=&mut current.value;callback(value);",
    );
    native("state-loop-explicit-borrows", &[("main", &borrowed)], "");
}

#[test]
fn explicit_result_context_can_supply_phantom_copy_data() {
    native(
        "expected-state-result",
        &[(
            "main",
            r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F}
fn empty<T:Copy,F:Fn()->i32>(callback:F)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback}}
fn select<T:Copy,G:Fn()->i32>(callback:G,value:T)->State<T,G>{return empty(callback)}
fn factory()->impl Fn()->i32{return move()=>7}
fn main()->i32{let a=select(factory(),i32(1));let b=select(factory(),i64(2));return a.callback()+b.callback()-14}
"#,
        )],
        "",
    );
}
