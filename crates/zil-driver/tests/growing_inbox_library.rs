//! Source library -> independent Verify -> LLVM/native, with nearest borrow failures.
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
const RECORDS: &str = include_str!("../../../examples/mainline/growing_inbox/queue.zil");

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn sources(main: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "queue.zil", RECORDS),
        ],
        paths: BTreeMap::from([(vec!["queue".into()], ModuleId(1))]),
    }
}
fn check(main: &str) -> CompileReport {
    compile_project(&sources(main), &CompileOptions::default())
}
fn native(main: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "inbox152-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), main).unwrap();
        fs::write(dir.join("queue.zil"), RECORDS).unwrap();
        let executable = dir.join("program");
        let report = compile_project(
            &sources(main),
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
        let result = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &result.stdout).unwrap();
        fs::write(dir.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
    }
}
#[test]
fn wrapped_growth_and_clipped_final_capacity_preserve_fifo_order() {
    native(
        r#"
import queue
fn main()->i32 {
 var q=queue.make(3,7)
 if q(10)!=1 || q(11)!=2 || q(12)!=3 || q(-1)!=10 || q(-1)!=11 {return 1}
 if q(13)!=2 || q(14)!=3 || q(15)!=4 || q(-4)!=6 {return 2}
 if q(-1)!=12 || q(-1)!=13 || q(-1)!=14 {return 3}
 var i:i32=16
 while i<21 {q(i);i+=1}
 if q(21)!=7 || q(-4)!=7 || q(22)!=-2 {return 4}
 i=15
 while i<22 {if q(-1)!=i64(i) {return 5};i+=1}
 if q(-1)!=-4 || q(-3)!=0 || q(-4)!=7 {return 6}
 return 0
}
"#,
    );
}
#[test]
fn close_drains_but_cancel_discards_and_eagerly_releases() {
    native(
        r#"
import queue
fn main()->i32 {
 var q=queue.make(0,1000)
 if q(-4)!=0 || q(7)!=1 || q(8)!=2 {return 1}
 if q(-2)!=2 || q(-6)!=1 || q(9)!=-1 || q(-4)!=2 {return 2}
 if q(-1)!=7 || q(-4)!=2 || q(-1)!=8 || q(-4)!=0 {return 3}
 if q(-2)!=0 || q(-5)!=0 || q(1)!=-1 {return 4}
 var c=queue.make(4,31)
 c(10);c(11);c(12)
 if c(-5)!=3 || c(-3)!=0 || c(-4)!=0 || c(-1)!=-4 || c(1)!=-1 {return 5}
 if c(-5)!=0 || c(-6)!=1 {return 6}
 return 0
}
"#,
    );
}
#[test]
fn invalid_configuration_and_commands_do_not_mutate_a_live_queue() {
    native(
        r#"
import queue
fn main()->i32 {
 var a=queue.make(-1,10);var b=queue.make(3,2);var c=queue.make(0,0)
 if a(1)!=-3 || b(1)!=-3 || c(1)!=-3 {return 1}
 var q=queue.make(0,1)
 if q(1)!=1 || q(2)!=-2 || q(256)!=-5 || q(-100)!=-5 || q(-3)!=1 {return 2}
 if q(-1)!=1 || q(2)!=1 || q(-1)!=2 {return 3}
 return 0
}
"#,
    );
}
#[test]
fn large_growth_wrap_and_close_check_every_event() {
    native(
        r#"
import queue
fn main()->i32 {
 var q=queue.make(3,65536)
 var i:i32=0
 while i<65536 {if q(i%256)!=i64(i)+1 {return 1};i+=1}
 if q(0)!=-2 || q(-4)!=65536 {return 2}
 i=0
 while i<32768 {if q(-1)!=i64(i%256) {return 3};i+=1}
 i=0
 while i<32768 {if q(i%256)!=32769+i64(i) {return 4};i+=1}
 if q(-2)!=65536 || q(0)!=-1 {return 5}
 i=0
 while i<65536 {if q(-1)!=i64(i%256) {return 6};i+=1}
 if q(-4)!=0 || q(-3)!=0 {return 7}
 return 0
}
"#,
    );
}
#[test]
fn owning_queue_cannot_be_copied_or_called_after_transfer() {
    let r = check("import queue\nfn main(){var q=queue.make(0,10);let other=q;q(1)}");
    assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
}
