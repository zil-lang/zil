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

#[test]
fn actual_byte_head_and_loan_proof_is_bounded_in_both_kernels() {
    let module = typed(SOURCE);
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for verify in [false, true] {
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            };
            let result = if verify {
                zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ())
            } else {
                zil_analyze::analyze(&module, &limits).map(|_| ())
            };
            match result {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .any(|error| error.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle + 1;
                }
            }
        }
        assert!(low > 0);
        for (steps, success) in [(low, true), (low - 1, false)] {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            let result = if verify {
                zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ())
            } else {
                zil_analyze::analyze(&module, &limits).map(|_| ())
            };
            assert_eq!(
                result.is_ok(),
                success,
                "verify={verify}, steps={steps}: {result:?}"
            );
        }
    }
}

#[test]
fn nested_managed_observation_has_no_hidden_owner_transfer() {
    let source = r#"
package Value{manages bytes:ByteBuffer;id:i32}
package Entry{value:Value;key:i32}
fn relay(entry:Entry)->Entry{return entry}
fn main()->i32{
 let bytes=bytes_new();if !bytes_reserve(bytes,1)||!bytes_push(bytes,66u8){return 1}
 var original=Entry{value:Value{bytes:give bytes,id:7},key:9}
 let description=original.value.bytes
 let view=bytes_view(description)
 let old=i32(view[0])
 let retried=relay(original)
 return old+retried.value.id+retried.key-82
}
"#;
    let module = typed(source);
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
    let bad = source
        .replace("let old=i32(view[0])", "let old=66")
        .replace(
            "return old+retried.value.id+retried.key-82",
            "return i32(view[0])+retried.value.id+retried.key-82",
        );
    let module = typed(&bad);
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    let mut dishonest = candidate;
    dishonest.module_fingerprint = module.fingerprint();
    assert!(
        zil_verify::verify_module(
            module.clone(),
            dishonest.clone(),
            &module.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &dishonest, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).is_err());
}
