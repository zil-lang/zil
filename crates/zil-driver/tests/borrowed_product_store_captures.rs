//! Borrowed complete Store owners retain actual roots through callback calls.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const SOURCE: &str = r#"
resource R{code:i32}
impl Drop for R{fn drop(self:Self){}}
package Item{manages marker:R;value:i32}
package Cursor{manages values:Store<Item>;index:i64}
fn run<F:FnMut(&Item)->void>(cursor:&mut Cursor,visit:&mut F){
 let item=store_at(cursor.values,0)
 visit(item)
}
fn main()->i32{
 let first:Store<Item>=store_new();if !store_reserve(first,1){return 1}
 store_push(first,Item{marker:R{code:65},value:7})
 let second:Store<Item>=store_new();if !store_reserve(second,1){return 2}
 store_push(second,Item{marker:R{code:66},value:42})
 var cursor=Cursor{values:give first,index:0}
 var other=Cursor{values:give second,index:0}
 var total=0
 var visit=(item:&Item)=>{cursor.index+=1;total+=item.value}
 run(&mut other,&mut visit)
 return total+i32(cursor.index)-43
}
"#;
fn typed(text: &str) -> zil_ir::TypedModule {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "borrowed-product-store-capture.zil",
        text,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &target, &limits).unwrap()
}

#[test]
fn borrowed_complete_store_owner_capture_uses_actual_exclusive_entry() {
    let module = typed(SOURCE);
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-borrowed-store-capture-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let output = dir.join("program");
            let report = zil_driver::compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "borrowed-store-capture.zil", SOURCE),
                &zil_driver::CompileOptions {
                    stop_after: zil_driver::Stage::Build,
                    backend,
                    optimization,
                    output: Some(output.clone()),
                    ..Default::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let result = std::process::Command::new(output).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert!(
                result.stdout.is_empty() && result.stderr.is_empty(),
                "{result:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
#[test]
fn callback_cannot_alias_its_actual_store_manager_argument() {
    let text = SOURCE.replace("run(&mut other,&mut visit)", "run(&mut cursor,&mut visit)");
    let module = typed(&text);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    let good = typed(SOURCE);
    let mut candidate = zil_analyze::analyze(&good, &Limits::default()).unwrap();
    candidate.module_fingerprint = module.fingerprint();
    assert!(
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err());
}

#[test]
fn raw_shared_entry_cannot_upgrade_an_exclusive_captured_product() {
    let mut module = typed(SOURCE);
    let mut candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let definition=module.closures.iter_mut().find(|definition|definition.captures.iter().any(|ty|matches!(ty,zil_ir::Type::Address{mutable:true,inner} if matches!(inner.as_ref(),zil_ir::Type::Package{..})))).unwrap();
    definition.kind = zil_ir::CallableKind::Fn;
    candidate.module_fingerprint = module.fingerprint();
    assert!(
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err());
}
