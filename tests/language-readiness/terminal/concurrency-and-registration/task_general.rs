//! General Task source consumers; scalar worker admission remains explicit.
use super::*;
const SOURCE: &str = include_str!("task-general-i32.zil");
const SHARED_INPUT: &str =
    include_str!("../../task-composition/verified-source-consumers/shared-input-retained-task.zil");
#[path = "../../task-composition/verified-source-consumers/source_consumers.rs"]
mod verified_source_consumers;
fn closed_task_shape() -> zil_ir::RetainedLifeShape {
    zil_ir::RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
#[test]
fn shared_task_frame_retains_real_input_through_close() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    let poll = SHARED_INPUT
        .find("            match task_poll(&mut task)? {")
        .unwrap();
    let close = SHARED_INPUT
        .find("    match task_poll_close(&mut task) {")
        .unwrap();
    let pc0 = format!("{}{}", &SHARED_INPUT[..poll], &SHARED_INPUT[close..]);
    let second = SHARED_INPUT[poll + 1..]
        .find("            match task_poll(&mut task)? {")
        .unwrap()
        + poll
        + 1;
    let cancel = format!(
        "{}            task_request_cancel(&mut task);\n{}",
        &SHARED_INPUT[..second],
        &SHARED_INPUT[close..]
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, input, expected) in [
                ("complete", SHARED_INPUT, b"PRCA".as_slice()),
                ("pc0-close", pc0.as_str(), b"CA".as_slice()),
                ("pending-cancel", cancel.as_str(), b"PCA".as_slice()),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "shared-task-port-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(input),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
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
                assert!(native.fallback.is_none(), "{native:?}");
                let output = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(output.status.code(), Some(0), "{output:?}");
                assert_eq!(output.stdout, expected);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn shared_task_frame_rejects_input_writes_before_close() {
    let before_poll = SHARED_INPUT.replace(
        "            match task_poll(&mut task)? {",
        "            value=43;\n            match task_poll(&mut task)? {",
    );
    let second = SHARED_INPUT
        .find("            match task_poll(&mut task)? {")
        .and_then(|first| {
            SHARED_INPUT[first + 1..]
                .find("            match task_poll(&mut task)? {")
                .map(|next| first + 1 + next)
        })
        .unwrap();
    let after_pending = format!(
        "{}            value=43;\n{}",
        &SHARED_INPUT[..second],
        &SHARED_INPUT[second..]
    );
    let after_cancel = SHARED_INPUT.replace("            match task_poll_close(&mut task) {", "            task_request_cancel(&mut task);\n            value=43;\n            match task_poll_close(&mut task) {");
    let after_ready = SHARED_INPUT.replace(
        "            match task_poll_close(&mut task) {",
        "            value=43;\n            match task_poll_close(&mut task) {",
    );
    let poll = SHARED_INPUT
        .find("            match task_poll(&mut task)? {")
        .unwrap();
    let premature_destroy = format!(
        "{}            task_finish_destroy(give task);\n{}",
        &SHARED_INPUT[..poll],
        &SHARED_INPUT[poll..]
    );
    for (case, input) in [
        ("before-poll", &before_poll),
        ("after-pending", &after_pending),
        ("after-ready", &after_ready),
        ("after-cancel", &after_cancel),
        ("premature-destroy", &premature_destroy),
    ] {
        let report = compile_source(&source(input), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        if case != "premature-destroy" {
            assert_eq!(
                report.phase,
                zil_base::Phase::Analyze,
                "{case}: {}",
                report.to_json()
            );
            assert_eq!(report.status, ResultCategory::SafeReject);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("live borrowed Task frame")),
                "{case}: {}",
                report.to_json()
            );
        }
    }
}
#[test]
fn shared_task_raw_owner_keeps_input_borrow_after_last_ssa_use() {
    use zil_ir::InstructionKind as I;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SHARED_INPUT, &limits).unwrap().core;
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    zil_analyze::analyze(&typed, &limits).unwrap();
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let value = main.locals.iter().find(|l| l.name == "value").unwrap().id;
    let initial_write = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::Store { local, .. } if local == value))
        .unwrap()
        .clone();
    let poll_block = main
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::TaskPoll { .. }))
        })
        .unwrap();
    let poll_index = poll_block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::TaskPoll { .. }))
        .unwrap();
    poll_block.instructions.insert(poll_index, initial_write);
    assert!(zil_analyze::analyze(&typed, &limits).is_err());
}
#[test]
fn shared_task_raw_rejects_untracked_parameter_and_result_shapes() {
    use zil_ir::Type;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut core = expand(SHARED_INPUT, &limits).unwrap().core;
    let good = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&good, &limits).unwrap();
    let make = good.functions.iter().find(|f| f.name == "make").unwrap();
    let Type::ErrorUnion {
        result: success, ..
    } = &make.return_type
    else {
        unreachable!()
    };
    let carrier = success.as_ref().clone();
    let Type::Managed(payload) = &carrier else {
        unreachable!()
    };
    let Type::Task { retained_life, .. } = payload.as_ref() else {
        unreachable!()
    };
    assert_eq!(retained_life.ports.len(), 1);

    let worker = core
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    worker.parameters[0].ty = carrier.clone();
    let error = zil_types::check(&core, &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("retained Task parameter")),
        "{error:?}"
    );

    let mut bad = good.clone();
    let main = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let input = zil_ir::LocalId(main.locals.len() as u32);
    main.locals.push(zil_ir::LocalDecl {
        id: input,
        name: "raw_owner".into(),
        ty: Type::Desc(payload.clone()),
        mutable: false,
        span: main.span,
    });
    main.parameters.push(input);
    main.blocks[main.entry.0 as usize].instructions.insert(
        1,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::StorageDead(input),
            span: main.span,
        },
    );
    let main_id = main.id;
    let entry = main.entry;
    let error = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("retained Task parameter")),
        "{error:?}"
    );
    let mut claim = plan.clone();
    claim.module_fingerprint = bad.fingerprint();
    let plan_main = claim
        .functions
        .iter_mut()
        .find(|f| f.function == main_id)
        .unwrap();
    plan_main.blocks[entry.0 as usize]
        .initialized_in
        .push(input);
    let error = zil_verify::verify_module(bad, claim, &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("retained Task parameter")),
        "{error:?}"
    );

    for result in [
        Type::Bool,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
    ] {
        let mut bad = good.clone();
        let make = bad.functions.iter_mut().find(|f| f.name == "make").unwrap();
        let Type::ErrorUnion {
            result: success, ..
        } = &mut make.return_type
        else {
            unreachable!()
        };
        let Type::Managed(task) = success.as_mut() else {
            unreachable!()
        };
        let Type::Task {
            result: payload, ..
        } = task.as_mut()
        else {
            unreachable!()
        };
        *payload = Box::new(result);
        let error = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("borrowed Task retained classes")),
            "{error:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = bad.fingerprint();
        let error = zil_verify::verify_module(bad, claim, &target, &limits).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("borrowed Task retained classes")),
            "{error:?}"
        );
    }
}
#[test]
fn shared_task_retained_port_rejects_raw_and_wire_forgery() {
    use zil_ir::InstructionKind as I;
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SHARED_INPUT, &limits).unwrap().core;
    let good = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), plan.clone(), &target, &limits).unwrap();
    for case in 0..2 {
        let mut bad = good.clone();
        let factory = bad.functions.iter_mut().find(|f| f.name == "make").unwrap();
        let local = factory
            .locals
            .iter()
            .find(|l| l.name == "other")
            .unwrap()
            .id;
        let other = factory
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                matches!(i.kind, I::BorrowLocal { local: id } if id == local)
                    .then_some(i.result)
                    .flatten()
            })
            .unwrap();
        let create = factory
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::TaskCreate { .. }))
            .unwrap();
        let I::TaskCreate {
            arguments,
            port_bindings,
            ..
        } = &mut create.kind
        else {
            unreachable!()
        };
        if case == 0 {
            port_bindings[0].port = 1;
        } else {
            arguments[0] = other;
        }
        assert!(zil_analyze::analyze(&bad, &limits).is_err());
        let mut claim = plan.clone();
        claim.module_fingerprint = bad.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
        for encoded in [false, true] {
            let (typed, candidate) = if encoded {
                zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
            } else {
                (bad.clone(), claim.clone())
            };
            assert!(
                zil_verify::verify_module(typed, candidate, &target, &limits).is_err(),
                "case={case} wire={encoded}"
            );
        }
    }
}
#[test]
fn shared_task_raw_container_cannot_hide_frame_loan() {
    use zil_ir::{InstructionKind as I, Type};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SHARED_INPUT, &limits).unwrap().core;
    let good = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&good, &limits).unwrap();
    let mut bad = good.clone();
    let main = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let (block, offset, owner) = main
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, b)| {
            b.instructions.iter().enumerate().find_map(|(offset, i)| {
                matches!(&i.kind, I::TakeVariantField { path, .. }
                if path.as_slice() == [zil_ir::PlaceProjection::ErrorArm(false)])
                .then(|| (block, offset, i.result.unwrap()))
            })
        })
        .unwrap();
    let hidden = zil_ir::ValueId(main.values.len() as u32);
    let carrier = main.values[owner.0 as usize].clone();
    main.values.push(Type::Tuple(vec![carrier]));
    main.blocks[block].instructions.insert(
        offset + 1,
        zil_ir::Instruction {
            result: Some(hidden),
            kind: I::MakeOwnedTuple {
                fields: vec![owner],
            },
            span: main.span,
        },
    );
    let main_id = main.id;
    assert!(zil_analyze::analyze(&bad, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = bad.fingerprint();
    let claim_main = claim
        .functions
        .iter_mut()
        .find(|f| f.function == main_id)
        .unwrap();
    claim_main.holds.push(zil_ir::ValueHold {
        value: hidden,
        roots: vec![],
    });
    let wire = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
    for encoded in [false, true] {
        let (typed, candidate) = if encoded {
            zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap()
        } else {
            (bad.clone(), claim.clone())
        };
        let error = zil_verify::verify_module(typed, candidate, &target, &limits).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("nested retained Task transport")
                    || d.message.contains("retained Task aggregate")),
            "wire={encoded}: {error:?}"
        );
    }
}
#[test]
fn shared_task_port_survives_cross_module_factory_return() {
    use zil_driver::{NativeBackend, Optimization, ProjectSources, Stage};
    let split = SHARED_INPUT.find("fn main").unwrap();
    let provider = SHARED_INPUT[..split]
        .replace("extern \"C\" fn putchar(code:i32)->i32;", "")
        .replace("fn make<life a>", "pub fn make<life a>");
    let consumer = format!(
        "import provider\nextern \"C\" fn putchar(code:i32)->i32;\n{}",
        SHARED_INPUT[split..].replace("make(&value)", "provider.make(&value)")
    );
    let project = ProjectSources {
        root: zil_base::ModuleId(0),
        modules: vec![
            SourceUnit::new(
                zil_base::FileId(0),
                zil_base::ModuleId(0),
                "main.zil",
                consumer,
            ),
            SourceUnit::new(
                zil_base::FileId(1),
                zil_base::ModuleId(1),
                "provider.zil",
                provider,
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["provider".into()], zil_base::ModuleId(1))]),
    };
    let dir = std::env::temp_dir().join(format!("shared-task-cross-module-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let report = zil_driver::compile_project(
        &project,
        &CompileOptions {
            backend: NativeBackend::Clang,
            optimization: Optimization::O0,
            stop_after: Stage::Build,
            output: Some(dir.join("program")),
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
    assert_eq!(native.backend, NativeBackend::Clang);
    assert!(native.fallback.is_none(), "{native:?}");
    let output = std::process::Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"PRCA");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn general_task_native_ordinary_poll_and_once_ready() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, text, expected) in [
                ("yield", SOURCE, b"PRC".as_slice()),
                (
                    "user-poll",
                    include_str!("task-general-user-poll.zil"),
                    b"PRC".as_slice(),
                ),
                (
                    "direct",
                    include_str!("task-general-direct.zil"),
                    b"RC".as_slice(),
                ),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "task-runtime-general-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, expected);
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn general_task_raw_constructor_result_and_pc0_boundaries() {
    use zil_ir::{InstructionKind as I, Type};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SOURCE, &limits).unwrap().core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for case in 0..6 {
        let mut raw = typed.clone();
        let caller = raw
            .functions
            .iter()
            .position(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, I::TaskCreate { .. }))
            })
            .unwrap();
        match case {
            0 => {
                let i = raw.functions[caller]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                if let I::TaskCreate { arguments, .. } = &mut i.kind {
                    arguments.clear();
                }
            }
            1 => {
                let i = raw.functions[caller]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                if let I::TaskCreate { worker, .. } = &mut i.kind {
                    *worker = zil_ir::FunctionId(u32::MAX);
                }
            }
            2 => {
                let f = &mut raw.functions[caller];
                let i = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                let out = i.result.unwrap();
                if let Type::ErrorUnion { result, .. } = &mut f.values[out.0 as usize] {
                    **result = Type::Managed(Box::new(Type::Task {
                        result: Box::new(Type::Bool),
                        retained_life: closed_task_shape(),
                    }));
                }
            }
            3 => {
                let r = raw
                    .resources
                    .iter_mut()
                    .find(|r| matches!(r.payload, Type::Task { .. }))
                    .unwrap();
                r.name.push_str("forged");
            }
            4 => {
                let f = raw
                    .functions
                    .iter_mut()
                    .find(|f| {
                        f.blocks
                            .iter()
                            .flat_map(|b| &b.instructions)
                            .any(|i| matches!(i.kind, I::TaskPoll { .. }))
                    })
                    .unwrap();
                let out = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, I::TaskPoll { .. }))
                    .unwrap()
                    .result
                    .unwrap();
                if let Type::ErrorUnion { result, .. } = &mut f.values[out.0 as usize]
                    && let Type::Enum { name, .. } = result.as_mut()
                {
                    name.push_str("forged");
                }
            }
            5 => {
                let worker = raw.functions[caller]
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| {
                        if let I::TaskCreate { worker, .. } = i.kind {
                            Some(worker)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let f = &mut raw.functions[worker.0 as usize];
                f.locals[f.parameters[0].0 as usize].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                };
            }
            _ => unreachable!(),
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze case {case}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let input = if wire {
                zil_ir::wire::decode_module_candidate(
                    &zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
                .0
            } else {
                raw.clone()
            };
            let errors =
                zil_verify::verify_module(input, claim.clone(), &target, &limits).unwrap_err();
            assert_ne!(
                errors[0].code, "E-VERIFY-PLAN-IDENTITY",
                "case {case} wire={wire}: {errors:?}"
            );
            eprintln!(
                "general raw case={case} wire={wire}: {} {}",
                errors[0].code, errors[0].message
            );
            let expected = [
                "E-VERIFY-TASK",
                "E-VERIFY-TASK",
                "E-VERIFY-TASK",
                "E-VERIFY-TASK",
                "E-VERIFY-TYPE",
                "E-VERIFY-TYPE",
            ][case];
            assert_eq!(
                errors[0].code, expected,
                "case {case} wire={wire}: {errors:?}"
            );
            if case == 2 {
                assert!(
                    errors[0]
                        .message
                        .contains("exact canonical sealed native definition")
                );
            }
        }
    }
}

#[test]
fn general_task_retained_context_cannot_enter_closed_frame_profile() {
    use zil_ir::{
        LifeDeclarationId, NominalLifeArgument, RetainedBorrowPort, StorageRetainedBinding,
        StoredBorrowMode, Type,
    };
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = zil_types::check(&expand(SOURCE, &limits).unwrap().core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for case in 0..4 {
        let mut raw = typed.clone();
        let payload = &mut raw
            .resources
            .iter_mut()
            .find(|r| matches!(r.payload, Type::Task { .. }))
            .unwrap()
            .payload;
        let Type::Task { retained_life, .. } = payload else {
            unreachable!()
        };
        match case {
            0 => retained_life.binder_count = 1,
            1 => retained_life.ports.push(RetainedBorrowPort {
                leaf: Type::I32,
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }),
            2 => retained_life.retained.push(StorageRetainedBinding {
                path: vec![],
                class: 0,
                binder: 0,
            }),
            3 => retained_life.nominal_arguments.push(NominalLifeArgument {
                path: vec![],
                declaration: LifeDeclarationId {
                    module: zil_base::ModuleId(0),
                    declaration: zil_base::NodeId(0),
                    compiler: false,
                },
                argument: 0,
                binder: 0,
            }),
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert_eq!(errors[0].code, "E-UNSUPPORTED", "case {case}: {errors:?}");
        assert!(errors[0].message.contains("borrowed Task"), "{errors:?}");
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let input = if wire {
                zil_ir::wire::decode_module_candidate(
                    &zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
                .0
            } else {
                raw.clone()
            };
            let errors =
                zil_verify::verify_module(input, claim.clone(), &target, &limits).unwrap_err();
            assert_eq!(
                errors[0].code, "E-UNSUPPORTED",
                "case {case}, wire={wire}: {errors:?}"
            );
            assert!(errors[0].message.contains("borrowed Task"), "{errors:?}");
        }
    }
}

#[test]
fn general_task_borrowed_initial_frame_is_not_erased_by_scalar_result() {
    let original = include_str!("task-general-borrowed-pc0.zil");
    let report = compile_source(&source(original), &CompileOptions::default());
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-ERROR-INJECTION"),
        "{}",
        report.to_json()
    );
}
#[test]
fn general_task_native_creation_failure_and_default_cleanup() {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let observed = SOURCE.replace("putchar", "task_general_observe");
    let default = observed.replace("    task_finish_destroy(give task);", "");
    let owning =
        include_str!("task-owned-argument-order.zil").replace("putchar", "task_general_observe");
    let owning_child = include_str!("task-owned-child-inputs.zil")
        .replace("putchar", "task_general_observe")
        .replace("// POLLS", "task_poll(&mut task); task_poll(&mut task);");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "task-runtime-general-fault-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let object = dir.join("observer.o");
            let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/language-readiness/terminal/concurrency-and-registration/task-general-observer.c");
            let output = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(observer)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",quote(&clang),quote(&object),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            for (text, success_bytes, failed_bytes) in [
                (observed.as_str(), b"APRCF".as_slice(), b"A".as_slice()),
                (default.as_str(), b"APRCF".as_slice(), b"A".as_slice()),
                (owning.as_str(), b"AABF".as_slice(), b"ABA".as_slice()),
                (
                    owning_child.as_str(),
                    b"AIJBAF".as_slice(),
                    b"ABA".as_slice(),
                ),
            ] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                for fail in [false, true] {
                    let mut command = Command::new(&report.artifacts["executable"]);
                    if fail {
                        command.env("ZIL343_TASK_ALLOC_FAIL", "1");
                    }
                    let actual = command.output().unwrap();
                    assert_eq!(actual.status.code(), Some(i32::from(fail)), "{actual:?}");
                    assert_eq!(
                        actual.stdout,
                        if fail { failed_bytes } else { success_bytes },
                        "{actual:?}"
                    );
                }
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn general_task_exact_kernel_work_boundary() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let core = expand(SOURCE, &limits).unwrap().core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for verify in [false, true] {
        let accepts = |steps| {
            let bounds = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &bounds).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &bounds).map(|_| ())
            }
        };
        let (mut lo, mut hi) = (1, limits.max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if accepts(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        accepts(lo).unwrap();
        assert_eq!(
            accepts(lo - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(lo).unwrap();
        eprintln!("general Task exact kernel verify={verify} N={lo}");
    }
}

#[test]
fn general_task_user_poll_shape_does_not_replace_standard_identity() {
    let source_text = include_str!("task-general-user-poll.zil");
    let wrong = source_text.replace(
        "let outcome = task_poll(task)?;",
        "let outcome:Poll<i32> = task_poll(task)?;",
    );
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
}

#[test]
fn general_task_generated_ascribed_and_sequence_head_hints_preserve_types() {
    use zil_ir::{CoreExprKind as E, CoreStatementKind as S, Type};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for sequence in [false, true] {
        for wrong in [false, true] {
            let mut core = expand(SOURCE, &limits).unwrap().core;
            let drive = core
                .functions
                .iter_mut()
                .find(|f| f.name == "drive")
                .unwrap();
            let S::Binding {
                initializer: Some(initializer),
                ..
            } = &mut drive.body.statements[0].kind
            else {
                panic!()
            };
            let E::Try(call) = &mut initializer.kind else {
                panic!()
            };
            let E::Call { arguments, .. } = &mut call.kind else {
                panic!()
            };
            let old = arguments[0].clone();
            let ty = Type::Address {
                mutable: true,
                inner: Box::new(Type::Managed(Box::new(Type::Task {
                    result: Box::new(if wrong { Type::Bool } else { Type::I32 }),
                    retained_life: closed_task_shape(),
                }))),
            };
            arguments[0].kind = E::Ascribed {
                ty,
                value: Box::new(old),
            };
            if sequence {
                let old = arguments[0].clone();
                arguments[0].kind = E::Sequence {
                    bindings: vec![],
                    value: Box::new(old),
                };
            }
            let typed = zil_types::check(&core, &target, &limits);
            if wrong {
                assert_eq!(typed.unwrap_err()[0].code, "E-TYPE-MISMATCH");
            } else {
                let typed = typed.unwrap();
                let plan = zil_analyze::analyze(&typed, &limits).unwrap();
                zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
            }
        }
    }
    // The earlier field admission gate remains the nearest rejection for this
    // original user projection; task hint selection does not decide its fate.
    let projected = include_str!("task-general-projected-head.zil");
    let report = compile_source(&source(projected), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("aggregate field")),
        "{}",
        report.to_json()
    );
}

#[test]
fn owned_task_native_input_before_poll_pending_and_completion() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, fixture) in [
                ("input", include_str!("task-owned-input.zil")),
                ("argument", include_str!("task-owned-argument.zil")),
                ("tuple", include_str!("task-owned-ssa.zil")),
            ] {
                for polls in 0..3 {
                    let text = fixture.replace("// POLLS", &"task_poll(&mut task);".repeat(polls));
                    let dir = std::env::temp_dir().join(format!(
                        "task-runtime-owned-input-{}-{case}-{polls}-{backend:?}-{optimization:?}",
                        std::process::id()
                    ));
                    std::fs::create_dir_all(&dir).unwrap();
                    let report = compile_source(
                        &source(&text),
                        &CompileOptions {
                            backend,
                            optimization,
                            stop_after: Stage::Build,
                            output: Some(dir.join("program")),
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    let native = report.native_codegen.as_ref().unwrap();
                    assert_eq!(native.backend, backend);
                    assert!(native.fallback.is_none(), "{native:?}");
                    let actual = std::process::Command::new(&report.artifacts["executable"])
                        .output()
                        .unwrap();
                    assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                    assert_eq!(actual.stdout, b"A", "{actual:?}");
                    std::fs::remove_dir_all(dir).unwrap();
                }
            }
        }
    }
}

#[test]
fn owned_task_native_dynamic_branch_order_matches_ordinary_teardown() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (text, expected) in [
                (
                    include_str!("task-owned-lexical-trace.zil"),
                    b"BADCABDC".as_slice(),
                ),
                (include_str!("task-owned-order.zil"), b"BADCABDC".as_slice()),
                (
                    include_str!("task-owned-refill-lexical.zil"),
                    b"ABC".as_slice(),
                ),
                (
                    include_str!("task-owned-refill-cancel.zil"),
                    b"ABC".as_slice(),
                ),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "task-runtime-owned-order-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none(), "{native:?}");
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, expected, "{actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn owned_task_native_ready_result_moves_once() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "task-runtime-owned-result-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            for (fixture, expected) in [
                (include_str!("task-owned-result.zil"), b"PRA".as_slice()),
                (include_str!("managed-generic-enum.zil"), b"A".as_slice()),
                (include_str!("task-owned-direct.zil"), b"AA".as_slice()),
            ] {
                let report = compile_source(
                    &source(fixture),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none(), "{native:?}");
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, expected, "{actual:?}");
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn owned_task_prepared_temp_cancellation_matches_recoverable_failure_order() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for fixture in [
                include_str!("task-owned-argument-failure-order.zil"),
                include_str!("task-owned-argument-order.zil"),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "task-runtime-temp-order-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(fixture),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none(), "{native:?}");
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, b"AB", "{actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn owned_task_raw_moves_effects_and_result_debt_are_independent() {
    use zil_ir::{CallKind, InstructionKind as I, PassingMode};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for case in 0..4 {
        let fixture = if case == 3 {
            include_str!("task-owned-result.zil")
        } else {
            include_str!("task-owned-argument-order.zil")
        };
        let typed =
            zil_types::check(&expand(fixture, &limits).unwrap().core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let verified =
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let worker = typed
            .functions
            .iter()
            .find(|f| f.name == "worker")
            .unwrap()
            .id;
        let ownership = verified
            .task_ownership(worker)
            .expect("actual verified owning frame");
        assert!(!ownership.owners().is_empty());
        assert!(!ownership.entry().is_empty());
        assert!(!ownership.boundaries().is_empty());
        assert_eq!(
            ownership
                .owners()
                .iter()
                .map(|o| o.slot())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            ownership.owners().len()
        );
        assert_eq!(ownership.result_type().is_some(), case == 3);
        let mut raw = typed.clone();
        match case {
            0 => {
                let i = raw
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::TaskCreate { .. }))
                    .unwrap();
                if let I::TaskCreate { arguments, .. } = &mut i.kind {
                    arguments[1] = arguments[0];
                }
            }
            1 => raw.functions[worker.0 as usize].call_kind = CallKind::Direct,
            2 => {
                let drop = raw.resources.iter().find_map(|r| r.teardown).unwrap();
                raw.functions[drop.0 as usize].call_kind = CallKind::Suspend;
            }
            3 => raw.functions[worker.0 as usize].result_mode = PassingMode::Create,
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "E-TASK-ABI" || e.code == "E-ANALYZE-IR"),
            "case={case}: {errors:?}"
        );
        let mut claim = plan;
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let input = if wire {
                zil_ir::wire::decode_module_candidate(
                    &zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
                .0
            } else {
                raw.clone()
            };
            let errors =
                zil_verify::verify_module(input, claim.clone(), &target, &limits).unwrap_err();
            let expected = if case != 1 {
                "E-VERIFY-RESOURCE"
            } else {
                "E-VERIFY-TASK"
            };
            assert_eq!(
                errors[0].code, expected,
                "case={case} wire={wire}: {errors:?}"
            );
        }
    }
}

#[test]
fn owned_task_exact_kernel_ledger_covers_frame_obligations() {
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::default();
    let typed = zil_types::check(
        &expand(include_str!("task-owned-argument-order.zil"), &defaults)
            .unwrap()
            .core,
        &target,
        &defaults,
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    for verify in [false, true] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &limits).map(|_| ())
            }
        };
        let (mut lo, mut hi) = (1, defaults.max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        run(lo).unwrap();
        assert_eq!(
            run(lo - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        run(lo).unwrap();
        eprintln!("owned Task exact verify={verify} N={lo}");
    }
}

#[test]
fn owned_task_managed_generic_fields_do_not_grant_copy_or_description_ownership() {
    let base = include_str!("managed-generic-enum.zil");
    for text in [
        base.replace("Managed<Token>", "Managed<Managed<Token>>"),
        base.replace("Managed<Token>", "Managed<i32>"),
        base.replace(
            "    match value",
            "    let first=give value;let second=give value;\n    match second",
        ),
        format!("{base}\nfn no_owner(token:Token){{let owner:Managed<Token>=token;}}"),
        format!("{base}\nstruct Managed{{code:i32}}"),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.phase,
            zil_base::Phase::Codegen,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn owned_task_explicit_managed_builtin_types_keep_canonical_resource_kind() {
    let text = "package Element{manages bytes:ByteBuffer}fn take_bytes(give value:Managed<ByteBuffer>){}fn take_task(give value:Managed<Task<i32>>){task_finish_destroy(give value);}fn take_store(give value:Managed<Store<Element>>){}fn main(){}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    // The canonical wrapper does not prove synchronous cleanup of an arbitrary
    // incoming frame. Preserve the original empty-body spelling as a nearest
    // safety rejection rather than weakening the new unknown-origin consumer.
    let unknown = text.replace("{task_finish_destroy(give value);}", "{}");
    let report = compile_source(&source(&unknown), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Analyze,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-TASK-ABI"
            && d.message
                .contains("Task automatic Drop has no complete synchronous worker origin")),
        "{}",
        report.to_json()
    );
}

#[test]
fn owned_task_wide_actual_action_vectors_are_prepaid_at_codegen_boundary() {
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut growth = vec![];
    for (width, text) in [
        (8, include_str!("task-owned-wide-8.zil")),
        (16, include_str!("task-owned-wide-16.zil")),
    ] {
        let typed =
            zil_types::check(&expand(text, &defaults).unwrap().core, &target, &defaults).unwrap();
        let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
        let verified =
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &defaults).unwrap();
        let worker = typed
            .functions
            .iter()
            .find(|f| f.name == "worker")
            .unwrap()
            .id;
        let ownership = verified.task_ownership(worker).unwrap();
        assert!(
            ownership
                .actions()
                .any(|actions| actions.before().len() >= width)
        );
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            };
            let verified =
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)?;
            let lir = zil_lower::lower(&verified)?;
            zil_backend_llvm::emit_with_options(
                &verified,
                &lir,
                &zil_backend_llvm::EmitOptions {
                    entry: zil_backend_llvm::EntryPoint::Main,
                },
            )
            .map(|_| ())
        };
        let (mut lo, mut hi) = (1, defaults.max_analysis_steps);
        run(hi).unwrap();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        run(lo).unwrap();
        let errors = run(lo - 1).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:?}"
        );
        assert_eq!(
            errors[0].phase,
            zil_base::Phase::Codegen,
            "boundary must belong to actual backend work: {errors:?}"
        );
        run(lo).unwrap();
        growth.push(lo);
        eprintln!("owned Task actual wide markers width={width} Codegen N={lo}");
    }
    assert!(
        growth[1] > growth[0] + 4096,
        "actual marker growth must be paid: {growth:?}"
    );
}

#[test]
fn owned_child_native_same_instance_result_and_cancel_stages() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, fixture, expected) in [
                (
                    "result",
                    include_str!("task-owned-child.zil"),
                    [b"A".as_slice(); 6],
                ),
                (
                    "sequential",
                    include_str!("task-owned-child-sequential.zil"),
                    [b"BA".as_slice(), b"AB", b"AB", b"BA", b"BA", b"BA"],
                ),
                (
                    "inputs",
                    include_str!("task-owned-child-inputs.zil"),
                    [
                        b"BA".as_slice(),
                        b"IJBA",
                        b"IJBA",
                        b"IJBA",
                        b"IJBA",
                        b"IJBA",
                    ],
                ),
                (
                    "loop",
                    include_str!("task-owned-child-loop.zil"),
                    [b"".as_slice(), b"A", b"A", b"A", b"A", b"A"],
                ),
            ] {
                for (polls, expected) in expected.iter().enumerate() {
                    let text = fixture.replace("// POLLS", &"task_poll(&mut task);".repeat(polls));
                    let dir = std::env::temp_dir().join(format!(
                        "task-runtime-owned-child-{}-{case}-{polls}-{backend:?}-{optimization:?}",
                        std::process::id()
                    ));
                    std::fs::create_dir_all(&dir).unwrap();
                    let report = compile_source(
                        &source(&text),
                        &CompileOptions {
                            backend,
                            optimization,
                            stop_after: Stage::Build,
                            output: Some(dir.join("program")),
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    let native = report.native_codegen.as_ref().unwrap();
                    assert_eq!(native.backend, backend);
                    assert!(native.fallback.is_none());
                    let actual = std::process::Command::new(&report.artifacts["executable"])
                        .output()
                        .unwrap();
                    assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                    assert_eq!(actual.stdout, *expected, "{case}/{polls}: {actual:?}");
                    std::fs::remove_dir_all(dir).unwrap();
                }
            }
        }
    }
}

#[test]
fn owned_child_created_ready_once_and_constructor_preparation_failure() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, fixture, expected) in [
                (
                    "created",
                    include_str!("task-owned-child-created.zil"),
                    b"PPPRA".as_slice(),
                ),
                (
                    "create-args",
                    include_str!("task-owned-create-argument-failure.zil"),
                    b"AB".as_slice(),
                ),
                (
                    "direct",
                    include_str!("task-owned-child-direct.zil"),
                    b"BA".as_slice(),
                ),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "task-runtime-owned-child-created-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(fixture),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{actual:?}");
                assert_eq!(actual.stdout, expected, "{case}: {actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn owned_child_raw_actual_target_transfer_and_result_mode() {
    use zil_ir::{CallKind, InstructionKind as I, PassingMode};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for case in 0..6 {
        let fixture = if case == 4 {
            include_str!("task-owned-child.zil")
        } else {
            include_str!("task-owned-child-inputs.zil")
        };
        let typed =
            zil_types::check(&expand(fixture, &limits).unwrap().core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let child = typed
            .functions
            .iter()
            .find(|f| f.name == "child")
            .unwrap()
            .id;
        let mut raw = typed.clone();
        match case {
            0 | 1 | 3 => {
                let i = raw
                    .functions
                    .iter_mut()
                    .flat_map(|f| &mut f.blocks)
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::SuspendCall { .. }))
                    .unwrap();
                let I::SuspendCall {
                    function,
                    arguments,
                } = &mut i.kind
                else {
                    unreachable!()
                };
                if case == 0 {
                    arguments[1] = arguments[0];
                }
                if case == 1 {
                    arguments[0] = arguments[2];
                }
                if case == 3 {
                    i.kind = I::Call {
                        function: *function,
                        arguments: arguments.clone(),
                    };
                }
            }
            2 => raw.functions[child.0 as usize].call_kind = CallKind::Direct,
            4 => raw.functions[child.0 as usize].result_mode = PassingMode::Create,
            5 => {
                let parent = raw
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "parent")
                    .unwrap();
                parent.call_kind = CallKind::Direct;
            }
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "E-TASK-ABI" || e.code == "E-ANALYZE-IR"),
            "case={case}: {errors:?}"
        );
        let mut claim = plan;
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let candidate = if wire {
                zil_ir::wire::decode_module_candidate(
                    &zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap(),
                    &limits,
                )
                .unwrap()
                .0
            } else {
                raw.clone()
            };
            let errors =
                zil_verify::verify_module(candidate, claim.clone(), &target, &limits).unwrap_err();
            let expected = if case == 0 || case == 4 {
                "E-VERIFY-RESOURCE"
            } else {
                "E-VERIFY-TASK"
            };
            assert_eq!(
                errors[0].code, expected,
                "case={case} wire={wire}: {errors:?}"
            );
        }
    }
}

#[test]
fn owned_child_actual_frame_result_width_and_kernel_work_boundaries() {
    let defaults = Limits::default();
    let target = zil_base::TargetProfile::default();
    let mut growth = vec![];
    for (width, text) in [
        (8, include_str!("task-owned-child-wide-8.zil")),
        (16, include_str!("task-owned-child-wide-16.zil")),
        (0, include_str!("task-copy-error-result.zil")),
    ] {
        let typed =
            zil_types::check(&expand(text, &defaults).unwrap().core, &target, &defaults).unwrap();
        let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
        for phase in 0..3 {
            let run = |n| {
                let limits = Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                };
                if phase == 0 {
                    return zil_analyze::analyze(&typed, &limits).map(|_| ());
                }
                let verified =
                    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)?;
                if phase == 1 {
                    return Ok(());
                }
                let lir = zil_lower::lower(&verified)?;
                zil_backend_llvm::emit_with_options(
                    &verified,
                    &lir,
                    &zil_backend_llvm::EmitOptions {
                        entry: zil_backend_llvm::EntryPoint::Main,
                    },
                )
                .map(|_| ())
            };
            let (mut lo, mut hi) = (1, defaults.max_analysis_steps);
            run(hi).unwrap();
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if run(mid).is_ok() {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            run(lo).unwrap();
            let errors = run(lo - 1).unwrap_err();
            assert_eq!(
                errors[0].category,
                ResultCategory::ResourceLimit,
                "{errors:?}"
            );
            if phase == 2 {
                assert_eq!(errors[0].phase, zil_base::Phase::Codegen, "{errors:?}");
                growth.push(lo);
            }
            run(lo).unwrap();
            eprintln!("owned child width={width} phase={phase} N={lo}");
        }
    }
    assert!(growth[1] > growth[0] + 4096, "{growth:?}");
}

#[test]
fn owned_child_unit_completion_and_recoverable_argument_error() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let unit = include_str!("task-unit-child.zil");
            let mut cases = vec![
                (
                    "tuple-result".to_owned(),
                    include_str!("task-tuple-result.zil").to_owned(),
                    b"PR".as_slice(),
                ),
                (
                    "bool-result".to_owned(),
                    include_str!("task-bool-result.zil").to_owned(),
                    b"PT".as_slice(),
                ),
                (
                    "unit-ready".to_owned(),
                    include_str!("task-unit-ready.zil").to_owned(),
                    b"PR".as_slice(),
                ),
                (
                    "argument-error".to_owned(),
                    include_str!("task-child-argument-error.zil").to_owned(),
                    b"AB".as_slice(),
                ),
                (
                    "copy-error".to_owned(),
                    include_str!("task-copy-error-result.zil").to_owned(),
                    b"PPABR".as_slice(),
                ),
            ];
            for (polls, expected) in [b"BA".as_slice(), b"AB", b"AB", b"AB", b"AB", b"AB"]
                .iter()
                .enumerate()
            {
                cases.push((
                    format!("unit-{polls}"),
                    unit.replace("// POLLS", &"task_poll(&mut task);".repeat(polls)),
                    *expected,
                ));
            }
            for (case, text, expected) in cases {
                let dir = std::env::temp_dir().join(format!(
                    "task-runtime-unit-error-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{case}: {actual:?}");
                assert_eq!(actual.stdout, expected, "{case}: {actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
