//! Same source is release-linked to both compiler versions. All source updates,
//! filesystem snapshots, fresh controls and native executions are outside the
//! timed compile_project call. No verifier authority is retained by this probe.
use std::{collections::BTreeMap, path::Path, time::Instant};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, Stage};

fn project(case: &str, edit: &str, root: &Path) -> (ProjectSources, i32) {
    let (modules, functions, flavor) = match case {
        "large-512" => (1, 512, "scalar"),
        "large-2048" => (1, 2048, "scalar"),
        "even-64x16" => (64, 16, "scalar"),
        "even-512x8" => (512, 8, "scalar"),
        "even-1024x4" => (1024, 4, "scalar"),
        "branch-64x16" => (64, 16, "branch"),
        "body-16x16" => (16, 16, "body"),
        _ => panic!("unknown case {case}"),
    };
    let single = modules == 1;
    let mut sources = Vec::new();
    let mut paths = BTreeMap::new();
    let mut expected = 0;
    let mut main = String::new();
    for m in 0..modules {
        let mut text = String::new();
        for f in 0..functions {
            let changed = m == 0 && f == 0;
            let public_edit = changed && edit == "public";
            let value = f % 7 + 1 + i32::from(changed && edit == "private");
            let args = if public_edit {
                "x:i32, extra:i32"
            } else {
                "x:i32"
            };
            let body = if changed && edit == "error" {
                "return true;".to_string()
            } else if flavor == "branch" {
                format!("if x<3 {{return x+{value};}} else {{return x*2+{value};}}")
            } else if flavor == "body" {
                format!("var y:i32=x;{}return y+{value};", "y=y+1;".repeat(64))
            } else {
                format!("return x+{value};")
            };
            text += &format!("pub fn h{f}({args})->i32{{{body}}}\n");
            let x = f % 5;
            expected += value
                + if flavor == "branch" && x >= 3 {
                    x * 2
                } else if flavor == "body" {
                    x + 64
                } else {
                    x
                };
        }
        text += "pub fn value()->i32{var total:i32=0;\n";
        for f in 0..functions {
            let extra = if m == 0 && f == 0 && edit == "public" {
                ",0"
            } else {
                ""
            };
            text += &format!("total=total+h{f}({}{extra});\n", f % 5);
        }
        text += "return total;}\n";
        let id = if single { 0 } else { m + 1 } as u32;
        let name = if single {
            "main.zil".to_string()
        } else {
            format!("m{m}.zil")
        };
        sources.push(SourceUnit::new(
            FileId(id),
            ModuleId(id),
            root.join(&name).to_string_lossy(),
            text,
        ));
        if !single {
            paths.insert(vec![format!("m{m}")], ModuleId(id));
            main += &format!("import m{m}\n");
        }
    }
    if !single {
        main += "fn value()->i32{var total:i32=0;\n";
        for m in 0..modules {
            main += &format!("total=total+m{m}.value();\n");
        }
        main += "return total;}\n";
        sources.insert(
            0,
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                root.join("main.zil").to_string_lossy(),
                main,
            ),
        );
    }
    (
        ProjectSources {
            root: ModuleId(0),
            modules: sources,
            paths,
        },
        expected,
    )
}

fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.verification_runs, b.verification_runs);
}
fn native(path: &Path, expected: i32) -> String {
    let output = std::process::Command::new(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty());
    output
        .stdout
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn preserve_sources(project: &ProjectSources, root: &Path, label: &str) {
    let archive = root.join("sources").join(label);
    std::fs::create_dir_all(&archive).unwrap();
    for source in &project.modules {
        let name = Path::new(&source.path).file_name().unwrap();
        std::fs::write(archive.join(name), &source.text).unwrap();
        std::fs::write(&source.path, &source.text).unwrap();
    }
}
fn controls(case: &str, root: &Path) {
    let options = CompileOptions::default();
    let (base, _) = project(case, "cold", root);
    preserve_sources(&base, root, "cold");
    let mut session = CompilationSession::default();
    assert_eq!(
        session.compile_project(&base, &options).status,
        ResultCategory::Success
    );
    for edit in ["private", "public", "error", "restore"] {
        let (input, _) = project(case, edit, root);
        preserve_sources(&input, root, edit);
        let current = session.compile_project(&input, &options);
        let fresh = CompilationSession::default().compile_project(&input, &options);
        equivalent(&current, &fresh);
        println!(
            "{{\"control\":\"fresh-{edit}\",\"report\":{},\"fresh_report\":{}}}",
            current.to_json(),
            fresh.to_json()
        );
    }
    let cancelled = options.clone();
    cancelled.cancellation.cancel();
    let stopped = session.compile_project(&base, &cancelled);
    assert_eq!(stopped.status, ResultCategory::Cancelled);
    println!(
        "{{\"control\":\"cancelled\",\"report\":{}}}",
        stopped.to_json()
    );
    let options = CompileOptions::default();
    let recovered = session.compile_project(&base, &options);
    let fresh = CompilationSession::default().compile_project(&base, &options);
    equivalent(&recovered, &fresh);
    println!(
        "{{\"control\":\"recovered\",\"report\":{},\"fresh_report\":{}}}",
        recovered.to_json(),
        fresh.to_json()
    );
    let mut low = 0;
    let mut high = options.limits.max_analysis_steps;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let mut limited = CompileOptions::default();
        limited.limits.max_analysis_steps = middle;
        let result = CompilationSession::default().compile_project(&base, &limited);
        if result.status == ResultCategory::Success {
            high = middle;
        } else {
            assert_eq!(result.status, ResultCategory::ResourceLimit);
            low = middle;
        }
    }
    for (label, limit) in [("N", high), ("N-minus-1", high - 1), ("fresh-N", high)] {
        let mut limited = CompileOptions::default();
        limited.limits.max_analysis_steps = limit;
        let current = session.compile_project(&base, &limited);
        let fresh = CompilationSession::default().compile_project(&base, &limited);
        equivalent(&current, &fresh);
        assert_eq!(current.status == ResultCategory::Success, limit == high);
        println!(
            "{{\"control\":\"{label}\",\"analysis_limit\":{limit},\"report\":{},\"fresh_report\":{}}}",
            current.to_json(),
            fresh.to_json()
        );
    }
    assert_eq!(session.active_query_count(), 0);
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let case = &args[1];
    let mode = &args[2];
    let root = Path::new(&args[3]);
    std::fs::create_dir_all(root).unwrap();
    if mode == "controls" {
        controls(case, root);
        return;
    }
    let mut options = CompileOptions::default();
    if mode != "check" {
        options.stop_after = Stage::Build;
        options.backend = zil_driver::NativeBackend::LlvmApi;
        options.entry = Some("value".into());
        options.output = Some(root.join("program"));
        options.object_cache = Some(root.join("objects"));
        options.codegen.units = 8;
        options.codegen.jobs = 2;
        if mode == "O2" {
            options.optimization = zil_driver::Optimization::O2;
        } else {
            assert_eq!(mode, "O0");
        }
    }
    let mut session = CompilationSession::default();
    for label in ["cold", "unchanged", "private", "public", "restore"] {
        let (input, expected) = project(case, label, root);
        preserve_sources(&input, root, label);
        if options.output.is_some() {
            let _ = std::fs::remove_file(root.join("program"));
        }
        let start = Instant::now();
        let report = session.compile_project(&input, &options);
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(session.active_query_count(), 0);
        let native_stdout = if mode != "check" {
            native(&root.join("program"), expected)
        } else {
            String::new()
        };
        let rss = std::fs::read_to_string("/proc/self/status").unwrap();
        let rss = rss
            .lines()
            .find(|line| line.starts_with("VmRSS:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        println!(
            "{{\"request\":\"{label}\",\"elapsed_ns\":{elapsed},\"rss_kib\":{rss},\"expected\":{expected},\"native_checked\":{},\"native_stdout_hex\":\"{native_stdout}\",\"report\":{}}}",
            mode != "check",
            report.to_json()
        );
    }
}
