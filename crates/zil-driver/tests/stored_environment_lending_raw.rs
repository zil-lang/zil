//! Untrusted executable origins and public lending claims are checked separately.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{
    CallableKind, CandidatePlan, Instruction, InstructionKind, Type, TypedModule, ValueId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
#[test]
fn task_borrowed_result_local_create_poll_has_no_header_dependency() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil");
    for text in [
        source.to_owned(),
        source.replace(
            "worker<life worker_bound>(pointer:&'worker_bound i32)->&'worker_bound i32",
            "worker(pointer:&i32)->&i32",
        ),
        source.replace("worker_bound", "a"),
        source.replace(
            "worker<life worker_bound>(pointer:&'worker_bound i32)->&'worker_bound i32",
            "worker<life worker_bound>(prefix:i32,pointer:&'worker_bound i32,suffix:i32)->&'worker_bound i32",
        ).replace("task_create(worker,input)", "task_create(worker,3,input,4)"),
        source.replace("->&'worker_bound i32", "->&i32"),
    ] {
        let module = typed_stored(&text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
        native_borrowed_enum(&zil_driver::ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "local-create-poll.zil",
                text,
            )],
            paths: Default::default(),
        });
    }
}

#[test]
#[cfg(target_os = "linux")]
fn task_borrowed_result_local_create_poll_allocation_failure_keeps_the_input() {
    native_task_result_allocation_failure(include_str!(
        "../../../tests/fixtures/task-borrow-results/local-create-poll.zil"
    ));
}

#[test]
fn task_borrowed_result_local_create_poll_keeps_the_imported_worker_pointee() {
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                include_str!(
                    "../../../tests/fixtures/task-borrow-results/module-local-create-poll.zil"
                ),
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "api.zil",
                include_str!("../../../tests/fixtures/task-borrow-results/module-worker.zil"),
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    });
}

#[test]
fn task_borrowed_result_local_create_poll_rejects_a_same_layout_wrong_pointee() {
    let source =
        include_str!("../../../tests/fixtures/task-borrow-results/module-local-create-poll.zil")
            .replace("import api", "import api\nstruct Other{number:i32}")
            .replace(
                "let created=task_create(api.worker,input)",
                "let other=Other{number:42}\n let created=task_create(api.worker,&other)",
            );
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "api.zil",
                include_str!("../../../tests/fixtures/task-borrow-results/module-worker.zil"),
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let report = zil_driver::compile_project(
        &project,
        &zil_driver::CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..Default::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.phase == Phase::Types
            || (d.phase == Phase::Expand
                && d.message.contains("no actual sealed result instance"))),
        "{}",
        report.to_json()
    );
}

#[test]
fn task_borrowed_result_local_create_poll_does_not_authorize_a_user_poll_enum() {
    let source = format!(
        "enum Poll<T>{{Pending,Ready(T)}}\n{}",
        include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil")
    );
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "fake-poll.zil", source);
    let report = zil_driver::compile_source(
        &unit,
        &zil_driver::CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..Default::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| matches!(d.phase, Phase::Expand | Phase::Types)),
        "{}",
        report.to_json()
    );
}

#[test]
fn task_borrowed_result_local_create_poll_does_not_extend_the_retained_protocol() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil");
    for text in [
        source
            .replace(
                "pointer:&'worker_bound i32)->&'worker_bound i32",
                "pointer:&'worker_bound i32,second:&'worker_bound i32)->&'worker_bound i32",
            )
            .replace(
                "task_create(worker,input)",
                "task_create(worker,input,input)",
            ),
        source
            .replace(
                "pointer:&'worker_bound i32)->&'worker_bound i32",
                "pointer:&'worker_bound mut i32)->&'worker_bound i32",
            )
            .replace(
                "task_create(worker,input)",
                "task_create(worker,&mut *parent)",
            ),
        source.replace(
            "worker<life worker_bound>(pointer:&'worker_bound i32)->&'worker_bound i32",
            "worker<life worker_bound,life other>(pointer:&'worker_bound i32)->&'other i32",
        ),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "invalid-task-result.zil", text);
        let report = zil_driver::compile_source(
            &unit,
            &zil_driver::CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.phase, Phase::Expand | Phase::Types)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn task_borrowed_result_local_catalog_budget_is_repeatable_at_the_exact_boundary() {
    let control = "fn worker(pointer:&i32)->&i32{return pointer}fn scalar(seed:i32)->i32{return seed}fn main()->TaskCreateError!i32{var owner=task_create(scalar,42)?;task_request_cancel(&mut owner);match task_poll_close(&mut owner){Poll.Pending()=>{return 1},Poll.Ready(_)=>{}}task_finish_destroy(give owner);return 0}";
    let no_task = "fn worker(pointer:&i32)->&i32{return pointer}fn main()->i32{return 0}";
    let scalar_task = "fn scalar(seed:i32)->i32{return seed}fn main()->TaskCreateError!i32{var owner=task_create(scalar,42)?;task_request_cancel(&mut owner);match task_poll_close(&mut owner){Poll.Pending()=>{return 1},Poll.Ready(_)=>{}}task_finish_destroy(give owner);return 0}";
    for (label, source, borrowed_instance) in [
        (
            "borrowed-task",
            include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil"),
            true,
        ),
        ("unused-shared-worker", control, false),
        ("no-task", no_task, false),
        ("scalar-task", scalar_task, false),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "catalog-budget.zil", source);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let expand = |steps| {
            zil_expand::expand_with_map_limits_target_context(
                &unit,
                &ast,
                &Limits {
                    max_analysis_steps: steps,
                    ..Limits::default()
                },
                &TargetProfile::host_llvm(),
                zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                    root: unit.module_id,
                    aliases: &Default::default(),
                    declarations: zil_expand::source_context::DeclarationPlacement::Original,
                },
            )
        };
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        assert!(expand(high).is_ok());
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match expand(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        eprintln!(
            "{label}: accepted budget {high}, rejected budget {}",
            high - 1
        );
        for _ in 0..2 {
            let accepted = expand(high).unwrap();
            assert!(!accepted.core.functions.is_empty());
            assert_eq!(!accepted.core.life_instances.is_empty(), borrowed_instance);
            let rejected = expand(high - 1).unwrap_err();
            assert!(
                rejected
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{rejected:?}"
            );
        }
    }
}

#[test]
fn task_borrowed_result_source_preserves_the_result_class_and_worker_relation() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/shared-input.zil"
    ));
    let worker = module
        .functions
        .iter()
        .find(|f| f.name == "worker")
        .unwrap();
    assert!(worker.storage_life.is_some());
    let mut count = 0;
    for function in &module.functions {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let InstructionKind::TaskCreate { .. } = instruction.kind {
                let result = &function.values[instruction.result.unwrap().0 as usize];
                let Type::ErrorUnion { result, .. } = result else {
                    panic!("TaskCreate error wrapper missing")
                };
                let Type::Managed(task) = result.as_ref() else {
                    panic!("actual Task owner missing")
                };
                let Type::Task {
                    result,
                    retained_life,
                } = task.as_ref()
                else {
                    panic!("Task missing")
                };
                assert!(matches!(
                    result.as_ref(),
                    Type::Address { mutable: false, .. }
                ));
                assert_eq!(retained_life.retained.len(), 1);
                assert_eq!(
                    retained_life.retained[0].path,
                    [zil_ir::LifeTypeProjection::Result]
                );
                assert_eq!(retained_life.retained[0].class, 0);
                assert_eq!(retained_life.retained[0].binder, 0);
                count += 1;
            }
        }
    }
    assert_eq!(count, 1);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn task_borrowed_result_checks_every_ordinary_owner_address_and_ready_hold() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/shared-input.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let mut selected = [None; 4];
    for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
        let Some(value) = instruction.result else {
            continue;
        };
        let ty = &function.values[value.0 as usize];
        let index = match (&instruction.kind, ty) {
            (InstructionKind::BorrowLocal { local }, _)
                if function.locals[local.0 as usize].name == "unrelated" =>
            {
                Some(0)
            }
            (_, Type::Managed(inner)) if matches!(inner.as_ref(), Type::Task { .. }) => Some(1),
            (
                _,
                Type::Address {
                    mutable: true,
                    inner,
                },
            ) if matches!(inner.as_ref(), Type::Managed(task) if matches!(task.as_ref(), Type::Task { .. })) => {
                Some(2)
            }
            (InstructionKind::ReadVariant { .. }, Type::Address { mutable: false, .. }) => Some(3),
            _ => None,
        };
        if let Some(index) = index {
            selected[index] = Some(value);
        }
    }
    for (index, value) in selected.into_iter().enumerate() {
        let value = value.expect("actual source fixture omitted a provenance consumer");
        assert!(
            !plan.functions[function.id.0 as usize].holds[value.0 as usize]
                .roots
                .is_empty()
        );
        for forge in [false, true] {
            let mut candidate = plan.clone();
            let roots =
                &mut candidate.functions[function.id.0 as usize].holds[value.0 as usize].roots;
            *roots = if forge {
                vec![zil_ir::HoldRoot::Static]
            } else {
                vec![]
            };
            reject(
                &format!("task-result-hold-{index}-{forge}"),
                module.clone(),
                candidate,
                "E-VERIFY-STORED",
            );
        }
    }
}

#[test]
fn task_borrowed_result_rejects_frame_local_with_unchanged_result_metadata() {
    for source in [
        include_str!("../../../tests/fixtures/task-borrow-results/shared-input.zil").to_owned(),
        include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil")
            .to_owned(),
        include_str!("../../../tests/fixtures/task-borrow-results/local-create-poll.zil").replace(
            "worker<life worker_bound>(pointer:&'worker_bound i32)->&'worker_bound i32",
            "worker(pointer:&i32)->&i32",
        ),
    ] {
        let mut module = typed_stored(&source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let worker = module
            .functions
            .iter_mut()
            .find(|f| f.name == "worker")
            .unwrap();
        let local = worker.locals.iter().find(|l| l.name == "local").unwrap().id;
        let result = worker
            .blocks
            .iter()
            .find_map(|b| match b.terminator {
                zil_ir::Terminator::Return(Some(v)) => Some(v),
                _ => None,
            })
            .unwrap();
        let definition = worker
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.result == Some(result))
            .unwrap();
        assert!(matches!(definition.kind, InstructionKind::Load(_)));
        definition.kind = InstructionKind::BorrowLocal { local };
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        reject("task-result-frame-local", module, plan, "E-VERIFY-STORED");
    }
}

#[test]
fn task_borrowed_result_rejects_erased_or_forged_result_mapping() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/shared-input.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for mutation in 0..5 {
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "start")
            .unwrap();
        let value = function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                matches!(i.kind, InstructionKind::TaskCreate { .. }).then(|| i.result.unwrap())
            })
            .unwrap();
        let Type::ErrorUnion { result, .. } = &mut function.values[value.0 as usize] else {
            panic!()
        };
        let Type::Managed(task) = result.as_mut() else {
            panic!()
        };
        let Type::Task { retained_life, .. } = task.as_mut() else {
            panic!()
        };
        match mutation {
            0 => retained_life.retained.clear(),
            1 => retained_life.retained[0].path.clear(),
            2 => retained_life.retained[0].class = 1,
            3 => retained_life.retained[0].binder = 1,
            4 => retained_life.ports[0].mode = zil_ir::StoredBorrowMode::ExclusiveAddress,
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
        let category = if mutation == 0 {
            ResultCategory::Unsupported
        } else {
            ResultCategory::SafeReject
        };
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == category),
            "{mutation}: {errors:?}"
        );
        if mutation == 0 {
            let mut candidate = plan.clone();
            candidate.module_fingerprint = changed.fingerprint();
            let bytes =
                zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
                    .unwrap();
            for errors in [
                zil_verify::verify_module(
                    changed.clone(),
                    candidate,
                    &changed.target,
                    &Limits::default(),
                )
                .unwrap_err(),
                zil_verify::verify_module_bytes(&bytes, &changed.target, &Limits::default())
                    .unwrap_err(),
            ] {
                assert!(
                    errors
                        .iter()
                        .any(|d| d.phase == Phase::Verify && d.category == category),
                    "{errors:?}"
                );
            }
        } else {
            reject(
                &format!("task-result-mapping-{mutation}"),
                changed,
                plan.clone(),
                "E-VERIFY-TASK-RESULT",
            );
        }
    }
}

#[test]
fn task_borrowed_result_direct_poll_needs_no_explicit_poll_header() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/shared-input.zil");
    let start = source.find("fn poll<").unwrap();
    let end = source[start..].find("fn main()").unwrap() + start;
    let source = format!("{}{}", &source[..start], &source[end..])
        .replace("match poll(&mut task)", "match task_poll(&mut task)");
    let module = typed_stored(&source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn task_borrowed_result_retains_the_actual_external_reborrow_chain() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/reborrowed-input.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "reborrowed-input.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/reborrowed-input.zil"),
        )],
        paths: Default::default(),
    });
}

#[test]
fn task_borrowed_result_helper_access_preserves_ports_and_requires_native_inventory() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/helper-access.zil");
    let valid = typed_stored(source);
    let invalid = typed_stored(&source.replace(
        "touch(&mut task,&mut unrelated)",
        "touch(&mut task,&mut number)",
    ));
    for clear_resources in [false, true] {
        let mut module = valid.clone();
        if clear_resources {
            module.resources.clear();
        }
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        if clear_resources {
            reject(
                "task-missing-native-inventory",
                module,
                plan.clone(),
                "E-VERIFY-TASK",
            );
        } else {
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits::default(),
            )
            .unwrap();
            let bytes =
                zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
            zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
        }
        let mut bad = invalid.clone();
        if clear_resources {
            bad.resources.clear();
        }
        let errors = zil_analyze::analyze(&bad, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        reject(
            &format!("task-port-alias-empty-inventory-{clear_resources}"),
            bad,
            plan,
            if clear_resources {
                "E-VERIFY-TASK"
            } else {
                "E-VERIFY-STORED"
            },
        );
    }
}

#[test]
fn task_borrowed_result_rejects_external_reborrow_write_before_ready_read() {
    let mut module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/reborrowed-input.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = main
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
        })
        .unwrap();
    let write = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
        .unwrap();
    let InstructionKind::WriteAddress { address, .. } = block.instructions[write].kind else {
        unreachable!()
    };
    let start = block
        .instructions
        .iter()
        .position(|i| i.result == Some(address))
        .unwrap();
    let read = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
        .unwrap();
    assert!(read < start && start < write);
    let moved = block.instructions.drain(start..=write).collect::<Vec<_>>();
    block.instructions.splice(read..read, moved);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    reject("task-ready-parent-write", module, plan, "E-VERIFY-STORED");
}

#[test]
fn task_borrowed_result_create_error_has_no_frame_and_cancel_releases_the_input() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/create-branches.zil");
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "create-branches.zil",
            source,
        )],
        paths: Default::default(),
    });
}

#[test]
fn task_borrowed_result_cross_module_factory_relay_and_poll_preserve_the_input() {
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                include_str!("../../../tests/fixtures/task-borrow-results/module-main.zil"),
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "api.zil",
                include_str!("../../../tests/fixtures/task-borrow-results/module-api.zil"),
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    });
}

#[test]
fn task_borrowed_result_coexists_with_exclusive_and_multiport_scalar_tasks() {
    let source =
        include_str!("../../../tests/fixtures/task-borrow-results/mixed-task-profiles.zil");
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "mixed-task-profiles.zil",
            source,
        )],
        paths: Default::default(),
    });
}

#[test]
fn task_borrowed_result_mixed_profile_checks_each_scalar_task_hold() {
    fn ports(ty: &Type) -> Option<usize> {
        match ty {
            Type::Task {
                result,
                retained_life,
            } if **result == Type::I32 => Some(retained_life.ports.len()),
            Type::Managed(inner) | Type::Address { inner, .. } => ports(inner),
            _ => None,
        }
    }
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/task-borrow-results/mixed-task-profiles.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    for count in [0, 1, 2] {
        for address in [false, true] {
            let value = function
                .values
                .iter()
                .enumerate()
                .find_map(|(i, ty)| {
                    (ports(ty) == Some(count) && matches!(ty, Type::Address { .. }) == address)
                        .then_some(ValueId(i as u32))
                })
                .expect("mixed fixture must exercise each actual scalar Task owner and address");
            assert!(
                plan.functions[function.id.0 as usize].holds[value.0 as usize]
                    .roots
                    .len()
                    >= count
            );
            assert_eq!(
                plan.functions[function.id.0 as usize].holds[value.0 as usize]
                    .roots
                    .is_empty(),
                count == 0 && !address
            );
            for forge in [false, true] {
                // An owner with no retained inputs has a proved empty row.
                if count == 0 && !address && !forge {
                    continue;
                }
                let mut candidate = plan.clone();
                candidate.functions[function.id.0 as usize].holds[value.0 as usize].roots = if forge
                {
                    vec![zil_ir::HoldRoot::Static]
                } else {
                    vec![]
                };
                reject(
                    &format!("scalar-task-hold-{count}-{address}-{forge}"),
                    module.clone(),
                    candidate,
                    "E-VERIFY-STORED",
                );
            }
        }
    }
}

#[test]
fn task_borrowed_result_mixed_profile_preserves_exclusive_port_access() {
    let source =
        include_str!("../../../tests/fixtures/task-borrow-results/mixed-task-profiles.zil");
    let valid = typed_stored(source);
    let plan = zil_analyze::analyze(&valid, &Limits::default()).unwrap();
    for (old, new, code) in [
        (
            "let safe_read=scalar_input",
            "let safe_read=scalar_output",
            "E-BORROW-CONFLICT",
        ),
        ("unrelated=13", "scalar_output=13", "E-VERIFY-STORED"),
    ] {
        let source = source.replace(old, new);
        let invalid = typed_stored(&source);
        let errors = zil_analyze::analyze(&invalid, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        reject(
            "scalar-task-exclusive-input-access",
            invalid,
            plan.clone(),
            code,
        );
    }
    for reverse in [false, true] {
        let mut invalid = valid.clone();
        let instruction = invalid
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| {
                matches!(&i.kind,
                InstructionKind::TaskCreate { port_bindings, .. } if port_bindings.len()==2)
            })
            .unwrap();
        let InstructionKind::TaskCreate { port_bindings, .. } = &mut instruction.kind else {
            unreachable!()
        };
        if reverse {
            port_bindings.reverse();
        } else {
            port_bindings[1].source.base = zil_ir::StorageBase::Parameter(0);
        }
        let errors = zil_analyze::analyze(&invalid, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{errors:?}"
        );
        reject(
            "scalar-task-actual-port-selection",
            invalid,
            plan.clone(),
            "E-VERIFY-TASK",
        );
    }
}

#[test]
fn task_borrowed_result_named_shared_input_retains_its_actual_parent() {
    for (name, source) in [
        (
            "named-shared-input.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/named-shared-input.zil"),
        ),
        (
            "local-parent-join.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-join.zil"),
        ),
        (
            "local-parent-overwrite.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-overwrite.zil"),
        ),
        (
            "local-parent-move.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-move.zil"),
        ),
    ] {
        let module = typed_stored(source);
        let plan = zil_analyze::analyze(&module, &Limits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        native_borrowed_enum(&zil_driver::ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), name, source)],
            paths: Default::default(),
        });
    }
}

#[test]
fn task_borrowed_result_local_parent_overwrites_and_joins_cannot_delegate_to_siblings() {
    for (name, source, old, new) in [
        (
            "overwrite",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-overwrite.zil"),
            "*first=before+1",
            "*second=before+1",
        ),
        (
            "join-first",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-join.zil"),
            "*spare=*spare+1",
            "*first=*first+1",
        ),
        (
            "join-second",
            include_str!("../../../tests/fixtures/task-borrow-results/local-parent-join.zil"),
            "*spare=*spare+1",
            "*second=*second+1",
        ),
    ] {
        let valid = typed_stored(source);
        let plan = zil_analyze::analyze(&valid, &Limits::default())
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let invalid = typed_stored(&source.replace(old, new));
        let errors = zil_analyze::analyze(&invalid, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{name}: {errors:?}"
        );
        reject(name, invalid, plan, "E-VERIFY-STORED");
    }
}
#[test]
fn task_borrowed_result_moved_local_does_not_retain_a_stale_alias_definition() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/local-parent-move.zil");
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
    );
    let invalid = typed_stored(&source.replace("let input=&*moved", "let input=&*base"));
    let errors = zil_analyze::analyze(&invalid, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-UNINITIALIZED"),
        "{errors:?}"
    );
    reject(
        "moved-alias-definition",
        invalid,
        plan,
        "E-VERIFY-INITIALIZATION",
    );
}

#[test]
fn task_borrowed_result_actual_exclusive_alias_moves_cannot_be_duplicated() {
    let source = include_str!("../../../tests/fixtures/task-borrow-results/local-parent-move.zil");
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let function = module
        .functions
        .iter()
        .find(|f| f.name == "attempt")
        .unwrap();
    let local = zil_ir::LocalId(
        function
            .locals
            .iter()
            .position(|l| l.name == "base")
            .unwrap() as u32,
    );
    let (block, position, moved) = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block.instructions.iter().enumerate().find_map(|(p, i)| {
                matches!(i.kind, InstructionKind::TakeLocal { local:l } if l==local)
                    .then(|| (b, p, i.result.unwrap()))
            })
        })
        .unwrap();
    for duplicate_store in [false, true] {
        let mut invalid = module.clone();
        let mut candidate = plan.clone();
        let f = &mut invalid.functions[function.id.0 as usize];
        let instruction = if duplicate_store {
            let original = f.blocks[block]
                .instructions
                .iter()
                .find(|i| matches!(i.kind, InstructionKind::Store { value, .. } if value==moved))
                .unwrap()
                .clone();
            let index = f.blocks[block]
                .instructions
                .iter()
                .position(|i| i == &original)
                .unwrap();
            f.blocks[block]
                .instructions
                .insert(index + 1, original.clone());
            original
        } else {
            let mut duplicate = f.blocks[block].instructions[position].clone();
            let value = ValueId(f.values.len() as u32);
            f.values.push(f.values[moved.0 as usize].clone());
            duplicate.result = Some(value);
            candidate.functions[function.id.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value,
                    roots: plan.functions[function.id.0 as usize].holds[moved.0 as usize]
                        .roots
                        .clone(),
                });
            f.blocks[block]
                .instructions
                .insert(position + 1, duplicate.clone());
            duplicate
        };
        let errors = zil_analyze::analyze(&invalid, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{instruction:?}: {errors:?}"
        );
        reject(
            "exclusive-alias-double-move",
            invalid,
            candidate,
            if duplicate_store {
                "E-VERIFY-MOVE"
            } else {
                "E-VERIFY-INITIALIZATION"
            },
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn task_borrowed_result_native_allocation_failure_preserves_the_input() {
    native_task_result_allocation_failure(include_str!(
        "../../../tests/fixtures/task-borrow-results/allocation-error.zil"
    ));
}
#[test]
#[cfg(target_os = "linux")]
fn task_borrowed_result_named_shared_input_survives_native_allocation_failure() {
    native_task_result_allocation_failure(include_str!(
        "../../../tests/fixtures/task-borrow-results/named-shared-input.zil"
    ));
}
#[cfg(target_os = "linux")]
fn native_task_result_allocation_failure(text: &str) {
    use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let directory = std::env::temp_dir().join(format!(
        "task-result-allocation-error-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let observer = directory.join("allocator-observer.o");
    let built = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../tests/language-readiness/terminal/concurrency-and-registration/task-observer.c",
        ))
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    let wrapper = directory.join("link.sh");
    fs::write(&wrapper, format!(
        "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",
        quote(&clang), quote(&observer), quote(&clang)
    )).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "allocation-error.zil", text);
    for backend in [
        zil_driver::NativeBackend::Clang,
        zil_driver::NativeBackend::LlvmApi,
    ] {
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let label = format!("{backend:?}-{optimization:?}");
            let executable = directory.join(&label);
            let report = zil_driver::compile_source(
                &source,
                &zil_driver::CompileOptions {
                    stop_after: zil_driver::Stage::Build,
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    output: Some(executable.clone()),
                    emit_dir: Some(directory.join(format!("phases-{label}"))),
                    ..Default::default()
                },
            );
            fs::write(
                directory.join(format!("compile-{label}.json")),
                report.to_json(),
            )
            .unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let summary = report.native_codegen.as_ref().unwrap();
            assert_eq!(summary.backend, backend);
            assert!(summary.fallback.is_none());
            let output = Command::new(&executable)
                .env("ZIL343_TASK_ALLOC_FAIL", "1")
                .output()
                .unwrap();
            fs::write(
                directory.join(format!("run-{label}.txt")),
                format!("{output:?}"),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
            assert_eq!(
                output.stdout, b"A",
                "one failed frame allocation, no nonnull free: {label}: {output:?}"
            );
            assert!(output.stderr.is_empty(), "{label}: {output:?}");
        }
    }
}

#[test]
fn task_borrowed_result_native_outlives_the_destroyed_task() {
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "shared-input.zil",
            include_str!("../../../tests/fixtures/task-borrow-results/shared-input.zil"),
        )],
        paths: Default::default(),
    });
}

#[test]
fn standard_poll_shared_payload_has_its_actual_sealed_declaration_and_source() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/poll.zil"
    ));
    let declaration = module
        .life_declarations
        .iter()
        .find(|d| d.id.compiler && d.id.declaration.0 == 1)
        .unwrap();
    assert_eq!(declaration.id.module, ModuleId(u32::MAX));
    assert_eq!(declaration.identity.package, "@zil.builtin");
    assert_eq!(declaration.identity.module_path, ["task"]);
    assert_eq!(declaration.identity.declaration_path, ["enum", "Poll"]);
    assert_eq!(declaration.parameters, [zil_ir::LifeParameterKind::Type]);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn standard_poll_shared_payload_native_keeps_ready_and_pending_sources() {
    for (name, text) in [
        (
            "poll.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/poll.zil"),
        ),
        (
            "poll-record.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/poll-record.zil"),
        ),
    ] {
        native_borrowed_enum(&zil_driver::ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), name, text)],
            paths: Default::default(),
        });
    }
}

#[test]
fn standard_poll_full_actual_resolves_the_complete_nominal_pointee() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/poll-record.zil"
    ));
    let instance = &module.life_instances[0];
    let [zil_ir::LifeNominalActual::Type(actual)] = instance.arguments.as_slice() else {
        panic!("full actual missing")
    };
    assert!(
        matches!(actual, Type::Address { mutable:false, inner } if matches!(inner.as_ref(), Type::Record { fields, .. } if fields.len()==2))
    );
    let Type::BorrowedStorage { storage, .. } = &instance.ty else {
        panic!("qualified Poll missing")
    };
    assert!(
        matches!(storage.as_ref(), Type::Enum { name, variants } if name==&format!("@std.task:v1:Poll:{}", actual.fingerprint()) && variants[1].1==[actual.clone()])
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn standard_poll_error_wrapped_cross_module_helpers_preserve_ready_sources() {
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                include_str!("../../../tests/fixtures/borrowed-enums/poll-module-main.zil"),
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "api.zil",
                include_str!("../../../tests/fixtures/borrowed-enums/poll-module-api.zil"),
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    });
}

#[test]
fn standard_poll_raw_and_wire_require_the_full_sealed_role_and_actual_payload() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/poll.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    type Mutation = (&'static str, fn(&mut TypedModule));
    let mutations: &[Mutation] = &[
        ("poll-role-compiler-flag", |m| {
            m.life_declarations[0].id.compiler = false;
            m.life_instances[0].declaration.compiler = false;
        }),
        ("poll-role-id", |m| {
            m.life_declarations[0].id.declaration.0 = 7;
            m.life_instances[0].declaration.declaration.0 = 7;
        }),
        ("poll-role-module", |m| {
            m.life_declarations[0].id.module = ModuleId(0);
            m.life_instances[0].declaration.module = ModuleId(0);
        }),
        ("poll-role-package", |m| {
            m.life_declarations[0].identity.package = "user".into()
        }),
        ("poll-role-path", |m| {
            m.life_declarations[0].identity.module_path = vec!["other".into()]
        }),
        ("poll-role-declaration", |m| {
            m.life_declarations[0].identity.declaration_path[1] = "Other".into()
        }),
        ("poll-role-generation", |m| {
            m.life_declarations[0].identity.generation_path.push(0)
        }),
        ("poll-role-formal", |m| {
            m.life_declarations[0].parameters[0] = zil_ir::LifeParameterKind::Lifetime
        }),
        ("poll-role-kind", |m| {
            m.life_declarations[0].kind = zil_ir::LifeNominalKind::Record
        }),
        ("poll-missing-instance", |m| m.life_instances.clear()),
        ("poll-wrong-actual", |m| {
            m.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Type(Type::I64)
        }),
        ("poll-retained-ordinal", |m| {
            change_enum_shape(m, |s| {
                s.retained[0].path = vec![zil_ir::LifeTypeProjection::TypeArgument(1)]
            })
        }),
        ("poll-retained-class", |m| {
            change_enum_shape(m, |s| s.retained[0].class = 1)
        }),
        ("poll-payload-path", |m| {
            change_enum_shape(m, |s| {
                s.slots[0].path = vec![zil_ir::PlaceProjection::Variant(
                    zil_ir::VariantProjection {
                        variant: 0,
                        index: 0,
                    },
                )]
            })
        }),
        ("poll-payload-mode", |m| {
            change_enum_shape(m, |s| {
                s.slots[0].mode = zil_ir::StoredBorrowMode::ExclusiveAddress
            })
        }),
        ("poll-payload-class", |m| {
            change_enum_shape(m, |s| s.slots[0].binder = 1)
        }),
        ("poll-reserved-physical-domain", |m| {
            let d = &mut m.life_declarations[0];
            d.id.compiler = false;
            d.id.module = ModuleId(0);
            d.id.declaration.0 = 17;
            d.identity.package = "user".into();
            d.identity.module_path = vec!["ordinary".into()];
            m.life_instances[0].declaration = d.id;
        }),
        ("poll-wrong-canonical-actual-name", |m| {
            change_poll_type(m, |t| {
                let Type::BorrowedStorage { storage, .. } = t else {
                    unreachable!()
                };
                let Type::Enum { name, .. } = storage.as_mut() else {
                    unreachable!()
                };
                *name = "@std.task:v1:Poll:wrong-actual".into();
            })
        }),
        ("poll-wrong-ready-layout", |m| {
            change_poll_type(m, |t| {
                let Type::BorrowedStorage { storage, .. } = t else {
                    unreachable!()
                };
                let Type::Enum { variants, .. } = storage.as_mut() else {
                    unreachable!()
                };
                variants[1].0 = "Some".into();
            })
        }),
    ];
    for (name, mutate) in mutations {
        let mut changed = module.clone();
        mutate(&mut changed);
        let errors = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{name}: {errors:?}"
        );
        reject(name, changed, plan.clone(), "E-VERIFY-STORED");
    }
}

fn change_poll_type(module: &mut TypedModule, mutate: fn(&mut Type)) {
    let before = module.life_instances[0].ty.clone();
    let mut after = before.clone();
    mutate(&mut after);
    for function in &mut module.functions {
        for ty in function
            .values
            .iter_mut()
            .chain(function.locals.iter_mut().map(|l| &mut l.ty))
            .chain(std::iter::once(&mut function.return_type))
        {
            if *ty == before {
                *ty = after.clone();
            }
        }
    }
    module.life_instances[0].ty = after;
}

#[test]
fn standard_poll_payload_permissions_require_real_variant_and_error_guards() {
    let error_source = format!(
        "{}fn main(){{}}",
        include_str!("../../../tests/fixtures/borrowed-enums/poll-module-api.zil")
    );
    for (name, source, error_guard) in [
        (
            "poll-missing-variant-guard",
            include_str!("../../../tests/fixtures/borrowed-enums/poll.zil"),
            false,
        ),
        ("poll-missing-error-guard", error_source.as_str(), true),
    ] {
        let mut module = typed_stored(source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let mut changed = false;
        for function in &mut module.functions {
            for i in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if (!error_guard && matches!(i.kind, InstructionKind::VariantTag { .. }))
                    || (error_guard && matches!(i.kind, InstructionKind::ErrorTag { .. }))
                {
                    i.kind = InstructionKind::Constant(if error_guard {
                        zil_ir::Constant::Bool(false)
                    } else {
                        zil_ir::Constant::I32(1)
                    });
                    changed = true;
                }
            }
        }
        assert!(changed, "{name}");
        let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{name}: {errors:?}"
        );
        reject(name, module, plan, "E-VERIFY-STORED");
    }
}

#[test]
fn standard_poll_core_cannot_claim_the_reserved_role_as_an_ordinary_declaration() {
    let mut core = stored_core(include_str!(
        "../../../tests/fixtures/borrowed-enums/poll.zil"
    ));
    core.life_declarations[0].id.compiler = false;
    core.life_instances[0].declaration.compiler = false;
    let errors =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject && d.code == "E-STORED-IDENTITY"),
        "{errors:?}"
    );
}

#[test]
fn user_poll_name_stays_an_ordinary_nominal_and_preserves_actual_borrows() {
    let source = "enum Poll<T>{Pending,Ready(T)}fn select<life a>(p:&'a i32)->Poll<&'a i32>{return Poll::<&'a i32>.Ready(p)}fn main()->i32{let n=42;match select(&n){Poll.Pending()=>{return 1},Poll.Ready(p)=>{return *p-42}}}";
    let module = typed_stored(source);
    assert!(module.life_declarations.iter().all(|d| !d.id.compiler));
    assert!(module.life_instances.iter().all(|i| matches!(&i.ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Enum { name, .. } if name.starts_with("@module")))));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn standard_poll_and_option_keep_distinct_roles_after_prelude_alias_insertion() {
    let source = concat!(
        "fn ready<life a>(p:&'a i32)->Poll<&'a i32>{return Poll::<&'a i32>.Ready(p)}",
        "fn some<life a>(p:&'a i32)->Option<&'a i32>{return Option::<&'a i32>.Some(p)}",
        "fn main()->i32{let n=42;let a=ready(&n);let b=some(&n);match a{Poll.Pending()=>{return 1},Poll.Ready(p)=>{match b{Option.None()=>{return 2},Option.Some(q)=>{return *p+*q-84}}}}}"
    );
    let module = typed_stored(source);
    let mut roles = module
        .life_declarations
        .iter()
        .map(|d| {
            assert!(d.id.compiler && d.id.module == ModuleId(u32::MAX));
            (d.id.declaration.0, d.identity.module_path.clone())
        })
        .collect::<Vec<_>>();
    roles.sort();
    assert_eq!(
        roles,
        vec![(0, vec!["option".into()]), (1, vec!["task".into()])]
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_source_uses_actual_tagged_slots() {
    let source = include_str!("../../../tests/fixtures/borrowed-enums/choice.zil");
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_generic_address_actuals_use_their_own_class_maps() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/generic-selection.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_empty_tag_has_no_payload_loan() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/vacant.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_error_wrapper_preserves_payload_permissions() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/error-wrapper.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_error_projection_requires_the_actual_error_tag_guard() {
    let mut module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/error-wrapper.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut changed = false;
    for function in &mut module.functions {
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::ErrorTag { value } = instruction.kind
                && matches!(&function.values[value.0 as usize], Type::ErrorUnion { result, .. }
                    if matches!(result.as_ref(), Type::BorrowedStorage { .. }))
            {
                instruction.kind = InstructionKind::Constant(zil_ir::Constant::Bool(false));
                changed = true;
            }
        }
    }
    assert!(changed);
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    reject(
        "borrowed-error-missing-tag-guard",
        module,
        plan,
        "E-VERIFY-STORED",
    );
}

#[test]
fn ordinary_borrowed_enum_same_class_preserves_the_actual_selected_root() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/same-life.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}

#[test]
fn ordinary_borrowed_enum_local_address_cannot_satisfy_the_return_class() {
    let source = "enum Choice<life a>{Empty,Value(&'a i32)}fn escape<life a>(input:&'a i32)->Choice<'a>{let local=17;return Choice::<'a>.Value(&local)}fn main(){}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "escape.zil", source);
    let project = zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![unit],
        paths: Default::default(),
    };
    let report = zil_driver::compile_project(&project, &Default::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("source class")),
        "{}",
        report.to_json()
    );
}

#[test]
fn ordinary_borrowed_enum_empty_join_cannot_ground_a_recursive_unknown_payload() {
    let source = "enum Choice<life a>{Empty,Value(&'a i32)}fn seed<life a>(p:&'a i32)->Choice<'a>{return Choice::<'a>.Value(p)}fn missing<life a>(p:&'a i32)->Choice<'a>{return seed(p)}fn next<life a>(p:&'a i32,flag:bool)->Choice<'a>{var local:Choice<'a>=Choice::<'a>.Empty();if flag{local=missing(p)}return local}fn main(){}";
    let module = typed_stored(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let bad = typed_stored(&source.replace("return seed(p)", "return missing(p)"));
    assert!(zil_analyze::analyze(&bad, &Limits::default()).is_err());
    reject("recursive-unknown-enum", bad, plan, "E-VERIFY-STORED");
}

#[test]
fn ordinary_borrowed_enum_raw_rejects_an_inactive_payload_and_cross_class_graft() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/choice.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut inactive = module.clone();
    let mut changed = false;
    for function in &mut inactive.functions {
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::ReadVariant { variant, .. } = &mut instruction.kind
                && *variant == 1
                && !changed
            {
                *variant = 2;
                changed = true;
            }
        }
    }
    assert!(changed);
    assert!(zil_analyze::analyze(&inactive, &Limits::default()).is_err());
    reject(
        "borrowed-enum-inactive-payload",
        inactive,
        plan.clone(),
        "E-VERIFY-STORED",
    );

    let mut graft = module;
    let mut changed = false;
    for function in &mut graft.functions {
        if function.parameters.len() != 3 {
            continue;
        }
        let right_local = function.parameters[1];
        let right_ty = function.locals[right_local.0 as usize].ty.clone();
        for block in &mut function.blocks {
            if let Some(position) = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::MakeVariant { variant: 1, .. }))
            {
                let id = ValueId(function.values.len() as u32);
                function.values.push(right_ty.clone());
                let span = block.instructions[position].span;
                block.instructions.insert(
                    position,
                    Instruction {
                        result: Some(id),
                        kind: InstructionKind::Load(right_local),
                        span,
                    },
                );
                if let InstructionKind::MakeVariant { fields, .. } =
                    &mut block.instructions[position + 1].kind
                {
                    fields[0] = id;
                    changed = true;
                }
            }
        }
    }
    assert!(changed);
    assert!(zil_analyze::analyze(&graft, &Limits::default()).is_err());
    reject(
        "borrowed-enum-cross-class-graft",
        graft,
        plan,
        "E-VERIFY-STORED",
    );
}

#[test]
fn ordinary_borrowed_enum_raw_requires_the_full_actual_shape_and_nominal_inventory() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/generic-selection.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    type Mutation = (&'static str, fn(&mut TypedModule));
    let mutations: &[Mutation] = &[
        ("missing-instance", |m| m.life_instances.clear()),
        ("missing-declaration", |m| m.life_declarations.clear()),
        ("wrong-formal-kind", |m| {
            m.life_declarations[0].parameters[0] = zil_ir::LifeParameterKind::Lifetime
        }),
        ("wrong-nominal-kind", |m| {
            m.life_declarations[0].kind = zil_ir::LifeNominalKind::Record
        }),
        ("duplicate-instance", |m| {
            m.life_instances.push(m.life_instances[0].clone())
        }),
        ("wrong-full-actual", |m| {
            m.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Type(Type::I64)
        }),
        ("wrong-retained-ordinal", |m| {
            change_enum_shape(m, |s| {
                s.retained[0].path = vec![zil_ir::LifeTypeProjection::TypeArgument(1)]
            })
        }),
        ("wrong-local-class", |m| {
            change_enum_shape(m, |s| s.retained[0].class = 1)
        }),
        ("wrong-slot-tag", |m| {
            change_enum_shape(m, |s| {
                s.slots[0].path = vec![zil_ir::PlaceProjection::Variant(
                    zil_ir::VariantProjection {
                        variant: 2,
                        index: 0,
                    },
                )]
            })
        }),
        ("wrong-slot-mode", |m| {
            change_enum_shape(m, |s| {
                s.slots[0].mode = zil_ir::StoredBorrowMode::ExclusiveAddress
            })
        }),
        ("missing-slot", |m| {
            change_enum_shape(m, |s| {
                s.slots.pop();
            })
        }),
        ("cross-class-slot", |m| {
            change_enum_shape(m, |s| s.slots[1].binder = 0)
        }),
    ];
    for (name, mutate) in mutations {
        let mut changed = module.clone();
        mutate(&mut changed);
        assert_ne!(changed, module, "{name}");
        let errors = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::SafeReject),
            "{name}: {errors:?}"
        );
        reject(name, changed, plan.clone(), "E-VERIFY-STORED");
    }
}

#[test]
fn ordinary_borrowed_enum_same_physical_leaf_cannot_change_a_short_source_into_a_long_class() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/borrowed-enums/mixed-classes.zil"
    ));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let mut graft = module.clone();
    let mut changed = false;
    for function in &mut graft.functions {
        for block in &mut function.blocks {
            if let Some(id) = block.instructions.iter().find_map(|i| match i.kind {
                InstructionKind::ReadVariant { variant: 1, .. } => i.result,
                _ => None,
            }) {
                block.terminator = zil_ir::Terminator::Return(Some(id));
                changed = true;
            }
        }
    }
    assert!(changed);
    assert!(zil_analyze::analyze(&graft, &Limits::default()).is_err());
    reject(
        "same-physical-short-source",
        graft.clone(),
        plan.clone(),
        "E-VERIFY-STORED",
    );
    // Coherently rewrite every complete type too. The actual constructor still
    // receives `second`; a TypeArgument map cannot replace that actual source.
    change_enum_shape(&mut graft, |shape| {
        let actual = shape.retained[0].binder;
        shape.slots[1].binder = actual;
    });
    assert_ne!(graft, module);
    assert!(zil_analyze::analyze(&graft, &Limits::default()).is_err());
    reject(
        "coherent-shape-short-source",
        graft,
        plan,
        "E-VERIFY-STORED",
    );
}

// Change both the inventory and every complete executable type. These raw
// counterfeits must fail their own shape proof, not merely table equality.
fn change_enum_shape(module: &mut TypedModule, mutate: fn(&mut zil_ir::StorageLifeShape)) {
    let before = module.life_instances[0].ty.clone();
    let mut after = before.clone();
    let Type::BorrowedStorage { life, .. } = &mut after else {
        panic!("qualified Enum is absent")
    };
    mutate(life);
    for function in &mut module.functions {
        for ty in function
            .values
            .iter_mut()
            .chain(function.locals.iter_mut().map(|l| &mut l.ty))
            .chain(std::iter::once(&mut function.return_type))
        {
            if *ty == before {
                *ty = after.clone();
            }
        }
    }
    module.life_instances[0].ty = after;
}

#[test]
fn ordinary_borrowed_enum_native_preserves_each_payload_source() {
    for (name, text) in [
        (
            "choice.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/choice.zil"),
        ),
        (
            "generic-selection.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/generic-selection.zil"),
        ),
        (
            "vacant.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/vacant.zil"),
        ),
        (
            "error-wrapper.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/error-wrapper.zil"),
        ),
        (
            "same-life.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/same-life.zil"),
        ),
        (
            "mixed-classes.zil",
            include_str!("../../../tests/fixtures/borrowed-enums/mixed-classes.zil"),
        ),
    ] {
        native_borrowed_enum(&zil_driver::ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), name, text)],
            paths: Default::default(),
        });
    }
}

#[test]
fn ordinary_borrowed_enum_cross_module_helpers_preserve_nominal_and_source_identity() {
    native_borrowed_enum(&zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                include_str!("../../../tests/fixtures/borrowed-enums/module-main.zil"),
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "api.zil",
                include_str!("../../../tests/fixtures/borrowed-enums/module-api.zil"),
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    });
}

fn native_borrowed_enum(project: &zil_driver::ProjectSources) {
    for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
        for backend in [
            zil_driver::NativeBackend::Clang,
            zil_driver::NativeBackend::LlvmApi,
        ] {
            let directory = std::env::temp_dir().join(format!(
                "borrowed-enum-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).unwrap();
            let executable = directory.join("program");
            let report = zil_driver::compile_project(
                project,
                &zil_driver::CompileOptions {
                    stop_after: zil_driver::Stage::Build,
                    optimization,
                    backend,
                    output: Some(executable.clone()),
                    emit_dir: Some(directory.join("phases")),
                    ..Default::default()
                },
            );
            fs::write(directory.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let summary = report.native_codegen.as_ref().unwrap();
            assert_eq!(summary.backend, backend);
            assert!(summary.fallback.is_none());
            let output = std::process::Command::new(&executable).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?} {optimization:?}: {output:?}"
            );
            assert!(output.stdout.is_empty());
        }
    }
}
const SOURCE: &str = "fn make(a:i32,b:i32)->impl Fn()->&i32{return move()=>{b;return &a}}fn main()->i32{let f=make(17,25);let s=f();return *s-17}";

fn typed(source: &str) -> TypedModule {
    let s = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "stored_environment_lending_raw.zil",
        source,
    );
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn stored_core(source: &str) -> zil_ir::CoreModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "stored-borrow-move-out.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand_with_map_limits_target_context(
        &unit,
        &ast,
        &Limits::default(),
        &TargetProfile::host_llvm(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core
}
fn typed_stored(source: &str) -> TypedModule {
    zil_types::check(
        &stored_core(source),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap()
}
fn baseline() -> (TypedModule, CandidatePlan) {
    let m = typed(SOURCE);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
fn record(name: &str, m: &TypedModule, p: &CandidatePlan, results: &[Vec<Diagnostic>]) {
    if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(base).join(format!(
            "raw184-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("module.txt"), format!("{m:#?}")).unwrap();
        fs::write(dir.join("candidate.txt"), format!("{p:#?}")).unwrap();
        fs::write(dir.join("results.txt"), format!("{results:#?}")).unwrap();
    }
}
fn reject(name: &str, m: TypedModule, mut p: CandidatePlan, code: &str) {
    p.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(decoded, (m.clone(), p.clone()));
    let results = [
        zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err(),
    ];
    record(name, &m, &p, &results);
    for ds in results {
        assert!(
            ds.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{name}: {ds:?}"
        );
    }
}
fn synchronize(m: &mut TypedModule) {
    for f in &mut m.functions {
        if let Some(c) = &mut f.callable_return_contract {
            c.lending_captures = m.closures[c.closure.0 as usize].lending_captures.clone();
            c.kind = m.closures[c.closure.0 as usize].kind;
        }
    }
}

#[test]
fn move_out_producer_emits_take_for_the_actual_exclusive_slot() {
    let module = typed_stored(include_str!(
        "../../../tests/fixtures/stored-borrow-move-out.zil"
    ));
    let takes: Vec<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter_map(|i| match &i.kind {
                    InstructionKind::LoadStoredBorrow {
                        path,
                        mode: zil_ir::StoredBorrowLoad::Take,
                        ..
                    } => Some((path, &f.values[i.result.unwrap().0 as usize])),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(takes.len(), 1);
    assert_eq!(takes[0].0, &[zil_ir::PlaceProjection::Field(1)]);
    assert_eq!(
        takes[0].1,
        &Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        }
    );
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::LoadStoredBorrow {
                    mode: zil_ir::StoredBorrowLoad::Reborrow { mutable: true },
                    ..
                }
            ))
    );
}

#[test]
fn move_out_producer_supports_a_qualified_record_slot() {
    let module = typed_stored(
        "struct Holder<life a>{pointer:&'a mut i32}fn exchange<life a>(holder:&mut Holder<'a>,next:&'a mut i32)->&'a mut i32{let previous=move_out(holder.pointer);holder.pointer=next;return previous}fn main(){}",
    );
    let paths: Vec<_> = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match &i.kind {
            InstructionKind::LoadStoredBorrow {
                path,
                mode: zil_ir::StoredBorrowLoad::Take,
                ..
            } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(paths, vec![&vec![zil_ir::PlaceProjection::Field(0)]]);
}

#[test]
fn move_out_producer_preserves_a_shared_slot_mode_across_refill() {
    let module = typed_stored(
        "struct Holder<life a>{pointer:&'a i32}fn exchange<life a>(holder:&mut Holder<'a>,next:&'a i32)->i32{let previous=move_out(holder.pointer);holder.pointer=next;return *previous+*holder.pointer}fn main(){}",
    );
    let takes: Vec<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter_map(|i| match &i.kind {
                    InstructionKind::LoadStoredBorrow {
                        path,
                        mode: zil_ir::StoredBorrowLoad::Take,
                        ..
                    } => Some((path, &f.values[i.result.unwrap().0 as usize])),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(takes.len(), 1);
    assert_eq!(takes[0].0, &[zil_ir::PlaceProjection::Field(0)]);
    assert_eq!(
        takes[0].1,
        &Type::Address {
            mutable: false,
            inner: Box::new(Type::I32)
        }
    );
}

#[test]
fn qualified_holder_copy_field_assignment_uses_ordinary_place() {
    let module = typed_stored(
        "struct SliceIter<life a>{data:&'a i32;index:i64}fn advance<life a>(holder:&mut SliceIter<'a>){holder.index=holder.index+1}fn main(){}",
    );
    let advance = module
        .functions
        .iter()
        .find(|f| f.name == "advance")
        .unwrap();
    assert!(
        advance
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::WriteAddress { .. }))
    );
    assert!(
        !advance
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::StoreStoredBorrow { .. }))
    );
}

#[test]
fn move_out_producer_rejects_unqualified_or_unavailable_places() {
    let sources = [
        (
            "struct Holder<life a>{pointer:&'a mut i32}fn take<life a>(holder:&Holder<'a>){move_out(holder.pointer)}fn main(){}",
            "E-ADDRESS-PERMISSION",
        ),
        (
            "struct Holder<life a>{pointer:&'a mut i32}fn take<life a>(pointer:&'a mut i32){let holder=Holder::<'a>{pointer:pointer};move_out(holder.pointer)}fn main(){}",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "struct Holder{count:i32}fn take(holder:&mut Holder){move_out(holder.count)}fn main(){}",
            "E-MOVE-OUT-PLACE",
        ),
        (
            "struct Holder<life a>{pointer:&'a mut i32,count:i32}fn take<life a>(holder:&mut Holder<'a>){move_out(holder.count)}fn main(){}",
            "E-MOVE-OUT-PLACE",
        ),
        (
            "fn main(){var n=1;let pointer=&mut n;move_out(pointer)}",
            "E-MOVE-OUT-PLACE",
        ),
        (
            "struct Holder<life a>{pointer:&'a mut i32}fn take<life a>(pointer:&'a mut i32){move_out(Holder::<'a>{pointer:pointer}.pointer)}fn main(){}",
            "E-UNSUPPORTED",
        ),
    ];
    for (source, code) in sources {
        let core = stored_core(source);
        let errors =
            zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == code),
            "{source}: {errors:?}"
        );
    }
}

#[test]
fn move_out_source_requires_a_single_argument_and_reserved_bare_name() {
    for (source, code) in [
        ("fn main(){move_out()}", "E-MOVE-OUT-ARGUMENT"),
        ("fn main(){move_out(1,2)}", "E-MOVE-OUT-ARGUMENT"),
        ("fn main(){move_out(...(1,2))}", "E-MOVE-OUT-ARGUMENT"),
        ("fn move_out(){}fn main(){}", "E-MOVE-OUT-NAME"),
        ("fn main(){let move_out=1}", "E-MOVE-OUT-NAME"),
        ("fn take(move_out:i32){}fn main(){}", "E-MOVE-OUT-NAME"),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "move-out-surface.zil", source);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let errors = zil_expand::expand(&unit, &ast).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.code == code),
            "{source}: {errors:?}"
        );
    }
}

#[test]
fn raw_and_wire_reconstruct_exact_environment_sources() {
    for (name, indices) in [("missing", vec![]), ("overclaim", vec![0, 1])] {
        let (mut m, p) = baseline();
        m.closures[0].lending_captures = indices;
        synchronize(&mut m);
        reject(name, m, p, "E-VERIFY-LENDING");
    }
    let (mut m, p) = baseline();
    let actual = m.closures[0].lending_captures[0];
    m.closures[0].lending_captures = vec![1 - actual];
    synchronize(&mut m);
    reject("wrong-source", m, p, "E-VERIFY-LENDING");
    let (mut m, _) = baseline();
    let entry = m.closures[0].function.0 as usize;
    let local = m.functions[entry]
        .locals
        .iter()
        .find(|l| l.name == "a")
        .unwrap()
        .id;
    let mut changed = false;
    for i in m.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if matches!(i.kind, InstructionKind::BorrowCapture { .. }) {
            i.kind = InstructionKind::BorrowLocal { local };
            changed = true;
        }
    }
    assert!(changed);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    reject("activation-address", m, p, "E-BORROW-ESCAPE");
}

#[test]
fn raw_and_wire_reject_consumed_and_by_value_environment_results() {
    let (mut m, p) = baseline();
    m.closures[0].kind = CallableKind::FnOnce;
    synchronize(&mut m);
    reject("consuming-kind", m, p, "E-VERIFY-LENDING");
    for once in [false, true] {
        let (mut m, _) = baseline();
        let mut changed = false;
        for f in &mut m.functions {
            for b in &mut f.blocks {
                let mut rebuilt = Vec::new();
                for mut i in std::mem::take(&mut b.instructions) {
                    if let InstructionKind::CallClosureShared { callee, arguments } = &i.kind {
                        let callee = *callee;
                        let arguments = arguments.clone();
                        let value = ValueId(f.values.len() as u32);
                        f.values.push(f.locals[callee.0 as usize].ty.clone());
                        rebuilt.push(Instruction {
                            result: Some(value),
                            kind: InstructionKind::Load(callee),
                            span: i.span,
                        });
                        i.kind = if once {
                            InstructionKind::CallClosureOnce {
                                callee: value,
                                arguments,
                            }
                        } else {
                            InstructionKind::CallClosure {
                                callee: value,
                                arguments,
                            }
                        };
                        changed = true;
                    }
                    rebuilt.push(i);
                }
                b.instructions = rebuilt;
            }
        }
        assert!(changed);
        let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
        reject(
            if once { "once-call" } else { "by-value-call" },
            m,
            p,
            if once {
                "E-VERIFY-CLOSURE-CALL"
            } else {
                "E-VERIFY-LENDING"
            },
        );
    }
}

#[test]
fn raw_and_wire_cannot_erase_result_hold_or_mutate_shared_entry() {
    let (m, mut p) = baseline();
    let mut changed = false;
    for (f, facts) in m.functions.iter().zip(&mut p.functions) {
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(i.kind, InstructionKind::CallClosureShared { .. }) {
                facts.holds[i.result.unwrap().0 as usize].roots.clear();
                changed = true;
            }
        }
    }
    assert!(changed);
    reject("erased-hold", m, p, "E-VERIFY-HOLD");
    let (mut m, p) = baseline();
    let entry = m.closures[0].function.0 as usize;
    let mut result = None;
    for i in m.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::BorrowCapture { mutable, .. } = &mut i.kind {
            *mutable = true;
            result = i.result;
            break;
        }
    }
    let Type::Address { mutable, .. } = &mut m.functions[entry].values[result.unwrap().0 as usize]
    else {
        unreachable!()
    };
    *mutable = true;
    reject("shared-write", m, p, "E-VERIFY-CLOSURE-CALL");
}

#[test]
fn raw_and_wire_keep_owned_payload_write_and_cleanup_obligations() {
    let source = "fn make()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}fn main()->i32{var f=make();let s=f();return i32(s.len())-1}";
    let mut m = typed(source);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    m.closures[0].kind = CallableKind::Fn;
    synchronize(&mut m);
    for i in m
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::CallClosureMut { callee, arguments } = &i.kind {
            i.kind = InstructionKind::CallClosureShared {
                callee: *callee,
                arguments: arguments.clone(),
            };
        }
    }
    reject("owned-write-as-shared", m, p, "E-VERIFY-RESOURCE");
    let mut m = typed(source);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    let mut changed = false;
    for f in &mut m.functions {
        for b in &mut f.blocks {
            if let Some(call) = b
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }))
            {
                let drop = b
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
                    .unwrap();
                assert!(drop > call);
                let cleanup = b.instructions.remove(drop);
                b.instructions.insert(call + 1, cleanup);
                changed = true;
            }
        }
    }
    assert!(changed);
    reject("early-environment-drop", m, p, "E-BORROW-CONFLICT");
}

#[test]
fn analyze_bounds_both_public_lending_vectors_before_fingerprinting() {
    for contract in [false, true] {
        let (mut m, _) = baseline();
        if contract {
            m.functions
                .iter_mut()
                .find_map(|f| f.callable_return_contract.as_mut())
                .unwrap()
                .lending_captures = vec![0; 17];
        } else {
            m.closures[0].lending_captures = vec![0; 17];
        }
        let limits = Limits {
            max_locals: 16,
            ..Limits::default()
        };
        let errors = zil_analyze::analyze(&m, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::ResourceLimit
                && d.message.contains("lending")),
            "{errors:?}"
        );
    }
}

#[test]
fn analyzer_rejects_opaque_lending_mismatch_and_pays_nonempty_metadata() {
    let (mut m, _) = baseline();
    m.functions
        .iter_mut()
        .find_map(|f| f.callable_return_contract.as_mut())
        .unwrap()
        .lending_captures
        .clear();
    let errors = zil_analyze::analyze(&m, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && d.message.contains("lending")),
        "{errors:?}"
    );
    let (m, _) = baseline();
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_analyze::analyze(&m, &limits) {
            Ok(_) => high = middle,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:?}"
                );
                low = middle;
            }
        }
    }
    assert!(
        zil_analyze::analyze(
            &m,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let below = zil_analyze::analyze(
        &m,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        below
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        fs::create_dir_all(&base).unwrap();
        fs::write(
            PathBuf::from(base).join("analyze184-boundary.json"),
            format!("{{\"accepted\":{high},\"rejected\":{low}}}\n"),
        )
        .unwrap();
    }
}

#[test]
fn optional_owned_store_inputs_and_identity_preserve_actual_sources() {
    for source in [
        include_str!("../../../tests/fixtures/optional-store-address/helper-input.zil"),
        include_str!("../../../tests/fixtures/optional-store-address/identity.zil"),
        include_str!("../../../tests/fixtures/optional-store-address/field-helper.zil"),
        include_str!("../../../tests/fixtures/optional-store-address/separate-managers.zil"),
    ] {
        let module = typed_stored(source);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    }
}

#[test]
fn optional_owned_store_live_some_pins_manager_before_projection_raw_and_wire() {
    let mut module = typed_stored(include_str!(
        "../../../tests/fixtures/optional-store-address/separate-managers.zil"
    ));
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "result")
        .unwrap();
    let map = function
        .locals
        .iter()
        .position(|l| l.name == "map")
        .unwrap();
    let other = function
        .locals
        .iter()
        .position(|l| l.name == "other")
        .unwrap();
    let mut changed = 0;
    for instruction in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        if let InstructionKind::DescribePackageField { local, .. } = &mut instruction.kind
            && local.0 as usize == other
        {
            *local = zil_ir::LocalId(map as u32);
            let value = instruction.result.unwrap();
            plan.functions[function.id.0 as usize].holds[value.0 as usize].roots =
                vec![zil_ir::HoldRoot::Local(zil_ir::LocalId(map as u32))];
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && (d.code == "E-BORROW-CONFLICT" || d.message.contains("live child"))),
        "{errors:?}"
    );
    // The mutation changes only a weak description's actual manager. It keeps
    // initialization and every public Hold claim valid, so those cannot hide
    // the independent pre-projection live-Some conflict.
    reject(
        "optional-store-live-some-manager",
        module,
        plan,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn optional_store_key_cannot_replace_the_actual_manager_source() {
    let mut module = typed_stored(include_str!(
        "../../../tests/fixtures/optional-store-address/key-source.zil"
    ));
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let getter = module
        .functions
        .iter_mut()
        .find(|f| f.name == "get")
        .unwrap();
    let key = getter.parameters[2];
    let key_value = getter
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::Load(local) if local == key)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for instruction in getter.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::MakeVariant { variant: 1, fields } = &mut instruction.kind {
            assert_eq!(fields.len(), 1);
            fields[0] = key_value;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(decoded, (module.clone(), plan.clone()));
    let results = [
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err(),
    ];
    record("optional-store-key-source", &module, &plan, &results);
    for errors in results {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-STORED"),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .all(|d| !d.code.contains("PLAN") && !d.code.contains("HOLD")),
            "{errors:?}"
        );
    }
}
