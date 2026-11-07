//! Source-to-native dyn consumers and source-derived independent raw/Wire controls.
use super::*;
use std::io::Write;
use std::process::Stdio;
use zil_base::TargetProfile;
use zil_driver::{CodegenOptions, CompilationSession, DiskCacheStatus, NativeBackend};
use zil_ir::{InstructionKind as I, Type};

const TWO: &str = include_str!("dyn-two-provider.zil");
const INPUT: &str = include_str!("dyn-input.zil");
const WIDE_ARGUMENT: &str = include_str!("dyn-wide-argument.zil");
const UNIT_RESULT: &str = include_str!("dyn-unit-result.zil");
const MUTABLE_RECEIVER: &str = include_str!("dyn-mutable-receiver.zil");
const MUTABLE_DEFAULT: &str = include_str!("dyn-mutable-default.zil");
const OWNED_PACKAGE: &str = include_str!("dyn-owned-package.zil");
const OWNED_MUTABLE: &str = include_str!("dyn-owned-mutable.zil");
const OWNED_ALLOCATION: &str = include_str!("dyn-owned-allocation.zil");
const OWNED_ALLOCATION_LOOP: &str = include_str!("dyn-owned-allocation-loop.zil");
const MUTABLE_PREFIX: &str = "trait Change{fn set(self:&mut Self,value:i32)->();}struct A{n:i32}impl Change for A{fn set(self:&mut Self,value:i32)->(){self.n=value;return ()}}";

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "dynamic-dispatch-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(text: &str, options: &CompileOptions) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "dynamic-dispatch.zil", text),
        options,
    )
}

fn backends() -> Vec<NativeBackend> {
    let mut result = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        result.push(NativeBackend::LlvmApi);
    }
    result
}

#[test]
fn owned_package_selects_concrete_cleanup_and_survives_return() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("owned-package");
            let executable = dir.join("program");
            let report = compile(
                OWNED_PACKAGE,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
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
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let result = Command::new(&executable).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert_eq!(result.stdout, b"30\n");
            assert!(result.stderr.is_empty(), "{result:?}");
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            assert_eq!(
                llvm.matches("define internal void @zil.dyn.payload.drop.")
                    .count(),
                2
            );
            assert!(llvm.contains("call void @zil.bytes.drop"));
            assert!(
                llvm.split("define internal void @zil.dyn.payload.drop.")
                    .skip(1)
                    .any(|body| body
                        .split("ret void")
                        .next()
                        .unwrap()
                        .matches("call void @zil.bytes.drop")
                        .count()
                        == 3)
            );
            assert!(llvm.contains("call void @free(ptr %payload)"));
        }
    }
}

#[test]
fn owned_package_crosses_modules_with_each_provider_cleanup() {
    let files = [
        ("main", include_str!("project/owned-main.zil")),
        ("a", include_str!("project/owned-a.zil")),
        ("b", include_str!("project/owned-b.zil")),
    ];
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("owned-package-project");
            let executable = dir.join("program");
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                entry: Some("main".into()),
                output: Some(executable.clone()),
                cache_dir: Some(dir.join("cache")),
                codegen: if backend == NativeBackend::LlvmApi {
                    CodegenOptions { units: 8, jobs: 4 }
                } else {
                    CodegenOptions::default()
                },
                ..CompileOptions::default()
            };
            for warm in [false, true] {
                let report = compile_project(&project(&files), &options);
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                if warm {
                    assert_eq!(report.disk_cache, DiskCacheStatus::Hit);
                }
                let native = report.native_codegen.as_ref().unwrap();
                assert!(native.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert_eq!(native.requested_units, 8);
                }
                let result = Command::new(&executable).output().unwrap();
                assert_eq!(result.status.code(), Some(0), "{result:?}");
                assert_eq!(result.stdout, b"30\n");
            }
        }
    }
}

#[test]
fn owned_dyn_exclusive_unit_method_updates_each_real_receiver() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("owned-mutable");
            let executable = dir.join("program");
            let report = compile(
                OWNED_MUTABLE,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
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
            let result = Command::new(&executable).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert_eq!(result.stdout, b"16\n");
        }
    }
    for rejected in [
        OWNED_MUTABLE.replace("var owned=select(which)?", "let owned=select(which)?"),
        OWNED_MUTABLE.replace(
            "owned.set(value);",
            "let shared:&dyn Change=&owned;shared.set(value);",
        ),
    ] {
        let report = compile(&rejected, &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn owned_dyn_allocation_failure_cleans_transferred_payload_once() {
    let source = r#"
#include <stddef.h>
#include <unistd.h>
extern void *__libc_malloc(size_t);
extern void *__libc_realloc(void *, size_t);
extern void __libc_free(void *);
static void *owned_buffer;
static int failed;
void *realloc(void *p, size_t n) {
    void *result = __libc_realloc(p, n);
    if (!p && n == 64) owned_buffer = result;
    return result;
}

void *malloc(size_t n) {
    if (owned_buffer && n == 32 && !failed) {
        failed = 1;
        write(2, "F", 1);
        return 0;
    }
    return __libc_malloc(n);
}
void free(void *p) {
    if (failed && p == owned_buffer) {
        write(2, "D", 1);
        owned_buffer = 0;
    }
    __libc_free(p);
}
"#;
    let dir = directory("owned-allocation");
    let source_path = dir.join("fail-allocator.c");
    let interceptor = dir.join("fail-allocator.so");
    fs::write(&source_path, source).unwrap();
    let clang = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".into());
    let compiled = Command::new(clang)
        .args(["-shared", "-fPIC", "-O0"])
        .arg(&source_path)
        .arg("-o")
        .arg(&interceptor)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let executable = dir.join(format!("program-{}-{optimization:?}", backend.name()));
            let report = compile(
                OWNED_ALLOCATION,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
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
            let success = Command::new(&executable).output().unwrap();
            assert_eq!(success.status.code(), Some(0), "{success:?}");
            assert_eq!(success.stdout, b"41\n");
            let failed = Command::new(&executable)
                .env("LD_PRELOAD", &interceptor)
                .output()
                .unwrap();
            assert_eq!(failed.status.code(), Some(0), "{failed:?}");
            assert_eq!(failed.stdout, b"77\n");
            assert_eq!(failed.stderr, b"FD");
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn repeated_owned_allocation_failure_uses_one_entry_scratch() {
    let shim = r#"
#include <stddef.h>
#include <unistd.h>
extern void *__libc_malloc(size_t);
extern void *__libc_realloc(void *, size_t);
extern void __libc_free(void *);
static void *buffer;
static int failed, freed;
void *realloc(void *p, size_t n) {
    void *r = __libc_realloc(p, n);
    if (!p && n == 64) buffer = r;
    return r;
}

void *malloc(size_t n) {
    if (buffer && n == 32) { failed++; return 0; }
    return __libc_malloc(n);
}
void free(void *p) {
    if (p == buffer) { freed++; buffer = 0; }
    __libc_free(p);
}
__attribute__((destructor)) static void result(void) {
    write(2, failed == 400000 && freed == 400000 ? "P" : "F", 1);
}
"#;
    let dir = directory("owned-loop-failure");
    let source = dir.join("fail.c");
    let interceptor = dir.join("fail.so");
    fs::write(&source, shim).unwrap();
    let clang = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".into());
    let compiled = Command::new(clang)
        .args(["-shared", "-fPIC", "-O0"])
        .arg(&source)
        .arg("-o")
        .arg(&interceptor)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    for backend in backends() {
        let executable = dir.join(format!("program-{}", backend.name()));
        let report = compile(
            OWNED_ALLOCATION_LOOP,
            &CompileOptions {
                backend,
                optimization: Optimization::O0,
                stop_after: Stage::Build,
                entry: Some("main".into()),
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
        let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
        assert!(llvm.contains("dyn.owner.scratch."));
        for block in llvm.split(".failure:").skip(1) {
            assert!(!block.split(".done:").next().unwrap().contains("alloca"));
        }
        let output = Command::new(&executable)
            .env("LD_PRELOAD", &interceptor)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"400000\n");
        assert_eq!(output.stderr, b"P");
    }
}
#[test]
fn owned_erasure_rejects_live_concrete_byte_views() {
    let prefix = "trait Read{fn read(self:&Self)->i32;}package P{manages data:ByteBuffer;pub n:i32}impl Read for P{fn read(self:&Self)->i32{return self.n}}";
    for (helper, view) in [
        ("", "bytes_view(p.data)"),
        (
            "fn view(p:&P)->&[u8]{return bytes_view(p.data)}",
            "view(&p)",
        ),
    ] {
        let source = format!(
            "{prefix}{helper}fn main()->DynCreateError!i32{{let p=P{{data:bytes_new(),n:1}};let before={view};let owner=owned_dyn::<dyn Read>(p)?;return i32(before.len())+owner.read()}}"
        );
        let report = compile(&source, &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Analyze | Phase::Verify),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn interface_only_owner_parameter_still_has_affine_debt() {
    let source = format!(
        "{OWNED_PACKAGE}\nfn relay(owner:OwnedDyn<dyn Read>)->OwnedDyn<dyn Read>{{return owner}}"
    );
    let (mut module, _) = source_candidate_for(&source);
    let mut relay = module
        .functions
        .iter()
        .find(|f| f.name.ends_with("relay"))
        .unwrap()
        .clone();
    relay.id = zil_ir::FunctionId(0);
    module.functions = vec![relay];
    module.resources.clear();
    module.dyn_vtables.clear();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verified = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let lir = zil_lower::lower_view(&verified).unwrap().to_owned();
    let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(llvm.contains("define internal void @zil.dyn.owner.drop."));
    assert!(llvm.contains("call void @free(ptr %payload)"));
    assert!(module.resources.is_empty());
    assert!(module.dyn_vtables.is_empty());
    let check = |candidate: zil_ir::TypedModule| {
        assert!(zil_analyze::analyze(&candidate, &Limits::default()).is_err());
        let mut plan = plan.clone();
        plan.module_fingerprint = candidate.fingerprint();
        let target = candidate.target.clone();
        let wire =
            zil_ir::wire::encode_module_candidate(&candidate, &plan, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module(candidate.clone(), plan, &target, &Limits::default())
                .is_err()
        );
        assert!(zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).is_err());
    };
    let mut doubled = module.clone();
    let f = &mut doubled.functions[0];
    let take = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::TakeLocal { .. }))
        .unwrap()
        .clone();
    let mut second = take.clone();
    second.result = Some(zil_ir::ValueId(f.values.len() as u32));
    f.values.push(Type::OwnedDyn {
        interface: module.dyn_interfaces[0].id,
    });
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::TakeLocal { .. }))
        })
        .unwrap();
    let first = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::TakeLocal { .. }))
        .unwrap();
    block.instructions.insert(first + 1, second);
    check(doubled);

    let (mut without_table, _) = source_candidate_for(&source);
    let mut consume = without_table
        .functions
        .iter()
        .find(|f| f.name.ends_with("consume"))
        .unwrap()
        .clone();
    consume.id = zil_ir::FunctionId(0);
    without_table.functions = vec![consume];
    without_table.resources.clear();
    without_table.dyn_vtables.clear();
    let mut consume_plan = zil_analyze::analyze(&without_table, &Limits::default()).unwrap();
    zil_verify::verify_module(
        without_table.clone(),
        consume_plan.clone(),
        &without_table.target,
        &Limits::default(),
    )
    .unwrap();
    let missing_drop = without_table.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::DropDynOwned { .. }))
        .unwrap();
    missing_drop.kind = I::StorageDead(zil_ir::LocalId(0));
    assert!(zil_analyze::analyze(&without_table, &Limits::default()).is_err());
    consume_plan.module_fingerprint = without_table.fingerprint();
    let wire =
        zil_ir::wire::encode_module_candidate(&without_table, &consume_plan, &Limits::default())
            .unwrap();
    assert!(
        zil_verify::verify_module(
            without_table.clone(),
            consume_plan,
            &without_table.target,
            &Limits::default()
        )
        .is_err()
    );
    assert!(
        zil_verify::verify_module_bytes(&wire, &without_table.target, &Limits::default()).is_err()
    );
}
#[test]
fn runtime_input_selects_two_real_vtables_in_both_backends() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("input");
            let executable = dir.join("program");
            let report = compile(
                INPUT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 2);
            assert!(typed.contains("CallDynSlot {"));
            for (input, expected) in [(b"1\n".as_slice(), b"20\n".as_slice()), (b"0\n", b"22\n")] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0));
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

#[test]
fn indirect_scalar_argument_preserves_full_u64_result_in_both_backends() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("wide-argument");
            let executable = dir.join("program");
            let report = compile(
                WIDE_ARGUMENT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 2);
            assert!(typed.contains("CallDynSlot {") && typed.contains("arguments: ["));
            for (input, expected) in [
                (b"1\n".as_slice(), b"18446744073709551615\n".as_slice()),
                (b"0\n", b"42\n"),
            ] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0), "{result:?}");
                assert_eq!(result.stdout, expected, "{result:?}");
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

#[test]
fn unit_result_slot_calls_real_provider_body() {
    let report = compile(UNIT_RESULT, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn mutable_slot_updates_actual_runtime_selected_receiver() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("mutable-receiver");
            let executable = dir.join("program");
            let report = compile(
                MUTABLE_RECEIVER,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
            assert_eq!(typed.matches("MakeDynBorrow {").count(), 4);
            assert!(typed.contains("mutable: true") && typed.contains("CallDynSlot {"));
            for (input, expected) in [(b"1\n".as_slice(), b"82\n".as_slice()), (b"0\n", b"22\n")] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0), "{result:?}");
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

#[test]
fn mutable_default_wrapper_calls_actual_provider_once() {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("mutable-default");
            let executable = dir.join("program");
            let report = compile(
                MUTABLE_DEFAULT,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            for (input, expected) in [(b"1\n".as_slice(), b"22\n".as_slice()), (b"0\n", b"8\n")] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let result = child.wait_with_output().unwrap();
                assert_eq!(result.status.code(), Some(0), "{result:?}");
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty(), "{result:?}");
            }
        }
    }
}

fn project_files(main: &'static str) -> [(&'static str, &'static str); 3] {
    [
        ("main", main),
        ("a", include_str!("project/a.zil")),
        ("b", include_str!("project/b.zil")),
    ]
}

#[test]
fn cross_module_mutable_default_and_value_providers_keep_actual_receiver() {
    let files = [
        ("main", include_str!("project/mutable-main.zil")),
        ("a", include_str!("project/mutable-a.zil")),
        ("b", include_str!("project/mutable-b.zil")),
    ];
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("mutable-default-project");
            let executable = dir.join("program");
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                entry: Some("main".into()),
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                cache_dir: Some(dir.join("cache")),
                object_cache: (backend == NativeBackend::LlvmApi).then(|| dir.join("objects")),
                ..CompileOptions::default()
            };
            for warm in [false, true] {
                let report = compile_project(&project(&files), &options);
                fs::write(
                    dir.join(if warm { "warm.json" } else { "cold.json" }),
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
                if warm {
                    assert_eq!(report.disk_cache, DiskCacheStatus::Hit);
                    if backend == NativeBackend::LlvmApi {
                        let native = report.native_codegen.as_ref().unwrap();
                        assert_eq!(native.object_cache.hits, native.actual_units);
                        assert_eq!(native.object_cache.misses, 0);
                    }
                }
                for (input, expected) in
                    [(b"1\n".as_slice(), b"82\n".as_slice()), (b"0\n", b"22\n")]
                {
                    let mut child = Command::new(&executable)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .unwrap();
                    child.stdin.take().unwrap().write_all(input).unwrap();
                    let result = child.wait_with_output().unwrap();
                    assert_eq!(result.status.code(), Some(0), "{result:?}");
                    assert_eq!(result.stdout, expected);
                    assert!(result.stderr.is_empty(), "{result:?}");
                }
            }
        }
    }
}

#[test]
fn cross_module_default_provider_and_nominal_interface_separation() {
    let files = project_files(include_str!("project/main.zil"));
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory("project");
            let executable = dir.join("program");
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                entry: Some("main".into()),
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                cache_dir: Some(dir.join("parsed-cache")),
                ..CompileOptions::default()
            };
            for warm in [false, true] {
                let report = compile_project(&project(&files), &options);
                fs::write(
                    dir.join(if warm { "warm.json" } else { "cold.json" }),
                    report.to_json(),
                )
                .unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1, "{}", report.to_json());
                if warm {
                    assert_eq!(
                        report.disk_cache,
                        DiskCacheStatus::Hit,
                        "{}",
                        report.to_json()
                    );
                }
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let result = Command::new(&executable).output().unwrap();
                assert_eq!(result.status.code(), Some(0));
                assert_eq!(result.stdout, b"29\n");
                assert!(result.stderr.is_empty());
            }
        }
    }
    let wrong = project_files(include_str!("project/wrong-interface.zil"));
    let report = compile_project(&project(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Expand | Phase::Types),
        "{}",
        report.to_json()
    );
}

#[test]
fn explicit_local_coercion_and_live_receiver_conflicts() {
    let explicit = TWO.replace(
        "return read_it(&a)+read_it(&b)",
        "let d:&dyn Read=&a;return read_it(d)+read_it(&b)",
    );
    let report = compile(&explicit, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let conflicting = TWO.replace(
        "fn main()->i32{let a=A{n:20};let b=B{n:22};return read_it(&a)+read_it(&b)}",
        "fn main()->i32{var a=A{n:20};let d:&dyn Read=&a;a=A{n:21};return read_it(d)}",
    );
    assert_ne!(conflicting, TWO);
    let report = compile(&conflicting, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}

fn source_candidate_for(text: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    // This fixture has no CT expressions. Use the same closed-project identity
    // and program expansion entry as Driver, so the original trait/impl origins
    // survive specialization instead of manufacturing a raw Core inventory.
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "dynamic-dispatch.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let aliases = BTreeMap::new();
    let expanded = zil_expand::expand_program_with_limits_target_context(
        std::slice::from_ref(&unit),
        &[&syntax],
        &[Vec::new()],
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let core = zil_ir::CoreProgram {
        root: unit.module_id,
        source_fingerprint: unit.fingerprint(),
        modules: expanded.into_iter().map(|(module, _)| module).collect(),
    };
    let typed = zil_types::check_program(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze_program(&typed, &limits).unwrap();
    let module = typed.module;
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn source_candidate() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    source_candidate_for(INPUT)
}

#[test]
fn owned_raw_wire_rejects_forged_route_provider_and_loan() {
    let (module, plan) = source_candidate_for(OWNED_PACKAGE);
    let target = module.target.clone();
    let assert_rejected = |candidate: zil_ir::TypedModule| {
        let mut plan = plan.clone();
        plan.module_fingerprint = candidate.fingerprint();
        let wire =
            zil_ir::wire::encode_module_candidate(&candidate, &plan, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module(candidate.clone(), plan, &target, &Limits::default())
                .is_err()
        );
        assert!(zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).is_err());
    };
    let mut no_route = module.clone();
    no_route.dyn_vtables[0].owned_storage = None;
    assert_rejected(no_route);
    let mut duplicate_source = module.clone();
    let function = duplicate_source
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, I::MakeDynOwned { .. }))
        })
        .unwrap();
    let original = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::MakeDynOwned { .. }))
        .unwrap()
        .clone();
    let mut repeated = original.clone();
    let original_result = original.result.unwrap();
    repeated.result = Some(zil_ir::ValueId(function.values.len() as u32));
    function
        .values
        .push(function.values[original_result.0 as usize].clone());
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::MakeDynOwned { .. }))
        })
        .unwrap();
    let index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::MakeDynOwned { .. }))
        .unwrap();
    block.instructions.insert(index + 1, repeated);
    assert!(zil_analyze::analyze(&duplicate_source, &Limits::default()).is_err());
    assert_rejected(duplicate_source);
    let mut wrong_body = module.clone();
    wrong_body.dyn_vtables[0].entries[0].function = wrong_body.dyn_vtables[1].entries[0].function;
    assert_rejected(wrong_body);
    let mut wrong_actual = module.clone();
    wrong_actual.dyn_vtables[0].self_type = wrong_actual.dyn_vtables[1].self_type.clone();
    assert_rejected(wrong_actual);
    let mut wrong_loan = module;
    let instruction = wrong_loan
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::BorrowDynOwned { .. }))
        .unwrap();
    if let I::BorrowDynOwned { mutable, .. } = &mut instruction.kind {
        *mutable = true;
    }
    assert_rejected(wrong_loan);

    let (mut moved_while_borrowed, mut plan) = source_candidate_for(OWNED_MUTABLE);
    let block = moved_while_borrowed
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::BorrowDynOwned { .. }))
                && b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, I::DropDynOwned { .. }))
        })
        .unwrap();
    let borrowed = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::BorrowDynOwned { .. }))
        .unwrap();
    let drop = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::DropDynOwned { .. }))
        .unwrap();
    let cleanup = block.instructions.remove(drop);
    block.instructions.insert(borrowed + 1, cleanup);
    plan.module_fingerprint = moved_while_borrowed.fingerprint();
    let target = moved_while_borrowed.target.clone();
    let wire =
        zil_ir::wire::encode_module_candidate(&moved_while_borrowed, &plan, &Limits::default())
            .unwrap();
    assert!(
        zil_verify::verify_module(moved_while_borrowed, plan, &target, &Limits::default()).is_err()
    );
    assert!(zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).is_err());
}

#[test]
fn owned_raw_wire_rejects_user_drop_hidden_behind_erasure() {
    let source = format!(
        "{OWNED_PACKAGE}\nextern \"C\" fn putchar(x:i32)->i32;resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{unsafe{{putchar(.code);}}}}}}\n"
    );
    let (mut module, mut plan) = source_candidate_for(&source);
    let token = module
        .resources
        .iter()
        .find(|r| r.name.ends_with("Token"))
        .unwrap();
    assert!(token.teardown.is_some());
    let token_type = token.payload.clone();
    let Type::Package { fields, .. } = &mut module.dyn_vtables[0].self_type else {
        panic!("owning table Self")
    };
    fields[0].1 = Type::Managed(Box::new(token_type));
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-DYN"),
            "{errors:?}"
        );
    }
    let source = source
        .replace("manages data:ByteBuffer", "manages data:Token")
        .replace("bytes_new()", "Token{code:65}");
    let report = compile(&source, &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

fn rejected(
    mut module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    mutate: impl FnOnce(&mut zil_ir::TypedModule),
) {
    mutate(&mut module);
    plan.module_fingerprint = module.fingerprint();
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    let target = module.target.clone();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.phase == Phase::Verify && e.code == "E-VERIFY-DYN"),
            "{errors:?}"
        );
    }
}

#[test]
fn source_derived_raw_wire_contract_identity_and_slot_controls() {
    let (module, plan) = source_candidate();
    let target = TargetProfile::host_llvm();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).unwrap();
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_interfaces[0].slots[0].contract.effects = None
    });
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_vtables[0].self_type = m.dyn_vtables[1].self_type.clone()
    });
    rejected(module.clone(), plan.clone(), |m| {
        let instruction = m
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::CallDynSlot { .. }))
            .unwrap();
        if let I::CallDynSlot { slot, .. } = &mut instruction.kind {
            *slot = u32::MAX;
        }
    });
    // The raw table is a declaration of its selected target, not a certificate
    // that an arbitrary raw program still implements the original Source.
    let mut other = module.clone();
    let spare = other
        .functions
        .iter()
        .find(|f| f.name.ends_with("spare"))
        .unwrap()
        .id;
    assert!(
        other
            .dyn_vtables
            .iter()
            .all(|t| t.entries[0].function != spare)
    );
    let table = other
        .dyn_vtables
        .iter_mut()
        .find(|t| matches!(&t.self_type,Type::Record{ name,.. } if name.ends_with('A')))
        .unwrap();
    table.entries[0].function = spare;
    let mut other_plan = plan;
    other_plan.module_fingerprint = other.fingerprint();
    zil_analyze::analyze(&other, &Limits::default()).unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&other, &other_plan, &Limits::default()).unwrap();
    zil_verify::verify_module(other, other_plan, &target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).unwrap();
}

#[test]
fn mutable_provider_raw_wire_requires_real_write_and_exclusive_source() {
    let (module, plan) = source_candidate_for(MUTABLE_RECEIVER);
    rejected(module.clone(), plan.clone(), |m| {
        let other = m.dyn_vtables[1].entries[0].function;
        m.dyn_vtables[0].entries[0].function = other;
    });
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_vtables[0].entries[0].slot = u32::MAX;
    });
    rejected(module.clone(), plan.clone(), |m| {
        m.dyn_interfaces[0].slots[0]
            .contract
            .call
            .as_mut()
            .unwrap()
            .writes
            .clear();
    });
    rejected(module.clone(), plan.clone(), |m| {
        let write = m
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::WriteAddress { .. }))
            .unwrap();
        write.kind = I::StorageDead(zil_ir::LocalId(0));
    });
    let mut forged = module.clone();
    let make = forged
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::MakeDynBorrow { .. }))
        .unwrap();
    if let I::MakeDynBorrow { mutable, .. } = &mut make.kind {
        *mutable = false;
    }
    let mut forged_plan = plan;
    forged_plan.module_fingerprint = forged.fingerprint();
    assert!(zil_analyze::analyze(&forged, &Limits::default()).is_err());
    let target = forged.target.clone();
    let wire =
        zil_ir::wire::encode_module_candidate(&forged, &forged_plan, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module(forged, forged_plan, &target, &Limits::default()).is_err());
    assert!(zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).is_err());
}

#[test]
fn shared_dyn_cannot_call_mutable_slot_or_create_exclusive_dyn() {
    for body in [
        "fn main()->i32{var a=A{n:1};let d:&dyn Change=&a;d.set(5);return a.n}",
        "fn main()->i32{var a=A{n:1};let shared:&A=&a;let d:&mut dyn Change=shared;d.set(5);return a.n}",
    ] {
        let report = compile(
            &format!("{MUTABLE_PREFIX}{body}"),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(matches!(
            report.phase,
            Phase::Expand | Phase::Types | Phase::Analyze | Phase::Verify
        ));
    }
    let conflict = format!(
        "{MUTABLE_PREFIX}fn main()->i32{{var a=A{{n:1}};let d:&mut dyn Change=&mut a;let shared:&A=&a;d.set(5);return shared.n}}"
    );
    let report = compile(&conflict, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}

#[test]
fn arithmetic_provider_cannot_claim_an_empty_dyn_effect_set() {
    let source = "trait Change{fn step(self:&mut Self)->();}struct A{n:i32}impl Change for A{fn step(self:&mut Self)->(){self.n+=1;return ()}}fn main()->i32{var a=A{n:1};let d:&mut dyn Change=&mut a;d.step();return a.n}";
    let report = compile(source, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn explicit_exclusive_dyn_local_retains_real_owner() {
    let source = format!(
        "{MUTABLE_PREFIX}fn main()->i32{{var a=A{{n:1}};let d:&mut dyn Change=&mut a;d.set(5);return a.n}}"
    );
    let dir = directory("exclusive-local");
    let executable = dir.join("program");
    let report = compile(
        &source,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("main".into()),
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
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stdout, b"5\n");
}

#[test]
fn mutable_provider_work_budget_has_exact_acceptance_edge() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "mutable-dispatch.zil",
        MUTABLE_RECEIVER,
    );
    let maximum = CompileOptions::default().limits.max_analysis_steps;
    let options = |work| {
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = work;
        options
    };
    let (mut low, mut high) = (1, maximum);
    while low < high {
        let middle = low + (high - low) / 2;
        let report = CompilationSession::default().compile_source(&unit, &options(middle));
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            _ => panic!("work={middle}: {}", report.to_json()),
        }
    }
    assert!(low > 1);
    for (work, status) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
    ] {
        let report = CompilationSession::default().compile_source(&unit, &options(work));
        assert_eq!(report.status, status, "{}", report.to_json());
        if status == ResultCategory::Success {
            assert_eq!(report.verification_runs, 1);
        }
    }
}

#[test]
fn owned_dyn_work_budget_has_same_cold_and_warm_edge() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned-dispatch-budget.zil",
        OWNED_MUTABLE,
    );
    let baseline = CompileOptions::default().limits.max_analysis_steps;
    let options = |work| {
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = work;
        options
    };
    let (mut low, mut high) = (1, baseline);
    while low < high {
        let middle = low + (high - low) / 2;
        let report = CompilationSession::default().compile_source(&unit, &options(middle));
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            _ => panic!("work={middle}: {}", report.to_json()),
        }
    }
    assert!(low > 1);
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
    ] {
        for report in [
            warm.compile_source(&unit, &options(work)),
            CompilationSession::default().compile_source(&unit, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::Success {
                assert_eq!(report.verification_runs, 1);
            }
        }
    }
}

#[test]
fn dyn_request_work_nodes_and_cache_recheck_current_verify() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "dynamic-dispatch-budget.zil", INPUT);
    let baseline = CompileOptions::default();
    let initial = CompilationSession::default().compile_source(&unit, &baseline);
    assert_eq!(
        initial.status,
        ResultCategory::Success,
        "{}",
        initial.to_json()
    );
    assert_eq!(initial.verification_runs, 1);
    for ledger in ["work", "nodes"] {
        let options = |n: usize| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => unreachable!(),
            }
            options
        };
        let maximum = match ledger {
            "work" => baseline.limits.max_analysis_steps,
            "nodes" => baseline.limits.max_nodes,
            _ => unreachable!(),
        };
        let (mut low, mut high) = (1, maximum);
        while low < high {
            let n = low + (high - low) / 2;
            let report = CompilationSession::default().compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{ledger} N={n}: {}", report.to_json()),
            }
        }
        assert!(low > 1, "{ledger} must have a meaningful N-1 control");
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(
                    report.status,
                    expected,
                    "{ledger} N={low}: {}",
                    report.to_json()
                );
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G5.2.1 dyn {ledger} N={low}; N-1/cold/warm/fresh agree");
    }

    let cache = directory("raw-cache");
    let options = CompileOptions {
        cache_dir: Some(cache.join("raw")),
        ..CompileOptions::default()
    };
    let cold = CompilationSession::default().compile_source(&unit, &options);
    let warm = CompilationSession::default().compile_source(&unit, &options);
    for report in [&cold, &warm] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
    assert_eq!(warm.disk_cache, DiskCacheStatus::Hit, "{}", warm.to_json());

    // semantic-v4 charge_type excludes Dyn. A trusted-local candidate cannot
    // authorize this type; both cold and warm requests rebuild before Verify.
    let options = CompileOptions {
        semantic_cache: Some(cache.join("semantic")),
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    let reports = [
        session.compile_source(&unit, &options),
        session.compile_source(&unit, &options),
        CompilationSession::default().compile_source(&unit, &options),
    ];
    for report in &reports {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
    for phase in [Phase::Types, Phase::Analyze] {
        assert!(
            reports[2]
                .query_events
                .iter()
                .any(|event| event.phase == phase
                    && event.disposition == zil_driver::QueryDisposition::Executed),
            "{}",
            reports[2].to_json()
        );
    }
    let candidates = cache.join("semantic").join("semantic-v4");
    if candidates.exists() {
        assert!(fs::read_dir(candidates).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|ext| ext != "candidate")
        }));
    }
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn project_request_eight_units_object_cache_reuses_actual_dyn_groups() {
    if !cfg!(feature = "llvm-api") {
        return;
    }
    let files = project_files(include_str!("project/main.zil"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("project-objects");
        let executable = dir.join("program");
        let options = CompileOptions {
            backend: NativeBackend::LlvmApi,
            optimization,
            stop_after: Stage::Build,
            entry: Some("main".into()),
            output: Some(executable.clone()),
            object_cache: Some(dir.join("objects")),
            codegen: CodegenOptions { units: 8, jobs: 4 },
            ..CompileOptions::default()
        };
        let mut baseline_units = None;
        for warm in [false, true] {
            let report = compile_project(&project(&files), &options);
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, NativeBackend::LlvmApi);
            assert!(native.fallback.is_none(), "{}", report.to_json());
            assert_eq!(native.requested_units, 8);
            assert!((1..=8).contains(&native.actual_units));
            if let Some(units) = baseline_units {
                assert_eq!(native.actual_units, units);
            } else {
                baseline_units = Some(native.actual_units);
            }
            if warm {
                assert_eq!(
                    native.object_cache.hits,
                    native.actual_units,
                    "{}",
                    report.to_json()
                );
                assert_eq!(native.object_cache.misses, 0);
            } else {
                assert_eq!(native.object_cache.hits, 0, "{}", report.to_json());
                assert_eq!(
                    native.object_cache.misses,
                    native.actual_units,
                    "{}",
                    report.to_json()
                );
            }
            let result = Command::new(&executable).output().unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert_eq!(result.stdout, b"29\n");
            assert!(result.stderr.is_empty(), "{result:?}");
        }
        eprintln!(
            "G5.2.1 dyn project requested 8, actual {} units at {optimization:?}",
            baseline_units.unwrap()
        );
    }
}
