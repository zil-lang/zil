//! Shared results borrow actual stored environment fields, with independent loans.
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
        "lending184-{}-{}",
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
const MAKE: &str = "pub fn make(n:i32)->impl Fn()->&i32{return move()=>&n}";
const COUNTER: &str =
    "pub fn counter(start:i32)->impl FnMut()->&i32{var n=start;return move()=>{n+=1;return &n}}";
#[test]
fn shared_factory_result_uses_real_stored_copy_environment() {
    native(
        &[
            (
                "main",
                "import factory;fn main()->i32{let first=factory.make(41);let second=first;let a=first();let b=second();return *a+*b-82}",
            ),
            ("factory", MAKE),
        ],
        0,
    );
}
#[test]
fn mutable_factory_result_allows_next_call_after_last_use() {
    native(
        &[
            (
                "main",
                "import factory;fn main()->i32{var f=factory.counter(4);let a=f();let old=*a;let b=f();return old+*b-11}",
            ),
            ("factory", COUNTER),
        ],
        0,
    );
}
#[test]
fn borrowed_generic_helper_forwards_environment_lending_origin() {
    native(
        &[
            (
                "main",
                "import factory;import helper;fn main()->i32{let f=factory.make(41);let a=helper.peek(&f);return *a-41}",
            ),
            ("factory", MAKE),
            (
                "helper",
                "pub fn peek<F:Fn()->&i32>(f:&F)->&i32{return f()}",
            ),
        ],
        0,
    );
}
#[test]
fn environment_results_reject_local_external_and_temporary_sources() {
    for source in [
        "fn make()->impl Fn()->&i32{return move()=>{let n=3;return &n}}fn main(){}".to_string(),
        "fn main(){let n=3;let p=&n;let f=move()=>p;let v=f();}".to_string(),
        "fn main(){let f=(n:i32)=>&n;let v=f(3);}".to_string(),
        format!("{MAKE}fn main()->i32{{return *make(3)()}}"),
        format!("{MAKE}fn main()->i32{{let f=make(3);return *FnOnce.call_once(f,())}}"),
        format!("{MAKE}fn bad()->&i32{{let f=make(3);return f()}}fn main(){{}}"),
        "fn main(){var n=3;let f=move()=>&mut n;}".to_string(),
    ] {
        let report = check(&source);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
    }
}
#[test]
fn active_mutable_result_prevents_overlapping_call_or_replacement() {
    for body in [
        "var f=counter(4);let a=f();let b=f();return *a+*b",
        "var f=counter(4);let a=f();f=counter(8);return *a",
    ] {
        let source = format!("{COUNTER}fn main()->i32{{{body}}}");
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn copy_environment_reads_and_address_writes_use_one_actual_slot() {
    native(
        &[(
            "main",
            "fn make(start:i32)->impl FnMut()->&i32{var n=start;return move()=>{let p=&mut n;*p+=1;let fresh=n;n=fresh+1;return &n}}fn main()->i32{var f=make(4);var g=f;let a=f();let b=g();return *a+*b-12}",
        )],
        0,
    );
}

#[test]
fn conditional_sources_and_ordinary_parameter_forwarding_are_preserved() {
    native(
        &[
            (
                "main",
                "import factory;fn main()->i32{let f=factory.pick(7,11);let a=f(true);let b=f(false);return *a+*b-18}",
            ),
            (
                "factory",
                "fn identity(p:&i32)->&i32{return p}pub fn pick(a:i32,b:i32)->impl Fn(bool)->&i32{return move(flag)=>{if flag{return identity(&a)}return identity(&b)}}",
            ),
        ],
        0,
    );
}

#[test]
fn inline_aggregate_projection_and_slice_results_keep_actual_environment_storage() {
    for source in [
        "struct Wide{small:i32;wide:i64}fn make(value:Wide)->impl Fn()->&i64{return move()=>&value.wide}fn main()->i32{let f=make(Wide{small:3,wide:41});let p=f();return i32(*p)-41}",
        "fn make()->impl Fn()->&[i32]{let values=[7,11];return move()=>&values[..]}fn main()->i32{let f=make();let values=f();return values[0]+values[1]+i32(values.len())-20}",
    ] {
        native(&[("main", source)], 0);
    }
}

const BYTES: &str = "pub fn bytes(value:u8)->impl Fn()->&[u8]{let b=bytes_new();bytes_reserve(b,4);bytes_push(b,value);return move()=>bytes_view(b)}pub fn grow()->impl FnMut(u8)->&[u8]{let b=bytes_new();return move(value)=>{bytes_reserve(b,bytes_len(b)+1);bytes_push(b,value);return bytes_view(b)}}";

#[test]
fn owned_byte_environment_lends_real_storage_and_survives_factory_return() {
    native(
        &[
            (
                "main",
                "import factory;fn main()->i32{let f=factory.bytes(7u8);let a=f();let b=f();if a.len()!=1{return 1}if b.len()!=1{return 2}return i32(a[0])+i32(b[0])-14}",
            ),
            ("factory", BYTES),
        ],
        0,
    );
}

#[test]
fn owned_byte_environment_can_mutate_after_prior_result_last_use() {
    native(
        &[
            (
                "main",
                "import factory;fn main()->i32{var f=factory.grow();let a=f(5u8);let first=i32(a[0]);let b=f(8u8);return first+i32(b[0])+i32(b[1])+i32(b.len())-20}",
            ),
            ("factory", BYTES),
        ],
        0,
    );
}

#[test]
fn live_byte_environment_views_reject_mutation_move_and_drop() {
    for body in [
        "var f=grow();let a=f(5u8);let b=f(8u8);return i32(a.len()+b.len())",
        "let f=bytes(7u8);let a=f();let g=f;return i32(a.len())",
        "var f=bytes(7u8);let a=f();f=bytes(8u8);return i32(a.len())",
        "let f=bytes(7u8);var a=f();if true{let g=bytes(8u8);a=g();}return i32(a.len())",
    ] {
        let report = check(&format!("{BYTES}fn main()->i32{{{body}}}"));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{body}: {}",
            report.to_json()
        );
    }
    for write in [
        "bytes_reserve(b,4)",
        "bytes_push(b,1u8)",
        "bytes_truncate(b,0)",
    ] {
        let report = check(&format!(
            "fn make()->impl FnMut()->&[u8]{{let b=bytes_new();return move()=>{{let s=bytes_view(b);{write};return s}}}}fn main(){{}}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{write}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{write}: {}",
            report.to_json()
        );
    }
}

#[test]
fn callable_field_view_keeps_its_actual_field_without_blocking_copy_sibling() {
    native(
        &[
            (
                "main",
                "import factory;import state;fn main()->i32{var current=state.make(factory.bytes(7u8));let callback=&current.callback;let view=callback();current.count+=1;return i32(view[0])+current.count-8}",
            ),
            ("factory", BYTES),
            (
                "state",
                "pub package State<F>{manages storage:ByteBuffer;pub callback:F;pub count:i32}pub fn make<F:Fn()->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback,count:0}}",
            ),
        ],
        0,
    );
}

#[test]
fn borrowed_mutable_helper_and_whole_parent_transfer_preserve_result_loans() {
    for source in [
        format!(
            "{COUNTER}fn peek<F:FnMut()->&i32>(f:&mut F)->&i32{{return f()}}fn main()->i32{{var f=counter(3);let a=peek(&mut f);let b=peek(&mut f);return *a+*b}}"
        ),
        format!(
            "{BYTES}package State<F>{{manages storage:ByteBuffer;callback:F;count:i32}}fn make<F:Fn()->&[u8]>(callback:F)->State<F>{{return State::<F>{{storage:bytes_new(),callback:callback,count:0}}}}fn main()->i32{{let current=make(bytes(7u8));let callback=&current.callback;let view=callback();let moved=current;return i32(view.len())}}"
        ),
    ] {
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{}",
            report.to_json()
        );
    }
}
