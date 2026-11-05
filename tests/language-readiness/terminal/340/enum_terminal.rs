//! Real Source witnesses for complete Enum address endpoints.
use super::*;

const PENDING: &str = include_str!("replace-enum-terminal-pending.zil");
const FRESH: &str = include_str!("replace-enum-terminal-fresh-rhs.zil");
const INCOMING: &str = include_str!("replace-enum-terminal-incoming-rhs.zil");
const FAILURE: &str = include_str!("replace-enum-terminal-failure.zil");

#[test]
fn source_tickets_and_actual_both_kernels() {
    for (name, text, abort) in [
        ("runtime", PENDING, false),
        ("fresh", FRESH, false),
        ("failure", FAILURE, true),
    ] {
        let temp = Temp::new();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Types,
                emit_dir: Some(temp.0.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
        assert!(
            typed.contains("ReservePlaceAddress") && typed.contains("CommitPlaceReplace"),
            "{name}: {typed}"
        );
        assert!(!typed.contains("WriteAddress"), "{name}: {typed}");
        if abort {
            assert!(typed.contains("AbortPlaceReservation"), "{typed}");
        }
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn actual_rhs_created_and_given_origins_are_distinct() {
    let report = compile_source(&source(INCOMING), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("create")),
        "{}",
        report.to_json()
    );
    let given = INCOMING.replace("result(create)", "result(give)");
    let report = compile_source(&source(&given), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn shared_formal_and_multiple_roots_stay_closed() {
    let shared = PENDING.replace("let target=&mut value", "let target=&value");
    let report = compile_source(&source(&shared), &CompileOptions::default());
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-ADDRESS-PERMISSION")
    );
    let prefix = "resource R{code:i32}enum E{Left(manages R,i32),Right(i32,manages R),Empty}";
    let formal = format!(
        "{prefix}fn replace(target:&mut E){{*target=E.Right(42,R{{code:66}});}}fn main()->i32{{var value=E.Left(R{{code:65}},1);replace(&mut value);return 42;}}"
    );
    let report = compile_source(&source(&formal), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("exact whole local owner"))
    );
    let joined = format!(
        "{prefix}fn work(flag:bool)->i32{{var x=E.Left(R{{code:65}},1);var y=E.Right(2,R{{code:67}});let target=if flag{{&mut x}}else{{&mut y}};*target=E.Right(42,R{{code:66}});return 42;}}fn main()->i32{{return work(true);}}"
    );
    let report = compile_source(&source(&joined), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("one actual local root"))
    );
    let moved = format!(
        "{prefix}fn identity(input:E)->E{{return input;}}fn main()->i32{{var value=E.Left(R{{code:65}},1);let target=&mut value;*target=identity(value);return 42;}}"
    );
    let report = compile_source(&source(&moved), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("live old shell"))
    );
    let old_view = format!(
        "{prefix}fn main()->i32{{var value=E.Left(R{{code:65}},1);match value{{E.Left(ref r,_)=>{{let old=r;let target=&mut value;*target=E.Right(2,R{{code:66}});return old.code;}},E.Right(_,_)=>{{return 0;}},E.Empty()=>{{return 0;}}}}}}"
    );
    let report = compile_source(&source(&old_view), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
}

#[test]
fn raw_payload_description_transport_stays_explicitly_closed() {
    // Legal source uses the payload before replacement. The raw neighbor then
    // retains an actual weak description, not its address loan, across Commit.
    let text = "resource R{code:i32}enum E{Left(manages R,i32),Right(i32,manages R),Empty}fn main()->i32{var value=E.Left(R{code:65},1);match value{E.Left(ref r,_)=>{let n=r.code;let target=&mut value;*target=E.Right(2,R{code:66});return n;},E.Right(_,_)=>{return 0;},E.Empty()=>{return 0;}}}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    zil_analyze::analyze(&typed, &limits).unwrap();
    let f = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let address = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::BorrowVariantLocal { .. }
            | InstructionKind::BorrowPlaceLocal { .. } => i.result,
            _ => None,
        })
        .unwrap();
    let Type::Address { inner, .. } = &f.values[address.0 as usize] else {
        panic!("actual payload address")
    };
    let Type::Managed(payload) = inner.as_ref() else {
        panic!("actual Managed payload")
    };
    let desc = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::Desc(payload.clone()));
    let read = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
        })
        .unwrap();
    let reserve = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
        .unwrap();
    let span = block.instructions[reserve].span;
    block.instructions.insert(
        reserve,
        zil_ir::Instruction {
            result: Some(desc),
            kind: InstructionKind::DescribeAddress { address },
            span,
        },
    );
    let commit = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
        .unwrap();
    block.instructions.insert(
        commit + 1,
        zil_ir::Instruction {
            result: Some(read),
            kind: InstructionKind::ReadDesc { desc, index: 0 },
            span,
        },
    );
    block.terminator = zil_ir::Terminator::Return(Some(read));
    let mut before = typed.clone();
    let f = before
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| b.instructions.iter().any(|i| i.result == Some(read)))
        .unwrap();
    let pos = block
        .instructions
        .iter()
        .position(|i| i.result == Some(read))
        .unwrap();
    let operation = block.instructions.remove(pos);
    let reserve = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
        .unwrap();
    block.instructions.insert(reserve, operation);
    for (name, neighbor) in [("before Commit", before), ("after Commit", typed)] {
        let error = zil_analyze::analyze(&neighbor, &limits)
            .expect_err("payload weak-Desc transport remains closed");
        assert!(
            error.iter().any(|d| d
                .message
                .contains("instruction cannot produce the claimed resource kind")),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn native_runtime_tags_full_values_origins_and_preparation_failure() {
    use std::io::Write;
    use std::process::Stdio;
    use zil_driver::{NativeBackend, Optimization};
    let given = INCOMING.replace("result(create)", "result(give)");
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (name, text, runs) in [
                (
                    "runtime",
                    PENDING,
                    vec![
                        (b"L".as_slice(), b"AB266\n".as_slice()),
                        (b"R".as_slice(), b"CD168\n".as_slice()),
                    ],
                ),
                ("fresh", FRESH, vec![(b"".as_slice(), b"AB42\n".as_slice())]),
                (
                    "given",
                    given.as_str(),
                    vec![(b"".as_slice(), b"AB42\n".as_slice())],
                ),
                (
                    "failure",
                    FAILURE,
                    vec![(b"".as_slice(), b"BA42\n".as_slice())],
                ),
            ] {
                let directory = Temp::new();
                let report = zil_driver::compile_project(
                    &review329_project(&[("main", text)]),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        entry: Some("main".into()),
                        output: Some(directory.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}/{backend:?}/{optimization:?}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert!(!codegen.object_reports.is_empty());
                    assert!(
                        codegen
                            .object_reports
                            .iter()
                            .all(|o| o.contains("\"input_kind\":\"builder\""))
                    );
                }
                for (input, expected) in runs {
                    let mut process = Command::new(&report.artifacts["executable"])
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .unwrap();
                    process.stdin.take().unwrap().write_all(input).unwrap();
                    let output = process.wait_with_output().unwrap();
                    assert!(
                        output.status.success(),
                        "{name}/{backend:?}/{optimization:?}: {output:?}"
                    );
                    assert_eq!(
                        output.stdout, expected,
                        "{name}/{backend:?}/{optimization:?}"
                    );
                    assert!(output.stderr.is_empty());
                    println!(
                        "G1.3.5 {name}/{backend:?}/{optimization:?} stdin={input:?} stdout={:?} exit=0 Verify=1 fallback=None",
                        String::from_utf8_lossy(&output.stdout)
                    );
                }
            }
        }
    }
}
