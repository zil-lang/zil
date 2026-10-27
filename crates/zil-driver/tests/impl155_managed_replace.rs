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
fn native(source: &str, expected: &[u8]) {
    native_exit(source, expected, 0)
}
fn native_exit(source: &str, expected: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "managed155-{}-{}",
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
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
        assert_eq!(out.stdout, expected);
    }
}

#[test]
fn byte_buffer_can_be_replaced_and_refilled() {
    native(
        "fn main()->i32{var b=bytes_new();bytes_reserve(b,64);bytes_push(b,9u8);b=bytes_new();if bytes_len(b)!=0{return 1};bytes_reserve(b,64);bytes_push(b,42u8);return i32(bytes_view(b)[0])-42}",
        b"",
    );
}
#[test]
fn replacement_keeps_slot_order_and_disposes_old_value_once() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32; resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}} fn main()->i32{var a=R{code:65};let b=R{code:66};a=R{code:67};return 0}"#,
        b"ABC",
    );
}
#[test]
fn replacement_through_branches_and_loops_uses_one_slot() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32; resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}} fn main()->i32{var a=R{code:65};for i in 0..3 {if i==1{continue};a=R{code:66+i}};return 0}"#,
        b"ABD",
    );
}
#[test]
fn owning_fnmut_can_replace_captured_buffer_without_manual_package() {
    native(
        r#"fn make()->impl FnMut(i32)->i32{var b=bytes_new();return move(n:i32)->i32=>{if n==0 {b=bytes_new()}else{bytes_reserve(b,64);bytes_push(b,u8(n))};return i32(bytes_len(b))}} fn main()->i32{var f=make();if f(42)!=1||f(43)!=2||f(0)!=0||f(44)!=1{return 1};return 0}"#,
        b"",
    );
}
#[test]
fn give_and_refill_keep_single_obligation() {
    native(
        r#"fn relay(give b:ByteBuffer)->ByteBuffer contract{result(give)}{return give b} fn main()->i32{var b=bytes_new();bytes_reserve(b,64);bytes_push(b,42u8);let c=relay(give b);b=bytes_new();bytes_reserve(b,64);bytes_push(b,7u8);return i32(bytes_view(c)[0])+i32(bytes_view(b)[0])-49}"#,
        b"",
    );
}
#[test]
fn lexical_and_closure_shadowing_do_not_rewrite_unrelated_bindings() {
    native(
        r#"fn main()->i32{var b=bytes_new();bytes_reserve(b,64);bytes_push(b,1u8);if true{let b=42;if b!=42{return 1}};let f=(b:i32)=>b+1;if f(41)!=42{return 2};let g=move()->i32=>{let b=41;return b+1};if g()!=42{return 3};b=bytes_new();return i32(bytes_len(b))}"#,
        b"",
    );
}
#[test]
fn propagated_rhs_error_preserves_old_slot_until_error_cleanup() {
    native_exit(
        r#"extern "C" fn putchar(x:i32)->i32; enum E{Fail} resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}} fn fail()->E!i32{return E.Fail} fn run()->E!i32{var a=R{code:65};let b=R{code:66};a=R{code:fail()?};return 0} fn main()->E!i32{return run()?}"#,
        b"BA",
        1,
    );
}
#[test]
fn live_old_views_descriptors_and_annotation_mismatch_are_rejected() {
    for source in [
        "fn main()->i32{var b=bytes_new();bytes_reserve(b,64);bytes_push(b,42u8);let v=bytes_view(b);b=bytes_new();return i32(v[0])}",
        "resource R{n:i32} fn main()->i32{var a=R{n:1};let d=a;a=R{n:2};return d.n}",
        "resource R{n:i32} fn main(){var a:R=bytes_new();a=R{n:0}}",
        "fn main(){let b=bytes_new();b=bytes_new()}",
    ] {
        let r = compile(source);
        assert_ne!(r.phase, zil_base::Phase::Parse, "{}", r.to_json());
        assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
    }
}
