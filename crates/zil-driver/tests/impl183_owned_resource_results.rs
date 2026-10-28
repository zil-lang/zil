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
        fs::write(
            dir.join("native.json"),
            format!(
                "{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout_hex\":\"{}\",\"expected_stdout_hex\":\"{}\"}}\n",
                output.status.code().map_or("null".into(), |code| code.to_string()),
                output.stdout.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
                stdout.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            ),
        ).unwrap();
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

#[test]
fn public_owned_success_crosses_four_modules_and_explicit_union_parameters() {
    native(
        "modules",
        &[
            (
                "main",
                "import model;import factory;import adapter;fn main()->model.E!i32{let pending=adapter.forward(factory.make());let r=pending?;return model.read(r)-65}",
            ),
            (
                "model",
                r#"extern "C" fn putchar(n:i32)->i32;pub enum E{Fail}pub resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}pub fn read(r:R)->i32=>r.code"#,
            ),
            (
                "factory",
                "import model;pub fn make()->model.E!model.R contract{result(create)}=>model.R{code:65}",
            ),
            (
                "adapter",
                "import model;fn identity(give x:model.E!model.R)->model.E!model.R contract{result(give)}=>x;pub fn forward(give x:model.E!model.R)->model.E!model.R contract{result(give)}=>identity(x)",
            ),
        ],
        b"A",
        0,
    );
}

#[test]
fn question_propagation_preserves_created_payload_and_each_cleanup_exit() {
    for (flag, expected, exit) in [
        ("true", b"LAM".as_slice(), 0),
        ("false", b"LM".as_slice(), 1),
    ] {
        let source = format!(
            r#"{RESOURCE}
fn make(ok:bool)->E!R contract{{result(create)}}{{if ok{{return R{{code:65}}}};return E.Fail}}
fn middle(ok:bool)->E!R contract{{result(create)}}{{let local=R{{code:76}};let r=make(ok)?;return give r}}
fn main()->E!i32{{let local=R{{code:77}};let pending=middle({flag});let r=pending?;return r.code-65}}
"#
        );
        native("question-created", &[("main", &source)], expected, exit);
    }
}

#[test]
fn failed_later_argument_disposes_staged_union_before_older_local() {
    for (flag, expected, exit) in [
        ("true", b"CAZ".as_slice(), 0),
        ("false", b"AZ".as_slice(), 1),
    ] {
        let source = format!(
            r#"{RESOURCE}
fn make()->E!R contract{{result(create)}}=>R{{code:65}}
fn next(ok:bool)->E!i32{{if ok{{return 0}};return E.Fail}}
fn consume(give x:E!R,n:i32)->i32{{unsafe{{putchar(67)}};return n}}
fn main()->E!i32{{let guard=R{{code:90}};return consume(make(),next({flag})?)}}
"#
        );
        native("staged", &[("main", &source)], expected, exit);
    }
}

#[test]
fn loop_and_prepare_failure_clean_actual_owned_success_once() {
    let loop_source = format!(
        r#"{RESOURCE}
fn make(n:i32)->E!R contract{{result(create)}}=>R{{code:n}}
fn main()->i32{{var i=0;while i<3{{let u=make(65+i);i+=1;if i==1{{continue}};break}};return 0}}
"#
    );
    native("loop", &[("main", &loop_source)], b"AB", 0);
    let failure_source = format!(
        r#"{RESOURCE}
package Pair{{manages a:R;manages b:R}}
fn make()->E!R contract{{result(create)}}=>R{{code:65}}
fn fail()->E!i32=>E.Fail
fn main()->E!i32{{let u=make();let guard=R{{code:90}};let p=Pair{{a:R{{code:66}},b:R{{code:fail()?}}}};return 0}}
"#
    );
    native("prepare-failure", &[("main", &failure_source)], b"BZA", 1);
}

#[test]
fn failure_only_create_result_does_not_relabel_or_leak_an_incoming_owner() {
    let source = format!(
        r#"{RESOURCE}
fn make()->E!R contract{{result(create)}}=>R{{code:65}}
fn discard(give x:E!R)->E!R contract{{result(create)}}{{let guard=R{{code:90}};return E.Fail}}
fn main()->i32{{discard(make());return 0}}
"#
    );
    native("failure-no-owner", &[("main", &source)], b"ZA", 0);
}

#[test]
fn unwrapped_payload_has_actual_description_and_nonlexical_lifetime() {
    let source = format!(
        r#"{RESOURCE}
fn make()->E!R contract{{result(create)}}=>R{{code:65}}
fn wrap(give r:R)->E!R contract{{result(give)}}=>give r
fn main()->E!i32{{let r=make()?;let old=r;let callback=()=>old.code;let n=callback();let moved=wrap(give r);return n-65}}
"#
    );
    native("nll", &[("main", &source)], b"A", 0);
}

#[test]
fn source_ownership_modes_and_existing_rejection_boundaries_stay_explicit() {
    use zil_base::Phase;
    let declarations = "resource R{n:i32}enum E{Fail}fn make()->E!R contract{result(create)}=>R{n:1};fn wrap(give r:R)->E!R contract{result(give)}=>give r;";
    for (label, tail, category, phase, code) in [
        (
            "implicit-result",
            "fn bad()->E!R=>R{n:1};fn main(){}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "implicit-parameter",
            "fn bad(x:E!R){}fn main(){}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "give-descriptor",
            "fn main(){let r=R{n:1};let alias=r;let u=wrap(give alias)}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-GIVE-DESCRIPTOR",
        ),
        (
            "descriptor-injection",
            "fn bad(r:R)->E!R contract{result(give)}=>r;fn main(){}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-ERROR-INJECTION",
        ),
        (
            "give-union",
            "fn main(){let u=make();let v=give u}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-GIVE-TYPE",
        ),
        (
            "duplicate-union",
            "fn main(){let u=make();let a=u;let b=u}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-UNINITIALIZED",
        ),
        (
            "duplicate-owner",
            "fn main(){let r=R{n:1};let a=wrap(give r);let b=wrap(give r)}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-UNINITIALIZED",
        ),
        (
            "live-description",
            "fn main()->i32{let r=R{n:1};let old=r;let u=wrap(give r);return old.n}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "create-input-union",
            "fn bad(give x:E!R)->E!R contract{result(create)}=>x;fn main(){}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "create-rewrap-input",
            "fn bad(give x:E!R)->E!R contract{result(create)}{let r=x?;return give r}fn main(){}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "create-resource-input",
            "fn bad(give r:R)->E!R contract{result(create)}=>give r;fn main(){}",
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "union-reassignment",
            "fn main(){var u=make();u=make()}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "union-capture",
            "fn main(){let u=make();let f=move()=>{let v=u}}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "resource-error",
            "fn bad()->R!i32=>0;fn main(){}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "package-error-resource-success",
            "package P{manages r:R}fn bad()->P!R contract{result(create)}=>R{n:1};fn main(){}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
    ] {
        let report = check(label, &format!("{declarations}{tail}"));
        assert_eq!(report.status, category, "{label}: {}", report.to_json());
        assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    let report = check(
        "live-byte-window",
        "enum E{Fail}fn wrap(give b:ByteBuffer)->E!ByteBuffer contract{result(give)}=>give b;fn main()->i32{let b=bytes_new();bytes_push(b,u8(65));let w=bytes_view(b);let u=wrap(give b);return i32(w[0])-65}",
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}
