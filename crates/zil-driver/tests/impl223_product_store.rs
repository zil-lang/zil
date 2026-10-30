//! Product Store behavior is checked from real source, independent raw/Wire
//! candidates and native cleanup. A successful source check is not execution.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, NativeBackend, Optimization, Stage, compile_source,
};
use zil_ir::{InstructionKind as I, Type};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages r:R;mark:i32}
package Pair{left:Leaf;right:Leaf;tag:i32}
package Root{child:Pair;tail:i32}
fn make(code:i32)->Root{return Root{child:Pair{left:Leaf{r:R{code:code},mark:code},right:Leaf{r:R{code:code+1},mark:code+1},tag:code},tail:code+2}}
"#;

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "product-store223-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn check(label: &str, source: &str, stage: Stage) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "store223.zil", source);
    let report = compile_source(
        &unit,
        &CompileOptions {
            stop_after: stage,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn pass(label: &str, source: &str) {
    let r = check(label, source, Stage::Verify);
    assert_eq!(
        r.status,
        ResultCategory::Success,
        "{label}: {}",
        r.to_json()
    );
    assert_eq!(r.verification_runs, 1);
}
fn native(label: &str, source: &str, stdout: &[u8], exit: i32) {
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for opt in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        fs::write(dir.join("source.zil"), source).unwrap();
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "store223.zil", source);
        let exe = dir.join("program");
        let report = compile_source(
            &unit,
            &CompileOptions {
                backend,
                stop_after: Stage::Build,
                optimization: opt,
                output: Some(exe.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
        let output = Command::new(exe).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout, "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

#[test]
fn original_217_and_unused_206_sources_are_promoted_without_rewriting() {
    pass(
        "original217",
        include_str!("../../../tools/language/217/probes/product-store-element/main.zil"),
    );
    pass(
        "unused206",
        include_str!("impl206_independent_fixtures/23_store_parent.zil"),
    );
}

#[test]
fn nested_elements_pop_and_destroy_each_real_leaf_in_declared_order() {
    native(
        "pop-and-drop",
        &format!(
            "{MODEL} fn main()->void!i32{{let s:Store<Root>=store_new();if !store_reserve(s,2){{return ()}};store_push(s,make(65));store_push(s,make(67));let p=store_pop(s)?;return p.tail-69}}"
        ),
        b"CDAB",
        0,
    );
    native(
        "prefix-order",
        &format!(
            "{MODEL} fn main()->i32{{let s:Store<Root>=store_new();if !store_reserve(s,2){{return 1}};store_push(s,make(65));store_push(s,make(67));return 0}}"
        ),
        b"ABCD",
        0,
    );
}

#[test]
fn failed_push_and_empty_pop_preserve_exact_active_responsibility() {
    native(
        "failed-push",
        &format!(
            "{MODEL} fn main()->i32{{let s:Store<Root>=store_new();store_push(s,make(65));return 0}}"
        ),
        b"AB",
        0,
    );
    native(
        "full-push",
        &format!(
            "{MODEL} fn main()->i32{{let s:Store<Root>=store_new();if !store_reserve(s,1){{return 1}};store_push(s,make(65));store_push(s,make(67));return 0}}"
        ),
        b"CDAB",
        0,
    );
    native(
        "empty-pop",
        &format!(
            "{MODEL} fn main()->void!i32{{let s:Store<Root>=store_new();let value=store_pop(s)?;return value.tail}}"
        ),
        b"",
        1,
    );
}

#[test]
fn negative_and_overflow_reserve_leave_nested_prefix_intact() {
    native(
        "reserve-failure",
        &format!(
            "{MODEL} fn main()->void!i32{{let s:Store<Root>=store_new();if !store_reserve(s,1){{return ()}};store_push(s,make(65));if store_reserve(s,-1)||store_reserve(s,9223372036854775807){{return ()}};if store_len(s)!=1||store_capacity(s)!=1{{return ()}};let p=store_pop(s)?;return p.tail-67}}"
        ),
        b"AB",
        0,
    );
}

#[test]
fn ordinary_store_element_bound_instantiates_complete_product() {
    native(
        "generic-element",
        &format!(
            "{MODEL} fn pass<T:StoreElement>(value:T)->T{{return value}} fn main()->i32{{let p=pass(make(65));return p.tail-67}}"
        ),
        b"AB",
        0,
    );
}

#[test]
fn zero_sized_owned_child_still_drops_when_whole_product_has_storage() {
    let source = r#"
extern "C" fn putchar(x:i32)->i32;
resource Z{}
impl Drop for Z{fn drop(self:Self){unsafe{putchar(90)}}}
package Leaf{manages zero:Z}
package Root{child:Leaf;tag:i64}
fn main()->i32{let s:Store<Root>=store_new();if !store_reserve(s,1){return 1};store_push(s,Root{child:Leaf{zero:Z{}},tag:7});return 0}
"#;
    native("zero-child", source, b"Z", 0);
    let r = check(
        "all-zero",
        &source.replace(";tag:i64", "").replace(",tag:7", ""),
        Stage::Verify,
    );
    assert_eq!(r.status, ResultCategory::Unsupported, "{}", r.to_json());
}

#[test]
fn source_cannot_push_partial_or_previously_transferred_products() {
    for (label, body) in [
        (
            "move-twice",
            "let p=make(65);store_push(s,p);store_push(s,p)",
        ),
        (
            "partial",
            "var p=make(65);let child=p.child;store_push(s,p)",
        ),
        (
            "read-after-push",
            "let p=make(65);store_push(s,p);let marker=p.tail",
        ),
    ] {
        let source = format!("{MODEL} fn main(){{let s:Store<Root>=store_new();{body}}}");
        let r = check(label, &source, Stage::Verify);
        assert_eq!(
            r.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}

#[test]
fn only_the_exact_unit_protocol_shells_gain_product_payloads() {
    pass(
        "pop-protocol",
        &format!("{MODEL} fn pop(s:Store<Root>)->void!Root{{return store_pop(s)}} fn main(){{}}"),
    );
    pass(
        "push-protocol",
        &format!(
            "{MODEL} fn push(s:Store<Root>,value:Root)->Root!void{{return store_push(s,value)}} fn main(){{}}"
        ),
    );
    for (label, source) in [
        (
            "arbitrary-sum",
            include_str!("impl206_independent_fixtures/25_sum_parent.zil").to_string(),
        ),
        (
            "product-result-i32",
            format!("{MODEL} fn unsupported(x:Root)->Root!i32{{return x}} fn main(){{}}"),
        ),
        (
            "product-owned-array",
            format!("{MODEL} fn unused(x:[Root;2]){{}} fn main(){{}}"),
        ),
        (
            "mut-product-window",
            format!("{MODEL} fn main(){{let s:Store<Root>=store_new();let w=store_at_mut(s,0)}}"),
        ),
        (
            "captured-product-store",
            format!(
                "{MODEL} fn main(){{let s:Store<Root>=store_new();let f=move ()=>i32(store_len(s))}}"
            ),
        ),
    ] {
        let r = check(label, &source, Stage::Verify);
        assert_eq!(
            r.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw223.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}
fn reject_raw(
    label: &str,
    module: zil_ir::TypedModule,
    plan: zil_ir::CandidatePlan,
    expected: &str,
) {
    reject_raw_messages(label, module, plan, &[expected]);
}
fn reject_raw_messages(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    expected: &[&str],
) {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory(label);
    fs::write(dir.join("candidate.bin"), &wire).unwrap();
    for (name, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ),
    ] {
        let errors = result.expect_err(label);
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify),
            "{label}: {errors:?}"
        );
        assert!(
            errors.iter().any(|d| expected.contains(&d.message.as_str())
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
            "{label}: {errors:?}"
        );
        fs::write(dir.join(format!("{name}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn raw_and_wire_duplicate_push_cannot_manufacture_a_second_element() {
    let source = format!(
        "{MODEL} fn main(){{let s:Store<Root>=store_new();store_reserve(s,2);store_push(s,make(65));store_push(s,make(67))}}"
    );
    let (mut module, plan) = candidate(&source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut first = None;
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::OwnedStorePush { value, .. } = &mut i.kind {
            if let Some(original) = first {
                *value = original;
                changed = true;
            } else {
                first = Some(*value);
            }
        }
    }
    assert!(changed);
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    reject_raw(
        "double-push",
        module,
        plan,
        "managed SSA value was not available exactly once",
    );
}

#[test]
fn raw_and_wire_canonical_store_definition_cannot_hide_a_different_product() {
    let source = format!("{MODEL} fn main(){{let s:Store<Root>=store_new()}}");
    let (mut module, plan) = candidate(&source);
    let definition = module
        .resources
        .iter_mut()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    let Type::OwnedStore { element } = &mut definition.payload else {
        unreachable!()
    };
    let Type::Package { name, .. } = element.as_mut() else {
        unreachable!()
    };
    name.push_str("_forged");
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(errors.iter().any(|d| d.phase == Phase::Analyze
        && d.category == ResultCategory::SafeReject
        && d.code == "E-ANALYZE-IR"
        && d.message == "Product Store lacks its exact canonical element definition"));
    reject_raw(
        "canonical-definition",
        module,
        plan,
        "Store definition must have its canonical element identity and no user teardown",
    );
}

#[test]
fn raw_and_wire_rebuild_unused_product_storage_and_leaf_qualification() {
    let source = format!("{MODEL} fn main(){{let s:Store<Root>=store_new()}}");
    let (original, plan) = candidate(&source);
    for variant in 0..3 {
        let mut module = original.clone();
        let definition = module
            .resources
            .iter_mut()
            .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
            .unwrap();
        let Type::OwnedStore { element } = &mut definition.payload else {
            unreachable!()
        };
        let Type::Package { fields, .. } = element.as_mut() else {
            unreachable!()
        };
        match variant {
            0 => {
                // A nominal child with no owning leaf cannot stand in for an
                // element, even when the stale plan claims the original type.
                fields[0].1 = Type::Package {
                    name: "@module0:Empty".into(),
                    fields: vec![],
                };
            }
            1 => {
                // Two identical nominal field names are not two distinct paths.
                fields[1].0 = fields[0].0.clone();
            }
            2 => {
                // Store inside a Product remains outside the finite profile.
                fields[1].1 = Type::Managed(Box::new(Type::OwnedStore {
                    element: Box::new(Type::I32),
                }));
            }
            _ => unreachable!(),
        }
        assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
        reject_raw(
            &format!("forbidden-profile-{variant}"),
            module,
            plan.clone(),
            [
                "each Product Package requires a transitive explicit manages leaf",
                "package fields must have unique nonempty names",
                "non-Copy type requires another checker",
            ][variant],
        );
    }
    let (mut module, plan) = candidate(
        "resource Z{} package Leaf{manages zero:Z} package Root{child:Leaf;tag:i64} fn main(){let s:Store<Root>=store_new()}",
    );
    // Keep every occurrence and the canonical name coherent, so this variant
    // must be rejected by rebuilding the actual zero-sized layout.
    fn zero_tag(ty: &mut Type) {
        match ty {
            Type::Package { name, fields } if name.ends_with(":Root") => {
                fields[1].1 = Type::Unit;
            }
            Type::Managed(inner) | Type::Desc(inner) | Type::OwnedStore { element: inner } => {
                zero_tag(inner)
            }
            _ => {}
        }
    }
    for definition in &mut module.resources {
        zero_tag(&mut definition.payload);
    }
    for function in &mut module.functions {
        for local in &mut function.locals {
            zero_tag(&mut local.ty);
        }
        for value in &mut function.values {
            zero_tag(value);
        }
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::OwnedStoreNew { element } = &mut instruction.kind {
                zero_tag(element);
            }
        }
    }
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(errors.iter().any(|d| d.phase == Phase::Analyze
        && d.category == ResultCategory::Unsupported
        && d.message == "Product Store element requires nonzero actual inline storage"));
    reject_raw(
        "zero-actual-layout",
        module,
        plan,
        "Product Store element requires nonzero actual inline layout",
    );
}

#[test]
fn unused_user_resource_head_cannot_hide_the_new_product_store() {
    let source = format!("{MODEL} fn main(){{let s:Store<Root>=store_new()}}");
    let (mut module, mut plan) = candidate(&source);
    let store = module
        .resources
        .iter()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap()
        .payload
        .clone();
    module.resources.push(zil_ir::ResourceDefinition {
        name: "@module0:Hidden".into(),
        payload: Type::Record {
            name: "@module0:Hidden".into(),
            fields: vec![("store".into(), Type::Managed(Box::new(store)))],
        },
        teardown: None,
    });
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(errors.iter().any(|d| d.phase == Phase::Analyze
        && d.category == ResultCategory::Unsupported
        && d.message == "ordinary Package Product is not a resource payload profile"));
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory("hidden-resource-head");
    fs::write(dir.join("candidate.bin"), &wire).unwrap();
    for (name, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::Unsupported)
        );
        fs::write(dir.join(format!("{name}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn store_qualification_cannot_open_an_incoming_mutable_product_address() {
    // Regression for the non-author 223 review. Every type occurrence agrees;
    // field index/type, WriteAddress and the incoming Hold are still valid.
    let source = format!("{MODEL} fn write(p:&mut Leaf){{p.mark=7}}fn main(){{}}");
    let (mut module, plan) = candidate(&source);
    let product = module
        .functions
        .iter()
        .find(|f| f.name == "make")
        .unwrap()
        .return_type
        .clone();
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "write")
        .unwrap();
    for ty in function
        .locals
        .iter_mut()
        .map(|local| &mut local.ty)
        .chain(function.values.iter_mut())
    {
        if let Type::Address { inner, .. } = ty
            && matches!(inner.as_ref(), Type::Package { name, .. } if name.ends_with(":Leaf"))
        {
            **inner = product.clone();
        }
    }
    reject_raw_messages(
        "incoming-mut-product",
        module,
        plan,
        &[
            "ordinary Product addresses require independent shared qualification",
            "ordinary Product addresses are shared synchronous inputs",
        ],
    );
}
