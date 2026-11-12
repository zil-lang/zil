//! Whole Source, raw candidate, decoded candidate and real machine consumers.
use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
fn accepted(name: &str, text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified = zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits);
    if verified.is_err() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/runs/receiver");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{name}-typed.txt")),
            format!("{module:#?}"),
        )
        .unwrap();
    }
    verified.unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-receiver-{}-{name}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &source,
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
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}
macro_rules! consumer {
    ($test:ident, $source:literal) => {
        #[test]
        fn $test() {
            accepted(
                $source,
                include_str!(concat!(
                    "../../../tests/fixtures/returned-owning-holders/",
                    $source,
                    ".zil"
                )),
            );
        }
    };
}
consumer!(
    captured_mutable_holder_updates_original_storage,
    "captured-mutable-holder"
);
consumer!(
    captured_mutable_holder_survives_borrowed_helpers,
    "captured-mutable-holder-helper"
);
consumer!(
    same_entry_receivers_keep_distinct_holders,
    "captured-mutable-holder-two-receivers"
);
consumer!(
    readonly_exclusive_capture_reborrows_without_copying_environment,
    "exclusive-capture-readonly-fn"
);
consumer!(
    call_before_return_keeps_actual_holder_update,
    "captured-mutable-holder-call-before-return"
);

mod source_receiver_contracts {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
    use zil_ir::{StorageBase, StorageBinder};

    fn typed(text: &str) -> zil_ir::TypedModule {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "receiver-borrow.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let core = zil_expand::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        zil_types::check(&core, &target, &limits).unwrap()
    }

    #[test]
    fn exclusive_capture_shared_view_uses_the_actual_receiver_head() {
        let text = include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view.zil"
        );
        let module = typed(text);
        let definition = &module.closures[0];
        let life = definition.storage_life.as_ref().unwrap();
        let receiver = life
            .bindings
            .iter()
            .find(|r| r.position.base == StorageBase::Receiver)
            .unwrap();
        assert!(receiver.position.path.is_empty());
        assert!(matches!(receiver.binder, StorageBinder::Invocation(_)));
        assert_eq!(
            life.bindings
                .iter()
                .find(|r| r.position.base == StorageBase::Result)
                .unwrap()
                .binder,
            receiver.binder
        );
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
        for change in 0..2 {
            let mut bad = module.clone();
            let life = bad.closures[0].storage_life.as_mut().unwrap();
            if change == 0 {
                life.bindings
                    .iter_mut()
                    .find(|r| r.position.base == StorageBase::Result)
                    .unwrap()
                    .binder = StorageBinder::Fixed(0);
            } else {
                life.bindings
                    .retain(|r| r.position.base != StorageBase::Receiver);
            }
            bad.functions[definition.function.0 as usize].storage_life = Some(life.clone());
            assert!(
                zil_analyze::analyze(&bad, &Limits::default()).is_err(),
                "a short returned head cannot become Fixed or lose its receiver input"
            );
        }
        let bad = text.replace(
            "let second=*view",
            "let another=f();let second=*view+*another",
        );
        assert!(
            zil_analyze::analyze(&typed(&bad), &Limits::default()).is_err(),
            "a live receiver view prevents another exclusive call"
        );
    }

    #[test]
    fn consuming_exclusive_capture_uses_a_real_take() {
        let text = include_str!(
            "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-capture-take.zil"
        );
        let module = typed(text);
        let definition = &module.closures[0];
        let entry = &module.functions[definition.function.0 as usize];
        assert!(
            entry
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::TakeCapture { .. }))
        );
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let mut bad = module.clone();
        for instruction in bad.functions[definition.function.0 as usize]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
        {
            if let zil_ir::InstructionKind::TakeCapture { index } = instruction.kind {
                instruction.kind = zil_ir::InstructionKind::ReadCapture { index };
            }
        }
        assert!(
            zil_analyze::analyze(&bad, &Limits::default()).is_err(),
            "ReadCapture cannot transfer the original Fixed exclusive capability"
        );
    }
}

consumer!(
    exclusive_capture_shared_view_keeps_the_live_receiver,
    "captured-exclusive-short-view"
);

consumer!(shared_external_capture_outlives_receiver_cleanup, "shared-capture-fixed-result");
consumer!(shared_pointee_slot_copy_outlives_receiver_cleanup, "captured-holder-fixed-data-result");
consumer!(short_receiver_view_crosses_two_helpers, "captured-exclusive-short-view-helper");
consumer!(inferred_consuming_capture_transfers_the_actual_exclusive_pointer,
    "fnonce-exclusive-capture-take-inferred-result");
consumer!(consuming_capture_publishes_actual_original_holder,
    "fnonce-captured-holder-publishes-taken-capability");
