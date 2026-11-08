//! Query reads use the actual weak source and its own delegation alternative.
use super::*;
use zil_ir::InstructionKind;

fn report_for(label: &str, program: &str) -> zil_driver::CompileReport {
    compile_source(&source(label, program), &CompileOptions::default())
}
fn assert_success(label: &str, program: &str) {
    let report = report_for(label, program);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn assert_loan(label: &str, program: &str) {
    let report = report_for(label, program);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual stored target/parent loan")
                || d.message.contains("live incompatible loan")),
        "{label}: {}",
        report.to_json()
    );
}

#[test]
fn weak_metadata_query_reaches_actual_source_and_parent_checks() {
    for (label, program) in [
        ("direct", include_str!("byte-query-controls/direct.zil")),
        ("identity", include_str!("byte-query-controls/identity.zil")),
        (
            "current-exclusive-parent",
            include_str!("byte-query-controls/exclusive-derived-query.zil"),
        ),
        (
            "both-derived-false",
            include_str!("byte-query-controls/both-derived-false-query.zil"),
        ),
        (
            "both-derived-true",
            include_str!("byte-query-controls/both-derived-true-query.zil"),
        ),
    ] {
        assert_success(label, program);
    }
    for (label, program) in [
        (
            "older-description-exclusive-parent",
            include_str!("byte-query-controls/exclusive-query.zil"),
        ),
        (
            "helper-read",
            include_str!("byte-query-controls/read-helper.zil"),
        ),
        (
            "helper-relay",
            include_str!("byte-query-controls/read-relay.zil"),
        ),
        (
            "mixed-false",
            include_str!("byte-query-controls/mixed-false-query.zil"),
        ),
        (
            "mixed-true",
            include_str!("byte-query-controls/mixed-true-query.zil"),
        ),
    ] {
        assert_loan(label, program);
    }
    let ignore = format!(
        "fn enable_query(b:ByteBuffer)->i64{{return bytes_len(b)}}{}",
        include_str!("byte-query-controls/ignore-helper-control.zil")
    );
    assert_success("ignore-is-not-a-read", &ignore);
}

fn activated_ignore(program: &str) -> String {
    format!(
        "fn enable_query(b:ByteBuffer)->i64{{return bytes_capacity(b)}}{}",
        program.replace(
            "fn inspect(buffer:ByteBuffer)->i64{return bytes_len(buffer)}",
            "fn inspect(buffer:ByteBuffer)->i64{let copy=buffer;return 0}"
        )
    )
}

#[test]
fn raw_query_rebuilds_helper_reads_and_delegation_without_candidate_authority() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let loop_relay = include_str!("byte-query-controls/loop-old-delegation-query.zil")
        .replace("sum+inspect(description)","sum+relay_twice(description)")
        .replace("fn main", "fn relay(buffer:ByteBuffer)->i64{return inspect(buffer)}fn relay_twice(buffer:ByteBuffer)->i64{return relay(buffer)}fn main");
    for (label, program, reject) in [
        (
            "old-parent",
            include_str!("byte-query-controls/read-helper.zil"),
            true,
        ),
        (
            "old-loop-generation",
            include_str!("byte-query-controls/loop-old-delegation-query.zil"),
            true,
        ),
        ("old-loop-two-relays", loop_relay.as_str(), true),
        (
            "mixed-false",
            include_str!("byte-query-controls/mixed-false-query.zil"),
            true,
        ),
        (
            "mixed-true",
            include_str!("byte-query-controls/mixed-true-query.zil"),
            true,
        ),
        (
            "derived-false",
            include_str!("byte-query-controls/both-derived-false-query.zil"),
            false,
        ),
        (
            "derived-true",
            include_str!("byte-query-controls/both-derived-true-query.zil"),
            false,
        ),
    ] {
        let control = activated_ignore(program);
        assert_success(label, &control);
        let mut raw = typed_source(label, &control, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        let descriptor = function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Load(local) if local == function.parameters[0] => i.result,
                _ => None,
            })
            .unwrap();
        assert!(
            matches!(&function.values[descriptor.0 as usize],Type::Desc(p) if p.as_ref()==&Type::ByteBuffer)
        );
        let returned = function
            .blocks
            .iter()
            .find_map(|b| match b.terminator {
                zil_ir::Terminator::Return(Some(v)) => Some(v),
                _ => None,
            })
            .unwrap();
        assert_eq!(function.values[returned.0 as usize], Type::I64);
        let instruction = function
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.result == Some(returned))
            .unwrap();
        instruction.kind = InstructionKind::ByteBufferLen { buffer: descriptor };
        // Retain every old claim and metadata row. Only the actual read and
        // its module fingerprint change; this is not a producer certificate.
        plan.module_fingerprint = raw.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let artifacts = std::env::temp_dir().join("byte-query-raw-controls");
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::write(artifacts.join(format!("{label}.zil")), &control).unwrap();
        std::fs::write(artifacts.join(format!("{label}-mutated.wire")), &wire).unwrap();

        if reject {
            let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
            assert!(
                a.iter()
                    .any(|d| d.message.contains("actual stored target/parent loan")),
                "{label}: {a:?}"
            );
            let v =
                zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
            assert!(
                v.iter()
                    .any(|d| d.message.contains("live incompatible loan")),
                "{label}: {v:?}"
            );
            let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
            assert!(
                w.iter()
                    .any(|d| d.message.contains("live incompatible loan")),
                "{label}: {w:?}"
            );
        } else {
            zil_analyze::analyze(&raw, &limits).unwrap();
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        }
    }
}

fn metadata_prefix() -> String {
    GETTER.split("fn main").next().unwrap().to_owned()
}
fn selected_query_program() -> String {
    format!(
        "{}fn size_second(left:ByteBuffer,right:ByteBuffer)->i64{{return bytes_len(right)}}fn keep<life a>(input:&mut Job<'a>)->i32{{return 7}}fn main()->i32{{var a=3;var b=7;var first=make(&a);let second=make(&b);let left=&first;let right=&second;let ignored=left.bytes;let selected=right.bytes;let exclusive=&mut first;let observed=size_second(ignored,selected);if observed==0 && keep(exclusive)==7{{return 0}}return 1}}",
        metadata_prefix()
    )
}
fn separated_formals_program() -> String {
    format!(
        "{}fn keep<life a>(input:&mut Job<'a>)->i32{{return 7}}fn inspect<life a>(buffer:ByteBuffer,owner:&mut Job<'a>)->i64{{let observed=bytes_len(buffer);keep(owner);return observed}}fn main()->i32{{var a=3;var b=7;var first=make(&a);var second=make(&b);let window=&first;let old=window.bytes;let exclusive=&mut second;let observed=inspect(old,exclusive);if observed==0{{return 0}}return 1}}",
        metadata_prefix()
    )
}

fn owner_liveness_program() -> String {
    let prefix = metadata_prefix()
        .replace(
            "manages bytes:ByteBuffer;refs:",
            "manages bytes:ByteBuffer;manages spare:ByteBuffer;refs:",
        )
        .replace(
            "var bytes=bytes_new()",
            "var bytes=bytes_new();var spare=bytes_new()",
        )
        .replace(
            "bytes:give bytes,refs:",
            "bytes:give bytes,spare:give spare,refs:",
        );
    format!(
        "{prefix}fn main()->i32{{var value=7;var job=make(&value);let window=&job;let old=window.bytes;let retired=give job.spare;let observed=bytes_len(old);if observed==0{{return 0}}return 1}}"
    )
}

#[test]
fn actual_parameter_selection_and_finished_windows_preserve_weak_semantics() {
    let selected = selected_query_program();
    assert_success("query-only-param1", &selected);
    let changed_body = selected.replace("return bytes_len(right)", "return bytes_len(left)");
    let mut session = zil_driver::CompilationSession::default();
    for program in [
        &selected,
        &changed_body,
        &selected,
        &changed_body,
        &selected,
    ] {
        for report in [
            session.compile_source(
                &source("current-query-body", program),
                &CompileOptions::default(),
            ),
            report_for("current-query-body", program),
        ] {
            if program == &selected {
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
            } else {
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
                        .any(|d| d.message.contains("actual stored target/parent loan")
                            || d.message.contains("live incompatible loan")),
                    "{}",
                    report.to_json()
                );
            }
        }
    }
    assert_loan(
        "query-param1-redirected",
        &selected.replace(
            "size_second(ignored,selected)",
            "size_second(selected,ignored)",
        ),
    );
    assert_success(
        "separate-descriptor-and-exclusive-formals",
        &separated_formals_program(),
    );
    let same = separated_formals_program()
        .replace("let first=make(&a)", "var first=make(&a)")
        .replace("let exclusive=&mut second", "let exclusive=&mut first");
    assert_loan("same-head-independent-formals", &same);
    let finished=include_str!("byte-query-controls/exclusive-query.zil").replace("let observed=bytes_len(old);return keep(exclusive)-7","let kept=keep(exclusive);let observed=bytes_len(old);if kept==7 && observed==0{return 0}return 1");
    assert_success("finished-exclusive-window", &finished);
    let helper_derived = include_str!("byte-query-controls/exclusive-derived-query.zil").replace(
        "let observed=bytes_len(current)",
        "let observed=inspect(current)",
    );
    assert_success(
        "helper-from-current-exclusive",
        &format!("fn inspect(buffer:ByteBuffer)->i64{{return bytes_len(buffer)}}{helper_derived}"),
    );
}

fn check_raw_owner_or_formal_separation(owner: bool) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    {
        let program = if owner {
            owner_liveness_program()
        } else {
            separated_formals_program()
        };
        assert_success("owner-or-apart-control", &program);
        let mut raw = typed_source("owner-or-apart-control", &program, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        if owner {
            let instruction = main
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::TakePackageField { index: 1, .. }))
                .unwrap();
            let InstructionKind::TakePackageField { index, .. } = &mut instruction.kind else {
                unreachable!()
            };
            *index = 0;
        } else {
            let from = main.locals.iter().find(|l| l.name == "second").unwrap().id;
            let to = main.locals.iter().find(|l| l.name == "first").unwrap().id;
            assert_eq!(
                main.locals[from.0 as usize].ty,
                main.locals[to.0 as usize].ty
            );
            let instruction = main
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind,InstructionKind::BorrowLocalMut {local} if local==from))
                .unwrap();
            instruction.kind = InstructionKind::BorrowLocalMut { local: to };
        }
        plan.module_fingerprint = raw.fingerprint();
        let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
        let v = zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let w = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        if owner {
            assert!(
                a.iter().any(|d| d
                    .message
                    .contains("invalidates a description required later")),
                "{a:?}"
            );
            for errors in [v, w] {
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("invalidate a live description")),
                    "{errors:?}"
                );
            }
        } else {
            assert!(
                a.iter()
                    .any(|d| d.message.contains("actual stored target/parent loan")),
                "{a:?}"
            );
            for errors in [v, w] {
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("live incompatible loan")),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn raw_owner_liveness_requires_the_actual_held_head() {
    check_raw_owner_or_formal_separation(true);
}
#[test]
fn raw_formal_separation_requires_the_actual_distinct_head() {
    check_raw_owner_or_formal_separation(false);
}

fn filled(program: &str) -> String {
    program.replace("var bytes=bytes_new()","var bytes=bytes_new();bytes_reserve(bytes,8);bytes_push(bytes,65u8);bytes_push(bytes,66u8)")
}
fn metadata_native_programs() -> Vec<(&'static str, String)> {
    let mut programs = Vec::new();
    for (label, program) in [
        ("direct", include_str!("byte-query-controls/direct.zil")),
        ("identity", include_str!("byte-query-controls/identity.zil")),
    ] {
        programs.push((
            label,
            filled(program).replace(
                "bytes_len(window.bytes)==0",
                "bytes_len(window.bytes)==2 && bytes_capacity(window.bytes)>=8",
            ),
        ));
    }
    for (label, program) in [
        (
            "exclusive-derived",
            include_str!("byte-query-controls/exclusive-derived-query.zil"),
        ),
        (
            "derived-phi-false",
            include_str!("byte-query-controls/both-derived-false-query.zil"),
        ),
        (
            "derived-phi-true",
            include_str!("byte-query-controls/both-derived-true-query.zil"),
        ),
    ] {
        programs.push((
            label,
            filled(program).replace(
                "return keep(exclusive)-7",
                "if observed==2 && keep(exclusive)==7{return 0}return 1",
            ),
        ));
    }
    programs.push((
        "selected-parameter1",
        filled(&selected_query_program())
            .replace("observed==0", "observed==2")
            .replace(
                "bytes_push(bytes,66u8)",
                "if *pointer==7{bytes_push(bytes,66u8)}",
            ),
    ));
    programs.push((
        "separate-formals",
        filled(&separated_formals_program()).replace("observed==0", "observed==2"),
    ));
    programs.push((
        "unrelated-owner-cleanup",
        filled(&owner_liveness_program()).replace("observed==0", "observed==2"),
    ));
    programs
}
#[test]
fn native_byte_metadata_queries_observe_real_headers_and_cleanup() {
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in metadata_native_programs() {
                let directory = std::env::temp_dir().join(format!(
                    "zil-byte-metadata-{}-{label}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&directory).unwrap();
                let executable = directory.join("program");
                let report = compile_source(
                    &source(label, &program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        output: Some(executable.clone()),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{label}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let output = std::process::Command::new(&executable).output().unwrap();
                assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
                assert!(output.stdout.is_empty(), "{output:?}");
                runs += 1;
            }
        }
    }
    assert_eq!(
        runs,
        metadata_native_programs().len() * (2 + 2 * usize::from(cfg!(feature = "llvm-api")))
    );
}

fn query_budget_model(width: usize, depth: usize, alternatives: usize) -> String {
    let fields = (1..width)
        .map(|n| format!(";pad{n}:i32"))
        .collect::<String>();
    let initial = (1..width)
        .map(|n| format!(",pad{n}:{n}"))
        .collect::<String>();
    let mut program = format!(
        "struct Refs<life a>{{pointer:&'a i32{fields}}}package Job<life a>{{manages bytes:ByteBuffer;refs:Refs<'a>}}fn make<life a>(pointer:&'a i32)->Job<'a>{{var bytes=bytes_new();let refs=Refs::<'a>{{pointer:pointer{initial}}};return Job::<'a>{{bytes:give bytes,refs:refs}} }}fn query0(buffer:ByteBuffer)->i64{{return bytes_len(buffer)+bytes_capacity(buffer)}}"
    );
    for n in 1..=depth {
        program += &format!(
            "fn query{n}(buffer:ByteBuffer)->i64{{return query{}(buffer)}}",
            n - 1
        );
    }
    let choice = if alternatives == 1 {
        "let descriptor=window.bytes;".to_owned()
    } else {
        "let other_job=make(&value);let other=&other_job;let descriptor=if flag(){window.bytes}else{other.bytes};".to_owned()
    };
    program += &format!(
        "fn flag()->bool{{return false}}fn main()->i32{{var value=7;let job=make(&value);let window=&job;{choice}let observed=query{depth}(descriptor);if observed==0 && *window.refs.pointer==7{{return 0}}return 1}}"
    );
    program
}
#[test]
fn byte_queries_pay_for_actual_types_effect_chains_and_origin_alternatives() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for alternatives in [1, 2] {
        for (width, depth) in [(1, 0), (8, 0), (1, 2), (8, 2)] {
            let program = query_budget_model(width, depth, alternatives);
            let input = source("byte-query-budget", &program);
            let raw = typed_source("byte-query-budget", &program, &target, &default);
            let plan = zil_analyze::analyze(&raw, &default).unwrap();
            let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &default).unwrap();
            let limits = |n| Limits {
                max_analysis_steps: n,
                ..default.clone()
            };
            let label = |kernel: &str| {
                format!(
                    "Byte query {kernel} width={width} depth={depth} alternatives={alternatives}"
                )
            };
            let analyze = exact_record_budget(&label("Analyze"), |n| {
                budget_category(zil_analyze::analyze(&raw, &limits(n)))
            });
            let verify = exact_record_budget(&label("Verify"), |n| {
                budget_category(zil_verify::verify_module(
                    raw.clone(),
                    plan.clone(),
                    &target,
                    &limits(n),
                ))
            });
            let decoded = exact_record_budget(&label("Wire"), |n| {
                budget_category(zil_verify::verify_module_bytes(&wire, &target, &limits(n)))
            });
            let whole = exact_record_budget(&label("Source"), |n| {
                compile_source(
                    &input,
                    &CompileOptions {
                        limits: limits(n),
                        ..CompileOptions::default()
                    },
                )
                .status
            });
            let mut session = zil_driver::CompilationSession::default();
            for (n, status) in [
                (whole, ResultCategory::Success),
                (whole - 1, ResultCategory::ResourceLimit),
                (whole, ResultCategory::Success),
            ] {
                let options = CompileOptions {
                    limits: limits(n),
                    ..CompileOptions::default()
                };
                assert_eq!(session.compile_source(&input, &options).status, status);
                assert_eq!(compile_source(&input, &options).status, status);
                assert_eq!(session.active_query_count(), 0);
            }
            assert_eq!(verify, decoded);
            boundaries.push([analyze, verify, decoded, whole]);
        }
    }
    for kernel in 0..4 {
        for offset in [0, 4] {
            assert!(
                boundaries[offset + 1][kernel] > boundaries[offset][kernel],
                "width: {boundaries:?}"
            );
            assert!(
                boundaries[offset + 2][kernel] > boundaries[offset][kernel],
                "effect chain: {boundaries:?}"
            );
            assert!(
                boundaries[offset + 3][kernel] > boundaries[offset + 1][kernel]
                    && boundaries[offset + 3][kernel] > boundaries[offset + 2][kernel],
                "{boundaries:?}"
            );
        }
        for model in 0..4 {
            assert!(
                boundaries[model + 4][kernel] > boundaries[model][kernel],
                "actual alternatives: {boundaries:?}"
            );
        }
    }
}

#[test]
fn byte_companion_requires_its_own_reachable_query_and_exact_input_kind() {
    let legal = separated_formals_program();
    assert_success("actual-query-companion", &legal);
    let no_query = legal.replace("let observed=bytes_len(buffer)", "let observed=i64(0)");
    for (label, program) in [
        ("no-query", no_query.clone()),
        (
            "unrelated-query",
            format!(
                "fn unrelated(buffer:ByteBuffer)->i64{{return bytes_capacity(buffer)}}{no_query}"
            ),
        ),
    ] {
        let typed = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Types,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            typed.status,
            ResultCategory::Success,
            "{label}: {}",
            typed.to_json()
        );
        let report = report_for(label, &program);
        assert_eq!(
            report.phase,
            zil_base::Phase::Analyze,
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = typed_source("byte-classifier-control", &legal, &target, &limits);
    let original = zil_analyze::analyze(&raw, &limits).unwrap();
    // The Source checker deliberately stops at a statement after return. Build
    // a well-typed unreachable block to exercise the consumer's own CFG proof.
    let unreachable_source = legal.replace(
        "let observed=bytes_len(buffer);keep(owner);return observed",
        "keep(owner);return 0;let observed=bytes_len(buffer)",
    );
    let report = report_for("unreachable-source-boundary", &unreachable_source);
    assert_eq!(report.phase, zil_base::Phase::Types);
    assert_eq!(report.status, ResultCategory::Unsupported);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message == "unreachable-statement type checking")
    );
    let mut dead_query = raw.clone();
    let f = dead_query
        .functions
        .iter_mut()
        .find(|f| f.name == "inspect")
        .unwrap();
    let span = f.span;
    let local = zil_ir::LocalId(f.locals.len() as u32);
    f.locals.push(zil_ir::LocalDecl {
        id: local,
        name: "unreachable_bytes".into(),
        ty: Type::Managed(Box::new(Type::ByteBuffer)),
        mutable: true,
        span,
    });
    let managed = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::Managed(Box::new(Type::ByteBuffer)));
    let descriptor = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::Desc(Box::new(Type::ByteBuffer)));
    let queried = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::I64);
    f.blocks.push(zil_ir::BasicBlock {
        id: zil_ir::BlockId(f.blocks.len() as u32),
        instructions: vec![
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::StorageLive(local),
                span,
            },
            zil_ir::Instruction {
                result: Some(managed),
                kind: InstructionKind::ByteBufferNew,
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::Store {
                    local,
                    value: managed,
                },
                span,
            },
            zil_ir::Instruction {
                result: Some(descriptor),
                kind: InstructionKind::DescribeLocal { local },
                span,
            },
            zil_ir::Instruction {
                result: Some(queried),
                kind: InstructionKind::ByteBufferLen { buffer: descriptor },
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::DropManaged { local },
                span,
            },
        ],
        terminator: zil_ir::Terminator::Return(Some(queried)),
    });
    let dead_block = f.blocks.last().unwrap().id;
    let mut dead_plan = zil_analyze::analyze(&dead_query, &limits).unwrap();
    zil_verify::verify_module(dead_query.clone(), dead_plan.clone(), &target, &limits).unwrap();
    let f = dead_query
        .functions
        .iter_mut()
        .find(|f| f.name == "inspect")
        .unwrap();
    let mut changed = 0;
    for block in &mut f.blocks {
        if block.id == dead_block {
            continue;
        }
        for instruction in &mut block.instructions {
            if matches!(instruction.kind, InstructionKind::ByteBufferLen { .. }) {
                instruction.kind = InstructionKind::Constant(zil_ir::Constant::I64(0));
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    dead_plan.module_fingerprint = dead_query.fingerprint();
    let errors = zil_analyze::analyze(&dead_query, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message
                == "Byte query requires a complete Direct actual description/read proof"),
        "{errors:?}"
    );
    let errors = zil_verify::verify_module(dead_query.clone(), dead_plan.clone(), &target, &limits)
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message
                == "Byte query requires a complete Direct actual description/read proof"),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&dead_query, &dead_plan, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message
                == "Byte query requires a complete Direct actual description/read proof"),
        "{errors:?}"
    );
    for mode in 0..6 {
        let mut changed = raw.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        match mode {
            0 => {
                let contract = f.storage_life.as_mut().unwrap();
                let mut row = contract.bindings[0].clone();
                row.position.base = zil_ir::StorageBase::Parameter(0);
                contract.bindings.insert(0, row);
            }
            1 => {
                let contract = f.storage_life.as_mut().unwrap();
                let mut row = contract.classes[0].clone();
                row.position.base = zil_ir::StorageBase::Parameter(0);
                contract.classes.insert(0, row);
            }
            2 => {
                let p = f.parameters[0];
                f.locals[p.0 as usize].ty = Type::Managed(Box::new(Type::ByteBuffer));
            }
            3 => {
                let p = f.parameters[0];
                f.locals[p.0 as usize].ty = Type::Desc(Box::new(Type::I32));
            }
            4 => f.return_type = Type::Desc(Box::new(Type::ByteBuffer)),
            _ => {
                let p = f.parameters[0];
                f.locals[p.0 as usize].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Desc(Box::new(Type::ByteBuffer))),
                };
            }
        }
        let mut plan = original.clone();
        plan.module_fingerprint = changed.fingerprint();
        let a = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            a.iter().all(|d| d.category == ResultCategory::SafeReject),
            "mode{mode}: {a:?}"
        );
        let v =
            zil_verify::verify_module(changed.clone(), plan.clone(), &target, &limits).unwrap_err();
        if mode == 4 {
            assert!(v.iter().any(|d|d.category==ResultCategory::Unsupported && d.message=="resource description returns require exported source and management contracts"),"{v:?}");
        } else {
            assert!(
                v.iter().all(|d| d.category == ResultCategory::SafeReject),
                "mode{mode}: {v:?}"
            );
        }
        let wire = zil_ir::wire::encode_module_candidate(&changed, &plan, &limits).unwrap();
        let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        if mode == 4 {
            assert!(w.iter().any(|d|d.category==ResultCategory::Unsupported && d.message=="resource description returns require exported source and management contracts"),"{w:?}");
        } else {
            assert!(
                w.iter().all(|d| d.category == ResultCategory::SafeReject),
                "mode{mode}: {w:?}"
            );
        }
    }
}

#[test]
fn byte_queries_reject_wrong_operand_result_and_sealed_resource() {
    let program = separated_formals_program().replace(
        "let observed=bytes_len(buffer)",
        "let marker=0;let observed=bytes_len(buffer)",
    );
    assert_success("canonical-byte-header", &program);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = typed_source("canonical-byte-header", &program, &target, &limits);
    let original = zil_analyze::analyze(&raw, &limits).unwrap();
    for mode in 0..3 {
        let mut changed = raw.clone();
        if mode == 2 {
            changed
                .resources
                .retain(|r| r.name != "@builtin:ByteBuffer");
        } else {
            let f = changed
                .functions
                .iter_mut()
                .find(|f| f.name == "inspect")
                .unwrap();
            let marker = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    if matches!(i.kind, InstructionKind::Constant(_))
                        && i.result
                            .is_some_and(|v| f.values[v.0 as usize] == Type::I32)
                    {
                        i.result
                    } else {
                        None
                    }
                })
                .unwrap();
            let query = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ByteBufferLen { .. }))
                .unwrap();
            if mode == 0 {
                query.kind = InstructionKind::ByteBufferLen { buffer: marker };
            } else {
                f.values[query.result.unwrap().0 as usize] = Type::I32;
            }
        }
        let mut plan = original.clone();
        plan.module_fingerprint = changed.fingerprint();
        let a = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            a.iter().all(|d| d.category == ResultCategory::SafeReject),
            "mode{mode}: {a:?}"
        );
        let v =
            zil_verify::verify_module(changed.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            v.iter().all(|d| d.category == ResultCategory::SafeReject),
            "mode{mode}: {v:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&changed, &plan, &limits).unwrap();
        let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            w.iter().all(|d| d.category == ResultCategory::SafeReject),
            "mode{mode}: {w:?}"
        );
    }
}

#[test]
fn existing_ordinary_query_domain_retains_its_independent_consumer() {
    let program = include_str!("byte-query-controls/ordinary-query-with-stored-neighbor.zil");
    assert_success("ordinary-query-with-stored-neighbor", program);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = typed_source(
        "ordinary-query-with-stored-neighbor",
        program,
        &target,
        &limits,
    );
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn descriptor_delegation_does_not_come_from_a_future_loop_iteration() {
    let prefix = format!(
        "{}fn keep<life a>(input:&mut Job<'a>)->i32{{return 7}}",
        metadata_prefix()
    );
    let bad = format!(
        "{prefix}fn main()->i32{{var value=7;var job=make(&value);var current=&job;var iteration=0;var sum=i64(0);while iteration<2{{let description=current.bytes;let exclusive=&mut job;sum=sum+QUERY;current=&*exclusive;iteration=iteration+1}}return i32(sum)}}"
    );
    let fresh = format!(
        "{prefix}fn main()->i32{{var value=7;var job=make(&value);var iteration=0;var sum=i64(0);while iteration<2{{let exclusive=&mut job;let description=exclusive.bytes;sum=sum+QUERY;sum=sum+i64(keep(exclusive))-7;iteration=iteration+1}}return i32(sum)}}"
    );
    let outer = format!(
        "{prefix}fn main()->i32{{var value=7;var job=make(&value);let exclusive=&mut job;let description=exclusive.bytes;var iteration=0;var sum=i64(0);while iteration<2{{sum=sum+QUERY;sum=sum+i64(keep(&mut *exclusive))-7;iteration=iteration+1}}return i32(sum)}}"
    );
    let saved = format!(
        "{prefix}fn main()->i32{{var value=7;var job=make(&value);let initial=&job;var saved=initial.bytes;var iteration=0;var sum=i64(0);while iteration<2{{let exclusive=&mut job;let current=exclusive.bytes;let description=if iteration==0{{current}}else{{saved}};sum=sum+QUERY;saved=current;sum=sum+i64(keep(exclusive))-7;iteration=iteration+1}}return i32(sum)}}"
    );
    for (label, helpers, query) in [
        ("direct", "", "bytes_len(description)"),
        (
            "helper",
            "fn inspect(buffer:ByteBuffer)->i64{return bytes_len(buffer)}",
            "inspect(description)",
        ),
        (
            "relay",
            "fn inspect(buffer:ByteBuffer)->i64{return bytes_len(buffer)}fn relay(buffer:ByteBuffer)->i64{return inspect(buffer)}fn relay_twice(buffer:ByteBuffer)->i64{return relay(buffer)}",
            "relay_twice(description)",
        ),
    ] {
        assert_success(
            &format!("loop-generation-control-{label}"),
            &format!("{helpers}{}", bad.replace("QUERY", "i64(0)")),
        );
        assert_loan(
            &format!("earlier-loop-description-{label}"),
            &format!("{helpers}{}", bad.replace("QUERY", query)),
        );
        assert_success(
            &format!("saved-loop-description-control-{label}"),
            &format!("{helpers}{}", saved.replace("QUERY", "i64(0)")),
        );
        assert_loan(
            &format!("saved-loop-description-{label}"),
            &format!("{helpers}{}", saved.replace("QUERY", query)),
        );
        assert_success(
            &format!("current-loop-description-{label}"),
            &format!("{helpers}{}", fresh.replace("QUERY", query)),
        );
        assert_success(
            &format!("outer-loop-description-{label}"),
            &format!("{helpers}{}", outer.replace("QUERY", query)),
        );
    }
    let control = format!(
        "fn enable_query(buffer:ByteBuffer)->i64{{return bytes_capacity(buffer)}}{}",
        bad.replace(
            "let description=current.bytes;",
            "let description=current.bytes;let copied=description;"
        )
        .replace(
            "let exclusive=&mut job;",
            "let exclusive=&mut job;let observed=i64(0);"
        )
        .replace("QUERY", "observed")
    );
    assert_success("direct-loop-raw-control", &control);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("direct-loop-raw-control", &control, &target, &limits);
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let assigned = |name: &str| {
        let local = f.locals.iter().find(|l| l.name == name).unwrap().id;
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Store { local: l, value } if l == local => Some(value),
                _ => None,
            })
            .unwrap()
    };
    let descriptor = assigned("copied");
    let result = assigned("observed");
    f.blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(result))
        .unwrap()
        .kind = InstructionKind::ByteBufferLen { buffer: descriptor };
    candidate.module_fingerprint = raw.fingerprint();
    let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        a.iter()
            .any(|d| d.message.contains("actual stored target/parent loan")),
        "{a:?}"
    );
    let v =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for errors in [v, w] {
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{errors:?}"
        );
    }
}

#[test]
fn byte_query_effects_cannot_cross_an_unproved_solver_partition() {
    let ordinary = include_str!("byte-query-controls/ordinary-query-with-stored-neighbor.zil");
    let helper = "fn scalar_query(input:i32)->i32{let bytes=bytes_new();let copy=bytes;let observed=i64(0);return input+i32(observed)}";
    let control = format!(
        "{helper}{}",
        ordinary.replace(
            "return *refs.pointer}",
            "return scalar_query(*refs.pointer)}"
        )
    );
    assert_success("scalar-seam-without-query", &control);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("scalar-seam-without-query", &control, &target, &limits);
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "scalar_query")
        .unwrap();
    let buffer = f.locals.iter().find(|l| l.name == "bytes").unwrap().id;
    let observed = f.locals.iter().find(|l| l.name == "observed").unwrap().id;
    let descriptor = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::DescribeLocal { local } if local == buffer => i.result,
            _ => None,
        })
        .unwrap();
    let value = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Store { local, value } if local == observed => Some(value),
            _ => None,
        })
        .unwrap();
    f.blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(value))
        .unwrap()
        .kind = InstructionKind::ByteBufferLen { buffer: descriptor };
    candidate.module_fingerprint = raw.fingerprint();
    let a = zil_analyze::analyze(&raw, &limits).unwrap_err();
    let v =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let w = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for errors in [a, v, w] {
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::Unsupported
                    && d.message
                        == "Byte query requires a complete Direct actual description/read proof"),
            "{errors:?}"
        );
    }
    let changed = control.replace("let observed=i64(0)", "let observed=bytes_len(copy)");
    let report = report_for("scalar-seam-actual-query", &changed);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message
                == "Byte query requires a complete Direct actual description/read proof"),
        "{}",
        report.to_json()
    );
}

#[test]
fn metadata_queries_do_not_grant_shared_heads_mutation_authority() {
    let program = format!(
        "{}fn main()->i32{{let value=7;let job=make(&value);let window=&job;let byte=65u8;let old=window.bytes;let permitted=false;let observed=bytes_len(old);if observed==0 && *window.refs.pointer==7{{return 0}}return 1}}",
        metadata_prefix()
    );
    assert_success("read-only-head-control", &program);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("read-only-head-control", &program, &target, &limits);
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let local = |name: &str| f.locals.iter().find(|l| l.name == name).unwrap().id;
    let assigned = |name: &str| {
        let local = local(name);
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Store { local: l, value } if l == local => Some(value),
                _ => None,
            })
            .unwrap()
    };
    let byte = assigned("byte");
    let descriptor = assigned("old");
    let result = assigned("permitted");
    f.blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(result))
        .unwrap()
        .kind = InstructionKind::ByteBufferPush {
        buffer: descriptor,
        value: byte,
    };
    candidate.module_fingerprint = raw.fingerprint();
    let errors =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for errors in [errors, decoded] {
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-HEAD-PERMISSION"),
            "{errors:?}"
        );
    }
    for (label, changed) in [
        (
            "direct-shared-write",
            program.replace("let permitted=false", "let permitted=bytes_push(old,byte)"),
        ),
        (
            "rich-companion-shared-write",
            separated_formals_program().replace(
                "let observed=bytes_len(buffer)",
                "bytes_push(buffer,65u8);let observed=bytes_len(buffer)",
            ),
        ),
    ] {
        let report = report_for(label, &changed);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-VERIFY-HEAD-PERMISSION"),
            "{}",
            report.to_json()
        );
    }
}
