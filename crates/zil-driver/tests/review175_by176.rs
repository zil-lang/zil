//! Independent 175 review: actual len, scalar subwindows, field and call roots.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source as compile};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir() -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review175-by176-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review175.zil", text)
}
const MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Cell{manages owner:R;samples:[i32;3];tag:i32}
package State{manages head:R;manages left:Store<Cell>;manages right:Store<Cell>}
fn empty()->Store<Cell>contract{result(create)}{return store_new()}
fn cell(n:i32)->Cell=>Cell{owner:R{code:n},samples:[1,2,3],tag:n}
fn state()->State{let a=empty();let b=empty();store_reserve(a,8);store_reserve(b,2);store_push(a,cell(65));store_push(b,cell(66));return State{head:R{code:90},left:give a,right:give b}}
fn relay(s:State)->State=>s
fn id(x:&i32)->&i32=>x
fn inspect(view:&Cell,other:Store<Cell>)->i32{store_reserve(other,7);let point=id(&view.samples[1]);return *point+view.tag}
"#;
fn native(text: &str, input: &[u8], expected: &[u8], trap: bool) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let d = dir();
        fs::write(d.join("source.zil"), text).unwrap();
        fs::write(d.join("stdin"), input).unwrap();
        let exe = d.join("program");
        let report = compile(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(exe.clone()),
                emit_dir: Some(d.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(d.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let mut child = Command::new(exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let out = child.wait_with_output().unwrap();
        fs::write(d.join("native.txt"), format!("{optimization:?} {out:?}\n")).unwrap();
        if trap {
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&out.status),
                Some(4)
            );
        } else {
            assert_eq!(out.status.code(), Some(0), "{out:?}");
        }
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.is_empty());
    }
}
fn rejection(text: &str, code: &str) {
    let d = dir();
    fs::write(d.join("source.zil"), text).unwrap();
    let report = compile(&source(text), &CompileOptions::default());
    fs::write(d.join("reject.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|e| e.code == code),
        "{}",
        report.to_json()
    );
}
#[test]
fn copy_array_subwindow_and_separate_parent_field_work_through_helpers() {
    native(
        &format!(
            "{MODEL}fn main()->i32{{let s=state();let view=store_at(s.left,0);let n=inspect(view,s.right);let moved=relay(s);return n-67}}"
        ),
        b"",
        b"ZAB",
        false,
    );
    native(
        &format!(
            "{MODEL}fn main()->i32{{let s=state();let view=store_at(s.left,0);let point=id(&view.samples[1]);let n=*point;store_reserve(s.left,16);return n-2}}"
        ),
        b"",
        b"ZAB",
        false,
    );
}
#[test]
fn alias_subwindow_and_whole_parent_moves_cannot_cross_live_reads() {
    for body in [
        "let s=state();let view=store_at(s.left,0);return inspect(view,s.left)",
        "let s=state();let view=store_at(s.left,0);let point=id(&view.samples[1]);store_reserve(s.left,16);return *point",
        "let s=state();let view=store_at(s.left,0);let moved=relay(s);return view.tag",
        "var s=state();let view=store_at(s.left,0);let point=id(&view.samples[1]);s.left=empty();return *point",
    ] {
        rejection(
            &format!("{MODEL}fn main()->i32{{{body}}}"),
            "E-BORROW-CONFLICT",
        );
    }
}
#[test]
fn runtime_input_uses_initialized_len_even_when_capacity_is_larger() {
    let text = format!(
        "{MODEL}extern \"C\" fn getchar()->i32;fn main()->i32{{let s=state();var input:i32=0;unsafe{{input=getchar()}};let index:i64=i64(input)-48;return store_at(s.left,index).tag-65}}"
    );
    native(&text, b"0", b"ZAB", false);
    native(&text, b"1", b"", true);
    native(&text, b"/", b"", true);
}
fn candidate(text: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let src = source(text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}
fn raw_rejection(
    label: &str,
    text: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
) {
    let d = dir();
    fs::write(d.join("source.zil"), text).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(d.join(format!("{label}.bin")), &bytes).unwrap();
    for (name, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            d.join(format!("{label}-{name}.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
    }
}
#[test]
fn raw_borrow_ssa_retarget_and_early_parent_transfer_reconstruct_actual_roots() {
    use zil_ir::{InstructionKind as I, Type};
    let text = format!(
        "{MODEL}fn main()->i32{{let s=state();let other=s.right;let view=store_at(s.left,0);store_reserve(other,7);return view.tag-65}}"
    );
    let (mut module, plan) = candidate(&text);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let other = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::DescribePackageField { index: 2, .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let borrow = main
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::OwnedStoreBorrow { .. }))
        .unwrap();
    if let I::OwnedStoreBorrow { store, .. } = &mut borrow.kind {
        *store = other;
    }
    raw_rejection("retarget-borrow-source", &text, module, plan);
    let text = format!(
        "{MODEL}fn main()->i32{{let s=state();let view=store_at(s.left,0);let n=view.tag;let moved=relay(s);return n-65}}"
    );
    let (mut module, plan) = candidate(&text);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = &mut main.blocks[0];
    let pos = block
        .instructions
        .iter()
        .position(|i| {
            matches!(i.kind, I::TakeLocal { local }
                if matches!(main.locals[local.0 as usize].ty, Type::Package { .. }))
        })
        .unwrap();
    let take = block.instructions.remove(pos);
    let borrow = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::OwnedStoreBorrow { .. }))
        .unwrap();
    block.instructions.insert(borrow + 1, take);
    raw_rejection("early-parent-transfer", &text, module, plan);
}
