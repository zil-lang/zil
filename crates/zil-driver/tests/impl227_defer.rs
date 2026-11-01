//! Author source-to-machine regression coverage for ordinary synchronous defer.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CallableKind, InstructionKind as I, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const IO: &str = r#"extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}"#;
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "defer227-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "defer227.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = directory(label);
        fs::write(path.join("source.zil"), source).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "defer227.zil", source),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
#[test]
fn declaration_registers_real_consuming_unit_environment() {
    let module = typed("fn main()->i32{let value=7;defer{let seen=value;}return 0}");
    let function = &module.functions[0];
    let registration = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::RegisterDefer { scope, local } => Some((scope, local)),
            _ => None,
        })
        .unwrap();
    let zil_ir::Type::Closure(id) = function.locals[registration.1.0 as usize].ty else {
        panic!("concrete closure required")
    };
    let definition = &module.closures[id.0 as usize];
    assert_eq!(definition.kind, CallableKind::FnOnce);
    assert_eq!(definition.result, zil_ir::Type::Unit);
    assert!(definition.parameters.is_empty());
    for block in &function.blocks {
        for (position, instruction) in block.instructions.iter().enumerate() {
            if let I::RegisterDefer { local, .. } = instruction.kind {
                assert!(
                    matches!(block.instructions[position-1].kind,I::Store{local:actual,..}if actual==local)
                );
            }
            if let I::TakeLocal { local } = instruction.kind
                && local == registration.1
            {
                assert!(
                    matches!(&block.instructions[position+1].kind,I::CallClosureOnce{callee,arguments} if Some(*callee)==instruction.result && arguments.is_empty())
                );
            }
        }
    }
}
#[test]
fn fallthrough_return_and_local_drop_share_construction_order() {
    let source = format!(
        "{IO}\nfn finish(){{let a=Ticket{{code:65}};defer{{let own=Ticket{{code:67}};emit(a.code);emit(68)}}let b=Ticket{{code:66}};}}\nfn selected()->i32{{emit(82);return 7}}fn early()->i32{{defer{{emit(69)}}return selected()}}fn main()->i32{{finish();return early()-7}}"
    );
    native("order-return", &source, "BADCARE", 0);
}
#[test]
fn each_loop_activation_and_conditional_registration_settle_once() {
    let source = format!(
        "{IO}\nfn main()->i32{{defer{{emit(90)}}if false{{defer{{emit(88)}}}}for i in 0..4{{defer{{emit(65+i)}}if i==0{{continue}}if i==2{{break}}}}return 0}}"
    );
    native("loop", &source, "ABCZ", 0);
}
#[test]
fn cleanup_owns_actual_managed_capture_and_discharges_it_once() {
    let source = format!(
        "{IO}\nfn main()->i32{{let token=Ticket{{code:65}};defer{{let owned=give token;emit(68)}}let later=Ticket{{code:66}};return 0}}"
    );
    native("owning", &source, "BDA", 0);
}
#[test]
fn defer_inside_an_ordinary_closure_captures_the_original_outer_binding() {
    let source = format!(
        "{IO}\nfn main()->i32{{let code=65;let callback=()=>{{defer{{emit(code)}}return 7}};return callback()-7}}"
    );
    native("outer-closure", &source, "A", 0);
}
#[test]
fn try_error_cleans_only_successfully_registered_actions() {
    let source = format!(
        "{IO}\nenum Fault{{Bad}}fn value(fail:bool)->Fault!i32{{if fail{{return Fault.Bad}}return 4}}fn run(fail:bool)->Fault!i32{{defer{{emit(65)}}let v=value(fail)?;defer{{emit(66)}}return v}}fn main()->i32{{let good=run(false);let bad=run(true);return 0}}"
    );
    native("try", &source, "BAA", 0);
}
#[test]
fn cleanup_body_has_its_own_loop_boundary() {
    let source = format!(
        "{IO}\nfn main()->i32{{defer{{for i in 0..4{{if i==0{{continue}}emit(65+i);if i==2{{break}}}}}}return 0}}"
    );
    native("inner-loop", &source, "BC", 0);
}
#[test]
fn declaration_binding_survives_later_shadow_and_generated_hygiene() {
    let source = format!(
        "{IO}\nfn main()->i32{{let code=65;defer{{emit(code)}}if true{{let code=88;emit(code)}}#for(i in 0..2) {{let code=66+i;defer{{emit(code)}}}}return 0}}"
    );
    native("hygiene", &source, "XCBA", 0);
}
#[test]
fn invalid_cleanup_control_and_nested_registration_are_rejected() {
    for body in [
        "return",
        "defer{}",
        "break",
        "continue",
        "let v=fail()?",
        "let f=()=>{defer{}};",
    ] {
        let source = format!(
            "enum Fault{{Bad}}fn fail()->Fault!i32{{return Fault.Bad}}fn main()->i32{{defer{{{body}}}return 0}}"
        );
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "defer227-reject.zil", source);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &syntax).unwrap();
        let diagnostics =
            zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
        assert!(
            diagnostics.iter().any(|d| d.code.starts_with("E-DEFER-")),
            "{body}: {diagnostics:?}"
        );
    }
}
#[test]
fn captured_resource_source_stays_live_until_cleanup_invocation() {
    let source = format!(
        "{IO}\nfn main()->i32{{let token=Ticket{{code:65}};defer{{emit(token.code)}}let consumed=give token;return 0}}"
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "defer227-loan.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn types_new_cleanup_work_is_bounded_before_growth() {
    let text = "fn main()->i32{let value=7;defer{let seen=value;}return 0}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "defer227-budget.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let expected = zil_types::check(&core, &target, &defaults).unwrap();
    let run = |n| {
        let limits = Limits {
            max_analysis_steps: n,
            ..defaults.clone()
        };
        match zil_types::check(&core, &target, &limits) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                true
            }
            Err(ds) => {
                assert!(!ds.is_empty());
                assert!(
                    ds.iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:?}"
                );
                false
            }
        }
    };
    let (mut lo, mut hi) = (0, defaults.max_analysis_steps);
    assert!(run(hi));
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid) { hi = mid } else { lo = mid }
    }
    assert!(run(hi));
    assert!(!run(hi - 1));
    assert!(run(hi));
    let path = directory("types-budget");
    fs::write(path.join("source.zil"), text).unwrap();
    fs::write(path.join("boundary.json"),format!("{{\"types_minimum\":{hi},\"n\":true,\"n_minus_one_resource_limit\":true,\"fresh_n\":true}}\n")).unwrap();
}

#[test]
fn deferred_mutation_persists_in_an_outer_move_callback() {
    let source = "fn main()->i32{var count=0;var callback=move()=>{if true{defer{count+=1}}return count};if callback()!=1{return 1}if callback()!=2{return 2}if callback()!=3{return 3}return 0}";
    let module = typed(source);
    let outer = module
        .closures
        .iter()
        .find(|definition| {
            definition.kind == CallableKind::FnMut && definition.captures == vec![zil_ir::Type::I32]
        })
        .unwrap();
    let entry = module
        .functions
        .iter()
        .find(|function| function.id == outer.function)
        .unwrap();
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction.kind, I::WriteCapture { index: 0, .. })),
        "deferred address writes need persistent outer Copy capture writeback"
    );
    native("capture-writeback", source, "", 0);
}

#[test]
fn deferred_mutation_crosses_borrowed_layers_but_keeps_move_snapshots_distinct() {
    let borrowed = "fn main()->i32{var count=0;var callback=move()=>{var inner=()=>{if true{defer{count+=1}}return count};return inner()};if callback()!=1{return 1}if callback()!=2{return 2}if callback()!=3{return 3}return 0}";
    native("capture-writeback-layers", borrowed, "", 0);
    let snapshot = "fn main()->i32{var count=0;var callback=move()=>{var inner=move()=>{if true{defer{count+=1}}return count};let first=inner();let second=inner();return first*10+second};if callback()!=12{return 1}if callback()!=12{return 2}return 0}";
    native("capture-writeback-snapshot", snapshot, "", 0);
}

#[test]
fn deferred_mutation_writes_back_on_early_return_and_try_error() {
    let early = "fn main()->i32{var count=0;var callback=move(flag:bool)=>{if true{defer{count+=1};if flag{return 7}}return count};if callback(true)!=7{return 1}if callback(false)!=2{return 2}if callback(false)!=3{return 3}return 0}";
    native("capture-writeback-return", early, "", 0);
    let error = "fn fail()->void!i32{return ()}fn okay(value:i32)->void!i32{return value}fn main()->void!i32{var count=0;var callback=move(flag:bool)->void!i32=>{if true{defer{count+=1};if flag{let skipped=fail()?;}}return okay(count)};let first=callback(true);if callback(false)?!=2{return 2}if callback(false)?!=3{return 3}return 0}";
    native("capture-writeback-error", error, "", 0);
}
