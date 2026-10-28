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
const RECORDS: &str = include_str!("../../../examples/mainline/record_vector/records.zil");
const INBOX: &str = include_str!("../../../examples/mainline/record_vector/inbox.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn sources(main: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "records.zil", RECORDS),
            SourceUnit::new(FileId(2), ModuleId(2), "inbox.zil", INBOX),
        ],
        paths: BTreeMap::from([
            (vec!["records".into()], ModuleId(1)),
            (vec!["inbox".into()], ModuleId(2)),
        ]),
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
            "records142-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), main).unwrap();
        fs::write(dir.join("records.zil"), RECORDS).unwrap();
        fs::write(dir.join("inbox.zil"), INBOX).unwrap();
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
fn ordered_and_unordered_operations_preserve_complete_records() {
    native(
        r#"
import records
fn main()->i32 {
 let v=bytes_new();let a=[1u8,2u8,3u8];let b=[4u8,5u8,6u8];let c=[7u8,8u8,9u8]
 if records.count(v,3)!=0 || records.pop(v,3) {return 1}
 if !records.push(v,3,&a[..],64) || !records.push(v,3,&c[..],64) {return 2}
 if !records.insert(v,3,1,&b[..],64) || records.count(v,3)!=3 {return 3}
 if records.at(v,3,1)[0]!=4u8 || records.at(v,3,2)[2]!=9u8 {return 4}
 if !records.set(v,3,0,&c[..]) || !records.remove(v,3,1) {return 5}
 if records.at(v,3,0)[1]!=8u8 || records.at(v,3,1)[1]!=8u8 {return 6}
 if !records.insert(v,3,0,&a[..],64) || !records.swap_remove(v,3,0) {return 7}
 if records.count(v,3)!=2 || records.at(v,3,0)[0]!=7u8 {return 8}
 if !records.pop(v,3) || !records.clear(v,3) || records.count(v,3)!=0 {return 9}
 return 0
}
"#,
    );
}
#[test]
fn invalid_width_alignment_index_limit_and_overflow_preserve_storage() {
    native(
        r#"
import records
fn main()->i32 {
 let v=bytes_new();let a=[11u8,12u8,13u8]
 if !records.push(v,3,&a[..],12) {return 1}
 let cap=bytes_capacity(v)
 if records.insert(v,3,-1,&a[..],12) || records.insert(v,3,9223372036854775807,&a[..],12) {return 2}
 if records.push(v,0,&a[..],12) || records.push(v,-9223372036854775808,&a[..],12) {return 3}
 if records.reserve(v,3,9223372036854775807,9223372036854775807) || records.reserve(v,3,1,-1) {return 4}
 if records.push(v,3,&a[..],3) || records.set(v,3,1,&a[..]) || records.remove(v,3,-1) {return 5}
 if bytes_capacity(v)!=cap || bytes_len(v)!=3 || records.at(v,3,0)[0]!=11u8 {return 6}
 if records.at(v,3,-1).len()!=0 || records.at(v,3,1).len()!=0 {return 7}
 bytes_push(v,99u8)
 if records.count(v,3)!=-1 || records.clear(v,3) || records.pop(v,3) || records.push(v,3,&a[..],12) {return 8}
 if bytes_len(v)!=4 || bytes_view(v)[3]!=99u8 {return 9}
 return 0
}
"#,
    );
}
#[test]
fn record_views_cannot_cross_mutation_or_escape_ownership() {
    let cases = [
        "import records\nfn main()->i32{let v=bytes_new();let a=[1u8];records.push(v,1,&a[..],8);let old=records.at(v,1,0);records.clear(v,1);return i32(old.len())}",
        "import records\nfn main()->i32{let v=bytes_new();let a=[1u8];records.push(v,1,&a[..],8);let old=records.at(v,1,0);records.push(v,1,old,8);return i32(bytes_len(v))}",
        "import records\nfn main()->i32{let v=bytes_new();let a=[1u8];records.push(v,1,&a[..],8);let old=records.at(v,1,0);records.set(v,1,0,old);return i32(bytes_len(v))}",
        "import records\nfn bad()->&[u8]{let v=bytes_new();return records.at(v,1,0)}fn main(){}",
    ];
    for (index, main) in cases.into_iter().enumerate() {
        let r = check(main);
        if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let dir = PathBuf::from(root);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(format!("record-negative-{index}.zil")), main).unwrap();
            fs::write(
                dir.join(format!("record-negative-{index}.json")),
                r.to_json(),
            )
            .unwrap();
        }
        if index < 3 {
            assert!(
                r.diagnostics.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
                "{}",
                r.to_json()
            );
        }
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(!r.artifacts.contains_key("executable"));
    }
}
#[test]
fn disjoint_package_fields_can_supply_a_record_during_target_growth() {
    native(
        r#"
import records
package Model {manages target:ByteBuffer;manages source:ByteBuffer}
fn main()->i32 {
 let m=Model{target:bytes_new(),source:bytes_new()};bytes_reserve(m.source,3);bytes_push(m.source,8u8);bytes_push(m.source,9u8);bytes_push(m.source,10u8)
 let item=bytes_view(m.source)
 if !records.push(m.target,3,item,100) || !records.insert(m.target,3,0,item,100) {return 1}
 if item[1]!=9u8 || records.count(m.target,3)!=2 || records.at(m.target,3,1)[2]!=10u8 {return 2}
 return 0
}
"#,
    );
}
#[test]
fn owned_callback_keeps_queue_state_across_many_calls() {
    native(
        r#"
import records
fn make()->impl FnMut(i32)->i64 {
 let queue=bytes_new()
 return move (command:i32)->i64=>{
  let item=[u8(command)]
  if command>0 {if !records.push(queue,1,&item[..],8192) {return -1}}
  else {if !records.pop(queue,1) {return -2}}
  return records.count(queue,1)
 }
}
fn main()->i32 {
 var q=make();var i:i32=0
 while i<4096 {if q(7)!=i64(i)+1 {return 1};i+=1}
 while i>0 {i-=1;if q(0)!=i64(i) {return 2}}
 if q(0)!=-2 {return 3}
 return 0
}
"#,
    );
}

#[test]
fn bounded_fifo_closes_drains_and_releases_without_late_posts() {
    native(
        r#"
import inbox
fn main()->i32 {
 var bad=inbox.make(0)
 if bad(1)!=-3 {return 1}
 var q=inbox.make(8);var round:i32=0
 while round<1024 {
  var j:i32=0
  while j<5 {if q((round+j)%256)!=i64(j)+1 {return 2};j+=1}
  j=0
  while j<5 {if q(-1)!=i64((round+j)%256) {return 3};j+=1}
  if q(-1)!=-4 || q(-3)!=0 {return 4}
  round+=1
 }
 var i:i32=0
 while i<8 {if q(11+i)!=i64(i)+1 {return 5};i+=1}
 if q(99)!=-2 || q(-2)!=8 || q(99)!=-1 {return 6}
 i=0
 while i<8 {if q(-1)!=i64(11+i) {return 7};i+=1}
 if q(-1)!=-4 || q(-3)!=0 || q(-2)!=0 || q(-4)!=0 || q(100)!=-1 {return 8}
 return 0
}
"#,
    );
}
