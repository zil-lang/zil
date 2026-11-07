//! Native no-future regression: a real thread holds userdata before trampoline
//! admission. The control invokes F only on the owner thread.
use super::*;

#[test]
#[cfg(target_os = "linux")]
fn foreign_raw_holder_is_pending_until_thread_and_queue_return() {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-raw-holder-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let observer = dir.join("host.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-c"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/foreign-thread-registration/raw-holder-host.c",
                ))
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper, format!(
                "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",
                quote(Path::new(&clang)), quote(&observer), quote(Path::new(&clang))
            )).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_source(
                &source(include_str!("raw-holder.zil")),
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("artifacts")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for failure in [
                None,
                Some("ZIL_RAW_ALLOC_FAIL"),
                Some("ZIL_RAW_INSTALL_FAIL"),
            ] {
                let mut run = Command::new(&report.artifacts["executable"]);
                if let Some(failure) = failure {
                    run.env(failure, "1");
                }
                let result = run.output().unwrap();
                assert_eq!(
                    result.status.code(),
                    Some(0),
                    "{backend:?} {optimization:?} {failure:?}: {result:?}"
                );
                let expected = if failure.is_some() {
                    "failed construction drained raw holders; environment dropped once\n"
                } else {
                    "pre-trampoline holder kept frame live; no-future before once cleanup\n"
                };
                assert_eq!(result.stdout, expected.as_bytes(), "{result:?}");
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn threaded_roles_do_not_grant_transfer_to_user_cleanup() {
    use zil_ir::ScopedCallbackRole as R;
    let original = include_str!("raw-holder.zil");
    let text = original.replace("async_callback_", "thread_async_callback_");
    let input = source(&text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    for (name, expected) in [
        ("raw_register", R::ThreadAsyncRegister),
        ("raw_request", R::ThreadAsyncRequestClose),
        ("raw_drain", R::ThreadAsyncPollDrain),
    ] {
        assert_eq!(
            core.functions
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .foreign
                .as_ref()
                .unwrap()
                .scoped_callback,
            Some(expected)
        );
    }
    // The same-thread control owns a Mark with a user FFI destructor. Thread
    // roles cannot turn that actual cleanup graph into Transfer authority.
    let report = compile_source(&input, &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_ne!(
        report.status,
        ResultCategory::InfraError,
        "{}",
        report.to_json()
    );
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(original);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut claim = zil_analyze::analyze(&typed, &limits).unwrap();
    for f in &mut typed.functions {
        if let Some(foreign) = &mut f.foreign {
            foreign.scoped_callback = match foreign.scoped_callback {
                Some(R::AsyncRegister) => Some(R::ThreadAsyncRegister),
                Some(R::AsyncRequestClose) => Some(R::ThreadAsyncRequestClose),
                Some(R::AsyncPollDrain) => Some(R::ThreadAsyncPollDrain),
                other => other,
            };
        }
    }
    claim.module_fingerprint = typed.fingerprint();
    assert!(zil_analyze::analyze(&typed, &limits).is_err());
    assert!(zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
    let (decoded, untrusted) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, typed);
    assert!(zil_verify::verify_module(decoded, untrusted, &target, &limits).is_err());
}

#[test]
fn callable_transfer_premise_survives_source_and_compile_time_reconstruction() {
    for bounds in [
        "FnMut(i32)->i32 + ClosedEnvironment + Transfer",
        "Transfer + FnMut(i32)->i32 + ClosedEnvironment",
        "ClosedEnvironment + Transfer + FnMut(i32)->i32",
    ] {
        let text = format!(
            "fn install<F:{bounds}>(f:F)->AsyncRegistration<F> contract{{result(create);}} {{unsafe{{return c_register_async(raw_register,raw_request,raw_drain,f)}}}}\n{}fn main(){{}}",
            include_str!("raw-holder.zil")
                .split("resource Mark")
                .next()
                .unwrap()
        );
        let input = source(&text);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &syntax).unwrap();
        let bound = &core
            .functions
            .iter()
            .find(|f| f.name == "install")
            .unwrap()
            .callable_parameters[0];
        assert!(bound.closed_environment && bound.transfer);
        let reconstructed = zil_ct::evaluate_source_constants(
            &core,
            &zil_base::TargetProfile::default(),
            &Limits::default(),
        )
        .unwrap();
        let after = &reconstructed
            .functions
            .iter()
            .find(|f| f.name == "install")
            .unwrap()
            .callable_parameters[0];
        assert_eq!(after, bound);
    }
    let input = source("fn bad<F:FnMut(i32)->i32 + Transfer>(f:F)->i32 {return 0}fn main(){}");
    assert!(zil_expand::expand(&input, &zil_syntax::parse(&input).into_result().unwrap()).is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn threaded_fnmut_preserves_writes_and_drains_every_raw_holder() {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for (lane, text) in [
        ("concrete", include_str!("threaded.zil")),
        ("generic", include_str!("threaded-generic.zil")),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-threaded-registration-{lane}-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                fs::create_dir_all(&dir).unwrap();
                let observer = dir.join("host.o");
                let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-c"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/foreign-thread-registration/threaded-host.c",
                ))
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
                assert!(compiled.status.success(), "{compiled:?}");
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper, format!(
                "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",
                quote(Path::new(&clang)), quote(&observer), quote(Path::new(&clang))
            )).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("artifacts")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                for failure in [
                    None,
                    Some("ZIL_THREAD_ALLOC_FAIL"),
                    Some("ZIL_THREAD_INSTALL_FAIL"),
                ] {
                    let mut run = Command::new(&report.artifacts["executable"]);
                    if let Some(failure) = failure {
                        run.env(failure, "1");
                    }
                    let result = run.output().unwrap();
                    assert_eq!(
                        result.status.code(),
                        Some(0),
                        "{backend:?} {optimization:?} {failure:?}: {result:?}"
                    );
                    let expected = if failure.is_some() {
                        "threaded construction failure drained; owned buffer freed once\n"
                    } else {
                        "thread F writes acquired; raw holder drained; cleanup reentry pending; once free\n"
                    };
                    assert_eq!(result.stdout, expected.as_bytes(), "{result:?}");
                }

                let invalid = Command::new(&report.artifacts["executable"])
                    .env("ZIL_THREAD_INVALID_DRAIN", "1")
                    .env("ZIL_THREAD_TRACE", "1")
                    .output()
                    .unwrap();
                assert_eq!(invalid.status.code(), None, "{invalid:?}");
                let trace = String::from_utf8_lossy(&invalid.stdout);
                assert!(
                    trace.contains("invalid drain; frame=1 buffer=1"),
                    "{invalid:?}"
                );
                assert!(
                    !trace.contains("buffer-free") && !trace.contains("frame-free"),
                    "{invalid:?}"
                );
                let original = text;
                let early = format!(
                    "{}async_registration_finish_destroy(give owner);return 0;}}}}",
                    original
                        .split("    if !async_registration_valid(owner)")
                        .next()
                        .unwrap()
                );
                let premature = compile_source(
                    &source(&early),
                    &CompileOptions {
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        stop_after: Stage::Build,
                        output: Some(dir.join("early")),
                        emit_dir: Some(dir.join("early-artifacts")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    premature.status,
                    ResultCategory::Success,
                    "{}",
                    premature.to_json()
                );
                let premature = Command::new(&premature.artifacts["executable"])
                    .env("ZIL_THREAD_TRACE", "1")
                    .output()
                    .unwrap();
                assert_eq!(premature.status.code(), None, "{premature:?}");
                let trace = String::from_utf8_lossy(&premature.stdout);
                assert!(
                    trace.contains("installed frame=1 buffer=1"),
                    "{premature:?}"
                );
                assert!(
                    !trace.contains("buffer-free") && !trace.contains("frame-free"),
                    "{premature:?}"
                );
                fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn threaded_callback_checks_actual_source_core_raw_and_wire_dependencies() {
    use zil_ir::{InstructionKind as I, ScopedCallbackRole as R};
    let original = include_str!("threaded.zil");
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(original);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let (decoded, proposal) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    zil_verify::verify_module(decoded, proposal, &target, &limits).unwrap();
    let generic = format!(
        "{}fn install<F:FnMut(i32)->i32 + ClosedEnvironment + Transfer>(f:F)->AsyncRegistration<F> contract{{result(create);}} {{unsafe{{return c_register_async(thread_register,thread_request,thread_drain,f)}}}}fn main(){{}}",
        original.split("fn closed").next().unwrap()
    );
    let report = compile_source(&source(&generic), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let generic_input = source(&generic.replace("thread_async_callback_", "async_callback_"));
    let syntax = zil_syntax::parse(&generic_input).into_result().unwrap();
    let mut missing = zil_expand::expand(&generic_input, &syntax).unwrap();
    for f in &mut missing.functions {
        if let Some(foreign) = &mut f.foreign {
            foreign.scoped_callback = match foreign.scoped_callback {
                Some(R::AsyncRegister) => Some(R::ThreadAsyncRegister),
                Some(R::AsyncRequestClose) => Some(R::ThreadAsyncRequestClose),
                Some(R::AsyncPollDrain) => Some(R::ThreadAsyncPollDrain),
                other => other,
            };
        }
    }
    // An explicit Transfer bit cannot fill in the absent thread-call premise,
    // including for an unused raw generic definition.
    assert!(zil_types::check(&missing, &target, &limits).is_err());
    missing
        .functions
        .iter_mut()
        .find(|f| f.name == "install")
        .unwrap()
        .callable_parameters[0]
        .transfer = false;
    assert!(zil_types::check(&missing, &target, &limits).is_err());
    let forbidden = "extern \"C\" fn forbidden_thread_dependency() contract{effects(ffi);};\n";
    let bad_entry = source(&format!(
        "{forbidden}{}",
        original.replace(
            "count+=1;",
            "count+=1;unsafe{forbidden_thread_dependency();}"
        )
    ));
    let syntax = zil_syntax::parse(&bad_entry).into_result().unwrap();
    let bad_core = zil_expand::expand(&bad_entry, &syntax).unwrap();
    assert!(zil_types::check(&bad_core, &target, &limits).is_err());
    // Transfer of this closed scalar environment is legal on the old route;
    // the premise cannot itself authorize calling that entry on another thread.
    let ordinary = format!(
        "{forbidden}{}fn main()->i32{{var count=37;let f=move(v:i32)->i32=>{{count+=1;unsafe{{forbidden_thread_dependency();}}return count+v}};var owner=install(f);async_registration_request_close(&mut owner);async_registration_poll_close(&mut owner);async_registration_finish_destroy(give owner);return 0}}",
        generic
            .replace("thread_async_callback_", "async_callback_")
            .split("fn main")
            .next()
            .unwrap()
    );
    let result = compile_source(&source(&ordinary), &CompileOptions::default());
    assert_eq!(
        result.status,
        ResultCategory::Success,
        "{}",
        result.to_json()
    );
    for text in [
        original.replace(" + ClosedEnvironment", ""),
        original.replace("FnMut(i32)", "FnOnce(i32)"),
        original.replace("let callback=move", "let callback="),
        format!(
            "{forbidden}{}",
            original.replace(
                "count+=1;",
                "count+=1;unsafe{forbidden_thread_dependency();}"
            )
        ),
        original.replace(
            "thread_async_callback_request_close",
            "async_callback_request_close",
        ),
        original.replace(
            "thread_async_callback_poll_drain",
            "async_callback_poll_drain",
        ),
    ] {
        let result = compile_source(&source(&text), &CompileOptions::default());
        assert_ne!(
            result.status,
            ResultCategory::Success,
            "{}",
            result.to_json()
        );
        assert_ne!(
            result.status,
            ResultCategory::InfraError,
            "{}",
            result.to_json()
        );
    }
    let buffer = typed
        .resources
        .iter()
        .position(|r| r.payload == zil_ir::Type::ByteBuffer)
        .unwrap();
    for variant in 0..4 {
        let mut raw = typed.clone();
        match variant {
            0 => {
                raw.resources[buffer].teardown =
                    Some(raw.functions.iter().find(|f| f.name == "main").unwrap().id)
            }
            1 => raw.resources[buffer].name = "same_layout_but_not_sealed".into(),
            2 => {
                raw.functions
                    .iter_mut()
                    .find(|f| f.name == "thread_request")
                    .unwrap()
                    .foreign
                    .as_mut()
                    .unwrap()
                    .scoped_callback = Some(R::AsyncRequestClose)
            }
            3 => {
                let function = raw
                    .functions
                    .iter_mut()
                    .find(|f| {
                        f.blocks
                            .iter()
                            .flat_map(|b| &b.instructions)
                            .any(|i| matches!(i.kind, I::WriteCapture { .. }))
                    })
                    .unwrap();
                function.foreign = Some(zil_ir::ForeignDeclaration {
                    abi: "C".into(),
                    symbol: function.name.clone(),
                    noescape_parameters: vec![],
                    static_callbacks: false,
                    scoped_callback: None,
                });
            }
            _ => unreachable!(),
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "raw {variant}"
        );
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "raw {variant}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let (decoded, proposal) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        assert!(
            zil_verify::verify_module(decoded, proposal, &target, &limits).is_err(),
            "wire {variant}"
        );
    }
}

#[test]
fn threaded_callback_kernel_budget_debits_are_finite_and_not_renewed() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let input = source(include_str!("threaded.zil"));
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for verify in [false, true] {
        let accepts = |steps| {
            let selected = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &selected)
                    .map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &selected).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, limits.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            if accepts(middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        accepts(low).unwrap();
        assert!(low > 0);
        assert_eq!(
            accepts(low - 1).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        accepts(low).unwrap();
        eprintln!("threaded registration exact kernel verify={verify} N={low}");
    }
}

#[test]
fn thread_callable_is_an_independent_checked_premise_in_source_core_and_ct() {
    let original = include_str!("threaded-generic.zil");
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    for bounds in [
        "FnMut(i32)->i32 + ClosedEnvironment + Transfer + ThreadCallable",
        "ThreadCallable + Transfer + ClosedEnvironment + FnMut(i32)->i32",
        "ClosedEnvironment + ThreadCallable + FnMut(i32)->i32 + Transfer",
    ] {
        let text = original.replace(
            "FnMut(i32)->i32 + ClosedEnvironment + Transfer + ThreadCallable",
            bounds,
        );
        let input = source(&text);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &syntax).unwrap();
        let bound = &core
            .functions
            .iter()
            .find(|f| f.name == "install")
            .unwrap()
            .callable_parameters[0];
        assert!(bound.closed_environment && bound.transfer && bound.thread_callable);
        let reconstructed = zil_ct::evaluate_source_constants(&core, &target, &limits).unwrap();
        let after = &reconstructed
            .functions
            .iter()
            .find(|f| f.name == "install")
            .unwrap()
            .callable_parameters[0];
        assert_eq!(after, bound);
        let typed = zil_types::check(&reconstructed, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
        for missing in 0..4 {
            let mut raw = reconstructed.clone();
            let bound = &mut raw
                .functions
                .iter_mut()
                .find(|f| f.name == "install")
                .unwrap()
                .callable_parameters[0];
            match missing {
                0 => bound.closed_environment = false,
                1 => bound.transfer = false,
                2 => bound.thread_callable = false,
                3 => bound.kind = zil_ir::CallableKind::FnOnce,
                _ => unreachable!(),
            }
            assert!(
                zil_types::check(&raw, &target, &limits).is_err(),
                "missing {missing}"
            );
            let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
            main.body.statements.clear();
            main.return_type = zil_ir::Type::Unit;
            assert!(
                zil_types::check(&raw, &target, &limits).is_err(),
                "unused missing {missing}"
            );
        }
    }
    for text in [
        original.replace(" + ThreadCallable", ""),
        original.replace(" + Transfer", ""),
        original.replace(" + ClosedEnvironment", ""),
        original.replace(" + ThreadCallable", " + Share"),
        format!("trait ThreadCallable {{}}\n{original}"),
        format!("struct Fake{{value:i32}}impl ThreadCallable for Fake{{}}\n{original}"),
        format!(
            "extern \"C\" fn forbidden_entry() contract{{effects(ffi);}};\n{}",
            original.replace("count+=1;", "count+=1;unsafe{forbidden_entry();}")
        ),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "{}",
            report.to_json()
        );
    }
}
