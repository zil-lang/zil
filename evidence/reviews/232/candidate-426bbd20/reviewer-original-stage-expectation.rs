//! Non-author probes for ordinary local Product default read capture.
//! Only accepted source reaches native execution; forged IR is never lowered.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, HoldRoot, InstructionKind as I, Type, TypedModule};

const MODEL: &str = r#"
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
"#;
const FIRST: &str = "Item{child:Leaf{ticket:Ticket{code:65},value:7},tag:3}";
const SECOND: &str = "Item{child:Leaf{ticket:Ticket{code:66},value:8},tag:4}";
static RUN: OnceLock<String> = OnceLock::new();

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(RUN.get_or_init(|| {
            format!(
                "review232-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        }))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}

fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review232.zil", text)
}

fn typed(text: &str) -> TypedModule {
    let source = unit(text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let out = directory(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let module = typed(text);
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.wire"), wire).unwrap();
    (module, plan)
}

fn native(label: &str, text: &str, stdout: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory(&format!("{label}-{optimization:?}"));
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                emit_dir: Some(out.join("phases")),
                output: Some(out.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        fs::write(
            out.join("exit.txt"),
            format!("{:?}\n", result.status.code()),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, stdout.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty(), "{label}: {result:?}");
    }
}

fn reject(label: &str, text: &str, code: Option<&str>) {
    let out = directory(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let report = compile_source(&unit(text), &CompileOptions::default());
    fs::write(out.join("compile.json"), report.to_json()).unwrap();
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
    if let Some(code) = code {
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn default_flat_and_recursive_capture_keep_owner_and_clean_once() {
    let recursive = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let callback=()=>owner.child.value+owner.tag;let n=callback();let moved=owner;return n+moved.tag-13}}"
    );
    let (module, _) = control("recursive", &recursive);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].captures.len(), 1);
    assert!(
        matches!(&module.closures[0].captures[0], Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::Package {..}))
    );
    native("recursive", &recursive, "A");
    let flat = format!(
        "{MODEL} fn main()->i32{{let owner=Leaf{{ticket:Ticket{{code:65}},value:7}};let callback=()=>owner.value+owner.ticket.code;let n=callback();let moved=owner;return n-72}}"
    );
    control("flat", &flat);
    native("flat", &flat, "A");
}

#[test]
fn two_roots_and_shadowed_names_preserve_actual_storage() {
    let distinct = format!(
        "{MODEL} fn main()->i32{{let first={FIRST};let second={SECOND};let callback=()=>first.child.value*10+second.child.value;let n=callback();let a=first;let b=second;return n-78}}"
    );
    control("two-roots", &distinct);
    native("two-roots", &distinct, "BA");
    let shadow = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let callback=()=>owner.child.value;if true{{let owner={SECOND};return callback()+owner.child.value-15}}else{{return 1}}}}"
    );
    control("shadow", &shadow);
    native("shadow", &shadow, "BA");
}

#[test]
fn live_default_capture_blocks_intersecting_move_take_and_replace() {
    for (label, mutation) in [
        ("whole-move", "let moved=owner;"),
        ("child-take", "let child=owner.child;"),
        (
            "child-replace",
            "owner.child=Leaf{ticket:Ticket{code:66},value:8};",
        ),
    ] {
        let text = format!(
            "{MODEL} fn main()->i32{{var owner={FIRST};let callback=()=>owner.child.value;{mutation}return callback()-7}}"
        );
        reject(label, &text, Some("E-BORROW-CONFLICT"));
    }
}

#[test]
fn whole_consumption_and_uncalled_shared_environment_keep_cleanup() {
    let consume = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let callback=()=>{{let taken=owner;return taken.child.value}};return callback()-7}}"
    );
    control("whole-consume", &consume);
    native("whole-consume", &consume, "A");
    let read_then_take = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let callback=()=>{{let before=owner.child.value;let taken=owner.child;return before+taken.value}};return callback()-14}}"
    );
    control("read-then-take", &read_then_take);
    native("read-then-take", &read_then_take, "A");
    let uncalled = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let callback=()=>owner.child.value;return 0}}"
    );
    control("uncalled", &uncalled);
    native("uncalled", &uncalled, "A");
}

#[test]
fn parameter_outer_capture_and_escaping_environment_remain_closed() {
    for (label, body) in [
        ("address-parameter", "fn read(p:&Item)->i32{let callback=()=>p.child.value;return callback()}fn main()->i32=>0".to_owned()),
        ("owning-parameter", "fn read(p:Item)->i32{let callback=()=>p.child.value;return callback()}fn main()->i32=>0".to_owned()),
        ("outer-capture", format!("fn main()->i32{{let owner={FIRST};let outer=move()=>{{let inner=()=>owner.child.value;return inner()}};return outer()-7}}")),
        ("escaping-local", format!("fn make()->impl Fn()->i32{{let owner={FIRST};return ()=>owner.child.value}}fn main()->i32{{let callback=make();return callback()-7}}")),
    ] {
        reject(label, &format!("{MODEL}{body}"), None);
    }
}

fn rejected_diagnostics<T>(result: Result<T, Vec<Diagnostic>>, marker: &str) {
    let errors = result
        .err()
        .expect("invalid source provenance must be rejected");
    assert!(
        errors.iter().any(|d| d.message.contains(marker)),
        "{errors:#?}"
    );
    assert!(
        errors
            .iter()
            .all(|d| d.category != ResultCategory::ResourceLimit
                && !d.code.starts_with("E-VERIFY-PLAN")),
        "{errors:#?}"
    );
}

fn attack(label: &str, module: TypedModule, mut plan: CandidatePlan, marker: &str) {
    plan.module_fingerprint = module.fingerprint();
    let out = directory(label);
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("plan.txt"), format!("{plan:#?}")).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    let analyze = zil_analyze::analyze(&module, &Limits::default());
    fs::write(out.join("analyze.txt"), format!("{analyze:#?}")).unwrap();
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default());
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    fs::write(out.join("wire.txt"), format!("{decoded:#?}")).unwrap();
    rejected_diagnostics(analyze, marker);
    rejected_diagnostics(raw, marker);
    rejected_diagnostics(decoded, marker);
}

#[test]
fn raw_and_wire_dynamic_store_source_cannot_impersonate_default_local_capture() {
    let text = format!(
        "{MODEL} fn main()->i32{{let owner={FIRST};let items:Store<Item>=store_new();store_reserve(items,1);store_push(items,{SECOND});let window=store_at(items,0);let callback=()=>owner.child.value;return callback()-7}}"
    );
    let (mut module, plan) = control("dynamic-origin-control", &text);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let window = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::OwnedStoreBorrow { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::MakeClosure { captures, .. } = &mut instruction.kind {
            assert_eq!(captures.len(), 1);
            assert_eq!(
                function.values[captures[0].0 as usize],
                function.values[window.0 as usize]
            );
            captures[0] = window;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    attack(
        "dynamic-origin-forgery",
        module,
        plan,
        "ordinary local Product origins",
    );
}

#[test]
fn raw_and_wire_same_type_other_owner_does_not_inherit_loan_permission() {
    let text = format!(
        "{MODEL} fn main()->i32{{let first={FIRST};let second={SECOND};let callback=()=>first.child.value;let moved=second;return callback()-7}}"
    );
    retarget_same_type("same-type", &text);
}

#[test]
fn raw_and_wire_same_type_explicit226_control() {
    let text = format!(
        "{MODEL} fn main()->i32{{let first={FIRST};let second={SECOND};let view=&first;let callback=()=>view.child.value;let moved=second;return callback()-7}}"
    );
    retarget_same_type("same-type-explicit226", &text);
}

fn retarget_same_type(label: &str, text: &str) {
    let (mut module, mut plan) = control(&format!("{label}-control"), text);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let first = function
        .locals
        .iter()
        .find(|l| l.name == "first")
        .unwrap()
        .id;
    let second = function
        .locals
        .iter()
        .find(|l| l.name == "second")
        .unwrap()
        .id;
    let mut changed = 0;
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if matches!(instruction.kind, I::BorrowLocal { local } if local == first) {
            instruction.kind = I::BorrowLocal { local: second };
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let function_plan = plan
        .functions
        .iter_mut()
        .find(|f| f.function == function.id)
        .unwrap();
    let mut roots = 0;
    for root in function_plan.holds.iter_mut().flat_map(|h| &mut h.roots) {
        if *root == HoldRoot::Local(first) {
            *root = HoldRoot::Local(second);
            roots += 1;
        }
    }
    assert!(roots > 0);
    attack(&format!("{label}-retarget"), module, plan, "borrow");
}
