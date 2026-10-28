//! Ordinary source library over the existing verified byte-buffer primitives.
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

const EDIT: &str = include_str!("../../../examples/mainline/owned_text/edit.zil");
const UTF8: &str = include_str!("../../../examples/mainline/owned_text/utf8.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn sources(main: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "edit.zil", EDIT),
            SourceUnit::new(FileId(2), ModuleId(2), "utf8.zil", UTF8),
        ],
        paths: BTreeMap::from([
            (vec!["edit".into()], ModuleId(1)),
            (vec!["utf8".into()], ModuleId(2)),
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
            "text139-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in [("main.zil", main), ("edit.zil", EDIT), ("utf8.zil", UTF8)] {
            fs::write(dir.join(name), text).unwrap();
        }
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
fn self_insertion_and_windows_crossing_public_edit_are_rejected() {
    for source in [
        "import edit\nfn main()->i32{let b=bytes_new();let old=bytes_view(b);let r=edit.replace_in_place(b,0,0,old,64);return r.code}",
        "import edit\nfn main()->i32{let b=bytes_new();let ins=bytes_new();let old=bytes_view(b);edit.replace_in_place(b,0,0,bytes_view(ins),64);return i32(old.len())}",
        "import edit\nfn main()->i32{let b=bytes_new();let ins=bytes_new();let old=bytes_view_mut(b);edit.replace_in_place(b,0,0,bytes_view(ins),64);return i32(old.len())}",
    ] {
        let report = check(source);
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
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn disjoint_package_insertion_window_survives_target_mutation() {
    native(
        r#"
import edit
package Pair {manages text:ByteBuffer; manages insertion:ByteBuffer}
fn main()->i32 {
    let pair=Pair{text:bytes_new(),insertion:bytes_new()}
    if !bytes_reserve(pair.text,64) || !bytes_reserve(pair.insertion,2) {return 1}
    bytes_push(pair.text,97u8);bytes_push(pair.text,98u8);bytes_push(pair.text,99u8)
    bytes_push(pair.insertion,195u8);bytes_push(pair.insertion,169u8)
    let insertion=bytes_view(pair.insertion)
    let result=edit.replace_in_place(pair.text,1,2,insertion,64)
    if result.code!=0 || insertion.len()!=2 || insertion[0]!=195u8 {return 2}
    let text=bytes_view(pair.text)
    if text.len()!=4 || text[0]!=97u8 || text[1]!=195u8 || text[2]!=169u8 || text[3]!=99u8 {return 3}
    return 0
}
"#,
    );
}

#[test]
fn owned_callback_can_repeatedly_edit_without_replacing_its_carrier() {
    native(
        r#"
import edit
fn make()->impl FnMut()->i32 {
    let text=bytes_new();let insertion=bytes_new()
    bytes_reserve(text,64);bytes_reserve(insertion,1)
    bytes_push(text,97u8);bytes_push(text,98u8);bytes_push(text,99u8);bytes_push(insertion,88u8)
    return move ()=>{
        let result=edit.replace_in_place(text,0,0,bytes_view(insertion),64)
        if result.code!=0 {return -100}
        return i32(bytes_len(text))
    }
}
fn main()->i32 {var callback=make();return callback()+callback()-9}
"#,
    );
}

#[test]
fn malformed_tail_and_extreme_boundaries_leave_all_storage_unchanged() {
    native(
        r#"
import edit
fn main()->i32 {
    let text=bytes_new();let insertion=bytes_new()
    bytes_reserve(text,32);bytes_push(text,97u8);bytes_push(text,255u8)
    let first=edit.replace_in_place(text,0,0,bytes_view(insertion),64)
    if first.code!=1 || first.source_error!=1 || bytes_len(text)!=2 || bytes_capacity(text)!=32 {return 1}
    let invalid=bytes_view(text)
    if invalid[0]!=97u8 || invalid[1]!=255u8 {return 2}
    bytes_truncate(text,1)
    let below=edit.replace_in_place(text,-9223372036854775808,0,bytes_view(insertion),64)
    let above=edit.replace_in_place(text,9223372036854775807,9223372036854775807,bytes_view(insertion),64)
    let limit=edit.replace_in_place(text,0,1,bytes_view(insertion),-9223372036854775808)
    if below.code!=3 || above.code!=3 || limit.code!=4 || bytes_len(text)!=1 || bytes_capacity(text)!=32 {return 3}
    let fresh=bytes_view(text)
    if fresh[0]!=97u8 {return 4}
    let empty=edit.replace_in_place(text,0,1,bytes_view(insertion),9223372036854775807)
    if empty.code!=0 || bytes_len(text)!=0 || bytes_capacity(text)!=32 {return 5}
    return 0
}
"#,
    );
}
