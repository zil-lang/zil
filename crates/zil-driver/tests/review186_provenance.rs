//! Independent symbolic provenance and branch cleanup probes for 186.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "review186-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
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
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    }
}
fn check(label: &str, source: &str) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn accepted(label: &str, source: &str) {
    let report = check(label, source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn rejected(label: &str, source: &str, category: ResultCategory, message: &str) {
    let report = check(label, source);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
}
fn native(label: &str, files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        for (name, source) in files {
            fs::write(dir.join(format!("{name}.zil")), source).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
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
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &output.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            dir.join("native.json"),
            format!(
                "{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout_hex\":\"{}\",\"expected_stdout_hex\":\"{}\"}}\n",
                output.status.code().map_or("null".into(), |code| code.to_string()),
                output.stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            ),
        ).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;pub n:i32}
package Q{lead:u8;manages r:R;pub wide:i64}
enum E{Fail}
fn p(code:i32)->P=>P{r:R{code:code},n:7}
fn q(code:i32)->Q=>Q{lead:u8(3),r:R{code:code},wide:123456789}
"#;
const HELPERS: &str = r#"
fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
fn make<T:StoreElement>(value:T,fail:bool)->E!Store<T> contract{result(create)}{
 let s:Store<T>=empty();if fail{return E.Fail};
 if !store_reserve(s,1){return E.Fail};store_push(s,value);return give s
}
fn forward<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}=>x
fn wrap<T:StoreElement>(give x:Store<T>)->E!Store<T> contract{result(give)}=>give x
"#;

#[test]
fn an_error_returning_old_branch_does_not_poison_the_surviving_fresh_branch() {
    let definition = r#"
fn select<T:StoreElement>(give old:Store<T>,flag:bool)->E!Store<T>contract{result(create)}{
 var slot:Store<T>=empty();
 if flag{slot=give old;return E.Fail};
 return give slot
}
"#;
    accepted(
        "unused-surviving-branch",
        &format!("{MODEL}{HELPERS}{definition}fn main(){{}}"),
    );
    for flag in [false, true] {
        let source = format!(
            "{MODEL}{HELPERS}{definition}fn main()->E!i32{{let incoming=make(p(65),false)?;let result=select(give incoming,{flag})?;return 0}}"
        );
        native(
            "surviving-branch",
            &[("main", &source)],
            b"A",
            i32::from(flag),
        );
    }
}

#[test]
fn unused_definitions_do_not_duplicate_a_union_through_aggregate_or_branch_paths() {
    for definition in [
        "fn bad<T:StoreElement>(give u:E!Store<T>){let tuple=(u,u)}",
        "fn bad<T:StoreElement>(give u:E!Store<T>,flag:bool){if flag{let moved=u};let again=u}",
        "fn bad<T:StoreElement>(give u:E!Store<T>){let s=u?;let d=s;let moved=wrap(give s);store_len(d)}",
        "fn bad<T:StoreElement>(v:T){let s=make(v,false)?;let d=s;let moved=wrap(give s);store_len(d)}",
    ] {
        let report = check(
            "unused-duplicate",
            &format!("{MODEL}{HELPERS}{definition}fn main(){{}}"),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{definition}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn a_transitive_give_source_never_becomes_a_create_source_in_an_unused_template() {
    for definition in [
        "fn bad<T:StoreElement>(give old:E!Store<T>)->E!Store<T>contract{result(create)}{let a=forward(old);let b=a;let c=b?;return give c}",
        "fn bad<T:StoreElement>(give old:E!Store<T>,flag:bool)->E!Store<T>contract{result(create)}{if flag{return forward(old)};return empty()}",
        "fn bad<T:StoreElement>(give old:Store<T>,flag:bool)->E!Store<T>contract{result(create)}{var a:Store<T>=empty();if flag{a=give old};let b=wrap(give a);return b}",
    ] {
        rejected(
            "transitive-old",
            &format!("{MODEL}{HELPERS}{definition}fn main(){{}}"),
            ResultCategory::SafeReject,
            "fresh source",
        );
    }
}

#[test]
fn two_consuming_success_branches_leave_no_union_cleanup_debt() {
    for flag in [false, true] {
        let source = format!(
            r#"{MODEL}{HELPERS}
fn split<T:StoreElement>(give old:E!Store<T>,flag:bool)->E!Store<T>contract{{result(give)}}{{
 if flag{{let a=old;return a}}else{{let b=old;return b}}
}}
fn main()->E!i32{{let first=split(make(p(65),false),{flag})?;let second=split(make(q(66),false),!{flag})?;return 0}}
"#
        );
        native("both-consuming", &[("main", &source)], b"BA", 0);
    }
}
