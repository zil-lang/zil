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
fn threaded_role_transport_does_not_grant_a_constructor_or_thread_capability() {
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
    // The public enum/Wire transports untrusted declarations. Until actual F
    // capability and native consumers exist, source and both kernels refuse it.
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
