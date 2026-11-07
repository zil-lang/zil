//! Independent public target / verified-LIR / heap ABI review.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, TypedModule, wire};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "review181-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
const SOURCE: &str = r#"
resource Mark{n:i32}
package Entry{lead:u16;manages mark:Mark;wide:i64;tail:u8}
fn main()->void!i32 {
 let store:Store<Entry> = store_new();
 store_reserve(store,1);
 store_push(store,Entry{lead:u16(2),mark:Mark{n:3},wide:4,tail:u8(5)});
 let e=store_pop(store)?;
 return e.mark.n+i32(e.wide)-7
}
"#;
fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review181.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::windows_x64_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

#[test]
fn unregistered_target_aliases_cannot_use_a_checked_win64_layout() {
    let (module, plan) = candidate(SOURCE);
    let limits = Limits::default();
    for triple in [
        "x86_64-pc-windows-gnu",
        "x86_64-pc-windows-msvc-extra",
        "i686-pc-windows-msvc",
        "aarch64-unknown-linux-gnu",
    ] {
        let mut bad = module.clone();
        let mut forged = plan.clone();
        bad.target.triple = triple.into();
        forged.module_fingerprint = bad.fingerprint();
        let target = bad.target.clone();
        let bytes = wire::encode_module_candidate(&bad, &forged, &limits).unwrap();
        let dir = directory("target-alias");
        fs::write(dir.join("target.txt"), triple).unwrap();
        fs::write(dir.join("candidate.bin"), &bytes).unwrap();
        for (route, result) in [
            (
                "raw",
                zil_verify::verify_module(bad, forged, &target, &limits),
            ),
            (
                "wire",
                zil_verify::verify_module_bytes(&bytes, &target, &limits),
            ),
        ] {
            let errors = result.unwrap_err();
            fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
            assert!(
                errors.iter().any(|d| d.code == "E-UNSUPPORTED"),
                "{triple}/{route}: {errors:#?}"
            );
        }
    }
}

#[test]
fn public_transport_version_and_verified_target_binding_are_independent() {
    let (module, plan) = candidate(SOURCE);
    let limits = Limits::default();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory("transport-version");
    for version in (0u32..wire::WIRE_VERSION).chain([wire::WIRE_VERSION + 1]) {
        let mut bad = bytes.clone();
        bad[8..12].copy_from_slice(&version.to_le_bytes());
        let errors = zil_verify::verify_module_bytes(&bad, &module.target, &limits).unwrap_err();
        fs::write(
            dir.join(format!("version-{version}.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(errors.iter().any(|d| d.code == "E-WIRE-VERSION"));
    }
    let verified =
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap();
    let mut lir = zil_lower::lower(&verified).unwrap();
    lir.target = TargetProfile::host_llvm();
    let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
    fs::write(dir.join("lir-target.txt"), format!("{errors:#?}\n")).unwrap();
    assert!(
        errors.iter().any(|d| d.code == "E-CODEGEN-UNBOUND-LIR"),
        "{errors:#?}"
    );
}

#[test]
fn store_only_windows_module_emits_real_heap_abi_and_rejects_symbol_conflicts() {
    let (module, plan) = candidate(SOURCE);
    let limits = Limits::default();
    assert!(
        !module
            .resources
            .iter()
            .any(|r| r.payload == zil_ir::Type::ByteBuffer)
    );
    let verified =
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(llvm.contains("declare ptr @HeapAlloc(ptr, i32, i64)"));
    assert!(llvm.contains("declare ptr @HeapReAlloc(ptr, i32, ptr, i64)"));
    assert!(llvm.contains("declare i32 @HeapFree(ptr, i32, ptr)"));
    assert!(llvm.contains("call ptr @HeapAlloc(ptr %heap, i32 0, i64 %size)"));
    assert!(llvm.contains("call ptr @HeapReAlloc(ptr %heap, i32 0, ptr %old, i64 %size)"));
    assert!(!llvm.contains("@realloc("));
    assert!(!llvm.contains("@free("));
    assert!(!llvm.contains(" noalias "));
    for declaration in [
        "extern \"C\" fn HeapAlloc(n:i64)->i32;",
        "extern \"C\" fn HeapReAlloc(n:i64)->i32;",
        "extern \"C\" fn HeapFree(n:i64)->i32;",
    ] {
        let source = format!("{declaration}{SOURCE}");
        let (module, plan) = candidate(&source);
        let verified =
            zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap();
        let lir = zil_lower::lower(&verified).unwrap();
        let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
        let dir = directory("heap-conflict");
        fs::write(dir.join("main.zil"), source).unwrap();
        fs::write(dir.join("codegen.txt"), format!("{errors:#?}\n")).unwrap();
        assert!(
            errors.iter().any(|d| d.code == "E-CODEGEN-FOREIGN"),
            "{errors:#?}"
        );
    }
}
