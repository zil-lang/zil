//! Generated declarations retain ownership markers; runtime and CT powers stay distinct.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, Stage,
};

const IO: &str = r#"extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}
"#;
const MANAGED: &str = "enum Event{Empty,Full(manages Ticket)}pub fn run()->i32{let t=Ticket{code:65};let e=Event.Full(give t);return 0}";
const ITEM: &str = "package Item{manages ticket:Ticket;id:i32;}";
const PRODUCT: &str = "enum Event{Empty,Full(Item)}pub fn run()->i32{let t=Ticket{code:65};let p=Item{ticket:give t,id:66};let e=Event.Full(p);match e{Event.Empty()=>{return 1},Event.Full(q)=>{emit(q.id);return 0}}}";
const MARKER: &str = "resource Ticket{code:i32}#for(n in [1]){enum Event{Empty,Full(manages Ticket)}}fn main()->i32=>0";
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generated-owned239.zil", text)
}
fn directory(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("generated-owned239-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&p).unwrap();
    p
}
fn check(label: &str, text: &str, stage: Stage) -> CompileReport {
    let p = directory(label);
    fs::write(p.join("source.zil"), text).unwrap();
    let report = CompilationSession::default().compile_source(
        &unit(text),
        &CompileOptions {
            stop_after: stage,
            emit_dir: Some(p.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(p.join("report.json"), report.to_json()).unwrap();
    report
}
fn paired(prefix: &str, body: &str, generated: bool) -> String {
    format!(
        "{IO}{prefix}{}fn main()->i32=>run()",
        if generated {
            format!("#for(n in [1]){{{body}}}")
        } else {
            body.to_owned()
        }
    )
}

#[test]
fn generated_and_handwritten_manages_survive_to_core_enum_payloads() {
    for (label, text) in [
        ("generated-marker", MARKER.to_owned()),
        (
            "plain-marker",
            MARKER
                .replace("#for(n in [1]){", "")
                .replace("}}fn main", "}fn main"),
        ),
    ] {
        let report = check(label, &text, Stage::Core);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let core = fs::read_to_string(&report.artifacts["core"]).unwrap();
        let enums = core
            .split_once("enums: [")
            .unwrap()
            .1
            .split_once("unsupported:")
            .unwrap()
            .0;
        assert_eq!(enums.matches("Managed(").count(), 1, "{core}");
        assert!(enums.contains("Ticket"), "{core}");
    }
}

#[test]
fn generated_and_handwritten_owned_payloads_clean_at_o0_and_o2() {
    for (label, prefix, body, expected) in [
        ("managed", "", MANAGED, "A"),
        ("product", ITEM, PRODUCT, "BA"),
    ] {
        for generated in [false, true] {
            let text = paired(prefix, body, generated);
            for opt in [Optimization::O0, Optimization::O2] {
                let p = directory(&format!("native-{label}-{generated}-{opt:?}"));
                fs::write(p.join("source.zil"), &text).unwrap();
                let backend = if cfg!(feature = "llvm-api") {
                    NativeBackend::LlvmApi
                } else {
                    NativeBackend::Clang
                };
                let report = CompilationSession::default().compile_source(
                    &unit(&text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        optimization: opt,
                        backend,
                        output: Some(p.join("program")),
                        emit_dir: Some(p.join("phases")),
                        ..CompileOptions::default()
                    },
                );
                fs::write(p.join("report.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                fs::write(p.join("stdout"), &output.stdout).unwrap();
                fs::write(p.join("stderr"), &output.stderr).unwrap();
                assert_eq!(output.status.code(), Some(0));
                assert_eq!(output.stdout, expected.as_bytes());
                assert!(output.stderr.is_empty());
            }
        }
    }
}

#[test]
fn owned_enum_rounds_remain_nominal_and_zero_rounds_do_not_declare_values() {
    let text = format!(
        "{IO}#for(n in [1,2]){{enum E{{Full(manages Ticket)}}#if(n==1){{pub fn make()->E{{let t=Ticket{{code:65}};return E.Full(give t)}}}}else{{pub fn accept(e:E)->i32{{return 0}}}}}}fn main()->i32=>accept(make())"
    );
    let report = check("cross-round-nominal", &text, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(!report.artifacts.contains_key("executable"));
    let report = check(
        "zero-round",
        "#for(n in []){enum E{Full(manages Missing)}}fn main()->i32=>0",
        Stage::Verify,
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn generation_does_not_turn_bare_buffers_or_future_types_into_ct_authority() {
    for generated in [false, true] {
        let body = "enum E{Full(ByteBuffer)}";
        let text = format!(
            "{}fn main()->i32=>0",
            if generated {
                format!("#for(n in [1]){{{body}}}")
            } else {
                body.to_owned()
            }
        );
        let report = check(&format!("bare-buffer-{generated}"), &text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    let report = check(
        "configready",
        "#for(n in [1]){pub enum E{A}}#if(E){fn selected()->i32=>0}fn main()->i32=>0",
        Stage::Verify,
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-PHASE-DEPENDENCY"),
        "{}",
        report.to_json()
    );
}

#[test]
fn generated_manages_snapshot_budget_has_n_n_minus_one_and_fresh_boundaries() {
    for nodes in [true, false] {
        let run = |n, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = n;
            } else {
                limits.max_analysis_steps = n;
            }
            session.compile_source(
                &unit(MARKER),
                &CompileOptions {
                    stop_after: Stage::Core,
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        let (mut lo, mut hi) = (0, if nodes { 100_000 } else { 1_000_000 });
        assert_eq!(
            run(hi, &mut CompilationSession::default()).status,
            ResultCategory::Success
        );
        while hi - lo > 1 {
            let n = (lo + hi) / 2;
            let r = run(n, &mut CompilationSession::default());
            if r.status == ResultCategory::Success {
                hi = n;
            } else {
                assert_eq!(r.status, ResultCategory::ResourceLimit, "{}", r.to_json());
                lo = n;
            }
        }
        let p = directory(if nodes { "budget-nodes" } else { "budget-work" });
        fs::write(p.join("source.zil"), MARKER).unwrap();
        fs::write(p.join("minimum.json"), format!("{{\"minimum\":{hi}}}\n")).unwrap();
        let mut warm = CompilationSession::with_capacity(128);
        for (i, n) in [hi, hi, hi - 1, hi].into_iter().enumerate() {
            let r = run(n, &mut warm);
            fs::write(p.join(format!("attempt-{i}.json")), r.to_json()).unwrap();
            assert_eq!(
                r.status,
                if i == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                r.to_json()
            );
            assert_eq!(warm.active_query_count(), 0);
        }
        let fresh = run(hi, &mut CompilationSession::default());
        fs::write(p.join("fresh.json"), fresh.to_json()).unwrap();
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
    }
}

#[test]
fn unchanged_copy_enum_generation_keeps_q_exact_logical_budgets() {
    // These exact Q values came from the original impl230 default workspace
    // receipt, not from searching a new threshold on the implementation here.
    let text = include_str!("impl239_fixtures/legacy230-widths.zil");
    assert_eq!(
        zil_base::Fingerprint::of(text.as_bytes()).to_hex(),
        "3f08482c95dd9995e1194f3c4d65119800ed9a8268601a6553d905c97d648008"
    );
    for (nodes, n) in [(true, 871), (false, 2698)] {
        let run = |limit, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = limit;
            } else {
                limits.max_analysis_steps = limit;
            }
            session.compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "generated-enums230.zil", text),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        let p = directory(if nodes {
            "old230-exact-nodes"
        } else {
            "old230-exact-work"
        });
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, limit) in [n, n, n - 1, n].into_iter().enumerate() {
            let r = run(limit, &mut session);
            fs::write(p.join(format!("attempt-{attempt}.json")), r.to_json()).unwrap();
            assert_eq!(
                r.status,
                if attempt == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                r.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
        }
        let fresh = run(n, &mut CompilationSession::default());
        fs::write(p.join("fresh.json"), fresh.to_json()).unwrap();
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
        assert_eq!(fresh.verification_runs, 1);
    }
}
