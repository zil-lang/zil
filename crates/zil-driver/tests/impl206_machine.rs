//! Actual source coverage for complete inline children and local field flags.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit, json_string};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf, bool);
impl Case {
    fn new() -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "machine206-child-flags-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

const SOURCE: &str = r#"
extern "C" fn putchar(code:i32)->i32;
extern "C" fn getchar()->i32;
fn emit(code:i32){unsafe{putchar(code)}}
fn read()->i32{unsafe{return getchar()}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
package Leaf{manages first:Marker;manages second:Marker}
package Middle{manages lead:Marker;child:Leaf;manages tail:Marker}
package Root{child:Middle}
fn main()->i32{
    var root=Root{child:Middle{lead:Marker{code:65},
        child:Leaf{first:Marker{code:66},second:Marker{code:67}},tail:Marker{code:68}}}
    let mode=read()
    if mode==49{
        var middle=root.child
        let leaf=middle.child
        emit(88)
        return 0
    }
    emit(89)
    return 0
}
"#;

#[test]
fn complete_children_have_inline_payloads_and_one_parent_obligation() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new();
        fs::write(case.0.join("source.zil"), SOURCE).unwrap();
        let executable = case.0.join("program");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "machine206.zil", SOURCE),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(case.0.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let llvm = fs::read_to_string(case.0.join("phases/llvm.ll")).unwrap();
        // This exact source layout has four i32 payloads in nested Products.
        // The embedded child ABI contains no shell or partial-state flag.
        assert!(
            llvm.contains("alloca { { { i32 }, { { i32 }, { i32 } }, { i32 } } }, align 4"),
            "the actual Root inline payload changed: {llvm}"
        );
        for (input, expected) in [(b'0', "YABCD"), (b'1', "XBCAD")] {
            let mut child = Command::new(&executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(&[input]).unwrap();
            let output = child.wait_with_output().unwrap();
            let stem = format!("input-{}", input as char);
            fs::write(case.0.join(format!("{stem}.stdout")), &output.stdout).unwrap();
            fs::write(case.0.join(format!("{stem}.stderr")), &output.stderr).unwrap();
            fs::write(
                case.0.join(format!("{stem}.json")),
                format!(
                    "{{\"command\":[{}],\"optimization\":\"{optimization:?}\",\"stdin_hex\":\"{input:02x}\",\"exit\":{},\"expected_exit\":0,\"stdout\":{},\"expected_stdout\":{}}}\n",
                    json_string(&executable.to_string_lossy()),
                    output.status.code().map_or("null".into(), |n| n.to_string()),
                    json_string(&String::from_utf8_lossy(&output.stdout)),
                    json_string(expected)
                ),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, expected.as_bytes());
            assert!(output.stderr.is_empty());
        }
    }
}
