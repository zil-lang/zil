//! Author regression evidence for explicit synchronous shared Product address
//! captures. These executable checks are not a non-author review receipt.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind as I, TypedModule};

const SOURCE: &str = include_str!("../../../examples/mainline/shared_product_capture226/local.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "capture226-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "capture226.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn verified(label: &str, text: &str) -> (TypedModule, CandidatePlan) {
    let path = dir(label);
    fs::write(path.join("source.zil"), text).unwrap();
    let module = typed(text);
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
            &SourceUnit::new(FileId(0), ModuleId(0), "capture226.zil", text),
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
fn explicit_local_shared_product_capture() {
    verified("local", SOURCE);
    native("local", SOURCE, "A", 0);
}
#[test]
fn explicit_address_move_is_not_product_ownership() {
    let source = SOURCE.replace("let callback = () =>", "let callback = move () =>");
    verified("move-address", &source);
    native("move-address", &source, "A", 0);
}
#[test]
fn direct_child_and_alias_preserve_whole_owner() {
    let source = SOURCE
        .replace(
            "let view = &item",
            "let first = &item.child; let view = first",
        )
        .replace("view.child.value + view.tag", "view.value")
        .replace("value - 10", "value - 7");
    verified("child-alias", &source);
    native("child-alias", &source, "A", 0);
}
#[test]
fn current_loan_blocks_whole_owner_move() {
    verified("loan-valid-control", SOURCE);
    let source = SOURCE.replace(
        "let value = callback()\n    let moved = item",
        "let moved = item; let value = callback()",
    );
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "bad226.zil", &source),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}
#[test]
fn dynamic_store_origins_do_not_acquire_capture_permission() {
    let source = SOURCE.replace("let view = &item", "let items: Store<Item> = store_new(); store_reserve(items, 1); store_push(items, item); let view = store_at(items, 0)")
        .replace("let moved = item", "store_pop(items)");
    let module = typed(&source);
    let diagnostics = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("ordinary local Product origins")),
        "{diagnostics:?}"
    );
}
#[test]
fn incoming_product_address_remains_conservatively_closed() {
    let source = SOURCE.replace(
        "fn main()",
        "fn read(p: &Item) -> i32 { let cb = () => p.tag; return cb() }\nfn main()",
    );
    let module = typed(&source);
    let d = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        d.iter()
            .any(|d| d.message.contains("ordinary local Product origins")),
        "{d:?}"
    );
}

#[test]
fn raw_store_operand_cannot_borrow_ordinary_capture_authority() {
    let source = SOURCE.replace("let view = &item", "let items: Store<Item> = store_new(); store_reserve(items, 1); store_push(items, Item { child: Leaf { ticket: Ticket { code: 66 }, value: 9 }, tag: 4 }); let window = store_at(items, 0); let view = &item");
    let (mut module, mut plan) = verified("origin-attack-control", &source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let store_value = f
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
    for block in &mut f.blocks {
        for instruction in &mut block.instructions {
            if let I::MakeClosure { captures, .. } = &mut instruction.kind {
                assert_eq!(captures.len(), 1);
                assert_eq!(
                    f.values[captures[0].0 as usize],
                    f.values[store_value.0 as usize]
                );
                captures[0] = store_value;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let path = dir("origin-attack");
    fs::write(path.join("source-control.zil"), source).unwrap();
    fs::write(path.join("mutated.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("candidate.wire"), &bytes).unwrap();
    let analyzed = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message.contains("ordinary local Product origins")),
        "{analyzed:?}"
    );
    fs::write(path.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(path.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::Unsupported
                    && d.message.contains("ordinary local Product origins")),
            "{errors:#?}"
        );
    }
}
#[test]
fn branch_origins_keep_both_actual_ordinary_owners_live() {
    let source=SOURCE.replace("let view = &item", "let other = Item { child: Leaf { ticket: Ticket { code: 66 }, value: 8 }, tag: 4 }; let view = if true { &item } else { &other }");
    verified("two-owners", &source);
    native("two-owners", &source, "AB", 0);
}
