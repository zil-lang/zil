#![cfg(target_os = "linux")]

//! Real backend/link-driver tests using hand-authored TypedIR checked by Verify.
//! Each configuration runs in a child test process so no test mutates the
//! process-global environment while other compiler tests are running.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use zil_backend_llvm::{BuildOptions, NativeBackend, Optimization, WindowsLinkOptions, build};
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;
use zil_verify::{VerifiedModule, verify_module};

fn fixture(windows: bool) -> (VerifiedModule, LirModule) {
    let target = if windows {
        TargetProfile::windows_x64_llvm()
    } else {
        TargetProfile::host_llvm()
    };
    let functions = [("main", 37), ("helper", 41)]
        .into_iter()
        .enumerate()
        .map(|(id, (name, value))| TypedFunction {
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
                    kind: InstructionKind::Constant(Constant::I32(value)),
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
        module_id: ModuleId(7),
        source_fingerprint: Fingerprint::of(b"CP07 hand-authored verified backend fixture"),
        target: target.clone(),
        functions,
        closures: vec![],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = (0..2)
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
    let verified = verify_module(module, plan, &target, &Limits::default()).expect("actual Verify");
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

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn child_case(case: &str) {
    let directory = PathBuf::from(std::env::var_os("ZIL_CP07_DIRECTORY").unwrap());
    let windows = case == "windows";
    let (verified, lir) = fixture(windows);
    let output = directory.join("program");
    let sidecar = directory.join("program.ll");
    fs::write(&output, b"earlier executable").unwrap();
    fs::write(&sidecar, b"earlier LLVM IR").unwrap();
    let mut options = BuildOptions::new(&output);
    options.clang = if case == "path" {
        "clang".into()
    } else {
        directory.join("clang")
    };
    if case.contains("api") {
        options.backend = NativeBackend::LlvmApi;
    }
    if case.contains("split") {
        options.codegen.units = 2;
        options.codegen.jobs = 2;
    }
    if case.contains("split") || case == "explicit" {
        options.optimization = Optimization::O2;
    }
    if windows {
        options.windows = Some(WindowsLinkOptions::default());
    }
    let result = build(&verified, &lir, &options);
    let error_code = match case {
        "empty" => Some("E-LINK-LINKER-CONFIG"),
        "missing" | "nonexecutable" => Some("E-LINK-LINKER-MISSING"),
        "failed" | "failed-api" => Some("E-LLVM-COMPILE"),
        "windows" => Some("E-WIN64-LINK"),
        _ => None,
    };
    if let Some(code) = error_code {
        let errors = result.unwrap_err();
        assert_eq!(errors[0].code, code, "{errors:?}");
        if case.starts_with("failed") {
            assert!(format!("{errors:?}").contains("CP07 deliberate linker failure"));
        }
        if windows {
            // Linux override is invalid here; reaching the real Windows linker
            // and diagnosing the missing import proves it was not consulted.
            assert!(format!("{errors:?}").contains("ExitProcess"));
        }
        assert_eq!(fs::read(&output).unwrap(), b"earlier executable");
        assert_eq!(fs::read(&sidecar).unwrap(), b"earlier LLVM IR");
    } else {
        let artifact = result.unwrap_or_else(|errors| panic!("{case}: {errors:?}"));
        let expected = if matches!(case, "system" | "absent") {
            None
        } else {
            Some(format!(
                "--ld-path={}",
                directory
                    .join(if case == "explicit" {
                        "explicit ld ;$"
                    } else {
                        "ld.lld"
                    })
                    .display()
            ))
        };
        let flags = artifact
            .commands
            .iter()
            .flatten()
            .filter(|arg| arg.starts_with("--ld-path="))
            .collect::<Vec<_>>();
        assert_eq!(flags, expected.iter().collect::<Vec<_>>(), "{case}");
        if case.contains("split") {
            assert_eq!(artifact.codegen.actual_units, 2);
            // Object-only Clang commands must not receive a link-driver flag.
            for command in &artifact.commands[..artifact.commands.len() - 1] {
                assert!(!command.iter().any(|arg| arg.starts_with("--ld-path=")));
            }
        }
        assert_eq!(
            Command::new(&artifact.executable).status().unwrap().code(),
            Some(37)
        );
        assert_ne!(fs::read(&sidecar).unwrap(), b"earlier LLVM IR");
        let log = directory.join("link.log");
        if expected.is_some() && case != "symlink" {
            assert!(
                fs::read_to_string(log)
                    .unwrap()
                    .contains("CP07 invoked LLD")
            );
        } else {
            assert!(!log.exists());
        }
    }
    assert!(fs::read_dir(&directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zil-build-")
    }));
}

#[test]
fn linux_linker_selection_and_publication() {
    if let Ok(case) = std::env::var("ZIL_CP07_CASE") {
        child_case(&case);
        return;
    }
    let clang = std::env::var_os("ZIL_CLANG").map_or_else(
        || {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|directory| directory.join("clang"))
                .find(|candidate| candidate.is_file())
                .expect("Clang must be installed")
        },
        PathBuf::from,
    );
    let clang = std::path::absolute(clang).unwrap();
    let lld = clang.parent().unwrap().join("ld.lld");
    assert!(lld.is_file(), "test requires the pinned LLVM SDK's ld.lld");
    let root = std::env::temp_dir().join(format!("zil-cp07 space ;$-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut cases = vec![
        "auto",
        "split",
        "path",
        "symlink",
        "system",
        "absent",
        "explicit",
        "empty",
        "missing",
        "nonexecutable",
        "failed",
        "windows",
    ];
    if cfg!(feature = "llvm-api") {
        cases.extend(["auto-api", "split-api", "failed-api"]);
    }
    for case in cases {
        let directory = root.join(case);
        fs::create_dir(&directory).unwrap();
        executable(
            &directory.join("clang"),
            "#!/bin/sh\nexec \"$ZIL_CP07_REAL_CLANG\" \"$@\"\n",
        );
        let linker = directory.join("ld.lld");
        let forward = "#!/bin/sh\nprintf 'CP07 invoked LLD\\n' > \"$ZIL_CP07_LINK_LOG\"\nexec \"$ZIL_CP07_REAL_LLD\" \"$@\"\n";
        if case == "symlink" {
            symlink(&lld, &linker).unwrap();
        } else if case.starts_with("failed") {
            executable(
                &linker,
                "#!/bin/sh\nprintf 'CP07 deliberate linker failure\\n' >&2\nexit 73\n",
            );
        } else if case != "absent" {
            executable(&linker, forward);
        }
        executable(&directory.join("explicit ld ;$"), forward);
        fs::write(directory.join("not executable"), b"not a linker").unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "linux_linker_selection_and_publication",
                "--nocapture",
            ])
            .env("ZIL_CP07_CASE", case)
            .env("ZIL_CP07_DIRECTORY", &directory)
            .env("ZIL_CP07_REAL_CLANG", &clang)
            .env("ZIL_CP07_REAL_LLD", &lld)
            .env("ZIL_CP07_LINK_LOG", directory.join("link.log"))
            .env_remove("ZIL_LINUX_LINKER");
        let paths = std::iter::once(directory.clone())
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ))
            .collect::<Vec<_>>();
        child.env("PATH", std::env::join_paths(paths).unwrap());
        match case {
            "system" => {
                child.env("ZIL_LINUX_LINKER", "system");
            }
            "explicit" => {
                child.env("ZIL_LINUX_LINKER", directory.join("explicit ld ;$"));
            }
            "empty" | "windows" => {
                child.env("ZIL_LINUX_LINKER", "");
            }
            "missing" => {
                child.env("ZIL_LINUX_LINKER", directory.join("missing linker"));
            }
            "nonexecutable" => {
                child.env("ZIL_LINUX_LINKER", directory.join("not executable"));
            }
            _ => {}
        }
        let result = child.output().unwrap();
        assert!(
            result.status.success(),
            "{case}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        println!("CP07 {case}: Pass");
    }
    fs::remove_dir_all(root).unwrap();
}
