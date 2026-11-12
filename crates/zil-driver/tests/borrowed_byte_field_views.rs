//! A byte payload view retains the actual Store element, header, and parent loan.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = r#"
package Value{manages bytes:ByteBuffer;id:i32}
package Bag{manages values:Store<Value>}
fn get<life a>(bag:&'a Bag)->Option<&'a Value>{
 if store_len(bag.values)==0{return Option::<&'a Value>.None()}
 return Option::<&'a Value>.Some(store_at(bag.values,0))
}
fn main()->i32{
 let a=bytes_new();if !bytes_reserve(a,1)||!bytes_push(a,66u8){return 1}
 let b=bytes_new();if !bytes_reserve(b,1)||!bytes_push(b,70u8){return 2}
 var first=Bag{values:store_new()};var second=Bag{values:store_new()}
 if !store_reserve(first.values,1)||!store_reserve(second.values,1){return 3}
 match store_push(first.values,Value{bytes:give a,id:66}){.err(_)=>{return 4},.ok(_)=>{}}
 match store_push(second.values,Value{bytes:give b,id:70}){.err(_)=>{return 5},.ok(_)=>{}}
 match get(&first){Option.None()=>{return 6},Option.Some(old_first)=>{
  match get(&second){Option.None()=>{return 7},Option.Some(old_second)=>{
   let marker=old_second.id
   let description=old_first.bytes
   let view=bytes_view(description)
   match store_pop(second.values){.err(_)=>{return 8},.ok(removed)=>{
    return i32(view[0])+removed.id+marker-206
   }}
  }}
 }}
}
"#;
fn typed(text: &str) -> zil_ir::TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "byte-field.zil", text);
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
fn actual_borrowed_byte_field_source_reaches_source_raw_wire_and_both_backends() {
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
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi)
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-byte-field-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let executable = dir.join("program");
            let unit = SourceUnit::new(FileId(0), ModuleId(0), "byte-field.zil", SOURCE);
            let report = zil_driver::compile_source(
                &unit,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let output = Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
#[test]
fn removing_the_actual_parent_cannot_leave_its_payload_view_live() {
    let bad = SOURCE.replace(
        "let description=old_first.bytes",
        "let description=old_second.bytes",
    );
    let module = typed(&bad);
    let errors = zil_analyze::analyze(&module, &Limits::default())
        .expect_err("removing the actual Store element must invalidate its byte payload");
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
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
