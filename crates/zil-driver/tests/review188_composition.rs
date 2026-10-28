//! Non-author nested helper order, environment identity, and returned loan controls.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
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
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "review188-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(files: &[(&str, &str)], options: CompileOptions, dir: &Path) -> CompileReport {
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn check(source: &str) -> CompileReport {
    let dir = directory();
    compile(&[("main", source)], CompileOptions::default(), &dir)
}
fn native(files: &[(&str, &str)], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        let executable = dir.join("program");
        let report = compile(
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn branch_and_zero_call_edges_preserve_a_real_mutable_environment_across_modules() {
    let leaf = "pub fn leaf<F:FnMut()->i32>(f:&mut F)->i32{return f()}";
    let middle = "import leaf;pub fn twice<F:FnMut()->i32>(f:&mut F)->i32{let n=leaf.leaf(f);return n+leaf.leaf(f)}pub fn choose<F:FnMut()->i32>(flag:bool,f:&mut F)->i32{if flag{return twice(f)};return 0}";
    let outer = "import middle;pub fn run<F:FnMut()->i32>(f:&mut F)->i32{let zero=middle.choose(false,f);let a=middle.choose(true,f);return zero+a+middle.twice(f)}";
    let main = "import outer;fn main()->i32{var n=0;var f=()=>{n+=1;return n};let total=outer.run(&mut f);return total+n-14}";
    native(
        &[
            ("main", main),
            ("leaf", leaf),
            ("middle", middle),
            ("outer", outer),
        ],
        0,
    );
}

#[test]
fn nested_lending_results_continue_to_block_the_actual_owning_environment() {
    let helper = "fn leaf<F:Fn()->&[u8]>(f:&F)->&[u8]{return f()}pub fn forward<F:Fn()->&[u8]>(f:&F)->&[u8]{let ignored=leaf(f);return leaf(f)}";
    let factory = "pub fn make(n:u8)->impl Fn()->&[u8]{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,n);return move()=>bytes_view(b)}";
    let main = "import helper;import factory;fn main()->i32{let a=factory.make(65u8);let b=factory.make(66u8);let view=helper.forward(&a);let other=b;return i32(view[0])+i32(other()[0])-131}";
    native(
        &[("main", main), ("helper", helper), ("factory", factory)],
        0,
    );
    let bad = main.replace("let other=b", "let other=a");
    let dir = directory();
    let report = compile(
        &[("main", &bad), ("helper", helper), ("factory", factory)],
        CompileOptions::default(),
        &dir,
    );
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
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn forward_declared_helper_chain_uses_dependency_order_instead_of_vector_order() {
    let mut source = String::new();
    for i in 0..24 {
        if i == 23 {
            source.push_str(&format!("fn h{i}<F:Fn()->i32>(f:&F)->i32{{return f()}}"));
        } else {
            source.push_str(&format!(
                "fn h{i}<F:Fn()->i32>(f:&F)->i32{{return h{}(f)+1}}",
                i + 1
            ));
        }
    }
    source.push_str("fn main()->i32{let value=5;let f=()=>value;return h0(&f)-28}");
    native(&[("main", &source)], 0);
}

#[test]
fn unused_three_node_generic_cycle_has_no_finite_forwarding_contract() {
    let source = "fn a<F:Fn()->i32>(f:&F)->i32{return b(f)}fn b<F:Fn()->i32>(f:&F)->i32{return c(f)}fn c<F:Fn()->i32>(f:&F)->i32{return a(f)}fn main(){}";
    let report = check(source);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Expand,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("recursive callable helper dependency")),
        "{}",
        report.to_json()
    );
}
