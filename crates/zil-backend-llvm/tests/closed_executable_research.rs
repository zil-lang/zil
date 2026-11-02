#![cfg(target_os = "linux")]

//! Hand-authored TypedIR is independently verified before these backend tests.
//! These are ABI/configuration/cache controls, not source-language acceptance.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zil_backend_llvm::{
    BuildOptions, EmitOptions, EntryPoint, NativeBackend, Optimization, WindowsLinkOptions, build,
    emit_with_options,
};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;
use zil_verify::{VerifiedModule, verify_module};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-cp13-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(windows: bool) -> (VerifiedModule, LirModule) {
    let target = if windows {
        TargetProfile::windows_x64_llvm()
    } else {
        TargetProfile::host_llvm()
    };
    let functions = ["main", "called", "unused"]
        .into_iter()
        .enumerate()
        .map(|(id, name)| TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            result_mode: PassingMode::Infer,
            foreign: None,
            id: FunctionId(id as u32),
            name: name.into(),
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            locals: vec![],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: if id == 0 {
                        InstructionKind::Call {
                            function: FunctionId(1),
                            arguments: vec![],
                        }
                    } else {
                        InstructionKind::Constant(Constant::I32(if id == 1 { 37 } else { 99 }))
                    },
                    span: Span::default(),
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span: Span {
                file: FileId(id as u32),
                ..Span::default()
            },
        })
        .collect();
    let module = TypedModule {
        resources: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(13),
        source_fingerprint: Fingerprint::of(b"CP13 checked closed-executable backend fixture"),
        target: target.clone(),
        functions,
        closures: vec![],
        unsupported: vec![],
    };
    let mut candidate = CandidatePlan::empty_for(&module);
    candidate.functions = (0..3)
        .map(|id| FunctionPlan {
            function: FunctionId(id),
            holds: vec![ValueHold {
                value: ValueId(0),
                roots: vec![],
            }],
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![],
                initialized_out: vec![],
            }],
        })
        .collect();
    let verified = verify_module(module, candidate, &target, &Limits::default()).unwrap();
    let lir = LirModule {
        resources: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: verified.module().module_id,
        verified_fingerprint: verified.fingerprint(),
        target,
        functions: verified.module().functions.clone(),
        closures: vec![],
        layouts: [
            (Type::Unit, 0, 1),
            (Type::Bool, 1, 1),
            (Type::I32, 4, 4),
            (Type::I64, 8, 8),
            (Type::U8, 1, 1),
            (Type::U16, 2, 2),
            (Type::ForeignHandle, 8, 8),
            (Type::CWideString, 8, 8),
        ]
        .into_iter()
        .map(|(ty, size, alignment)| ScalarLayout {
            ty,
            size,
            alignment,
        })
        .collect(),
    };
    (verified, lir)
}

fn options(output: &Path) -> BuildOptions {
    let mut options = BuildOptions::new(output);
    options.clang = std::env::var_os("ZIL_CLANG").map_or_else(|| "clang".into(), PathBuf::from);
    options.optimization = Optimization::O2;
    options
}

#[test]
fn closed_policy_cannot_bypass_invalid_codegen_options() {
    let directory = Directory::new();
    let (verified, lir) = fixture(false);
    let executable = directory.0.join("previous-program");
    fs::write(&executable, b"previous executable").unwrap();
    for (units, jobs) in [(0, 2), (65, 2), (8, 0), (8, 65)] {
        let mut options = options(&executable);
        options.closed_executable = true;
        options.codegen.units = units;
        options.codegen.jobs = jobs;
        let diagnostics = build(&verified, &lir, &options).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E-CODEGEN-OPTIONS");
        assert_eq!(fs::read(&executable).unwrap(), b"previous executable");
        // Keep the existing rejection order when two configurations are wrong.
        options.emit.entry = EntryPoint::None;
        let diagnostics = build(&verified, &lir, &options).unwrap_err();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "E-LINK-ENTRY");
        assert_eq!(fs::read(&executable).unwrap(), b"previous executable");
    }
}

#[test]
fn closed_mode_preserves_entry_binding_and_unsupported_target_rejection() {
    let directory = Directory::new();
    let output = directory.0.join("program");
    let sidecar = directory.0.join("program.ll");
    fs::write(&output, b"previous executable").unwrap();
    fs::write(&sidecar, b"previous sidecar").unwrap();
    let (verified, lir) = fixture(false);
    let mut options = options(&output);
    options.closed_executable = true;
    options.emit.entry = EntryPoint::None;
    assert_eq!(
        build(&verified, &lir, &options).unwrap_err()[0].code,
        "E-LINK-ENTRY"
    );
    options.emit.entry = EntryPoint::Main;
    let mut forged = lir.clone();
    forged.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(88));
    let closed_errors = build(&verified, &forged, &options).unwrap_err();
    options.closed_executable = false;
    assert_eq!(
        closed_errors,
        build(&verified, &forged, &options).unwrap_err()
    );
    let (verified, lir) = fixture(true);
    options.windows = Some(WindowsLinkOptions::default());
    options.closed_executable = true;
    assert_eq!(
        build(&verified, &lir, &options).unwrap_err()[0].code,
        "E-CODEGEN-CLOSED-TARGET"
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous executable");
    assert_eq!(fs::read(&sidecar).unwrap(), b"previous sidecar");
}

#[test]
fn closed_link_failure_keeps_previous_artifacts() {
    let directory = Directory::new();
    let output = directory.0.join("program");
    let sidecar = directory.0.join("program.ll");
    fs::write(&output, b"previous executable").unwrap();
    fs::write(&sidecar, b"previous sidecar").unwrap();
    let (verified, lir) = fixture(false);
    let mut options = options(&output);
    options.closed_executable = true;
    options.clang = directory.0.join("missing-clang");
    assert!(build(&verified, &lir, &options).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"previous executable");
    assert_eq!(fs::read(&sidecar).unwrap(), b"previous sidecar");
    assert!(fs::read_dir(&directory.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zil-build-")
    }));
}

#[test]
fn closed_o0_build_keeps_runtime_behavior_with_one_or_many_units() {
    let directory = Directory::new();
    let (verified, lir) = fixture(false);
    let backends = if cfg!(feature = "llvm-api") {
        vec![NativeBackend::Clang, NativeBackend::LlvmApi]
    } else {
        vec![NativeBackend::Clang]
    };
    for backend in backends {
        for units in [1, 8] {
            let mut options = options(&directory.0.join(format!("{}-{units}", backend.name())));
            options.backend = backend;
            options.optimization = Optimization::O0;
            options.codegen.units = units;
            options.codegen.jobs = 2;
            options.closed_executable = true;
            let artifact = build(&verified, &lir, &options).unwrap();
            assert_eq!(
                Command::new(&artifact.executable).status().unwrap().code(),
                Some(37)
            );
            assert_eq!(
                artifact.codegen.actual_units,
                if units == 1 { 1 } else { 3 }
            );
        }
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn object_cache_separates_actual_linkage_and_reuses_each_policy() {
    let directory = Directory::new();
    let (verified, lir) = fixture(false);
    let mut options = options(&directory.0.join("program"));
    options.backend = NativeBackend::LlvmApi;
    options.object_cache = Some(directory.0.join("cache"));
    for (closed, hits, misses) in [(false, 0, 1), (true, 0, 1), (true, 1, 0), (false, 1, 0)] {
        options.closed_executable = closed;
        let artifact = build(&verified, &lir, &options).unwrap();
        assert!(
            artifact.codegen.object_cache.enabled,
            "{:?}",
            artifact.codegen.object_cache
        );
        assert_eq!(artifact.codegen.object_cache.hits, hits);
        assert_eq!(artifact.codegen.object_cache.misses, misses);
        assert_eq!(
            Command::new(&artifact.executable).status().unwrap().code(),
            Some(37)
        );
        if hits != 0 {
            assert!(artifact.codegen.object_reports[0].contains("\"input_kind\":\"object-cache\""));
            assert!(artifact.codegen.object_cache.current_preflight_ns > 0);
        }
    }
}

#[test]
fn selected_entry_and_public_emission_remain_separate_from_closed_build_policy() {
    let directory = Directory::new();
    let (verified, lir) = fixture(false);
    let before = emit_with_options(
        &verified,
        &lir,
        &EmitOptions {
            entry: EntryPoint::None,
        },
    )
    .unwrap();
    let mut options = options(&directory.0.join("selected"));
    options.closed_executable = true;
    options.emit.entry = EntryPoint::ReturnValue(FunctionId(2));
    let artifact = build(&verified, &lir, &options).unwrap();
    let native = Command::new(&artifact.executable).output().unwrap();
    assert!(native.status.success());
    assert_eq!(native.stdout, b"99\n");
    let after = emit_with_options(
        &verified,
        &lir,
        &EmitOptions {
            entry: EntryPoint::None,
        },
    )
    .unwrap();
    assert_eq!(before, after);
    for id in 0..3 {
        assert!(after.contains(&format!("define i32 @zil.fn.{id}(")));
    }
}
