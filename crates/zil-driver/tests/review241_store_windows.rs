//! A Sum transfer must keep the two same-typed Store instances distinct.
//! Source-only borrows below are not executed (the empty Store index traps).
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
const MODEL: &str = "resource R{n:i32}package Leaf{manages r:R;mark:i32}package Set{manages items:Store<Leaf>}enum Pair{Empty,Both(Set,Set)}fn set()->Set{return Set{items:store_new()}}fn relay(p:Pair)->Pair{return p}fn flip(p:Pair)->Pair{match p{Pair.Empty()=>{return Pair.Empty}\nPair.Both(a,b)=>{return Pair.Both(b,a)}}}";
fn source(body: &str, callee: &str) -> String {
    format!(
        "{MODEL}fn main(){{let pair={callee}(Pair.Both(set(),set()));match pair{{Pair.Empty()=>{{}}\nPair.Both(a,b)=>{{var left=a;var right=b;{body}}}}}}}"
    )
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "sum-stores241.zil", text);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn directory(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review241-window-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    out
}
fn rejected<T: std::fmt::Debug>(label: &str, result: Result<T, Vec<zil_base::Diagnostic>>) {
    let errors = result.expect_err(label);
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{label}: {errors:?}"
    );
    assert!(
        errors.iter().all(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: {errors:?}"
    );
}
#[test]
fn recursive_sum_roundtrip_retains_distinct_store_windows_and_last_use() {
    for callee in ["relay", "flip"] {
        for (label, body, accept) in [
            (
                "twin",
                "let w=store_at(left.items,0);store_reserve(right.items,2);let n=w.mark",
                true,
            ),
            (
                "last-use",
                "let w=store_at(left.items,0);let n=w.mark;store_reserve(left.items,2)",
                true,
            ),
            (
                "same-live",
                "let w=store_at(left.items,0);store_reserve(left.items,2);let n=w.mark",
                false,
            ),
            (
                "move-live",
                "let w=store_at(left.items,0);let moved=Pair.Both(left,right);let n=w.mark",
                false,
            ),
        ] {
            let out = directory(&format!("{callee}-{label}"));
            let text = source(body, callee);
            fs::write(out.join("source.zil"), &text).unwrap();
            let module = typed(&text);
            fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
            let result = zil_analyze::analyze(&module, &Limits::default());
            fs::write(out.join("analyze.txt"), format!("{result:#?}")).unwrap();
            if accept {
                let plan = result.unwrap();
                let wire =
                    zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default())
                        .unwrap();
                fs::write(out.join("candidate.wire"), &wire).unwrap();
                zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
                    .unwrap();
                zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
            } else {
                rejected(label, result);
            }
        }
    }
}
#[test]
fn forged_reserve_cannot_switch_to_the_borrowed_twin_after_sum_unpack() {
    let text = source(
        "let w=store_at(left.items,0);store_reserve(right.items,2);let n=w.mark",
        "flip",
    );
    let mut module = typed(&text);
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let borrowed = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let InstructionKind::OwnedStoreBorrow { store, .. } = i.kind {
                Some(store)
            } else {
                None
            }
        })
        .expect("actual borrowed Store description");
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::OwnedStoreReserve { store, .. } = &mut i.kind {
            assert_ne!(*store, borrowed);
            *store = borrowed;
            changed = true;
        }
    }
    assert!(changed);
    plan.module_fingerprint = module.fingerprint();
    let out = directory("forged-twin");
    fs::write(out.join("source-control.zil"), &text).unwrap();
    fs::write(out.join("mutated-module.txt"), format!("{module:#?}")).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default());
    let bytes = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    fs::write(out.join("wire.txt"), format!("{bytes:#?}")).unwrap();
    rejected("raw borrowed twin", raw);
    rejected("Wire borrowed twin", bytes);
    rejected(
        "Analyze borrowed twin",
        zil_analyze::analyze(&module, &Limits::default()),
    );
}
