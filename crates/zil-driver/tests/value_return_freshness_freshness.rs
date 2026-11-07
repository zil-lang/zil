//! Ordinary Value returns preserve carriers, not per-leaf Created authority.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review241-fresh-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn typed(source: &str) -> TypedModule {
    let u = SourceUnit::new(FileId(0), ModuleId(0), "fresh241.zil", source);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(r: Result<impl Sized, Vec<zil_base::Diagnostic>>, label: &str) {
    let errors = match r {
        Ok(_) => panic!("{label}: ordinary Value returned Created authority"),
        Err(e) => e,
    };
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{label}: {errors:#?}"
    );
    assert!(
        errors.iter().all(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: only producer claims rejected: {errors:#?}"
    );
}
fn check(label: &str, source: &str, accept: bool) {
    let out = dir(label);
    fs::write(out.join("source.zil"), source).unwrap();
    let module = typed(source);
    let limits = Limits::default();
    let target = module.target.clone();
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    let analyzed = zil_analyze::analyze(&module, &limits);
    fs::write(out.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    let mut plan = if accept {
        analyzed.unwrap()
    } else {
        rejected(analyzed, label);
        let mut give = module.clone();
        give.functions
            .iter_mut()
            .find(|f| f.name == "extract")
            .unwrap()
            .result_mode = PassingMode::Give;
        zil_analyze::analyze(&give, &limits)
            .expect("same carrier route remains a legal give result")
    };
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    let raw = zil_verify::verify_module(module, plan, &target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &target, &limits);
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    fs::write(out.join("wire.txt"), format!("{bytes:#?}")).unwrap();
    if accept {
        raw.unwrap();
        bytes.unwrap();
    } else {
        rejected(raw, &format!("{label}/raw"));
        rejected(bytes, &format!("{label}/wire"));
    }
}
const BASE: &str = "resource R{n:i32}enum Boxed{Hold(manages R)}";
#[test]
fn direct_pack_unpack_and_explicit_managed_create_remain_fresh() {
    for (label, value, extra) in [
        ("direct", "R{n:7}", ""),
        (
            "managed-create",
            "make_r()",
            "fn make_r()->R contract{result(create)}{return R{n:7}}",
        ),
    ] {
        check(
            label,
            &format!(
                "{BASE}{extra}fn extract()->R contract{{result(create)}}{{let boxed=Boxed.Hold({value});match boxed{{Boxed.Hold(owner)=>{{return give owner}}}}}}fn main(){{}}"
            ),
            true,
        );
    }
}
#[test]
fn value_identity_does_not_reexport_a_created_leaf() {
    check(
        "identity",
        &format!(
            "{BASE}fn relay(value:Boxed)->Boxed{{return value}}fn extract()->R contract{{result(create)}}{{let boxed=relay(Boxed.Hold(R{{n:7}}));match boxed{{Boxed.Hold(owner)=>{{return give owner}}}}}}fn main(){{}}"
        ),
        false,
    );
}
#[test]
fn value_factories_and_nonidentity_repacking_do_not_export_freshness() {
    for (label, body) in [
        (
            "factory",
            "fn factory()->Boxed{return Boxed.Hold(R{n:7})}fn get()->Boxed{return factory()}",
        ),
        (
            "repacking",
            "fn remake(value:Boxed)->Boxed{match value{Boxed.Hold(owner)=>{return Boxed.Hold(give owner)}}}fn get()->Boxed{return remake(Boxed.Hold(R{n:7}))}",
        ),
    ] {
        check(
            label,
            &format!(
                "{BASE}{body}fn extract()->R contract{{result(create)}}{{let boxed=get();match boxed{{Boxed.Hold(owner)=>{{return give owner}}}}}}fn main(){{}}"
            ),
            false,
        );
    }
}
#[test]
fn same_typed_inline_fields_keep_their_actual_freshness_separate() {
    for (label, field, accept) in [
        ("second-created", "new", true),
        ("first-incoming", "old", false),
    ] {
        check(
            label,
            &format!(
                "resource R{{n:i32}}enum Pair{{Both(manages R,manages R)}}fn extract(give incoming:R)->R contract{{result(create)}}{{let pair=Pair.Both(give incoming,R{{n:9}});match pair{{Pair.Both(old,new)=>{{return give {field}}}}}}}fn main(){{}}"
            ),
            accept,
        );
    }
}
