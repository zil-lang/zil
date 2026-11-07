//! Non-author probes: definitions must carry their own mode and alias evidence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "review178-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn check(definition: &str, accepted: bool) {
    let text = format!("{definition}\nfn main() {{}}\n");
    let path = directory();
    fs::write(path.join("input.zil"), &text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "definition178.zil", text);
    let report = compile_source(&source, &CompileOptions::default());
    fs::write(path.join("result.json"), report.to_json()).unwrap();
    if accepted {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    } else {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{definition}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(report.diagnostics.iter().any(|d| d.phase == Phase::Expand));
    }
}

#[test]
fn unused_create_result_cannot_claim_an_incoming_store_is_fresh() {
    check(
        "fn bad<T:StoreElement>(give incoming:Store<T>)->Store<T> contract{result(create)} { return give incoming }",
        false,
    );
}

#[test]
fn unused_fresh_local_may_be_returned_by_explicit_give() {
    check(
        "fn good<T:StoreElement>()->Store<T> contract{result(create)} { let fresh:Store<T>=store_new(); return give fresh }",
        true,
    );
}

#[test]
fn descriptor_rebinding_never_transfers_its_source_carrier() {
    for definition in [
        "fn bad<T:StoreElement>(give source:Store<T>)->i64 { var alias=source; let alias2=alias; let moved=give source; return store_len(alias2) }",
        "fn bad<T:StoreElement>(source:Store<T>)->Store<T> contract{result(create)} { return source }",
        "fn take<T:StoreElement>(give source:Store<T>)->Store<T> contract{result(give)}=>give source;fn bad<T:StoreElement>(source:Store<T>)->Store<T> contract{result(give)}=>take(source)",
        "package Owner<T:StoreElement>{manages items:Store<T>}fn bad<T:StoreElement>(owner:Owner<T>)->i64{var next=owner;let first=next.items;let second=first;next.items=store_new();return store_len(second)}",
    ] {
        check(definition, false);
    }
}

#[test]
fn raw_and_wire_reject_erasing_the_factory_result_mode() {
    use zil_ir::PassingMode;
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw-review178.zil",
        "resource R { n:i32 }package P { manages r:R }fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new();fn main(){let s:Store<P>=empty();}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), candidate.clone(), &target, &limits).unwrap();
    let mut forged = original;
    let factory = forged
        .functions
        .iter_mut()
        .find(|f| f.result_mode == PassingMode::Create)
        .unwrap();
    factory.result_mode = PassingMode::Infer;
    let mut candidate = candidate;
    candidate.module_fingerprint = forged.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&forged, &candidate, &limits).unwrap();
    let path = directory();
    for (label, result) in [
        (
            "raw",
            zil_verify::verify_module(forged, candidate, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ),
    ] {
        let diagnostics = result.unwrap_err();
        fs::write(
            path.join(format!("{label}.txt")),
            format!("{diagnostics:#?}"),
        )
        .unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject)
        );
    }
}

#[test]
fn callable_resource_exception_does_not_open_noncallable_generic_give() {
    let source = "resource R { n:i32 } fn bad<T>(give owned:R)->i32=>owned.n; fn main() {}";
    let input = SourceUnit::new(FileId(0), ModuleId(0), "helper-boundary178.zil", source);
    let report = compile_source(&input, &CompileOptions::default());
    let path = directory();
    fs::write(path.join("input.zil"), source).unwrap();
    fs::write(path.join("result.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == Phase::Expand && d.message.contains("callable helper"))
    );
}
