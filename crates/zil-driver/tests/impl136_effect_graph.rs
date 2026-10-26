//! Actual source admission and independent Verify, not a handwritten graph oracle.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn source(n: usize, width: usize, cyclic: bool, mutable: bool) -> String {
    let mut s = String::from("resource R{n:i32}\n");
    let params = (0..width)
        .map(|i| format!("a{i}:R"))
        .collect::<Vec<_>>()
        .join(",");
    let args = (0..width)
        .map(|i| format!("a{}", (i + 1) % width))
        .collect::<Vec<_>>()
        .join(",");
    for i in 0..n {
        if cyclic {
            s += &format!(
                "fn f{i}({params},steps:i32){{if steps>0{{f{}({args},steps-1)}}else{{a0.n+=1}}}}\n",
                (i + 1) % n
            );
        } else if i + 1 < n {
            s += &format!("fn f{i}({params},steps:i32){{f{}({args},steps)}}\n", i + 1);
        } else {
            s += &format!("fn f{i}({params},steps:i32){{a0.n+=1}}\n");
        }
    }
    s += "fn make()->impl FnMut()->i32{";
    for i in 0..width {
        s += &format!("let a{i}=R{{n:0}};");
    }
    let names = (0..width)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sum = (0..width)
        .map(|i| format!("a{i}.n"))
        .collect::<Vec<_>>()
        .join("+");
    s += &format!("return move ()=>{{f0({names},17);return {sum}}}}}");
    s += &format!(
        "fn main()->i32{{{} f=make();return f()-1}}",
        if mutable { "var" } else { "let" }
    );
    s
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "effects136.zil", text)
}

#[test]
fn parameter_permutations_and_recursive_effects_reach_independent_verify() {
    for (n, width, cyclic) in [(16, 2, false), (24, 4, false), (16, 4, true), (20, 8, true)] {
        let text = source(n, width, cyclic, true);
        let r = compile_source(&unit(&text), &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
    }
}
#[test]
fn caller_cannot_hide_transitive_writes_by_binding_the_environment_immutably() {
    for cyclic in [false, true] {
        let r = compile_source(
            &unit(&source(16, 4, cyclic, false)),
            &CompileOptions::default(),
        );
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "E-CLOSURE-MUTABILITY"),
            "{}",
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}
#[test]
fn bounded_recursive_calls_execute_the_same_write_at_o0_and_o2() {
    for cyclic in [false, true] {
        for opt in [Optimization::O0, Optimization::O2] {
            let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let dir = root.join(format!(
                "effects136-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let text = source(16, 4, cyclic, true);
            fs::write(dir.join("input.zil"), &text).unwrap();
            let exe = dir.join("program");
            let r = compile_source(
                &unit(&text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    output: Some(exe.clone()),
                    emit_dir: Some(dir.join("phases")),
                    optimization: opt,
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("compile.json"), r.to_json()).unwrap();
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            let out = Command::new(exe).output().unwrap();
            fs::write(dir.join("stdout"), &out.stdout).unwrap();
            fs::write(dir.join("stderr"), &out.stderr).unwrap();
            assert_eq!(out.status.code(), Some(0), "{out:?}");
        }
    }
}
