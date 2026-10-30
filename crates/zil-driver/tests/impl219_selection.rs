//! Actual parsed source enters the public driver and independent safety gate.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources};

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("selection219-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&root).unwrap();
    root
}
fn check(label: &str, text: &str) -> CompileReport {
    let out = directory(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "source.zil", text);
    let report = CompilationSession::default().compile_source(
        &source,
        &CompileOptions {
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    report
}
fn pass(label: &str, text: &str) -> CompileReport {
    let report = check(label, text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    report
}
fn reject(label: &str, text: &str) -> CompileReport {
    let report = check(label, text);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    report
}

#[test]
fn original_probes_and_selected_lists_reach_the_independent_gate() {
    for (label, text) in [
        (
            "original-item",
            include_str!("../../../tools/language/219/original/ct-item-if.zil"),
        ),
        (
            "original-statement",
            include_str!("../../../tools/language/219/original/ct-statement-if.zil"),
        ),
        (
            "local-after-list",
            "fn main()->i32{#if(true){let answer=7}else{let answer=unavailable}return answer-7}",
        ),
        (
            "selected-else",
            "#if(false){fn value()->Missing=>bad}else{fn value()->i32=>9}fn main()->i32{return value()-9}",
        ),
        (
            "unselected-attribute",
            "#if(true){fn main()->i32=>0}else{#[unknown] fn unused()->Missing=>absent}",
        ),
        (
            "nested-inactive",
            "fn main()->i32{#if(false){#if(missing()){return absent}}else{#if(2*3==6){let value=3}return value-3}}",
        ),
        (
            "closure-list",
            "fn main()->i32{let f=()=>{#if(true){let n=3}else{let n=absent}return n};return f()-3}",
        ),
        (
            "loop-control",
            "fn main()->i32{var n=0;loop{#if(true){n+=1;if n==1{continue}break}else{return absent}}return n-2}",
        ),
        (
            "shadow-restored",
            "fn enabled()->bool=>true;fn main()->i32{if true{let enabled=7;}#if(enabled()){return 0}else{return absent}}",
        ),
    ] {
        pass(label, text);
    }
    let duplicate = reject(
        "list-duplicate",
        "fn main()->i32{let n=1;#if(true){let n=2}return n}",
    );
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|d| d.code == "E-NAME-DUPLICATE"),
        "{}",
        duplicate.to_json()
    );
}

#[test]
fn actual_scalar_function_dependencies_loops_recursion_and_short_circuit_execute() {
    pass(
        "function-closure",
        "fn twice(n:i32)->i32=>n*2;fn sum(n:i32)->i32{var i=0;var total=0;while i<n{total+=twice(i);i+=1}return total}fn recurse(n:i32)->i32{if n==0{return 0}return 1+recurse(n-1)}fn compact()->bool=>sum(4)==12 && recurse(3)==3;#if(compact()){fn value()->i32=>6}else{fn value()->i32=>missing}fn main()->i32{#if(twice(3)==6){let n=value()}else{let n=absent}return n-6}",
    );
    for (label, condition) in [
        ("short-false", "false && (1/0==0)"),
        ("short-true", "true || (1/0==0)"),
        ("integer-u16", "u16(255)+1u16==256u16"),
        ("integer-i64", "i64(42)>i64(1)"),
    ] {
        pass(
            label,
            &format!("fn main()->i32{{#if({condition}){{return 0}}else{{return 0}}}}"),
        );
    }
    pass(
        "large-i64-context",
        "fn large()->i64=>9000000000;fn main()->i32{#if(large()>1){return 0}else{return missing}} ",
    );
    let pipeline = pass(
        "pipeline-dependencies",
        "fn inc(n:i32)->i32=>n+1;fn good(n:i32)->bool=>(n |> inc)>1;fn main()->i32{#if(1 |> good){return 0}else{return missing}}",
    );
    let trace = fs::read_to_string(&pipeline.artifacts["selection"]).unwrap();
    assert!(trace.contains("\"function\":\"good\""));
    assert!(trace.contains("\"function\":\"inc\""));
    let non_bool = reject("not-bool", "fn main()->i32{#if(1){return 0}else{return 1}}");
    assert_eq!(non_bool.phase, Phase::Ct);
    let arithmetic = reject(
        "division",
        "fn div(x:i32)->i32=>10/x;fn main()->i32{#if(div(0)==1){return 0}else{return 1}}",
    );
    assert!(
        arithmetic
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-ARITHMETIC" || d.code == "E-CT-TRAP"),
        "{}",
        arithmetic.to_json()
    );
    let overflow = reject(
        "overflow",
        "fn main()->i32{#if(2147483647+1==0){return 0}else{return 1}}",
    );
    assert_eq!(overflow.phase, Phase::Ct);
}

#[test]
fn config_snapshot_is_closed_and_runtime_shadowing_is_not_a_global_fallback() {
    for (label, text) in [
        (
            "generated-function",
            "#if(true){fn enabled()->bool=>true}#if(enabled()){fn main()->i32=>0}else{fn main()->i32=>1}",
        ),
        (
            "unmaterialized-body",
            "fn enabled()->bool{#if(true){return true}else{return false}}fn main()->i32{#if(enabled()){return 0}else{return 1}}",
        ),
        (
            "pipeline-generated-dependency",
            "#if(true){fn generated(n:i32)->bool=>n>0}fn inner(n:i32)->bool=>generated(n);fn outer(n:i32)->bool=>n |> inner;fn main()->i32{#if(outer(1)){return 0}else{return 1}}",
        ),
        (
            "helper-unready",
            "fn outer()->bool=>inner();fn inner()->bool{#if(true){return true}else{return false}}fn main()->i32{#if(outer()){return 0}else{return 1}}",
        ),
    ] {
        let r = reject(label, text);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "E-CT-PHASE-DEPENDENCY" && d.span.is_some()),
            "{}",
            r.to_json()
        );
    }
    for (label, text) in [
        (
            "runtime-local",
            "fn main()->i32{let n=true;#if(n){return 0}else{return 1}}",
        ),
        (
            "runtime-parameter",
            "fn choose(n:bool)->i32{#if(n){return 0}else{return 1}}fn main()->i32=>choose(true)",
        ),
        (
            "shadow-call",
            "fn enabled()->bool=>true;fn main()->i32{let enabled=()=>false;#if(enabled()){return 0}else{return 1}}",
        ),
        (
            "splice-shadow-call",
            "fn enabled()->bool=>true;fn main()->i32{#if(true){let enabled=()=>false}#if(enabled()){return 0}else{return 1}}",
        ),
        (
            "loop-shadow",
            "fn enabled()->bool=>true;fn main()->i32{for enabled in 0..1{#if(enabled()){return 0}}return 1}",
        ),
        (
            "closure-parameter",
            "fn enabled()->bool=>true;fn main()->i32{let f=(enabled:bool)=>{#if(enabled){return 0}else{return 1}};return f(true)}",
        ),
        (
            "foreign",
            "extern \"C\" fn external()->i32;fn main()->i32{#if(external()==0){return 0}else{return 1}}",
        ),
        (
            "resource",
            "fn active()->bool{var b=bytes_new();return bytes_len(b)==0}fn main()->i32{#if(active()){return 0}else{return 1}}",
        ),
    ] {
        let r = reject(label, text);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "E-CT-FORBIDDEN"),
            "{label}: {}",
            r.to_json()
        );
    }
    let unknown = reject(
        "unknown-ordinary",
        "fn main()->i32{#if(missing()){return 0}else{return 1}}",
    );
    assert!(
        unknown
            .diagnostics
            .iter()
            .all(|d| d.code != "E-CT-PHASE-DEPENDENCY")
    );
    let syntax = reject(
        "unselected-syntax",
        "fn main()->i32{#if(true){return 0}else{let = ;}}",
    );
    assert_eq!(syntax.phase, Phase::Parse);
    let selected = reject(
        "selected-name",
        "fn main()->i32{#if(true){return missing}else{return 0}}",
    );
    assert!(
        selected
            .diagnostics
            .iter()
            .all(|d| d.code != "E-CT-PHASE-DEPENDENCY")
    );
    let attribute = reject(
        "selected-attribute",
        "#if(false){fn main()->i32=>0}else{#[unknown] fn main()->i32=>0}",
    );
    assert!(
        attribute
            .diagnostics
            .iter()
            .any(|d| d.message.contains("unexecuted declaration attribute"))
    );
    let import = reject(
        "conditional-import",
        "#if(true){import unseen}fn main()->i32=>0",
    );
    assert!(
        import
            .diagnostics
            .iter()
            .any(|d| d.message.contains("conditional imports")),
        "{}",
        import.to_json()
    );
}

fn project(settings: &str, body: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                format!("import settings\n{body}"),
            ),
            SourceUnit::new(FileId(1), ModuleId(1), "settings.zil", settings),
        ],
        paths: BTreeMap::from([(vec!["settings".into()], ModuleId(1))]),
    }
}
#[test]
fn ordinary_cross_module_visibility_and_generated_exports_are_preserved() {
    let settings = "fn helper(n:i32)->i32=>n*2;pub fn slots(n:i32)->i32=>helper(n);pub fn compact()->bool=>slots(3)==6";
    let p = project(
        settings,
        "#if(settings.compact()){fn width()->i32=>6}else{fn width()->i32=>unavailable}fn main()->i32{#if(settings.slots(3)==6){let x=width()}else{let x=unknown}return x-6}",
    );
    let out = directory("project");
    let report = CompilationSession::default().compile_project(
        &p,
        &CompileOptions {
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let trace = fs::read_to_string(&report.artifacts["selection"]).unwrap();
    assert!(trace.contains("\"function\":\"helper\""));
    let private = project(
        settings,
        "fn main()->i32{#if(settings.helper(3)==6){return 0}else{return 1}}",
    );
    let rejected =
        CompilationSession::default().compile_project(&private, &CompileOptions::default());
    assert_ne!(
        rejected.status,
        ResultCategory::Success,
        "{}",
        rejected.to_json()
    );
    let exported = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import generated\nfn main()->i32=>generated.value()-6",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "generated.zil",
                "fn configured()->bool=>true;#if(configured()){pub fn value()->i32=>6}else{pub fn value()->i32=>absent}",
            ),
        ],
        paths: BTreeMap::from([(vec!["generated".into()], ModuleId(1))]),
    };
    let report =
        CompilationSession::default().compile_project(&exported, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn grouped_import_objects_keep_the_same_closed_dependencies_and_lexical_shadowing() {
    let compile = |label: &str, main: &str, config: &str| {
        let out = directory(label);
        fs::write(out.join("main.zil"), main).unwrap();
        fs::write(out.join("cfg.zil"), config).unwrap();
        let project = ProjectSources {
            root: ModuleId(0),
            modules: vec![
                SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
                SourceUnit::new(FileId(1), ModuleId(1), "cfg.zil", config),
            ],
            paths: BTreeMap::from([(vec!["cfg".into()], ModuleId(1))]),
        };
        let report = CompilationSession::default().compile_project(
            &project,
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        report
    };
    let config = "fn leaf(n:i32)->bool{return n==7}\npub fn choose(n:i32)->bool{return leaf(n)}\n";
    let generated = "#if(true){pub fn leaf(n:i32)->bool{return n==7}}\npub fn choose(n:i32)->bool{return leaf(n)}\n";
    let mut expected_dependencies = None;
    for (label, condition) in [
        ("plain", "cfg.choose(7)"),
        ("grouped", "(cfg).choose(7)"),
        ("nested", "(((cfg))).choose(7)"),
        ("callee-grouped", "((cfg).choose)(7)"),
    ] {
        let main = format!(
            "import cfg\n#if({condition}){{fn main()->i32{{return 0}}}}else{{fn main()->i32{{return 1}}}}\n"
        );
        let report = compile(&format!("alias-{label}"), &main, config);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let trace = fs::read_to_string(&report.artifacts["selection"]).unwrap();
        let dependencies = trace.split("\"dependencies\":").nth(1).unwrap();
        assert!(
            dependencies.contains("\"module\":1,\"function\":\"choose\""),
            "{label}: {trace}"
        );
        assert!(
            dependencies.contains("\"module\":1,\"function\":\"leaf\""),
            "{label}: {trace}"
        );
        if let Some(expected) = &expected_dependencies {
            assert_eq!(dependencies, expected, "{label}");
        } else {
            expected_dependencies = Some(dependencies.to_string());
        }
        let report = compile(&format!("alias-{label}-generated"), &main, generated);
        assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|d| d.code == "E-CT-PHASE-DEPENDENCY")
            .unwrap_or_else(|| panic!("{label}: {}", report.to_json()));
        let span = diagnostic.span.unwrap();
        assert_eq!(span.file, FileId(1));
        assert_eq!(span.start as usize, generated.rfind("leaf(n)").unwrap());
        assert_eq!(&generated[span.start as usize..span.end as usize], "leaf");

        let shadow = format!(
            "import cfg\nfn main()->i32{{#if(true){{let cfg=0}}#if({condition}){{return 0}}else{{return 1}}}}\n"
        );
        let report = compile(&format!("alias-{label}-shadow"), &shadow, config);
        assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|d| d.code == "E-CT-FORBIDDEN")
            .unwrap_or_else(|| panic!("{label}: {}", report.to_json()));
        let span = diagnostic.span.unwrap();
        assert_eq!(span.file, FileId(0));
        assert_eq!(span.start as usize, shadow.rfind("cfg").unwrap());
        assert_eq!(&shadow[span.start as usize..span.end as usize], "cfg");
    }
}

#[test]
fn the_frozen_native_consumer_checks_both_source_configurations() {
    for (label, config) in [
        (
            "compact",
            include_str!("../../../examples/mainline/ct_selection219/config.zil"),
        ),
        (
            "wide",
            include_str!("../../../examples/mainline/ct_selection219/config_wide.zil"),
        ),
    ] {
        let project = ProjectSources {
            root: ModuleId(0),
            modules: vec![
                SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    "main.zil",
                    include_str!("../../../examples/mainline/ct_selection219/main.zil"),
                ),
                SourceUnit::new(FileId(1), ModuleId(1), "config.zil", config),
                SourceUnit::new(
                    FileId(2),
                    ModuleId(2),
                    "io.zil",
                    include_str!("../../../examples/mainline/ct_selection219/io.zil"),
                ),
            ],
            paths: BTreeMap::from([
                (vec!["config".into()], ModuleId(1)),
                (vec!["io".into()], ModuleId(2)),
            ]),
        };
        let out = directory(&format!("consumer-{label}"));
        let report = CompilationSession::default().compile_project(
            &project,
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}
