//! Anonymous qualified values preserve the actual Tuple class partition,
//! affine owner debt and each concrete address loan.
use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, TypedModule};
const SHARED: &str =
    include_str!("../../../tests/fixtures/qualified-values/anonymous-compound.zil");
const OWNED: &str = include_str!("../../../tests/fixtures/qualified-values/owning-anonymous.zil");
const EXCLUSIVE: &str =
    include_str!("../../../tests/fixtures/qualified-values/exclusive-anonymous.zil");
fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "anonymous-tuple.zil", text)
}
fn prepare(text: &str) -> (TypedModule, CandidatePlan) {
    let input = input(text);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    (module, candidate)
}
fn verify(module: TypedModule, candidate: CandidatePlan) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}
#[test]
fn actual_anonymous_tuple_sources_verify_raw_wire() {
    for (name, text) in [
        ("shared", SHARED),
        ("owned", OWNED),
        ("exclusive", EXCLUSIVE),
    ] {
        let (module, candidate) = prepare(text);
        if let Some(dir) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                std::path::PathBuf::from(dir).join(format!("anonymous-{name}-typed.txt")),
                format!("{module:#?}"),
            )
            .unwrap();
        }
        verify(module, candidate);
    }
}
#[test]
fn anonymous_tuple_values_run_both_backends_with_real_cleanup() {
    for (name, text, stdout) in [
        ("shared", SHARED, b"".as_slice()),
        ("owned", OWNED, b"A".as_slice()),
        ("exclusive", EXCLUSIVE, b"".as_slice()),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-anonymous-tuple-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &input(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("phases")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert!(!native.object_reports.is_empty());
                    assert!(
                        native
                            .object_reports
                            .iter()
                            .all(|r| r.contains("\"input_kind\":\"builder\"")
                                && r.contains("\"verify_before_after\":true"))
                    );
                }
                let ran = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(ran.status.code(), Some(0), "{name}: {ran:?}");
                assert_eq!(ran.stdout, stdout);
                assert!(ran.stderr.is_empty());
            }
        }
    }
}

#[test]
fn anonymous_tuple_raw_wire_cannot_forge_modes_classes_or_drop_debt() {
    use zil_ir::{InstructionKind as I, StorageBase, StorageBinder, Type};
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (shared, shared_plan) = prepare(SHARED);
    let (owned, owned_plan) = prepare(OWNED);
    let (exclusive, exclusive_plan) = prepare(EXCLUSIVE);
    for attack in 0..6 {
        let (mut module, mut candidate) = match attack {
            0 => (owned.clone(), owned_plan.clone()),
            1..=3 => (exclusive.clone(), exclusive_plan.clone()),
            _ => (shared.clone(), shared_plan.clone()),
        };
        match attack {
            0 => {
                let mut removed = 0;
                for function in &mut module.functions {
                    let Some(ticket) = function
                        .locals
                        .iter()
                        .find(|l| l.name == "ticket" && matches!(l.ty, Type::Managed(_)))
                        .map(|l| l.id)
                    else {
                        continue;
                    };
                    for block in &mut function.blocks {
                        block.instructions.retain(|i| {
                            if matches!(i.kind,I::DropManaged {local} if local==ticket) {
                                removed += 1;
                                false
                            } else {
                                true
                            }
                        });
                    }
                }
                assert_eq!(
                    removed, 1,
                    "remove the live unpacked owner, not an already-taken temporary"
                );
            }
            1 => {
                let instruction = module
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::MakeOwnedTuple { .. }))
                    .unwrap();
                let I::MakeOwnedTuple { fields } = &instruction.kind else {
                    unreachable!()
                };
                instruction.kind = I::MakeAggregate {
                    fields: fields.clone(),
                };
            }
            2 => {
                let fields = module
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| {
                        if let I::UnpackTuple { fields, .. } = &mut i.kind {
                            Some(fields)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_eq!(fields.len(), 2);
                assert_ne!(fields[0], fields[1]);
                fields[1] = fields[0];
            }
            3 => {
                let helper=module.functions.iter_mut().find(|f| matches!(&f.return_type,Type::BorrowedStorage {storage,..} if matches!(storage.as_ref(),Type::Tuple(_)))).unwrap();
                let row = helper
                    .storage_life
                    .as_mut()
                    .unwrap()
                    .classes
                    .iter_mut()
                    .find(|r| r.position.base == StorageBase::Result && r.position.class == 1)
                    .unwrap();
                assert_eq!(row.binder, StorageBinder::Fixed(1));
                row.binder = StorageBinder::Fixed(0);
            }
            4 => {
                let fields = module
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find_map(|i| {
                        if let I::MakeAggregate { fields } = &mut i.kind {
                            (fields.len() == 2).then_some(fields)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                assert_ne!(fields[0], fields[1]);
                fields[1] = fields[0];
            }
            5 => {
                let ty=module.functions.iter_mut().flat_map(|f|&mut f.values).find(|ty|matches!(ty,Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Tuple(_)))).unwrap();
                let Type::BorrowedStorage { life, .. } = ty else {
                    unreachable!()
                };
                assert_eq!(life.slots.len(), 2);
                life.slots.pop();
            }
            _ => unreachable!(),
        }
        candidate.module_fingerprint = module.fingerprint();
        for decoded in [false, true] {
            let (raw, plan) = if decoded {
                let wire =
                    zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
                zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
            } else {
                (module.clone(), candidate.clone())
            };
            let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.category == ResultCategory::SafeReject),
                "attack={attack} Wire={decoded}: {errors:?}"
            );
            eprintln!("attack={attack} Wire={decoded}: {errors:?}");
        }
    }
}
