//! Actual nominal values cross callable-helper boundaries; expansion is not a proof.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "callable121-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn record(&self, input: &ProjectSources, report: &CompileReport) {
        for module in &input.modules {
            fs::write(
                self.0.join(format!("module{}.zil", module.module_id.0)),
                &module.text,
            )
            .unwrap();
        }
        fs::write(self.0.join("compile.json"), report.to_json()).unwrap();
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
fn fixture(shape: &str, mode: &str) -> ProjectSources {
    let model = match shape {
        "record" => {
            "pub struct Cell<T>{pub value:T;}pub fn pack(n:i64)->Cell<i64>{return Cell::<i64>{value:n}}pub fn unpack(x:Cell<i64>)->i64{return x.value}"
        }
        "enum" => {
            "pub enum Cell<T>{Some(T),None}pub fn pack(n:i64)->Cell<i64>{return Cell::<i64>.Some(n)}pub fn unpack(x:Cell<i64>)->i64{match x{Cell.Some(n)=>{return n},Cell.None()=>{return 0}}}"
        }
        _ => unreachable!(),
    };
    let (parameter, argument, body) = match mode {
        "Fn" => (
            "&F",
            "&f",
            "let f=(x:api.Number)=>model.pack(model.unpack(x)+2);let answer=api.apply(&f,model.pack(40));return model.unpack(answer)",
        ),
        "FnMut" => (
            "&mut F",
            "&mut f",
            "var visits:i64=0;var f=(x:api.Number)=>{visits+=1;return model.pack(model.unpack(x)+visits+1)};let answer=api.apply(&mut f,model.pack(40));return model.unpack(answer)+visits-1",
        ),
        "FnOnce" => (
            "F",
            "f",
            "let token=Token{bump:2};let f=move (x:api.Number)=>{let bump=token.bump;consume(give token);return model.pack(model.unpack(x)+bump)};let answer=api.apply(f,model.pack(40));return model.unpack(answer)",
        ),
        _ => unreachable!(),
    };
    let _ = argument;
    let api = format!(
        "import model\npub type Boxed<T>=model.Cell<T>;pub type Number=Boxed<i64>;pub fn apply<F:{mode}(Number)->Number>(f:{parameter},x:Number)->Number{{return f(x)}}"
    );
    let main = format!(
        "import model\nimport api\nresource Token{{bump:i64}}fn consume(give token:Token){{}}fn value()->i64{{{body}}}"
    );
    project(&[("main", &main), ("model", model), ("api", &api)])
}
fn compile(
    input: &ProjectSources,
    case: &Case,
    optimization: Option<Optimization>,
) -> CompileReport {
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Lower
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            entry: Some("value".into()),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.record(input, &report);
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

#[test]
fn actual_nominal_arguments_and_results_cross_all_three_callable_modes() {
    for shape in ["record", "enum"] {
        for mode in ["Fn", "FnMut", "FnOnce"] {
            let input = fixture(shape, mode);
            for optimization in [Optimization::O0, Optimization::O2] {
                let case = Case::new(&format!("{shape}-{mode}-{optimization:?}"));
                let report = compile(&input, &case, Some(optimization));
                success(&report);
                let binary = &report.artifacts["executable"];
                fs::write(
                    case.0.join("command.json"),
                    format!(
                        "{{\"command\":[{}]}}",
                        json_string(&binary.to_string_lossy())
                    ),
                )
                .unwrap();
                let output = Command::new(binary).output().unwrap();
                fs::write(case.0.join("stdout"), &output.stdout).unwrap();
                fs::write(case.0.join("stderr"), &output.stderr).unwrap();
                fs::write(
                    case.0.join("exit-code"),
                    format!("{:?}\n", output.status.code()),
                )
                .unwrap();
                assert_eq!(output.status.code(), Some(0), "{shape} {mode} {output:?}");
                assert_eq!(output.stdout, b"42\n", "{shape} {mode} {output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
}

#[test]
fn wrong_specialization_and_same_spelling_other_owner_are_rejected() {
    for other in ["model.Cell<bool>", "other.Cell<i64>"] {
        let mut input = fixture("record", "Fn");
        input.modules[0].text = format!(
            "import model\nimport api\nimport other\nfn value()->i64{{let f=(x:{other})=>x;let answer=api.apply(&f,model.pack(40));return model.unpack(answer)}}"
        );
        input.modules.push(SourceUnit::new(
            FileId(3),
            ModuleId(3),
            "other.zil",
            "pub struct Cell<T>{pub value:T;}",
        ));
        input.paths.insert(vec!["other".into()], ModuleId(3));
        let case = Case::new("wrong-nominal");
        let report = compile(&input, &case, None);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            "E-TYPE-MISMATCH",
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn unsupported_callable_contracts_are_not_widened() {
    for (text, status, code) in [
        (
            "struct Cell<T>{value:T;}fn apply<F:FnMut(Cell<i64>)->Cell<i64>>(f:F,x:Cell<i64>)->Cell<i64>{return f(x)}fn value()->i64{return 42}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "struct Cell<T>{value:T;}fn apply<F:FnOnce(Cell<i64>)->Cell<i64>>(f:&F,x:Cell<i64>)->Cell<i64>{return f(x)}fn value()->i64{return 42}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "fn apply<F:Fn(F)->i64>(f:&F)->i64{return 42}fn value()->i64{return 42}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
    ] {
        let case = Case::new("boundary");
        let report = compile(&project(&[("main", text)]), &case, None);
        assert_eq!(report.status, status, "{}", report.to_json());
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn expansion_charges_nested_callable_work_at_exact_boundaries() {
    let text = "struct Cell<T>{value:T;}fn apply<F:Fn(Cell<i64>)->Cell<i64>>(f:&F,x:Cell<i64>)->Cell<i64>{return f(x)}fn value()->i64{let f=(x:Cell<i64>)=>Cell::<i64>{value:x.value+2};let n=apply(&f,Cell::<i64>{value:40});return n.value}";
    let input = SourceUnit::new(FileId(0), ModuleId(0), "boundaries121.zil", text);
    let parsed = zil_syntax::parse(&input).into_result().unwrap();
    let full = zil_expand::expand_with_map_limits(&input, &parsed, &Limits::default()).unwrap();
    let case = Case::new("budget");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let mut records = vec![];
    for field in ["work", "nodes", "depth"] {
        let probe = |n| {
            let mut limits = Limits::default();
            match field {
                "work" => limits.max_analysis_steps = n,
                "nodes" => limits.max_nodes = n,
                _ => limits.max_call_depth = n,
            }
            zil_expand::expand_with_map_limits(&input, &parsed, &limits)
        };
        let mut high = match field {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_call_depth,
        };
        let mut low = 0;
        assert!(probe(high).is_ok());
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert_eq!(
                        errors[0].category,
                        ResultCategory::ResourceLimit,
                        "{field} {middle} {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(probe(high).unwrap().core, full.core);
        let errors = probe(high - 1).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].phase, Phase::Expand);
        records.push(format!("{{\"dimension\":{},\"N\":{high},\"at_N\":\"Success\",\"at_N_minus_1\":\"ResourceLimit\",\"diagnostic\":{}}}",json_string(field),json_string(&format!("{errors:?}"))));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]\n", records.join(",")),
    )
    .unwrap();
}

#[test]
fn independent_verifier_rejects_a_forged_nominal_callable_signature() {
    let text = "struct Cell<T>{value:T;}fn apply<F:Fn(Cell<i64>)->Cell<i64>>(f:&F,x:Cell<i64>)->Cell<i64>{return f(x)}fn value()->i64{let f=(x:Cell<i64>)=>Cell::<i64>{value:x.value+2};let n=apply(&f,Cell::<i64>{value:40});return n.value}";
    let input = SourceUnit::new(FileId(0), ModuleId(0), "raw121.zil", text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = zil_base::TargetProfile::default();
    let limits = Limits::default();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    let original_bytes = zil_ir::wire::encode_module_candidate(&original, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&original_bytes, &target, &limits).unwrap();
    let mut changed = original.clone();
    assert_eq!(changed.closures.len(), 1);
    assert!(matches!(
        changed.closures[0].parameters[0],
        zil_ir::Type::Record { .. }
    ));
    changed.closures[0].parameters[0] = zil_ir::Type::Bool;
    let changed_bytes = zil_ir::wire::encode_module_candidate(&changed, &plan, &limits).unwrap();
    let direct = zil_verify::verify_module(changed, plan, &target, &limits).unwrap_err();
    let wire = zil_verify::verify_module_bytes(&changed_bytes, &target, &limits).unwrap_err();
    for errors in [&direct, &wire] {
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(errors[0].code, "E-VERIFY-CLOSURE-ENTRY");
    }
    let case = Case::new("raw-wire");
    fs::write(case.0.join("source.zil"), text).unwrap();
    fs::write(case.0.join("original-typed.txt"), format!("{original:#?}")).unwrap();
    fs::write(case.0.join("original.wire"), original_bytes).unwrap();
    fs::write(case.0.join("changed.wire"), changed_bytes).unwrap();
    fs::write(
        case.0.join("rejections.txt"),
        format!("direct: {direct:#?}\nwire: {wire:#?}\n"),
    )
    .unwrap();
}
