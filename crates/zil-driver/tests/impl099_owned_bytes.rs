//! Real source admission and nearest counterexamples for owning composition.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CallableKind, HoldRoot, InstructionKind, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const PRELUDE: &str = "package P{manages data:ByteBuffer;n:i64} fn make()->P{return P{data:bytes_new(),n:0}} fn fill(b:ByteBuffer)->bool{return bytes_reserve(b,8)&&bytes_push(b,7u8)} fn view(b:ByteBuffer)->&[u8]{return bytes_view(b)} fn size(b:ByteBuffer)->i64{return bytes_len(b)} fn consume(give b:ByteBuffer)->i64{return bytes_len(b)}";

fn source(body: &str) -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned099.zil",
        format!("{PRELUDE}{body}"),
    )
}
fn typed(body: &str) -> TypedModule {
    let source = source(body);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn check(body: &str, expected_rejection: Option<(&str, &str)>) {
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR").map(|base| {
        let path = PathBuf::from(base).join(format!(
            "source099-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    });
    let input = source(body);
    let report = compile_source(
        &input,
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: directory.as_ref().map(|path| path.join("phases")),
            ..CompileOptions::default()
        },
    );
    if let Some(path) = directory {
        fs::write(path.join("source.zil"), &input.text).unwrap();
        fs::write(path.join("report.json"), report.to_json()).unwrap();
    }
    if let Some((phase, code)) = expected_rejection {
        assert_eq!(report.phase.as_str(), phase, "{body}\n{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            code,
            "{body}\n{}",
            report.to_json()
        );
        assert_eq!(
            report.status,
            if code == "E-UNSUPPORTED" {
                ResultCategory::Unsupported
            } else {
                ResultCategory::SafeReject
            }
        );
    } else {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{body}\n{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn packages_move_return_replace_and_refill_real_byte_owners() {
    for body in [
        "fn relay(p:P)->P{return p}fn entry099()->i32{var p=relay(make());fill(p.data);let old=give p.data;p.data=bytes_new();fill(p.data);return i32(bytes_len(old)+bytes_len(p.data))}",
        "fn entry099()->i32{var p=make();fill(p.data);p.data=bytes_new();return i32(bytes_len(p.data))}",
        "fn entry099()->i32{let p=make();fill(p.data);let s=view(p.data);let n=s.len();let q=p;return i32(n+bytes_len(q.data))}",
        "fn relay(p:P)->P{return p}fn entry099(){}",
    ] {
        check(body, None);
    }
    let module = typed("fn relay(p:P)->P{return p}");
    assert!(
        module
            .resources
            .iter()
            .any(|resource| resource.payload == Type::ByteBuffer)
    );
}

#[test]
fn direct_and_package_owning_factories_have_real_call_modes() {
    for (body, kind) in [
        (
            "fn factory()->impl Fn()->i64{let p=make();return move()=>bytes_len(p.data)}fn entry099()->i32{let f=factory();return i32(f()+f())}",
            CallableKind::Fn,
        ),
        (
            "fn factory()->impl Fn()->i64{let b=bytes_new();return move()=>size(b)}fn entry099()->i32{let f=factory();return i32(f()+f())}",
            CallableKind::Fn,
        ),
        (
            "fn factory()->impl FnMut()->i64{let p=make();return move()=>{fill(p.data);return bytes_len(p.data)}}fn entry099()->i32{var f=factory();return i32(f()+f())}",
            CallableKind::FnMut,
        ),
        (
            "fn factory()->impl FnMut()->i64{let b=bytes_new();return move()=>{bytes_reserve(b,8);bytes_push(b,9u8);return bytes_len(b)}}fn entry099()->i32{var f=factory();return i32(f()+f())}",
            CallableKind::FnMut,
        ),
        (
            "fn factory()->impl FnOnce()->i64{let b=bytes_new();return ()=>consume(give b)}fn entry099()->i32{let f=factory();return i32(f())}",
            CallableKind::FnOnce,
        ),
        (
            "fn factory()->impl FnOnce()->i64{let p=make();return move()=>consume(give p.data)}fn entry099()->i32{let f=factory();return i32(f())}",
            CallableKind::FnOnce,
        ),
    ] {
        assert_eq!(typed(body).closures[0].kind, kind, "{body}");
        check(body, None);
    }
}

#[test]
fn owned_views_are_grounded_in_actual_shells_and_captures() {
    for (body, captured) in [
        (
            "fn entry099()->i64{let p=make();fill(p.data);return view(p.data).len()}",
            false,
        ),
        (
            "fn entry099()->i64{let b=bytes_new();fill(b);let f=move()=>{let s=bytes_view(b);return s.len()};return f()}",
            true,
        ),
    ] {
        let module = typed(body);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let mut seen = false;
        for (function, facts) in module.functions.iter().zip(&plan.functions) {
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if (captured && matches!(instruction.kind, InstructionKind::DescribeCapture { .. }))
                    || (!captured
                        && matches!(
                            instruction.kind,
                            InstructionKind::DescribePackageField { .. }
                        ))
                {
                    let roots = &facts.holds[instruction.result.unwrap().0 as usize].roots;
                    assert!(
                        matches!(roots.as_slice(), [HoldRoot::Capture(_)]) && captured
                            || matches!(roots.as_slice(), [HoldRoot::Local(_)]) && !captured
                    );
                    seen = true;
                }
            }
        }
        assert!(seen);
        zil_verify::verify_module(module, plan, &TargetProfile::default(), &Limits::default())
            .unwrap();
        check(body, None);
    }
}

#[test]
fn live_windows_block_package_field_and_whole_transfers() {
    for body in [
        "fn entry099()->i64{var p=make();let s=bytes_view(p.data);let b=give p.data;return s.len()}",
        "fn entry099()->i64{var p=make();let s=bytes_view(p.data);p.data=bytes_new();return s.len()}",
        "fn entry099()->i64{let p=make();let s=bytes_view(p.data);let q=p;return s.len()}",
        "fn entry099()->i64{let p=make();let s=bytes_view(p.data);let f=move()=>bytes_len(p.data);return s.len()+f()}",
        "fn entry099()->i64{let b=bytes_new();let s=bytes_view(b);let f=move()=>bytes_len(b);return s.len()+f()}",
        "fn entry099()->i64{let p=make();let f=move()=>{let s=bytes_view(p.data);let b=give p.data;return s.len()};return f()}",
        "fn entry099()->i64{let b=bytes_new();let f=move()=>{let s=bytes_view(b);let old=give b;return s.len()};return f()}",
    ] {
        check(body, Some(("verify", "E-BORROW-CONFLICT")));
    }
}

#[test]
fn owned_windows_retain_helper_alias_and_lifetime_boundaries() {
    for (body, phase, code) in [
        (
            "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);bytes_reserve(b,4);return s.len()}fn entry099()->i64{let p=make();return act(p.data,p.data)}",
            "verify",
            "E-BORROW-CONFLICT",
        ),
        (
            "fn entry099()->i64{let p=make();var f=move()=>{let s=bytes_view(p.data);fill(p.data);return s.len()};return f()}",
            "verify",
            "E-BORROW-CONFLICT",
        ),
        (
            "fn entry099()->i64{let b=bytes_new();var f=move()=>{let s=bytes_view(b);fill(b);return s.len()};return f()}",
            "verify",
            "E-BORROW-CONFLICT",
        ),
        (
            "fn escaped(p:P)->&[u8]{return bytes_view(p.data)}fn entry099(){}",
            "verify",
            "E-BORROW-ESCAPE",
        ),
        (
            "fn entry099()->i64{let b=bytes_new();let d=b;let f=move()=>bytes_len(d);return f()}",
            "types",
            "E-UNSUPPORTED",
        ),
        (
            "fn entry099()->i64{let b=bytes_new();let f=()=>bytes_len(b);return f()}",
            "types",
            "E-UNSUPPORTED",
        ),
    ] {
        check(body, Some((phase, code)));
    }
    check(
        "fn act(a:ByteBuffer,b:ByteBuffer)->i64{let s=bytes_view(a);let n=s.len();bytes_reserve(b,4);return n}fn entry099()->i64{let p=make();return act(p.data,p.data)}",
        None,
    );
}

#[test]
fn an_owned_byte_environment_can_lend_its_actual_payload() {
    // Original099 factory input; 184 now carries and independently verifies
    // the actual environment lending source instead of leaving it unsupported.
    check(
        "fn escaped()->impl Fn()->&[u8]{let b=bytes_new();return move()=>bytes_view(b)}fn entry099(){}",
        None,
    );
}

#[test]
fn package_refill_keeps_fnmut_but_partial_error_is_once() {
    check(
        "fn fail(ok:bool)->bool!i64{if ok{return i64(1)}return true}fn factory()->impl FnMut(bool)->bool!i64{var p=make();return move(ok)=>{let n=fail(ok)?;p.data=bytes_new();fill(p.data);return n+bytes_len(p.data)}}fn entry099()->bool!i32{var f=factory();let e=f(false);return i32(f(true)?)}",
        None,
    );
    check(
        "fn fail()->bool!i64{return true}fn factory()->impl FnOnce()->bool!i64{let p=make();return move()=>{let b=give p.data;return fail()?+bytes_len(b)}}fn entry099()->bool!i32{let f=factory();return i32(f()?)}",
        None,
    );
    check(
        "fn fail()->bool!i64{return true}fn factory()->impl FnMut()->bool!i64{let p=make();return move()=>{let b=give p.data;return fail()?+bytes_len(b)}}fn entry099(){}",
        Some(("types", "E-UNSUPPORTED")),
    );
}

#[test]
fn ordinary_owned_aggregates_desc_captures_and_error_payloads_stay_closed() {
    for body in [
        "struct S{data:ByteBuffer}fn entry099(){}",
        "package Bad{data:ByteBuffer;n:i64}fn entry099(){}",
        "enum E{Stop}fn bad()->E!ByteBuffer{return E.Stop}fn entry099(){}",
    ] {
        check(body, Some(("types", "E-UNSUPPORTED")));
    }
}
