//! Complete finite helper sequences preserve exact counts, exits and budgets.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CallableExit, CallableKind};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(name: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "sequence126-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep.is_some())
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), data).unwrap();
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
            .collect::<BTreeMap<_, _>>(),
    }
}
fn compile(
    input: &ProjectSources,
    case: &Case,
    optimization: Option<Optimization>,
) -> CompileReport {
    let native_main = input
        .modules
        .iter()
        .find(|unit| unit.module_id == input.root)
        .unwrap()
        .text
        .contains("fn main(");
    for unit in &input.modules {
        case.write(&format!("module{}.zil", unit.module_id.0), &unit.text);
    }
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            entry: if native_main {
                None
            } else {
                Some("value".into())
            },
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn native(input: &ProjectSources, label: &str, expected: &str, code: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(&format!("{label}-{opt:?}"));
        let report = compile(input, &case, Some(opt));
        success(&report);
        let path = &report.artifacts["executable"];
        case.write(
            "native-command.json",
            format!("[{}]\n", json_string(&path.to_string_lossy())),
        );
        let output = Command::new(path).output().unwrap();
        case.write("native.stdout", &output.stdout);
        case.write("native.stderr", &output.stderr);
        case.write("native.exit", format!("{:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(code), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

fn straight(count: usize) -> String {
    let calls = (1..=count)
        .map(|n| format!("f({n})"))
        .collect::<Vec<_>>()
        .join("+");
    format!(
        "fn batch<F:FnMut(i64)->i64>(f:&mut F)->i64{{return {calls}}} fn value()->i64{{var total:i64=0;var f=(n:i64)=>{{total+=n;return total}};let result=batch(&mut f);return result+total}}"
    )
}
fn core(text: &str) -> zil_ir::CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "sequence126.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}
#[test]
fn three_four_sixteen_calls_update_actual_mutable_state() {
    for count in [3, 4, 16] {
        let source = straight(count);
        let total = count * (count + 1) / 2;
        let sum = count * (count + 1) * (count + 2) / 6;
        native(
            &project(&[("main", &source)]),
            &format!("straight-{count}"),
            &format!("{}\n", sum + total),
            0,
        );
    }
}
#[test]
fn nine_distinct_exits_keep_complete_public_sequences() {
    let mut body = String::new();
    for count in 1..9 {
        let sum = vec!["f()"; count].join("+");
        body.push_str(&format!("if n=={count}{{return {sum}}}"));
    }
    body.push_str(&format!("return {}", ["f()"; 9].join("+")));
    let source = format!(
        "fn fan<F:Fn()->i32>(f:&F,n:i32)->i32{{{body}}} fn main()->i32{{let x=1;let f=()=>x;return fan(&f,3)-3}}"
    );
    let case = Case::new("nine-exits");
    success(&compile(&project(&[("main", &source)]), &case, None));
    let module = zil_types::check(
        &core(&source),
        &TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let claim = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    assert_eq!(
        claim
            .traces
            .as_finite()
            .unwrap()
            .iter()
            .map(|t| t.calls.len())
            .collect::<Vec<_>>(),
        (1..=9).collect::<Vec<_>>()
    );
    assert!(claim.traces.as_finite().unwrap().iter().all(|t| {
        t.exit == CallableExit::Return
            && t.calls
                .iter()
                .all(|event| event.parameter == 0 && event.kind == CallableKind::Fn)
    }));
    case.write("contract.txt", format!("{claim:#?}"));
}
#[test]
fn four_module_event_batch_preserves_success_error_and_cleanup() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/event_batch");
    let contents: Vec<_> = ["main", "batch", "events", "io"]
        .into_iter()
        .map(|name| {
            (
                name,
                fs::read_to_string(root.join(format!("{name}.zil"))).unwrap(),
            )
        })
        .collect();
    let files: Vec<_> = contents
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    native(&project(&files), "event-success", "EEEEST", 0);
    let error_main = "import events;fn main()->bool!i32{return events.run(3)}";
    let error_files: Vec<_> = files
        .iter()
        .map(|(name, text)| (*name, if *name == "main" { error_main } else { *text }))
        .collect();
    native(&project(&error_files), "event-error", "EEET", 1);
}
#[test]
fn new_sequences_still_pay_three_independent_budgets() {
    let source = straight(16);
    let input = core(&source);
    let defaults = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&input, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let case = Case::new("budgets");
    case.write("source.zil", &source);
    let mut records = Vec::new();
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let succeeds = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            };
            match phase {
                Phase::Types => zil_types::check(&input, &target, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Analyze => zil_analyze::analyze(&module, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Verify => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
                        .map(|_| ())
                        .map_err(|e| e[0].category)
                }
                _ => unreachable!(),
            }
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if succeeds(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert!(low > 0);
        assert!(succeeds(low).is_ok());
        assert_eq!(succeeds(low - 1), Err(ResultCategory::ResourceLimit));
        assert!(succeeds(low).is_ok());
        records.push(format!("{{\"phase\":\"{phase:?}\",\"N\":{low},\"N_minus_1\":{},\"at_N\":\"Pass\",\"at_N_minus_1\":\"ResourceLimit\",\"retry\":\"Pass\"}}",low-1));
    }
    case.write("boundaries.json", format!("[{}]\n", records.join(",")));
}
#[test]
fn consuming_reuse_rejects_and_the_original_cyclic_neighbor_runs() {
    for (label, text, phase, category, needle) in [(
        "once",
        "fn batch<F:FnOnce()->i32>(f:F)->i32{return f()+f()+f()} fn main()->i32{return 0}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "consumed",
    )] {
        let case = Case::new(label);
        let report = compile(&project(&[("main", text)]), &case, None);
        assert_eq!(report.status, category, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == phase && d.message.contains(needle)),
            "{}",
            report.to_json()
        );
    }
    // Task190 admits the original loop through a checked Flow; the source and
    // its observable return value are unchanged from the former scope probe.
    let source = "fn batch<F:Fn()->i32>(f:&F)->i32{var n=0;while n<3{n+=f()}return n}fn main()->i32{let x=1;let f=()=>x;return batch(&f)}";
    let input = project(&[("main", source)]);
    let case = Case::new("loop");
    success(&compile(&input, &case, None));
    native(&input, "original-loop190", "", 3);
}
