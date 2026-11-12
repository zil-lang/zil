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
const BUFFER: &str = include_str!("../../../tests/fixtures/qualified-values/buffer-anonymous.zil");
const NESTED: &str = include_str!("../../../tests/fixtures/qualified-values/nested-anonymous.zil");
const NESTED_EXCLUSIVE: &str =
    include_str!("../../../tests/fixtures/qualified-values/nested-exclusive-anonymous.zil");
const ELIDED: &str = include_str!("../../../tests/fixtures/qualified-values/elided-anonymous.zil");
const ELIDED_RELAY: &str =
    include_str!("../../../tests/fixtures/qualified-values/elided-relay-anonymous.zil");
const PHANTOM: &str =
    include_str!("../../../tests/fixtures/qualified-values/phantom-anonymous.zil");
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
        ("buffer", BUFFER),
        ("nested", NESTED),
        ("nested-exclusive", NESTED_EXCLUSIVE),
        ("elided", ELIDED),
        ("elided-relay", ELIDED_RELAY),
        ("phantom", PHANTOM),
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
        ("buffer", BUFFER, b"".as_slice()),
        ("nested", NESTED, b"".as_slice()),
        ("nested-exclusive", NESTED_EXCLUSIVE, b"".as_slice()),
        ("elided", ELIDED, b"".as_slice()),
        ("elided-relay", ELIDED_RELAY, b"".as_slice()),
        ("phantom", PHANTOM, b"".as_slice()),
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
            if attack == 0 {
                assert!(
                    errors.iter().any(|e| e.code == "E-VERIFY-RESOURCE"
                        && e.message.contains("outstanding resource responsibility")),
                    "owner debt was not independently checked: {errors:?}"
                );
            }
            eprintln!("attack={attack} Wire={decoded}: {errors:?}");
        }
    }
}

#[test]
fn anonymous_tuple_verification_has_exact_fresh_work_boundaries() {
    for (label, text) in [
        ("shared", SHARED),
        ("owner", OWNED),
        ("exclusive", EXCLUSIVE),
        ("buffer", BUFFER),
        ("nested", NESTED),
        ("nested-exclusive", NESTED_EXCLUSIVE),
        ("elided", ELIDED),
        ("elided-relay", ELIDED_RELAY),
        ("phantom", PHANTOM),
    ] {
        let (module, candidate) = prepare(text);
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
        for decoded in [false, true] {
            let run = |steps| {
                let mut bounded = limits.clone();
                bounded.max_analysis_steps = steps;
                let result = if decoded {
                    zil_verify::verify_module_bytes(&wire, &target, &bounded)
                } else {
                    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &bounded)
                };
                match result {
                    Ok(_) => ResultCategory::Success,
                    Err(errors) => {
                        assert!(
                            errors
                                .iter()
                                .all(|e| e.category == ResultCategory::ResourceLimit),
                            "{label} Wire={decoded} steps={steps}: {errors:?}"
                        );
                        ResultCategory::ResourceLimit
                    }
                }
            };
            let (mut low, mut high) = (0, limits.max_analysis_steps);
            assert_eq!(run(high), ResultCategory::Success);
            while high - low > 1 {
                let mid = low + (high - low) / 2;
                if run(mid) == ResultCategory::Success {
                    high = mid
                } else {
                    low = mid
                }
            }
            assert_eq!(run(high), ResultCategory::Success);
            assert_eq!(run(high - 1), ResultCategory::ResourceLimit);
            assert_eq!(run(high), ResultCategory::Success);
            let (fresh_module, fresh_candidate) = prepare(text);
            let mut bounded = limits.clone();
            bounded.max_analysis_steps = high;
            if decoded {
                let fresh =
                    zil_ir::wire::encode_module_candidate(&fresh_module, &fresh_candidate, &limits)
                        .unwrap();
                zil_verify::verify_module_bytes(&fresh, &target, &bounded).unwrap();
            } else {
                zil_verify::verify_module(fresh_module, fresh_candidate, &target, &bounded)
                    .unwrap();
            }
            eprintln!("anonymous {label} Wire={decoded} N={high}; N-1=ResourceLimit, freshN=Pass");
        }
    }
}

#[test]
fn a_real_task_failure_family_does_not_authorize_copying_an_affine_tuple() {
    use zil_ir::{InstructionKind as I, Type};
    let combined = format!(
        "{EXCLUSIVE}\n{}",
        r#"
fn dormant_worker<life p>(pointer:&'p i32)->i32 {task_yield();return *pointer}
fn dormant_task<life p>(pointer:&'p i32)->i32 {
 match task_create_owned(dormant_worker,pointer) {
  .err(failure)=>{let inputs=failure.inputs;return *inputs.0},
  .ok(created)=>{
   var owner=give created
   task_request_cancel(&mut owner)
   loop {match task_poll_close(&mut owner){Poll.Pending()=>{continue},Poll.Ready(_)=>{break}}}
   task_finish_destroy(give owner)
   return 0
  }
 }
}
"#
    );
    let (module, candidate) = prepare(&combined);
    assert!(
        module
            .life_declarations
            .iter()
            .any(|d| d.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
                || d.id == zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE)
    );
    verify(module.clone(), candidate.clone());
    let mut changed = module;
    let mut replaced = false;
    for f in &mut changed.functions {
        for block in &mut f.blocks {
            for i in &mut block.instructions {
                let Some(result) = i.result else { continue };
                if !matches!(&f.values[result.0 as usize],Type::BorrowedStorage {storage,life} if matches!(storage.as_ref(),Type::Tuple(_)) && life.slots.len()==2 && life.slots.iter().all(|s|s.mode==zil_ir::StoredBorrowMode::ExclusiveAddress))
                {
                    continue;
                }
                if let I::MakeOwnedTuple { fields } = &i.kind {
                    i.kind = I::MakeAggregate {
                        fields: fields.clone(),
                    };
                    replaced = true;
                    break;
                }
            }
        }
    }
    assert!(replaced);
    let mut forged = candidate;
    forged.module_fingerprint = changed.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for decoded in [false, true] {
        let (raw, plan) = if decoded {
            let wire = zil_ir::wire::encode_module_candidate(&changed, &forged, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
        } else {
            (changed.clone(), forged.clone())
        };
        let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == ResultCategory::SafeReject && e.message.contains("affine")),
            "Wire={decoded}: {errors:?}"
        );
    }
}

#[test]
fn byte_owner_in_anonymous_tuple_has_one_real_free_on_success_and_allocation_failure() {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};
    fn quote(path: &std::path::Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-anonymous-buffer-fault-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let clang = std::env::var_os("ZIL_CLANG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "clang".into());
            let observer = dir.join("allocation.o");
            let built = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/fixtures/qualified-values/anonymous-allocation.c"),
                )
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(built.status.success(), "{built:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\nfor arg in \"$@\"; do case \"$arg\" in -c|-S|-E|--version) link=0;; esac; done\nif [ \"$link\" = 1 ]; then exec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free; fi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_source(
                &input(BUFFER),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
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
            for (ordinal, exit, counts) in [
                (0, 0, b"|1:1:1:0:0:0".as_slice()),
                (1, 215, b"|1:0:0:0:0:0".as_slice()),
            ] {
                let mut command = Command::new(&report.artifacts["executable"]);
                command.env_remove("ZIL_ANONYMOUS_FAIL_AT");
                if ordinal != 0 {
                    command.env("ZIL_ANONYMOUS_FAIL_AT", ordinal.to_string());
                }
                let ran = command.output().unwrap();
                fs::write(dir.join(format!("run-{ordinal}.txt")), format!("{ran:?}")).unwrap();
                assert_eq!(ran.status.code(), Some(exit), "{ran:?}");
                assert_eq!(ran.stdout, counts);
                assert!(ran.stderr.is_empty());
            }
        }
    }
}

#[test]
fn elided_inputs_preserve_independent_domains_and_reject_ambiguous_results() {
    use zil_ir::InstructionKind as I;
    let (module, candidate) = prepare(ELIDED);
    verify(module.clone(), candidate.clone());
    let sum = module.functions.iter().find(|f| f.name == "sum").unwrap();
    let life = sum.storage_life.as_ref().unwrap();
    assert_eq!(life.fixed_binder_count, 0);
    assert_eq!(life.invocation_binder_count, 2);
    let mut changed = module;
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "sum")
        .unwrap();
    let instruction = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(&i.kind,I::MakeAggregate {fields} if fields.len()==2))
        .unwrap();
    let I::MakeAggregate { fields } = &mut instruction.kind else {
        unreachable!()
    };
    fields[1] = fields[0];
    let mut forged = candidate;
    forged.module_fingerprint = changed.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for decoded in [false, true] {
        let (raw, plan) = if decoded {
            let wire = zil_ir::wire::encode_module_candidate(&changed, &forged, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
        } else {
            (changed.clone(), forged.clone())
        };
        let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == ResultCategory::SafeReject),
            "elided source forgery Wire={decoded}: {errors:?}"
        );
    }
    // Actual no-input and two-independent-input bare result elision has no
    // invented source. These are whole Source consumers, not shape-only tests.
    for invalid in [
        "fn ambiguous(left:&i32,right:&i32)->&i32{return left}fn main(){}",
        "fn missing()->&i32{var local:i32=7;return &local}fn main(){}",
    ] {
        let report = compile_source(&input(invalid), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn nested_anonymous_headers_preserve_each_actual_child_boundary() {
    use zil_ir::{InstructionKind as I, LifeTypeProjection, PlaceProjection, Type};
    let (original, candidate) = prepare(NESTED);
    for attack in 0..4 {
        let mut changed = original.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "sum")
            .unwrap();
        let result = function.blocks.iter().flat_map(|b| &b.instructions).find_map(|i| {
            let result=i.result?;
            (matches!(i.kind,I::MakeAggregate{..}) && matches!(&function.values[result.0 as usize],Type::BorrowedStorage{life,..} if life.retained.len()==2)).then_some(result)
        }).unwrap();
        let Type::BorrowedStorage { storage, life } = &mut function.values[result.0 as usize]
        else {
            unreachable!()
        };
        match attack {
            0 => {
                life.retained.pop();
            }
            1 => {
                life.retained[1].path = vec![LifeTypeProjection::Place(PlaceProjection::Field(1))];
            }
            2 => {
                life.retained[1].binder = 0;
            }
            3 => {
                let Type::Tuple(fields) = storage.as_mut() else {
                    unreachable!()
                };
                let Type::BorrowedStorage { life: child, .. } = &mut fields[0] else {
                    unreachable!()
                };
                assert_eq!(child.binder_count, 2);
                child.binder_count = 1;
            }
            _ => unreachable!(),
        }
        assert_raw_wire_reject(
            changed,
            candidate.clone(),
            &format!("nested-boundary-{attack}"),
            match attack {
                0 => "omits actual address or child-domain slots",
                1 => "differs from complete actual retained boundary",
                2 => "changes its invariant semantic class",
                3 => "class partition is not complete and canonical",
                _ => unreachable!(),
            },
        );
    }
}

#[test]
fn nested_affine_children_cannot_alias_and_byte_views_keep_the_owner_alive() {
    use zil_ir::{InstructionKind as I, Type};
    let (mut module, candidate) = prepare(NESTED_EXCLUSIVE);
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
    assert_raw_wire_reject(
        module,
        candidate,
        "nested-exclusive-duplicate-child",
        "unpack aliases or retags an actual child",
    );

    let (mut module, candidate) = prepare(BUFFER);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "read")
        .unwrap();
    let bytes = function
        .locals
        .iter()
        .find(|l| l.name == "bytes" && l.ty == Type::Managed(Box::new(Type::ByteBuffer)))
        .unwrap()
        .id;
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::ByteBufferBorrow { .. }))
        })
        .unwrap();
    let drop_index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind,I::DropManaged{local} if local==bytes))
        .unwrap();
    let drop = block.instructions.remove(drop_index);
    let borrowed = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::ByteBufferBorrow { .. }))
        .unwrap();
    // Same initialized local and same cleanup count; only move its real cleanup
    // before the borrowed bytes' last read. StorageDead still clears the same
    // initialization claim, so this tests actual parent-loan retirement.
    block.instructions.insert(borrowed + 1, drop);
    assert_raw_wire_reject(
        module,
        candidate,
        "byte-view-parent-retired",
        "intersects a live incompatible loan",
    );
}

fn assert_raw_wire_reject(
    module: TypedModule,
    mut candidate: CandidatePlan,
    label: &str,
    reason: &str,
) {
    candidate.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for decoded in [false, true] {
        let (raw, plan) = if decoded {
            let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
        } else {
            (module.clone(), candidate.clone())
        };
        let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == ResultCategory::SafeReject
                    && e.code == "E-VERIFY-STORED"
                    && e.message.contains(reason)),
            "{label} Wire={decoded}: {errors:?}"
        );
        eprintln!("{label} Wire={decoded}: {errors:?}");
    }
}

#[test]
fn anonymous_phantom_child_keeps_a_nonempty_semantic_domain() {
    use zil_ir::{LifeTypeProjection, PlaceProjection, Type};
    let (module, candidate) = prepare(PHANTOM);
    if let Some(dir) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            std::path::PathBuf::from(dir).join("anonymous-phantom-typed.txt"),
            format!("{module:#?}"),
        )
        .unwrap();
    }
    verify(module.clone(), candidate.clone());
    let f = module.functions.iter().find(|f| f.name == "read").unwrap();
    let (index, life) = f
        .values
        .iter()
        .enumerate()
        .find_map(|(index, ty)| {
            let Type::BorrowedStorage { storage, life } = ty else {
                return None;
            };
            matches!(storage.as_ref(), Type::Tuple(_)).then_some((index, life))
        })
        .unwrap();
    assert_eq!(life.binder_count, 2);
    assert_eq!(life.slots.len(), 1);
    assert_eq!(life.slots[0].binder, 1);
    assert_eq!(life.retained.len(), 1);
    assert_eq!(life.retained[0].binder, 0);
    assert_eq!(life.nominal_arguments.len(), 1);
    assert_eq!(life.nominal_arguments[0].binder, 0);
    assert_eq!(
        life.nominal_arguments[0].path,
        vec![LifeTypeProjection::Place(PlaceProjection::Field(0))]
    );
    for attack in 0..4 {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "read")
            .unwrap();
        let Type::BorrowedStorage { life, .. } = &mut f.values[index] else {
            unreachable!()
        };
        match attack {
            0 => {
                life.nominal_arguments.clear();
            }
            1 => {
                life.nominal_arguments[0].argument = 1;
            }
            2 => {
                life.nominal_arguments[0].path.clear();
            }
            3 => {
                life.retained[0].binder = 1;
            }
            _ => unreachable!(),
        }
        assert_raw_wire_reject(
            changed,
            candidate.clone(),
            &format!("phantom-child-{attack}"),
            if attack == 0 {
                "omits or fabricates a physical child occurrence"
            } else {
                "nominal occurrence differs from its complete actual child"
            },
        );
    }
}
