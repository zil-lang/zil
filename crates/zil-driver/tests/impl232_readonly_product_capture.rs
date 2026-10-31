//! Source and independent-kernel checks for ordinary Product default captures.
//! Author checks do not replace the separate non-author review receipt.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CallableKind, CandidatePlan, InstructionKind as I, Type, TypedModule};

const SOURCE: &str = include_str!("../../../examples/mainline/readonly_product232/local.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "capture232-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> Result<TypedModule, Vec<Diagnostic>> {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "capture232.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default())
}
fn verified(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let path = dir(label);
    fs::write(path.join("source.zil"), text).unwrap();
    let module = typed(text).unwrap_or_else(|errors| panic!("{text}\n{errors:#?}"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    fs::write(path.join("candidate.wire"), &bytes).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    (module, plan)
}
fn shared_capture(module: &TypedModule) {
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::Fn);
    assert!(
        matches!(&module.closures[0].captures[..], [Type::Address { mutable: false, inner }] if matches!(inner.as_ref(), Type::Package { .. }))
    );
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let capture = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::MakeClosure { captures, .. } = &i.kind {
                Some(captures[0])
            } else {
                None
            }
        })
        .unwrap();
    let address = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if i.result == Some(capture)
                && let I::ReborrowAddress {
                    address,
                    mutable: false,
                } = i.kind
            {
                Some(address)
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        main.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| i.result == Some(address) && matches!(i.kind, I::BorrowLocal { .. }))
    );
}
fn native(label: &str, text: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = dir(label);
        fs::write(path.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "capture232.zil", text),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            assert!(!summary.object_reports.is_empty());
            assert!(
                summary
                    .object_reports
                    .iter()
                    .all(|r| r.contains("\"input_kind\":\"builder\""))
            );
        }
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        fs::write(path.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty());
    }
}
#[test]
fn default_recursive_product_read_is_a_repeatable_borrow() {
    let (module, _) = verified("recursive", SOURCE);
    shared_capture(&module);
    native("recursive", SOURCE, "A", 0);
}
#[test]
fn default_flat_product_read_is_a_repeatable_borrow() {
    let source = SOURCE
        .replace(
            "let item = Item { child: Leaf { ticket: Ticket { code: 65 }, value: 7 }, tag: 3 }",
            "let item = Leaf { ticket: Ticket { code: 65 }, value: 10 }",
        )
        .replace("item.child.value + item.tag", "item.value");
    let (module, _) = verified("flat", &source);
    shared_capture(&module);
    native("flat", &source, "A", 0);
}
#[test]
fn shared_resource_head_and_explicit_local_reborrow_do_not_take_children() {
    let head = SOURCE
        .replace(
            "item.child.value + item.tag",
            "item.child.ticket.code + item.tag",
        )
        .replace("value - 20", "value - 136");
    let (module, _) = verified("head", &head);
    shared_capture(&module);
    native("head", &head, "A", 0);
    let alias = SOURCE.replace(
        "return item.child.value + item.tag",
        "let view = &item; let child = &view.child; return child.value + view.tag",
    );
    let (module, _) = verified("alias", &alias);
    shared_capture(&module);
    native("alias", &alias, "A", 0);
}
#[test]
fn explicit_move_keeps_owning_repeatable_environment() {
    let source = SOURCE
        .replace("let callback = () =>", "let callback = move () =>")
        .replace("    let moved = item\n", "");
    let (module, _) = verified("move", &source);
    assert_eq!(module.closures[0].kind, CallableKind::Fn);
    assert!(matches!(
        module.closures[0].captures[0],
        Type::Package { .. }
    ));
    native("move", &source, "A", 0);
}
#[test]
fn read_then_real_child_take_infers_owning_fnonce() {
    let source = SOURCE.replace("return item.child.value + item.tag", "let before = item.child.value; let taken = item.child; return before + taken.value + item.tag")
        .replace("callback() + callback()", "callback()").replace("    let moved = item\n", "").replace("value - 20", "value - 17");
    let (module, _) = verified("read-take", &source);
    assert_eq!(module.closures[0].kind, CallableKind::FnOnce);
    assert!(matches!(
        module.closures[0].captures[0],
        Type::Package { .. }
    ));
    native("read-take", &source, "A", 0);
}
#[test]
fn nested_real_ownership_use_is_not_hidden_by_outer_reads() {
    let source = SOURCE.replace("return item.child.value + item.tag", "let before = item.child.value + item.tag; let inner = () => { let taken = item.child; return taken.value }; return before + inner()")
        .replace("callback() + callback()", "callback()").replace("    let moved = item\n", "").replace("value - 20", "value - 17");
    let (module, _) = verified("nested-take", &source);
    assert_eq!(module.closures.len(), 2);
    assert!(
        module
            .closures
            .iter()
            .all(|c| c.kind == CallableKind::FnOnce)
    );
    assert!(matches!(
        module.closures[0].captures[0],
        Type::Package { .. }
    ));
    native("nested-take", &source, "A", 0);
    let flat = SOURCE.replace("let item = Item { child: Leaf { ticket: Ticket { code: 65 }, value: 7 }, tag: 3 }", "let item = Leaf { ticket: Ticket { code: 65 }, value: 10 }")
        .replace("return item.child.value + item.tag", "let before = item.value; let inner = () => { let taken = item; return taken.value }; return before + inner()")
        .replace("callback() + callback()", "callback()").replace("    let moved = item\n", "");
    let (module, _) = verified("nested-flat-take", &flat);
    assert!(
        module
            .closures
            .iter()
            .all(|c| c.kind == CallableKind::FnOnce)
    );
    native("nested-flat-take", &flat, "A", 0);
}
#[test]
fn live_shared_capture_rejects_owner_move_and_child_replacement() {
    let moved = SOURCE.replace(
        "let value = callback() + callback()\n    let moved = item",
        "let moved = item; let value = callback() + callback()",
    );
    let replaced = SOURCE.replace("let item = Item", "var item = Item")
        .replace("let value = callback() + callback()", "item.child = Leaf { ticket: Ticket { code: 66 }, value: 9 }; let value = callback() + callback()");
    for source in [moved, replaced] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "bad232.zil", &source),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn mutable_product_capture_and_unknown_incoming_roots_remain_closed() {
    let written = SOURCE
        .replace("let item = Item", "var item = Item")
        .replace(
            "return item.child.value + item.tag",
            "item.tag = 4; return item.tag",
        );
    let errors = typed(&written).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("mutable Product capture")),
        "{errors:?}"
    );
    let incoming = SOURCE.replace(
        "fn main()",
        "fn read(p: Item) -> i32 { let cb = () => p.tag; return cb() }\nfn main()",
    );
    let errors = typed(&incoming).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("parameter and outer capture origins")),
        "{errors:?}"
    );
    let source = SOURCE.replace(
        "fn main()",
        "fn read(p: &Item) -> i32 { let cb = () => p.tag; return cb() }\nfn main()",
    );
    let module = typed(&source).unwrap();
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("ordinary local Product origins")),
        "{errors:?}"
    );
}
#[test]
fn identical_product_types_keep_distinct_capture_roots() {
    let source = SOURCE.replace("let callback =", "let other = Item { child: Leaf { ticket: Ticket { code: 66 }, value: 9 }, tag: 4 }; let callback =")
        .replace("item.child.value + item.tag", "item.child.value + other.child.value")
        .replace("value - 20", "value - 32");
    let (module, _) = verified("two-roots", &source);
    assert_eq!(module.closures[0].captures.len(), 2);
    assert_eq!(
        module.closures[0].captures[0],
        module.closures[0].captures[1]
    );
    native("two-roots", &source, "AB", 0);
}
#[test]
fn raw_store_window_cannot_replace_ordinary_default_capture() {
    let source = SOURCE.replace("let callback =", "let items: Store<Item> = store_new(); store_reserve(items, 1); store_push(items, Item { child: Leaf { ticket: Ticket { code: 66 }, value: 9 }, tag: 4 }); let window = store_at(items, 0); let callback =");
    let (mut module, mut plan) = verified("raw-control", &source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let store = f
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
    for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::MakeClosure { captures, .. } = &mut instruction.kind {
            assert_eq!(captures.len(), 1);
            assert_eq!(f.values[captures[0].0 as usize], f.values[store.0 as usize]);
            captures[0] = store;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("ordinary local Product origins")),
        "{errors:?}"
    );
    for result in [
        zil_verify::verify_module(module, plan, &target, &Limits::default()),
        zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::Unsupported
                    && d.message.contains("ordinary local Product origins")),
            "{errors:?}"
        );
    }
}
#[test]
fn uncalled_and_error_exit_cleanup_stays_with_the_actual_owner() {
    let uncalled = SOURCE
        .replace("callback() + callback()", "0")
        .replace("value - 20", "value");
    verified("uncalled-shared", &uncalled);
    native("uncalled-shared", &uncalled, "A", 0);
    let owned = uncalled
        .replace("let callback = () =>", "let callback = move () =>")
        .replace("    let moved = item\n", "");
    verified("uncalled-owned", &owned);
    native("uncalled-owned", &owned, "A", 0);
    let failure = SOURCE.replace("fn main() -> i32", "fn step(fail: bool) -> i64!i32 { if fail { unsafe { putchar(69) }; return i64(9) }; unsafe { putchar(83) }; return 3 }\nfn run(fail: bool) -> i64!i32")
        .replace("let callback = () => { return item.child.value + item.tag }", "let callback = (fail: bool) -> i64!i32 => { let n = item.child.value + item.tag; let extra = step(fail)?; return n + extra }")
        .replace("callback() + callback()", "callback(fail)?").replace("value - 20", "value - 13")
        + "\nfn main() -> i32 { run(false); run(true); return 0 }\n";
    verified("error", &failure);
    native("error", &failure, "SAEA", 0);
}
