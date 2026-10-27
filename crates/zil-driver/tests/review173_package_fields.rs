//! Independent edge consumers for the reviewed 170 Package field candidate.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
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
        "review173-fields-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
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
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

const ZERO: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource Empty {}
impl Drop for Empty { fn drop(self:Self) { unsafe { putchar(65) } } }
package Leaf { manages h:Empty }
package State { manages tag:Empty; items:[Leaf;2] }
fn fresh()->Leaf=>Leaf{h:Empty{}}
fn state()->State=>State{tag:Empty{},items:[fresh(),fresh()]}
"#;

#[test]
fn zero_sized_owned_leaves_keep_separate_cleanup_responsibilities() {
    let source = format!(
        "{ZERO} fn main()->i32{{var s=state();var a=s.items;let old=a[0];a[0]=fresh();s.items=a;return 0}}"
    );
    native(&[("main", &source)], b"AAAA", 0);
}

#[test]
fn repeated_whole_field_transfer_does_not_recreate_element_debt() {
    let source = format!(
        "{ZERO} fn main()->i32{{var s=state();var n=0;while n<4{{let a=s.items;s.items=a;n+=1}};return 0}}"
    );
    native(&[("main", &source)], b"AAA", 0);
}

#[test]
fn private_owned_array_field_is_not_exposed_by_take() {
    let report = check(&[
        (
            "main",
            "import model;fn main(){let s=model.make();let a=s.items}",
        ),
        (
            "model",
            r#"resource R{code:i32}package Item{manages r:R}pub package State{manages r:R;items:[Item;2]}pub fn make()->State=>State{r:R{code:0},items:[Item{r:R{code:1}},Item{r:R{code:2}}]}"#,
        ),
    ]);
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
            .any(|d| d.code == "E-FIELD-PRIVATE" && d.phase == zil_base::Phase::Types),
        "{}",
        report.to_json()
    );
}

#[test]
fn a_live_child_buffer_window_blocks_return_to_the_parent_field() {
    let source = r#"
resource R{code:i32}
package Item{manages r:R;manages data:ByteBuffer}
package State{manages r:R;items:[Item;2]}
fn item()->Item{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));return Item{r:R{code:0},data:give b}}
fn main()->i32{var s=State{r:R{code:1},items:[item(),item()]};let a=s.items;let view=bytes_view(a[0].data);s.items=a;return i32(view[0])-65}
"#;
    let report = check(&[("main", source)]);
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
            .any(|d| d.code == "E-BORROW-CONFLICT" && d.phase == zil_base::Phase::Verify),
        "{}",
        report.to_json()
    );
}
