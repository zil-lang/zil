//! Independent target selection remains checked after Win64 Store admission.
use zil_base::{Endian, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, TypedModule, wire};

const SOURCE: &str = r#"
resource R { tag: i32 }
package Item { pad: u8; manages r: R; wide: i64; tail: [u16; 3] }
fn make(n: i32) -> Item {
    return Item { pad: u8(7), r: R { tag: n }, wide: i64(n), tail: [u16(1), u16(2), u16(3)] }
}
fn main() -> void!i32 {
    let items: Store<Item> = store_new()
    if !store_reserve(items, 2) { return () }
    store_push(items, make(65))
    store_push(items, make(66))
    let last = store_pop(items)?
    return i32(last.wide) - 66
}
"#;

fn typed(target: &TargetProfile) -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "win64-store181.zil", SOURCE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let module = zil_types::check(&core, target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    (module, plan)
}

#[test]
fn linux_and_win64_accept_the_same_padded_store_and_owned_error_shapes() {
    for target in [
        TargetProfile::host_llvm(),
        TargetProfile::windows_x64_llvm(),
    ] {
        let (module, plan) = typed(&target);
        let limits = Limits::default();
        let verified =
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let from_wire = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        assert_eq!(verified.fingerprint(), from_wire.fingerprint());
        let lowered = zil_lower::lower(&verified).unwrap();
        assert_eq!(lowered.target, target);
        assert_eq!(lowered.resources, module.resources);
        assert!(
            module
                .resources
                .iter()
                .any(|r| matches!(r.payload, zil_ir::Type::OwnedStore { .. }))
        );
        assert!(
            !module
                .resources
                .iter()
                .any(|r| r.payload == zil_ir::Type::ByteBuffer)
        );
    }
}

#[test]
fn forged_store_target_profiles_cannot_use_the_host_layout() {
    let target = TargetProfile::windows_x64_llvm();
    let (module, _) = typed(&target);
    for change in 0..6 {
        let mut bad = module.clone();
        match change {
            0 => bad.target.triple = "aarch64-pc-windows-msvc".into(),
            1 => bad.target.pointer_width = 32,
            2 => bad.target.endian = Endian::Big,
            3 => bad.target.backend = "c".into(),
            4 => bad.target.checked_arithmetic = false,
            5 => bad.target.rule_version = "ZIL-LANG-1.3".into(),
            _ => unreachable!(),
        }
        let limits = Limits::default();
        let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
        plan.module_fingerprint = bad.fingerprint();
        let bytes = wire::encode_module_candidate(&bad, &plan, &limits).unwrap();
        let selected = bad.target.clone();
        for result in [
            zil_verify::verify_module(bad, plan, &selected, &limits),
            zil_verify::verify_module_bytes(&bytes, &selected, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn candidate_cannot_swap_linux_and_windows_after_verification() {
    let linux = TargetProfile::host_llvm();
    let windows = TargetProfile::windows_x64_llvm();
    let (module, plan) = typed(&linux);
    let limits = Limits::default();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &windows, &limits),
        zil_verify::verify_module_bytes(&bytes, &windows, &limits),
    ] {
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-VERIFY-TARGET")
        );
    }
}
