//! Full Source -> independent Verify -> current machine -> native mixed inputs.
use super::*;
use std::io::Write;
use std::process::{Command, Stdio};

fn native_options(dir: &Directory) -> CompileOptions {
    let mut result = CompileOptions {
        stop_after: Stage::Build,
        backend: NativeBackend::LlvmApi,
        optimization: Optimization::O2,
        output: Some(dir.0.join("program")),
        object_cache: Some(dir.0.join("objects")),
        ..options(false)
    };
    result.codegen.units = 1;
    result.codegen.jobs = 1;
    // Ordinary O2: this slice changes no optional optimization policy.
    assert!(!result.incremental_o2 && !result.closed_executable);
    result
}
fn execute(path: &Path, input: &[u8]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    (result.status.code(), result.stdout, result.stderr)
}

#[test]
fn successful_native_build_without_reuse_releases_old_backend_history() {
    let dir = Directory::new();
    let options = native_options(&dir);
    let project = dir.project(&[(
        "main",
        "fn private(x:i32)->i32{return x+1} fn main()->i32{return private(1)-2}",
    )]);
    let mut session = CompilationSession::default();
    let cold = session.compile_project(&project, &options);
    oracle(&session, &cold, &project, &options, &[b""]);
    let warm = session.compile_project(&project, &options);
    successful(&warm);
    assert!(warm.backend_work.as_ref().unwrap().reused_machine_module);

    let disabled = CompileOptions {
        producer_reuse: false,
        push_reuse: false,
        ..options.clone()
    };
    let invalid = dir.project(&[("main", "fn main()->i32{return missing}")]);
    let failure = session.compile_project(&invalid, &disabled);
    assert_eq!(failure.status, ResultCategory::SafeReject);
    let after_failure = session.compile_project(&project, &options);
    successful(&after_failure);
    assert!(after_failure.backend_work.as_ref().unwrap().reused_machine_module);

    let fresh = session.compile_project(&project, &disabled);
    oracle(&session, &fresh, &project, &disabled, &[b""]);
    assert!(fresh.backend_work.is_none());
    let restarted = session.compile_project(&project, &options);
    oracle(&session, &restarted, &project, &options, &[b""]);
    let work = restarted.backend_work.as_ref().unwrap();
    assert!(!work.reused_machine_module);
    assert!(work.lowered_functions > 0);
}

fn oracle(
    session: &CompilationSession,
    report: &CompileReport,
    project: &ProjectSources,
    options: &CompileOptions,
    inputs: &[&[u8]],
) {
    successful(report);
    let llvm = fs::read(&report.artifacts["llvm"]).unwrap();
    let elf = fs::read(&report.artifacts["executable"]).unwrap();
    let runs = inputs
        .iter()
        .map(|input| execute(&report.artifacts["executable"], input))
        .collect::<Vec<_>>();
    let fresh_options = CompileOptions {
        object_cache: None,
        ..options.clone()
    };
    let expected = compare(session, report, project, &fresh_options);
    successful(&expected);
    assert_eq!(
        llvm,
        fs::read(&expected.artifacts["llvm"]).unwrap(),
        "complete current LLVM bytes"
    );
    assert_eq!(
        elf,
        fs::read(&expected.artifacts["executable"]).unwrap(),
        "complete current native bytes"
    );
    for (input, result) in inputs.iter().zip(runs) {
        assert_eq!(result, execute(&expected.artifacts["executable"], input));
    }
}

#[test]
fn original_resource_closure_project_reuses_independent_private_scalar_machine() {
    const MAIN: &str = include_str!("../../../examples/mainline/resource_closures/main.zil");
    const TOKEN: &str = include_str!("../../../examples/mainline/resource_closures/token.zil");
    const IO: &str = include_str!("../../../examples/mainline/resource_closures/io.zil");
    let dir = Directory::new();
    let options = native_options(&dir);
    let main = format!(
        "{}\nfn private(x:i32)->i32{{return x+1}}\nfn sibling(x:i32)->i32{{return x+2}}",
        MAIN.replace("return 8", "return private(sibling(5))")
    );
    let mut session = CompilationSession::default();
    for (index, source) in [
        main.clone(),
        main.clone(),
        main.replace("return x+1", "return x+3"),
    ]
    .into_iter()
    .enumerate()
    {
        let project = dir.project(&[("main", &source), ("token", TOKEN), ("io", IO)]);
        let report = session.compile_project(&project, &options);
        oracle(
            &session,
            &report,
            &project,
            &options,
            &[b"D", b"S", b"O", b"M", b"?"],
        );
        for (input, output) in [
            (b"D", b"C".as_slice()),
            (b"S", b"A".as_slice()),
            (b"O", b"CB".as_slice()),
            (b"M", b"C".as_slice()),
        ] {
            assert_eq!(
                execute(&report.artifacts["executable"], input),
                (Some(0), output.to_vec(), vec![])
            );
        }
        assert_eq!(
            execute(&report.artifacts["executable"], b"?"),
            (Some(if index == 2 { 10 } else { 8 }), vec![], vec![])
        );
        let work = report
            .backend_work
            .as_ref()
            .expect("actual backend session");
        assert!(!work.reused_machine_transforms && !work.reused_partition_recipe);
        assert!(
            work.lowered_functions > 0,
            "resource and closure functions still run original emitter"
        );
        if index > 0 {
            assert!(
                work.reused_functions > 0,
                "mixed project must preserve independent scalar products"
            );
        }
    }
    // A real current resource inventory/name change cannot reuse old context.
    let renamed_main = main.replace("Token", "RenamedToken");
    let renamed_token = TOKEN.replace("Token", "RenamedToken");
    let project = dir.project(&[
        ("main", &renamed_main),
        ("token", &renamed_token),
        ("io", IO),
    ]);
    let report = session.compile_project(&project, &options);
    oracle(&session, &report, &project, &options, &[b"D", b"?"]);
    assert_eq!(report.backend_work.as_ref().unwrap().reused_functions, 0);
}

#[test]
fn nonyield_scalar_worker_is_never_reused_as_an_ordinary_function_batch() {
    const ORIGINAL: &str = include_str!(
        "../../../tests/language-readiness/terminal/concurrency-and-registration/task-nonyield.zil"
    );
    let dir = Directory::new();
    let options = native_options(&dir);
    let main = format!(
        "{}\nfn helper(x:i32)->i32{{return x+2}}\nfn stable(x:i32)->i32{{return x+4}}",
        ORIGINAL.replace("plain(7)", "plain(helper(5))")
    );
    let mut session = CompilationSession::default();
    let ordinary = "fn plain(value:i32)->i32{return value+1} fn main()->i32{return plain(helper(5))-8} fn helper(x:i32)->i32{return x+2} fn stable(x:i32)->i32{return x+4}".to_owned();
    for (index, source) in [
        ordinary,
        main.clone(),
        main.clone(),
        main.replace("return x+2", "return x+3"),
    ]
    .into_iter()
    .enumerate()
    {
        let project = dir.project(&[("main", &source)]);
        let report = session.compile_project(&project, &options);
        oracle(&session, &report, &project, &options, &[b""]);
        assert_eq!(
            execute(&report.artifacts["executable"], b""),
            (Some(i32::from(index == 3)), vec![], vec![])
        );
        let work = report.backend_work.as_ref().unwrap();
        assert!(!work.reused_machine_transforms && !work.reused_partition_recipe);
        if index <= 1 {
            assert_eq!(
                work.reused_functions, 0,
                "new current task roles invalidate ordinary cached bodies"
            );
        } else if index == 2 {
            assert_eq!(work.reused_functions, 2);
            assert_eq!(
                work.lowered_functions, 2,
                "plain worker plus owning task main"
            );
        } else if index == 3 {
            assert_eq!(work.reused_functions, 1);
            assert_eq!(
                work.lowered_functions, 3,
                "plain must still produce a current task machine"
            );
        }
    }
}
