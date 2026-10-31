//! Author regression evidence for a complete finite Product in an owned Store
//! capture. These executable checks are not a non-author review receipt.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};

const SOURCE: &str =
    include_str!("../../../examples/mainline/product_store_capture225/readonly.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "capture225-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "capture225.zil", text);
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
            &SourceUnit::new(FileId(0), ModuleId(0), "capture225.zil", text),
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
fn once() -> String {
    SOURCE.replace("impl Fn()", "impl FnOnce()").replace(
        "return move () => { return i32(store_len(items)) }",
        "return () => { let taken = give items; return i32(store_len(taken)) }",
    )
}
#[test]
fn product_capture_raw_wire_and_native_declared_cleanup() {
    verified("readonly", SOURCE);
    native("readonly", SOURCE, "AB", 0);
    let once = once();
    verified("once", &once);
    native("once", &once, "AB", 0);
    let unused = SOURCE.replace("return callback() - 1", "return 0");
    verified("uninvoked", &unused);
    native("uninvoked", &unused, "AB", 0);
}
#[test]
fn repeated_mutation_and_full_push_cleanup() {
    let text=SOURCE.replace("impl Fn()", "impl FnMut()")
        .replace("return move () => { return i32(store_len(items)) }", "return move () => { store_push(items, Item { first: Leaf { ticket: Ticket { code: 67 }, value: 1 }, second: Leaf { ticket: Ticket { code: 68 }, value: 2 }, tag: 4 }); return i32(store_len(items)) }")
        .replace("let callback = make(); return callback() - 1", "var callback = make(); callback(); return callback() - 2");
    verified("mutating-full", &text);
    native("mutating-full", &text, "CDABCD", 0);
}
#[test]
fn two_same_type_actual_stores_remain_distinct() {
    let source=SOURCE.replace("return move () => { return i32(store_len(items)) }", "let other: Store<Item> = store_new(); return move () => { return i32(store_len(items) + store_len(other)) }");
    verified("two", &source);
    native("two", &source, "AB", 0);
}
#[test]
fn child_only_three_level_wrapper_and_package_capture() {
    let source=SOURCE.replace("fn make()", "package Wrap { child: Item }\npackage Bag { manages items: Store<Wrap>; manages side: Ticket }\nfn make()")
        .replace("let items: Store<Item>","let items: Store<Wrap>")
        .replace("store_push(items, Item {", "store_push(items, Wrap { child: Item {")
        .replace("tag: 3 })", "tag: 3 } })")
        .replace("return move () => { return i32(store_len(items)) }", "let bag = Bag { items: give items, side: Ticket { code: 90 } }; return move () => { return i32(store_len(bag.items)) }");
    verified("wrapper-bag", &source);
    native("wrapper-bag", &source, "ABZ", 0);
}
#[test]
fn live_child_window_still_blocks_capture_transfer() {
    let source=SOURCE.replace("return move () => { return i32(store_len(items)) }", "let view = store_at(items, 0); let child = &view.first; let callback = move () => { return i32(store_len(items)) }; child.value; return callback");
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "capture225.zil", &source),
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
}
#[test]
fn hidden_copy_callback_cannot_enter_owned_product_store() {
    let source=SOURCE.replace("package Leaf { manages ticket: Ticket; value: i32 }", "struct Hidden { f: fn(i32) -> i32 }\npackage Leaf { manages ticket: Ticket; value: i32; hidden: Hidden }\nfn identity(x:i32)->i32 {return x}")
        .replace("value: 7", "value: 7, hidden: Hidden { f: identity }").replace("value: 9", "value: 9, hidden: Hidden { f: identity }");
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "capture225.zil", &source),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
}
#[test]
fn raw_canonical_and_actual_capture_mutations_are_rejected_independently() {
    let (module, plan) = verified("raw-control", SOURCE);
    let mut bad = module.clone();
    let def = bad
        .resources
        .iter_mut()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    def.name.push_str("-forged");
    assert!(zil_analyze::analyze(&bad, &Limits::default()).is_err());
    let fake = CandidatePlan::empty_for(&bad);
    let wire = zil_ir::wire::encode_module_candidate(&bad, &fake, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module(bad.clone(), fake, &bad.target, &Limits::default()).is_err());
    assert!(zil_verify::verify_module_bytes(&wire, &bad.target, &Limits::default()).is_err());
    let mut changed = module;
    let mut found = false;
    for f in &mut changed.functions {
        for b in &mut f.blocks {
            for i in &mut b.instructions {
                if let I::MakeClosure { captures, .. } = &mut i.kind
                    && captures.len() == 1
                {
                    captures.clear();
                    found = true;
                }
            }
        }
    }
    assert!(found);
    assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
    let mut plan = plan;
    plan.module_fingerprint = changed.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&changed, &plan, &Limits::default()).unwrap();
    assert!(
        zil_verify::verify_module(changed.clone(), plan, &changed.target, &Limits::default())
            .is_err()
    );
    assert!(zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err());
}

#[test]
fn captured_store_child_views_read_actual_inline_payload() {
    let source=SOURCE.replace("return move () => { return i32(store_len(items)) }", "return move () => { let item = store_at(items, 0); return item.first.value + item.second.value + item.tag }").replace("return callback() - 1", "return callback() - 19");
    verified("read-element", &source);
    native("read-element", &source, "AB", 0);
}
#[test]
fn raw_hidden_callback_in_nested_capture_is_not_a_candidate_permission() {
    // This original ordinary Store is legal. Mutate only the unused environment
    // metadata after source, raw and Wire controls have passed; its canonical
    // Store definition stays exact, so a canonical mismatch cannot mask the gate.
    const CONTROL: &str = "struct Code{f:fn(i32)->i32} resource R{n:i32} package Leaf{manages owner:R;code:Code} package Root{child:Leaf} fn main()->i32{let items:Store<Root>=store_new();let n=7;let c=move()=>n;return c()-7}";
    let (module, original) = verified("hidden-ordinary-control", CONTROL);
    let mut bad = module;
    let payload = bad
        .resources
        .iter()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap()
        .payload
        .clone();
    assert_eq!(bad.closures.len(), 1);
    assert_eq!(bad.closures[0].captures.len(), 1);
    bad.closures[0].captures[0] = Type::Managed(Box::new(payload));
    let path = dir("hidden-attack");
    fs::write(path.join("mutated.txt"), format!("{bad:#?}")).unwrap();
    let analyzed = zil_analyze::analyze(&bad, &Limits::default());
    fs::write(path.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    let errors = analyzed.unwrap_err();
    assert!(errors.iter().any(|d|d.category==ResultCategory::Unsupported&&d.message.contains("cannot hide")),"{errors:#?}");
    let mut plan = original;
    plan.module_fingerprint = bad.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&bad, &plan, &Limits::default()).unwrap();
    fs::write(path.join("candidate.wire"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(bad.clone(), plan, &bad.target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &bad.target, &Limits::default()),
        ),
    ] {
        fs::write(
            path.join(format!("verify-{route}.txt")),
            format!("{result:#?}"),
        )
        .unwrap();
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::Unsupported
                    && d.message.contains("cannot hide")),
            "{errors:#?}"
        );
    }
}
