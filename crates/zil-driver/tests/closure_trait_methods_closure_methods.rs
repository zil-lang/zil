//! Local nominal trait declarations -> ordinary calls -> independent Verify/native.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn compile(source: &str) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
        &CompileOptions::default(),
    )
}
fn native(source: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "methods151-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), source).unwrap();
        let executable = dir.join("program");
        let r = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        fs::write(dir.join("stderr"), &out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
    }
}
#[test]
fn read_trait_and_inherent_methods_work_inside_shared_and_move_closures() {
    native(
        r#"
trait Read{fn get(self:&Self)->i32;}
struct Cell{x:i32}
impl Read for Cell{fn get()->i32=>.x}
impl Cell{fn twice(self:&Self)->i32=>.x*2}
fn main()->i32{let c=Cell{x:7};let f=()->i32=>c.get();let g=move()->i32=>c.twice();return f()+g()-21}
"#,
    );
}
#[test]
fn mutable_receiver_writes_persist_across_owned_callback_calls_and_early_returns() {
    native(
        r#"
struct Cell{x:i32}
impl Cell{fn add(self:&mut Self,x:i32){.x+=x}fn get(self:&Self)->i32=>.x}
fn make()->impl FnMut(i32)->i32{var c=Cell{x:0};return move(x:i32)->i32=>{c.add(x);if x<0{return c.get()};return c.get()}}
fn main()->i32{var f=make();if f(10)!=10 || f(-3)!=7 || f(5)!=12{return 1};return 0}
"#,
    );
}
#[test]
fn nonmove_method_mutation_updates_actual_outer_record() {
    native(
        r#"
struct Cell{x:i32}
impl Cell{fn add(self:&mut Self,x:i32){.x+=x}}
fn main()->i32{var c=Cell{x:2};if true {var f=(x:i32)=>{c.add(x)};f(40)};return c.x-42}
"#,
    );
}
#[test]
fn methods_on_nested_copy_fields_and_borrowed_capture_keep_writeback() {
    native(
        r#"
struct Cell{x:i32}
struct Pair{c:Cell,y:i32}
impl Cell{fn add(self:&mut Self,x:i32){.x+=x}fn get(self:&Self)->i32=>.x}
fn main()->i32{var p=Pair{c:Cell{x:1},y:9};var f=move()->i32=>{p.c.add(1);return p.c.get()};if f()!=2 || f()!=3{return 1};var c=Cell{x:8};if true {let a=&mut c;var g=()->i32=>{a.add(1);return a.get()};if g()!=9 || g()!=10{return 2}};return c.x-10}
"#,
    );
}
#[test]
fn by_value_and_slice_len_and_function_fields_preserve_existing_resolution() {
    native(
        r#"
struct Cell{x:i32}
impl Cell{fn plus(self:Self,x:i32)->i32=>.x+x}
fn add(x:i32)->i32=>x+2
struct Ops{call:fn(i32)->i32}
fn main()->i32{let c=Cell{x:20};let ops=Ops{call:add};let a=[1u8,2u8];let f=()->i32=>c.plus(20)+ops.call(i32((&a[..]).len()));return f()-44}
"#,
    );
}
#[test]
fn immutable_positions_and_shared_aliases_cannot_gain_mutable_receiver_permission() {
    for source in [
        "struct C{x:i32}impl C{fn set(self:&mut Self){.x=9}}fn main(){let c=C{x:0};var f=()=>{c.set()};f()}",
        "struct C{x:i32}impl C{fn set(self:&mut Self){.x=9}}fn main(){var c=C{x:0};let a=&c;var f=()=>{a.set()};f()}",
        "struct C{x:i32}impl C{fn set(self:&mut Self){.x=9}}fn main()->i32{var c=C{x:0};let a=&c;var f=()=>{c.set()};f();return a.x}",
    ] {
        let r = compile(source);
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    }
}
#[test]
fn method_arity_and_signature_mismatch_remain_rejected() {
    for source in [
        "struct C{x:i32}impl C{fn add(self:&Self,x:i32)->i32=>.x+x}fn main(){let c=C{x:0};let f=()=>c.add();f()}",
        "struct C{x:i32}impl C{fn add(self:&Self,x:i32)->i32=>.x+x}fn main(){let c=C{x:0};let f=()=>c.add(true);f()}",
    ] {
        let r = compile(source);
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    }
}
