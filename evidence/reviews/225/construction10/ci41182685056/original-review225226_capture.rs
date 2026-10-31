//! Independent construction10 review of the exact 225/226 implementations.
//! Invalid source/IR is verified only; only positive controls reach native code.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str = r#"
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
"#;
const ITEM: &str = "Item{child:Leaf{ticket:Ticket{code:65},value:7},tag:3}";
const OTHER: &str = "Item{child:Leaf{ticket:Ticket{code:66},value:8},tag:4}";

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "review225226-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review225226.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), text).unwrap();
    let module = typed(text);
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
    fs::write(dir.join("candidate.wire"), wire).unwrap();
    (module, plan)
}

fn unsupported<T>(result: Result<T, Vec<Diagnostic>>, marker: &str) {
    let errors = match result {
        Ok(_) => panic!("expected independent rejection containing {marker}"),
        Err(errors) => errors,
    };
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported && d.message.contains(marker)),
        "{errors:#?}"
    );
    assert!(errors.iter().all(|d| !d.code.starts_with("E-VERIFY-PLAN")));
}

fn attack(label: &str, module: TypedModule, mut plan: CandidatePlan, marker: &str) {
    plan.module_fingerprint = module.fingerprint();
    let dir = directory(label);
    fs::write(dir.join("mutated.txt"), format!("{module:#?}")).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    fs::write(dir.join("candidate.wire"), &wire).unwrap();
    unsupported(zil_analyze::analyze(&module, &Limits::default()), marker);
    unsupported(
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()),
        marker,
    );
    unsupported(
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()),
        marker,
    );
}

fn native(label: &str, text: &str, expected: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        fs::write(dir.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "review225226.zil", text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                emit_dir: Some(dir.join("phases")),
                output: Some(dir.join("program")),
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
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn two_nonempty_same_type_stores_preserve_payloads_and_cleanup() {
    let text = format!(
        "{MODEL} fn make()->impl Fn()->i32{{
        let first:Store<Item>=store_new();store_reserve(first,1);store_push(first,{ITEM});
        let second:Store<Item>=store_new();store_reserve(second,1);store_push(second,{OTHER});
        return move()=>{{let a=store_at(first,0);let b=store_at(second,0);return a.child.value*10+b.child.value}}
        }} fn main()->i32{{let callback=make();return callback()-78}}"
    );
    control("two-nonempty", &text);
    native("two-nonempty", &text, "AB");
}

#[test]
fn callable_hidden_in_managed_resource_head_is_not_capture_authority() {
    let text = "resource Hidden{code:fn(i32)->i32} package Leaf{manages owner:Hidden} package Root{child:Leaf} fn identity(n:i32)->i32{return n} fn main()->i32{let items:Store<Root>=store_new();let n=7;let callback=move()=>n;return callback()-7}";
    let (mut module, plan) = control("resource-head-control", text);
    let payload = module
        .resources
        .iter()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap()
        .payload
        .clone();
    assert_eq!(module.closures.len(), 1);
    module.closures[0].captures[0] = Type::Managed(Box::new(payload));
    attack("resource-head-attack", module, plan, "cannot hide");
    let owning = text.replace(
        "let callback=move()=>n",
        "let callback=move()=>i32(store_len(items))",
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "hidden225.zil", &owning),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

fn branches(mixed: bool) -> String {
    let other = if mixed { "store_at(items,0)" } else { "&other" };
    format!(
        "{MODEL} fn run(flag:bool)->i32{{
        let item={ITEM};let other={OTHER};let items:Store<Item>=store_new();store_reserve(items,1);
        store_push(items,Item{{child:Leaf{{ticket:Ticket{{code:67}},value:9}},tag:5}});
        let window=store_at(items,0);let view=if flag{{&item}}else{{{other}}};
        let callback=()=>view.child.value;return callback()
        }} fn main()->i32{{return run(true)-7}}"
    )
}

#[test]
fn mixed_ordinary_and_dynamic_branch_origins_are_rejected() {
    control("ordinary-branch-control", &branches(false));
    let module = typed(&branches(true));
    unsupported(
        zil_analyze::analyze(&module, &Limits::default()),
        "ordinary local Product origins",
    );
}

#[test]
fn raw_one_branch_store_injection_cannot_reuse_ordinary_origin_plan() {
    let (mut module, plan) = control("branch-injection-control", &branches(false));
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "run")
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
    let borrowed: Vec<_> = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| {
            matches!(i.kind, I::BorrowLocal { .. })
                .then_some(i.result)
                .flatten()
        })
        .collect();
    let mut changed = 0;
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            if let I::Store { value, .. } = &mut instruction.kind
                && borrowed.contains(value)
                && function.values[value.0 as usize] == function.values[window.0 as usize]
                && changed == 0
            {
                *value = window;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    attack(
        "mixed-branch-attack",
        module,
        plan,
        "ordinary local Product origins",
    );
}

#[test]
fn child_take_and_replacement_wait_for_last_actual_callback_use() {
    for (label, operation, declaration, stdout) in [
        ("take", "let moved=item.child", "let", "A"),
        (
            "replace",
            "item.child=Leaf{ticket:Ticket{code:66},value:8}",
            "var",
            "AB",
        ),
    ] {
        let good = format!(
            "{MODEL} fn main()->i32{{{declaration} item={ITEM};let view=&item.child;let callback=()=>view.value;let value=callback();{operation};return value-7}}"
        );
        control(label, &good);
        native(label, &good, stdout);
        let bad = good.replace(
            &format!("let value=callback();{operation}"),
            &format!("{operation};let value=callback()"),
        );
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "active226.zil", &bad),
            &CompileOptions::default(),
        );
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
fn ordinary_branch_union_keeps_each_possible_owner_live() {
    for name in ["item", "other"] {
        let good = format!(
            "{MODEL} fn run(flag:bool)->i32{{let item={ITEM};let other={OTHER};let view=if flag{{&item}}else{{&other}};let callback=()=>view.tag;let value=callback();let moved={name};return value}}fn main()->i32{{return run(true)+run(false)-7}}"
        );
        control("owner-union", &good);
        let bad = good.replace(
            &format!("let value=callback();let moved={name}"),
            &format!("let moved={name};let value=callback()"),
        );
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "union226.zil", &bad),
            &CompileOptions::default(),
        );
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
                .any(|d| d.code == "E-BORROW-CONFLICT")
        );
    }
}

#[test]
fn zero_storage_ordinary_product_capture_does_not_need_store_layout() {
    let text = "resource Empty{}package Leaf{manages owner:Empty}package Root{child:Leaf}fn inspect(p:&Root)->i32{return 7}fn main()->i32{let root=Root{child:Leaf{owner:Empty{}}};let view=&root;let callback=()=>inspect(view);let n=callback();let moved=root;return n-7}";
    control("zero-storage-shared", text);
    native("zero-storage-shared", text, "");
}

#[test]
fn one_environment_keeps_owned_store_and_shared_product_responsibilities_separate() {
    let good = format!(
        "{MODEL} fn consume(item:Item){{}} fn main()->i32{{
        let item={ITEM};let items:Store<Item>=store_new();store_reserve(items,1);store_push(items,{OTHER});
        let view=&item;let callback=move()=>{{let stored=store_at(items,0);return stored.child.value+view.child.value}};
        let value=callback();consume(item);return value-15}}"
    );
    control("mixed-owned-and-shared", &good);
    // consume destroys A immediately after the final call. The still-live
    // environment later drops its owned Store's B without using view again.
    native("mixed-owned-and-shared", &good, "AB");
    let bad = good.replace(
        "let value=callback();consume(item)",
        "consume(item);let value=callback()",
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "mixed225226.zil", &bad),
        &CompileOptions::default(),
    );
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
