//! Nested source closures retain lexical origins and one actual resource debt.
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
        &SourceUnit::new(FileId(0), ModuleId(0), "nested161.zil", source),
        &CompileOptions::default(),
    )
}
fn native(source: &str, expected: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "nested161-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), source).unwrap();
        let executable = dir.join("program");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "nested161.zil", source),
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
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        fs::write(dir.join("stderr"), &out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
        assert_eq!(out.stdout, expected);
    }
}

#[test]
fn nested_signatures_infer_across_three_lexical_layers() {
    native(
        "fn main()->i32{let x=10;let f=a=>{let g=b=>{let h=c=>x+a+b+c;return h(4)};return g(8)};return f(20)-42}",
        b"",
        0,
    );
}

#[test]
fn activation_local_managed_owner_moves_into_inner_environment() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn main()->i32{let f=()=>{let a=R{code:65};let g=move()=>a.code;return g()};return f()+f()-130}"#,
        b"AA",
        0,
    );
}

#[test]
fn nested_move_transfers_outer_owned_capture_once() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn main()->i32{let a=R{code:65};let f=move()=>{let g=move()=>a.code;return g()};return f()-65}"#,
        b"A",
        0,
    );
}

#[test]
fn nested_move_snapshot_is_distinct_from_borrowed_writeback() {
    native(
        "fn main()->i32{var x=40;let f=()=>{var g=move()=>{x+=1;return x};return g()+g()};if f()!=83||f()!=83||x!=40{return 1};var h=()=>{var g=()=>{x+=1;return x};return g()};if h()!=41||h()!=42{return 2};return x-42}",
        b"",
        0,
    );
}

#[test]
fn inner_borrowed_writes_persist_in_outer_owned_copy_capture() {
    native(
        "fn make()->impl FnMut()->i32{var x=40;return move()=>{var g=()=>{x+=1;return x};return g()}}fn main()->i32{var f=make();if f()!=41||f()!=42{return 1};return 0}",
        b"",
        0,
    );
}

#[test]
fn inner_move_promotes_an_outer_nonmove_managed_capture() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn main()->i32{let a=R{code:65};let f=()=>{let g=move()=>a.code;return g()};return f()-65}"#,
        b"A",
        0,
    );
}

#[test]
fn nested_package_move_and_inner_refill_keep_distinct_responsibilities() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages a:R,manages b:R}fn main()->i32{var p=P{a:R{code:65},b:R{code:66}};let f=move()=>{var g=move()=>{let old=give p.a;p.a=R{code:old.code+1};return p.a.code};return g()+g()-133};return f()}"#,
        b"ABCB",
        0,
    );
}

#[test]
fn outer_package_refill_after_nested_move_keeps_fnmut() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages a:R}fn make()->impl FnMut()->i32{var p=P{a:R{code:65}};return move()=>{let g=move()=>p.a.code;let n=g();p=P{a:R{code:n+1}};return n}}fn main()->i32{var f=make();return f()+f()-131}"#,
        b"ABC",
        0,
    );
}

#[test]
fn nested_shadowed_parameters_and_locals_do_not_capture_outer_names() {
    native(
        "fn main()->i32{var x=100;let f=x=>{let g=x=>{let h=()=>x+1;return h()};return g(x)+1};if f(40)!=42||x!=100{return 1};let unused=bytes_new();let h=()=>{let unused=7;let i=()=>unused;return i()};return h()-7}",
        b"",
        0,
    );
}

#[test]
fn nested_error_exit_cleans_only_actual_remaining_owners() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;enum E{Fail}resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn fail()->E!i32{return E.Fail}fn main()->E!i32{let a=R{code:65};let f=move()->E!i32=>{let b=R{code:66};let g=move()->E!i32=>{let n=a.code;return fail()?};return g()?};return f()?}"#,
        b"AB",
        1,
    );
}

#[test]
fn capturing_a_callable_environment_and_function_pointer_captures_remain_rejected() {
    let report = compile("fn main()->i32{let x=1;let f=()=>x;let g=move()=>f();return g()}");
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    let report =
        compile("fn main()->i32{let x=1;let f=()=>{let g:fn()->i32=()=>x;return g()};return f()}");
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
            .any(|d| d.code == "E-CAPTURED-FN-POINTER")
    );
}

#[test]
fn nested_closure_function_and_depth_budgets_remain_enforced() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "budget161.zil",
        "fn main()->i32{let f=()=>{let g=()=>42;return g()};return f()-42}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    for limits in [
        zil_base::Limits {
            max_functions: 2,
            ..Default::default()
        },
        zil_base::Limits {
            max_call_depth: 4,
            ..Default::default()
        },
    ] {
        let errors =
            zil_types::check(&core, &zil_base::TargetProfile::default(), &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }
}

#[test]
fn independent_verify_rejects_forged_shared_kind_and_second_capture_transfer() {
    use zil_ir::{CallableKind, InstructionKind, ValueHold, ValueId};
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw161.zil",
        "resource R{n:i32}fn main()->i32{let a=R{n:42};let f=move()=>{let g=move()=>a.n;return g()};return f()-42}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    assert_eq!(module.closures[0].kind, CallableKind::FnOnce);
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    for forge_kind in [true, false] {
        let mut bad = module.clone();
        let mut proof = plan.clone();
        let message = if forge_kind {
            bad.closures[0].kind = CallableKind::Fn;
            "moving an owned capture requires a Consume entry"
        } else {
            let function = bad.closures[0].function;
            let f = &mut bad.functions[function.0 as usize];
            let block = f
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions
                        .iter()
                        .any(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
                })
                .unwrap();
            let position = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
                .unwrap();
            let mut second = block.instructions[position].clone();
            let ty = f.values[second.result.unwrap().0 as usize].clone();
            let result = ValueId(f.values.len() as u32);
            f.values.push(ty);
            second.result = Some(result);
            block.instructions.insert(position + 1, second);
            proof
                .functions
                .iter_mut()
                .find(|p| p.function == function)
                .unwrap()
                .holds
                .push(ValueHold {
                    value: result,
                    roots: vec![],
                });
            "captured responsibility was already transferred"
        };
        proof.module_fingerprint = bad.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&bad, &proof, &limits).unwrap();
        for result in [
            zil_verify::verify_module(bad, proof, &target, &limits),
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == zil_base::Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.message.contains(message)),
                "forge_kind={forge_kind}: {errors:?}"
            );
        }
    }
}

#[test]
fn inner_unused_environment_and_outer_unused_environment_drop_once() {
    native(
        r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}fn main()->i32{let a=R{code:65};let f=move()=>{let g=move()=>a.code;return 0};f();let b=R{code:66};let h=move()=>{let i=move()=>b.code;return i()};return 0}"#,
        b"AB",
        0,
    );
}

#[test]
fn nested_owning_buffer_replacement_cleans_real_slots() {
    native(
        "fn main()->i32{var b=bytes_new();bytes_reserve(b,64);bytes_push(b,42u8);var f=move()=>{var g=move()=>{if bytes_len(b)!=1{return 1};b=bytes_new();bytes_reserve(b,64);bytes_push(b,7u8);return i32(bytes_view(b)[0])-7};return g()};return f()}",
        b"",
        0,
    );
}

#[test]
fn nested_transfer_and_shared_alias_misuse_are_rejected() {
    for source in [
        "resource R{n:i32}fn main()->i32{let a=R{n:1};let f=move()=>{let g=move()=>a.n;return g()};return f()+f()}",
        "resource R{n:i32}fn main()->i32{let a=R{n:1};let f=move()=>{let g=move()=>a.n;return a.n+g()};return f()}",
        "fn main()->i32{var x=0;let a=&x;var f=()=>{var g=()=>{x+=1;return x};return g()};return f()+*a}",
    ] {
        let report = compile(source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}
