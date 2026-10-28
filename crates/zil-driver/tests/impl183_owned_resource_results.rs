//! Direct Managed success payloads use the existing owning Sum operations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, Optimization, ProjectSources, Stage, compile_project, compile_source,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "impl183-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn check(label: &str, source: &str) -> zil_driver::CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source);
    let report = compile_source(&source, &CompileOptions::default());
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, files: &[(&str, &str)], stdout: &[u8], exit: i32) -> Vec<PathBuf> {
    let modules = files
        .iter()
        .enumerate()
        .map(|(i, (name, source))| {
            SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                format!("{name}.zil"),
                *source,
            )
        })
        .collect::<Vec<_>>();
    let project = ProjectSources {
        root: ModuleId(0),
        modules,
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    };
    let mut dirs = Vec::new();
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        for (name, source) in files {
            fs::write(dir.join(format!("{name}.zil")), source).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &output.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            dir.join("exit.txt"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        dirs.push(dir);
    }
    dirs
}

const RESOURCE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}
"#;

#[test]
fn original159_direct_bytebuffer_success_probe_is_accepted_without_rewriting() {
    let source = "enum E{Fail} fn make()->E!ByteBuffer contract{result(give)}{return bytes_new()} fn main(){}";
    let report = check("original159", source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn create_and_give_success_transfer_once_and_copy_error_has_no_resource() {
    for (flag, expected, exit) in [("true", b"AZ".as_slice(), 0), ("false", b"Z".as_slice(), 1)] {
        let source = format!(
            r#"{RESOURCE}
fn make(ok:bool)->E!R contract{{result(create)}}{{if ok{{return R{{code:65}}}};return E.Fail}}
fn forward(give x:E!R)->E!R contract{{result(give)}}{{return x}}
fn main()->E!i32{{let guard=R{{code:90}};let r=forward(make({flag}))?;return r.code-65}}
"#
        );
        native("create-give", &[("main", &source)], expected, exit);
    }
}

#[test]
fn discarded_and_stored_unions_drop_only_the_active_resource() {
    let source = format!(
        r#"{RESOURCE}
fn success()->E!R contract{{result(create)}}=>R{{code:65}}
fn failure()->E!R contract{{result(create)}}=>E.Fail
fn main()->i32{{success();failure();let a=success();let b=failure();return 0}}
"#
    );
    native("discard", &[("main", &source)], b"AA", 0);
}

#[test]
fn bytebuffer_and_store_success_payloads_reach_real_native_operations() {
    let source = format!(
        r#"{RESOURCE}
package P{{manages r:R}}
fn bytes()->E!ByteBuffer contract{{result(create)}}{{let b=bytes_new();if !bytes_reserve(b,2){{return E.Fail}};bytes_push(b,u8(65));return give b}}
fn store()->E!Store<P> contract{{result(create)}}{{return store_new()}}
fn main()->E!i32{{let b=bytes()?;let s=store()?;if !store_reserve(s,1){{return E.Fail}};store_push(s,P{{r:R{{code:66}}}});let w=bytes_view(b);unsafe{{putchar(i32(w[0]))}};return i32(bytes_len(b)+store_len(s))-2}}
"#
    );
    native("bytes-store", &[("main", &source)], b"AB", 0);
}
