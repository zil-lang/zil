//! Explicit slice arguments retain their own synchronous address permissions.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(name: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "slice193-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(name: &str, text: &str, options: CompileOptions) -> (CompileReport, PathBuf) {
    let dir = directory(name);
    fs::write(dir.join("source.zil"), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), format!("{name}.zil"), text);
    let report = compile_source(
        &source,
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            output: Some(dir.join("program")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    (report, dir)
}
fn check(name: &str, text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let (r, _) = compile(name, text, CompileOptions::default());
    assert_eq!(r.status, category, "{name}: {}", r.to_json());
    assert_eq!(r.phase, phase, "{name}: {}", r.to_json());
    if code.is_empty() {
        assert!(r.diagnostics.is_empty(), "{}", r.to_json());
    } else {
        assert!(
            r.diagnostics.iter().any(|d| d.code == code),
            "{name}: {}",
            r.to_json()
        );
    }
    if phase == Phase::Verify {
        assert_eq!(r.verification_runs, 1, "{}", r.to_json());
    }
}
fn native(name: &str, text: &str, success: bool) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let (r, dir) = compile(
            name,
            text,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                ..CompileOptions::default()
            },
        );
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let program = dir.join("program");
        let output = Command::new(&program).output().unwrap();
        fs::write(dir.join("command.txt"), format!("{}\n", program.display())).unwrap();
        fs::write(dir.join("stdout.log"), &output.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            dir.join("exit.txt"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
        assert_eq!(output.status.success(), success, "{name}: {output:?}");
        assert!(output.stdout.is_empty(), "{name}: {output:?}");
        if success {
            assert!(output.stderr.is_empty(), "{name}: {output:?}");
        }
    }
}

#[test]
fn original_nineteen_preflight_inputs_now_reach_the_promised_stage_unchanged() {
    for (name, text, status, phase, code) in [
        (
            "ordinary_shared",
            "fn read(s: &[i32]) -> i32 { return s[0] + s[1] }\nfn main() -> i32 { let values = [7, 9]; return read(&values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "ordinary_exclusive",
            "fn edit(s: &mut [i32]) -> i32 { s[0] += s[1]; return s[0] }\nfn main() -> i32 { var values = [7, 9]; return edit(&mut values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "ordinary_shared_write",
            "fn edit(s: &[i32]) -> i32 { s[0] = 11; return s[0] }\nfn main() -> i32 { let values = [7, 9]; return edit(&values[..]) }\n",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-ADDRESS-PERMISSION",
        ),
        (
            "ordinary_desc_same",
            "fn edit(buffer: ByteBuffer, s: &[u8]) -> i32 { bytes_reserve(buffer, 8); return i32(s[0]) }\nfn main() -> i32 { let buffer = bytes_new(); bytes_push(buffer, 7u8); let view = bytes_view(buffer); return edit(buffer, view) }\n",
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
        (
            "ordinary_desc_other",
            "fn edit(buffer: ByteBuffer, s: &[u8]) -> i32 { bytes_reserve(buffer, 8); return i32(s[0]) }\nfn main() -> i32 { let buffer = bytes_new(); let other = bytes_new(); bytes_push(other, 7u8); let view = bytes_view(other); return edit(buffer, view) - 7 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "ordinary_desc_read",
            "fn read(buffer: ByteBuffer, s: &[u8]) -> i32 { return i32(bytes_len(buffer)) + i32(s[0]) }\nfn main() -> i32 { let buffer = bytes_new(); bytes_push(buffer, 7u8); let view = bytes_view(buffer); return read(buffer, view) - 8 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_shared",
            "fn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => s[0] + s[1]; return f(&values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_exclusive",
            "fn main() -> i32 { var values = [7, 9]; let f = (s: &mut [i32]) => { s[0] += s[1]; return s[0] }; return f(&mut values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_unused",
            "fn main() -> i32 { let f = (s: &[i32]) => 0; return 0 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_shared_write",
            "fn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => { s[0] = 11; return s[0] }; return f(&values[..]) }\n",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "helper_shared",
            "fn apply<F: Fn(&[i32]) -> i32>(f: &F, s: &[i32]) -> i32 { return f(s) }\nfn main() -> i32 { let values = [7, 9]; let f = (s: &[i32]) => s[0] + s[1]; return apply(&f, &values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "helper_exclusive",
            "fn apply<F: Fn(&mut [i32]) -> i32>(f: &F, s: &mut [i32]) -> i32 { return f(s) }\nfn main() -> i32 { var values = [7, 9]; let f = (s: &mut [i32]) => { s[0] += s[1]; return s[0] }; return apply(&f, &mut values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "helper_extra_only",
            "fn apply<F: Fn() -> i32>(f: &F, s: &[i32]) -> i32 { return f() + s[0] }\nfn main() -> i32 { let values = [7, 9]; let f = () => 9; return apply(&f, &values[..]) - 16 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "factory_signature",
            "fn make() -> impl Fn(&[i32]) -> i32 { return (s: &[i32]) => s[0] }\nfn main() -> i32 { let values = [7, 9]; let f = make(); return f(&values[..]) - 7 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_desc_same",
            "fn main() -> i32 { let buffer = bytes_new(); bytes_push(buffer, 7u8); let view = bytes_view(buffer); let f = (s: &[u8]) => { bytes_reserve(buffer, 8); return i32(s[0]) }; return f(view) }\n",
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
        (
            "callback_desc_other",
            "fn main() -> i32 { let buffer = bytes_new(); let other = bytes_new(); bytes_push(other, 7u8); let view = bytes_view(other); let f = (s: &[u8]) => { bytes_reserve(buffer, 8); return i32(s[0]) }; return f(view) - 7 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_desc_read",
            "fn main() -> i32 { let buffer = bytes_new(); bytes_push(buffer, 7u8); let view = bytes_view(buffer); let f = (s: &[u8]) => i32(bytes_len(buffer)) + i32(s[0]); return f(view) - 8 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
        (
            "callback_exclusive_same",
            "fn main() -> i32 { var values = [7, 9]; let f = (a: &[i32], b: &mut [i32]) => { b[0] = a[1]; return b[0] }; return f(&values[..], &mut values[..]) }\n",
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        ),
        (
            "callback_exclusive_other",
            "fn main() -> i32 { let first = [7, 9]; var second = [2, 4]; let f = (a: &[i32], b: &mut [i32]) => { b[0] = a[1]; return b[0] }; return f(&first[..], &mut second[..]) - 9 }\n",
            ResultCategory::Success,
            Phase::Verify,
            "",
        ),
    ] {
        check(name, text, status, phase, code);
    }
}

#[test]
fn current_call_checks_shared_exclusive_and_whole_backing_aliases() {
    check(
        "shared-alias",
        "fn main()->i32{let a=[20,22];let f=(p:&[i32],q:&[i32])=>p[0]+q[1];return f(&a[..],&a[..])-42}",
        ResultCategory::Success,
        Phase::Verify,
        "",
    );
    for (name, text) in [
        (
            "exclusive-unused",
            "fn main()->i32{var a=[1,2];let f=(p:&mut[i32],q:&mut[i32])=>0;return f(&mut a[..],&mut a[..])}",
        ),
        (
            "disjoint-ranges-same-backing",
            "fn main()->i32{var a=[1,2];let f=(p:&mut[i32],q:&mut[i32])=>{p[0]+=q[0];return p[0]};return f(&mut a[..1],&mut a[1..])}",
        ),
        (
            "parent-under-live-child",
            "fn main()->i32{var a=[1,2];let f=(p:&mut[i32])=>{let q=&mut p[..];let n=p[0];return q[0]+n};return f(&mut a[..])}",
        ),
        (
            "exclusive-with-desc-read",
            "fn main()->i32{let b=bytes_new();bytes_push(b,7u8);let f=(p:&mut[u8])=>i32(bytes_len(b))+i32(p[0]);return f(bytes_view_mut(b))}",
        ),
    ] {
        check(
            name,
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn helper_actual_capture_and_slice_backing_are_instantiated_together() {
    for same in [false, true] {
        let backing = if same { "edited" } else { "other" };
        let text = format!(
            "fn apply<F:Fn(&[u8])->i32>(f:&F,p:&[u8])->i32{{return f(p)}}fn main()->i32{{let edited=bytes_new();let other=bytes_new();bytes_push(edited,7u8);bytes_push(other,7u8);let p=bytes_view({backing});let f=(q:&[u8])=>{{bytes_reserve(edited,8);return i32(q[0])}};return apply(&f,p)-7}}"
        );
        check(
            if same { "helper-same" } else { "helper-other" },
            &text,
            if same {
                ResultCategory::SafeReject
            } else {
                ResultCategory::Success
            },
            Phase::Verify,
            if same { "E-BORROW-CONFLICT" } else { "" },
        );
    }
}

#[test]
fn new_parameters_do_not_open_captures_lending_nested_or_noncopy_shapes() {
    for (name, text) in [
        (
            "capture",
            "fn main()->i32{let a=[1,2];let p=&a[..];let f=()=>p[0];return f()}",
        ),
        ("borrowed-result", "fn main(){let f=(p:&[i32])=>p;}"),
        (
            "helper-borrowed-result",
            "fn apply<F:Fn()->i32>(f:&F,p:&[i32])->&[i32]{f();return p}fn main(){let a=[1];let f=()=>0;apply(&f,&a[..]);}",
        ),
        ("bare-slice", "fn main(){let f=(p:[i32])=>0;}"),
        ("nested-slice", "fn main(){let f=(p:&[[i32]])=>0;}"),
        ("nested-address", "fn main(){let f=(p:&&[i32])=>0;}"),
        ("resource-element", "fn main(){let f=(p:&[ByteBuffer])=>0;}"),
    ] {
        check(
            name,
            text,
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn explicit_mutable_arguments_do_not_upgrade_shared_inputs() {
    check(
        "shared-as-exclusive",
        "fn main()->i32{let a=[1,2];let p=&a[..];let f=(q:&mut[i32])=>{q[0]+=1;return q[0]};return f(p)}",
        ResultCategory::SafeReject,
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
}

#[test]
fn sequential_slice_reborrows_and_dead_last_use_execute_without_allocation() {
    native(
        "sequential",
        "fn main()->i32{var a=[20,21];let p=&mut a[..];let f=(q:&mut[i32])=>{let r=&mut q[1..];r[0]+=1;return r[0]};let x=f(p);let y=f(p);let g=(q:&[i32])=>q[0]+q[1];return x+y+g(p)+a[1]-111}",
        true,
    );
    native(
        "dead-last-use",
        "fn main()->i32{let b=bytes_new();bytes_push(b,7u8);let p=bytes_view(b);let f=(q:&[u8])=>i32(q[0]);let n=f(p);if !bytes_reserve(b,8){return 1};return n-7}",
        true,
    );
}

#[test]
fn slice_parameter_index_and_range_checks_reach_machine_traps() {
    native(
        "empty-index",
        "fn main()->i32{let a:[i32;0]=[];let f=(q:&[i32])=>q[0];return f(&a[..])}",
        false,
    );
    native(
        "reversed-range",
        "fn main()->i32{let a=[1,2];let f=(q:&[i32])=>q[0];return f(&a[2..1])}",
        false,
    );
}
