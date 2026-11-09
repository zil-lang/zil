//! Complete owning carriers retain the actual targets of an inline Record.
use super::*;

const NORMAL: &str = include_str!("owned-package-borrowed-record.zil");
const GETTER: &str = include_str!("qualified-record-getter.zil");
const MAPPING: &str = include_str!("qualified-record-class-mapping.zil");
const FAILURE: &str = include_str!("owned-package-borrowed-record-failure.zil");

#[test]
fn qualified_record_source_raw_and_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [
        ("record-normal", NORMAL),
        ("record-failure", FAILURE),
        ("record-mapping", MAPPING),
        ("record-getter", GETTER),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
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
        let raw = typed_source(name, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
fn qualified_record_source_preserves_external_loan_and_rejects_local_escape() {
    for (label, program) in [
        (
            "live source overwrite",
            NORMAL.replacen(
                "if read(&moved)!=7",
                "value=8\n        if read(&moved)!=7",
                1,
            ),
        ),
        (
            "live Record window prevents parent movement",
            NORMAL.replacen("if read(&moved)!=7", "let child=&moved.refs\n        let again=relay(moved)\n        if *child.pointer!=7", 1),
        ),
        (
            "local source escape",
            NORMAL
                .replacen(
                    "var bytes=bytes_new()",
                    "let local=11\n    var bytes=bytes_new()",
                    1,
                )
                .replacen(
                    "refs:Refs::<'a>{pointer:pointer}",
                    "refs:Refs::<'a>{pointer:&local}",
                    1,
                ),
        ),
        (
            "wrong result lifetime",
            NORMAL.replacen(
                "fn relay<life a>(job:Job<'a>)->Job<'a>",
                "fn relay<life a,life b>(job:Job<'a>)->Job<'b>",
                1,
            ),
        ),
    ] {
        assert_ne!(program, NORMAL);
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn qualified_record_raw_and_wire_reject_wrong_slot_mapping() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = typed_source("record-mapping", NORMAL, &target, &limits);
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    for wrong_class in [false, true] {
        let mut raw = original.clone();
        let mut changed = 0;
        for f in &mut raw.functions {
            for ty in f
                .locals
                .iter_mut()
                .map(|l| &mut l.ty)
                .chain(&mut f.values)
                .chain(std::iter::once(&mut f.return_type))
            {
                let Type::BorrowedStorage { storage, life } = ty else {
                    continue;
                };
                if !matches!(storage.as_ref(), Type::Package { .. }) {
                    continue;
                }
                assert_eq!(life.slots.len(), 1);
                assert_eq!(life.retained.len(), 1);
                if wrong_class {
                    life.retained[0].binder = life.binder_count;
                } else {
                    life.slots[0].path = vec![zil_ir::PlaceProjection::Field(0)];
                }
                changed += 1;
            }
        }
        assert!(changed > 0);
        assert!(zil_analyze::analyze(&raw, &limits).is_err());
        assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[cfg(unix)]
#[test]
fn qualified_record_native_cleanup_on_normal_and_error_exit() {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let shim = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/composed-store-closures/allocator_probe.c");
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program, stdout, allocations) in [
                ("normal", NORMAL, b"R".as_slice(), 1),
                ("error", FAILURE, b"F".as_slice(), 1),
                ("mapping", MAPPING, b"".as_slice(), 0),
                ("getter", GETTER, b"".as_slice(), 0),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-borrowed-record-{}-{backend:?}-{optimization:?}-{label}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let wrapper = dir.join("probe-clang.sh");
                std::fs::write(
                    &wrapper,
                    format!(
                        "#!/bin/sh\nexec {} \"$@\" -x c {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
                        quote(&clang),
                        quote(&shim)
                    ),
                )
                .unwrap();
                std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
                let executable = dir.join("program");
                let report = compile_source(
                    &source(label, program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper),
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
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let run = std::process::Command::new(executable)
                    .env("ZIL_TEST_FAIL_AT", "0")
                    .output()
                    .unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/{label}: {run:?}"
                );
                assert_eq!(run.stdout, stdout);
                let stats = String::from_utf8(run.stderr).unwrap();
                for field in [
                    format!("\"allocated\":{allocations}"),
                    format!("\"frees\":{allocations}"),
                    "\"live\":0".into(),
                    "\"failures\":0".into(),
                ] {
                    assert!(
                        stats.contains(&field),
                        "{backend:?}/{optimization:?}/{label}: {stats}"
                    );
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn qualified_record_raw_and_wire_cannot_return_an_activation_local_target() {
    use zil_ir::InstructionKind;
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let program = NORMAL.replacen(
        "var bytes=bytes_new()",
        "let decoy=11\n    let unused=&decoy\n    var bytes=bytes_new()",
        1,
    );
    let mut raw = typed_source("record-local-forgery", &program, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let decoy = f.locals.iter().find(|l| l.name == "decoy").unwrap().id;
    let address = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { local } if local == decoy)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::MakeAggregate { fields } = &mut i.kind {
            let output = i.result.unwrap();
            if matches!(&f.values[output.0 as usize], Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. }))
            {
                assert_eq!(fields.len(), 1);
                fields[0] = address;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn qualified_record_child_does_not_gain_an_exclusive_window() {
    let program = NORMAL.replacen(
        "fn main()->i32{",
        "fn mutate<life a>(job:&mut Job<'a>){let child=&mut job.refs}\nfn main()->i32{",
        1,
    );
    let report = compile_source(
        &source("record-exclusive-child", &program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
}

#[test]
fn qualified_record_raw_and_wire_reject_an_in_range_child_class_swap() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("record-class-swap", MAPPING, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let mut changed = 0;
    for f in &mut raw.functions {
        for ty in f
            .locals
            .iter_mut()
            .map(|l| &mut l.ty)
            .chain(&mut f.values)
            .chain(std::iter::once(&mut f.return_type))
        {
            if let Type::BorrowedStorage { storage, life } = ty
                && matches!(storage.as_ref(), Type::Package { .. })
            {
                assert_eq!(life.binder_count, 2);
                assert_eq!(life.retained.len(), 2);
                life.retained[0].binder = 1 - life.retained[0].binder;
                changed += 1;
            }
        }
    }
    assert!(changed > 0);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

fn getter_model(width: usize, depth: usize) -> String {
    let fields = (0..width)
        .map(|i| format!("p{i}:&'a i32"))
        .collect::<Vec<_>>()
        .join(";");
    let values = (0..width)
        .map(|i| format!("p{i}:pointer"))
        .collect::<Vec<_>>()
        .join(",");
    let mut program = format!(
        "struct Refs<life a>{{{fields}}}package Job<life a>{{manages bytes:ByteBuffer;refs:Refs<'a>}}"
    );
    let mut carrier = "Job".to_string();
    let mut value = format!("Job::<'a>{{bytes:give bytes,refs:Refs::<'a>{{{values}}}}}");
    let mut path = "job".to_string();
    for level in 0..depth {
        let next = format!("Layer{level}");
        program.push_str(&format!("package {next}<life a>{{inner:{carrier}<'a>}}"));
        value = format!("{next}::<'a>{{inner:{value}}}");
        carrier = next;
        path.push_str(".inner");
    }
    program.push_str(&format!("fn make<life a>(pointer:&'a i32)->{carrier}<'a>{{var bytes=bytes_new();return {value}}}fn read<life a>(job:&{carrier}<'a>)->i32{{let child=&{path}.refs;return *child.p0}}fn main()->i32{{let n=7;let job=make(&n);return read(&job)-7}}"));
    program
}

fn budget_category<T>(result: Result<T, Vec<zil_base::Diagnostic>>) -> ResultCategory {
    match result {
        Ok(_) => ResultCategory::Success,
        Err(errors)
            if errors
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit) =>
        {
            ResultCategory::ResourceLimit
        }
        Err(errors) => panic!("unexpected budget failure: {errors:?}"),
    }
}

fn exact_record_budget(label: &str, mut run: impl FnMut(usize) -> ResultCategory) -> usize {
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    assert_eq!(
        run(high),
        ResultCategory::Success,
        "{label}: default budget"
    );
    while low < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            other => panic!("{label}: unexpected {other:?}"),
        }
    }
    for _ in 0..2 {
        assert_eq!(run(low), ResultCategory::Success, "{label}: N/fresh");
        assert_eq!(
            run(low - 1),
            ResultCategory::ResourceLimit,
            "{label}: N-1/fresh"
        );
        assert_eq!(run(low), ResultCategory::Success, "{label}: retry");
    }
    println!("{label}: N={low}, N-1=ResourceLimit, fresh/retry=Pass");
    low
}

#[test]
fn qualified_record_getter_width_and_depth_budgets_are_exact_and_fresh() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    // Historical dense-window limits remain probes. Word rows and paid path
    // indexes preserve the proofs while changing complete request costs.
    let prior = [
        [3220, 32911, 45276, 45276, 45276],
        [6111, 97665, 142372, 142372, 142372],
        [19476, 159886, 195052, 195052, 195052],
        [37767, 415174, 542990, 542990, 542990],
    ];
    let expected = [
        [3220, 31059, 43867, 43867, 43867],
        [6111, 93974, 137924, 137924, 137924],
        [19476, 155222, 189511, 189511, 189511],
        [37767, 411827, 549673, 549673, 549673],
    ];
    for (case, (width, depth)) in [(1, 0), (8, 0), (1, 2), (8, 2)].into_iter().enumerate() {
        let program = getter_model(width, depth);
        let input = source("record-getter-budget", &program);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &input,
            &syntax,
            &default,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: input.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &default).unwrap();
        let plan = zil_analyze::analyze(&raw, &default).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &default).unwrap();
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..default.clone()
        };
        let types = exact_record_budget(
            &format!("Record getter Types width={width} depth={depth}"),
            |n| budget_category(zil_types::check(&core, &target, &limits(n))),
        );
        let analyze = exact_record_budget(
            &format!("Record getter Analyze width={width} depth={depth}"),
            |n| budget_category(zil_analyze::analyze(&raw, &limits(n))),
        );
        let verify = exact_record_budget(
            &format!("Record getter Verify width={width} depth={depth}"),
            |n| {
                budget_category(zil_verify::verify_module(
                    raw.clone(),
                    plan.clone(),
                    &target,
                    &limits(n),
                ))
            },
        );
        let decoded = exact_record_budget(
            &format!("Record getter Wire width={width} depth={depth}"),
            |n| budget_category(zil_verify::verify_module_bytes(&wire, &target, &limits(n))),
        );
        let whole = exact_record_budget(
            &format!("Record getter Source width={width} depth={depth}"),
            |n| {
                compile_source(
                    &input,
                    &CompileOptions {
                        stop_after: Stage::Verify,
                        limits: limits(n),
                        ..CompileOptions::default()
                    },
                )
                .status
            },
        );
        let mut session = zil_driver::CompilationSession::default();
        for (n, status) in [
            (whole, ResultCategory::Success),
            (whole - 1, ResultCategory::ResourceLimit),
            (whole, ResultCategory::Success),
        ] {
            let options = CompileOptions {
                stop_after: Stage::Verify,
                limits: limits(n),
                ..CompileOptions::default()
            };
            assert_eq!(session.compile_source(&input, &options).status, status);
            assert_eq!(
                zil_driver::CompilationSession::default()
                    .compile_source(&input, &options)
                    .status,
                status
            );
            assert_eq!(session.active_query_count(), 0);
        }
        assert_eq!(types, prior[case][0]);
        for (phase, status) in [
            budget_category(zil_analyze::analyze(&raw, &limits(prior[case][1]))),
            budget_category(zil_verify::verify_module(
                raw.clone(),
                plan.clone(),
                &target,
                &limits(prior[case][2]),
            )),
            budget_category(zil_verify::verify_module_bytes(
                &wire,
                &target,
                &limits(prior[case][3]),
            )),
            compile_source(
                &input,
                &CompileOptions {
                    limits: limits(prior[case][4]),
                    ..CompileOptions::default()
                },
            )
            .status,
        ]
        .into_iter()
        .enumerate()
        {
            let category = if prior[case][phase + 1] < expected[case][phase + 1] {
                ResultCategory::ResourceLimit
            } else {
                ResultCategory::Success
            };
            assert_eq!(
                status, category,
                "historical window limit: width={width}, depth={depth}, phase={phase}"
            );
        }
        boundaries.push([types, analyze, verify, decoded, whole]);
    }
    // Fixed complete-consumer boundaries catch a missing scan or snapshot
    // charge as well as an increase; N-1 is checked independently above.
    assert_eq!(boundaries.as_slice(), expected.as_slice());
    for kernel in 0..5 {
        assert!(
            boundaries[1][kernel] > boundaries[0][kernel],
            "wide Record must cost more: {boundaries:?}"
        );
        assert!(
            boundaries[2][kernel] > boundaries[0][kernel],
            "deep getter must cost more: {boundaries:?}"
        );
        assert!(
            boundaries[3][kernel] > boundaries[1][kernel]
                && boundaries[3][kernel] > boundaries[2][kernel],
            "wide deep getter must retain both costs: {boundaries:?}"
        );
    }
}

fn record_window_programs() -> Vec<(&'static str, String)> {
    let refs = "struct Refs<life a>{pointer:&'a i32}fn make<life a>(p:&'a i32)->Refs<'a>{return Refs::<'a>{pointer:p}}";
    let main = |creation: &str, window: &str, read: &str| {
        format!(
            "fn main()->i32{{var value=7;var spare=0;{creation}let window={window};spare=11;if *{read}==7{{return 0}}return 1}}"
        )
    };
    let identity_refs =
        "fn identity<life a,life b>(input:&'a Refs<'b>)->&'a Refs<'b>{return input}";
    let job = GETTER.split("fn main").next().unwrap();
    vec![
        (
            "record-window",
            format!(
                "{refs}{}",
                main("let refs=make(&value);", "&refs", "window.pointer")
            ),
        ),
        (
            "record-identity-window",
            format!(
                "{refs}{identity_refs}{}",
                main(
                    "let refs=make(&value);",
                    "identity(&refs)",
                    "window.pointer"
                )
            ),
        ),
        (
            "record-child-window",
            format!(
                "{job}{}",
                main("let job=make(&value);", "&job.refs", "window.pointer")
            ),
        ),
        (
            "record-parent-window",
            format!(
                "{job}{}",
                main("let job=make(&value);", "&job", "window.refs.pointer")
            ),
        ),
        (
            "record-child-identity-window",
            format!(
                "{job}{identity_refs}{}",
                main(
                    "let job=make(&value);",
                    "identity(&job.refs)",
                    "window.pointer"
                )
            ),
        ),
        (
            "whole-package-identity-window",
            whole_package_identity_program(),
        ),
        (
            "whole-package-second-input",
            whole_package_second_input_program(),
        ),
        (
            "whole-package-merged-input-false",
            whole_package_merged_input_program(false),
        ),
        (
            "whole-package-merged-input-true",
            whole_package_merged_input_program(true),
        ),
    ]
}

#[test]
fn qualified_record_windows_retain_actual_leaf_loans() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (label, program) in record_window_programs() {
        let good = compile_source(&source(label, &program), &CompileOptions::default());
        assert_eq!(
            good.status,
            ResultCategory::Success,
            "{label}: {}",
            good.to_json()
        );
        assert_eq!(good.verification_runs, 1);
        let rejected = program.replacen("spare=11;", "value=11;", 1);
        assert_ne!(rejected, program);
        let report = compile_source(&source(label, &rejected), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            matches!(
                report.phase,
                zil_base::Phase::Analyze | zil_base::Phase::Verify
            ),
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

        // Preserve the successful candidate, all metadata and the instruction
        // shape. Only retarget the final scalar write from spare to the actual
        // source held by the live window. Each core reconstructs its own loans.
        let original = typed_source(label, &program, &target, &limits);
        let mut plan = zil_analyze::analyze(&original, &limits).unwrap();
        zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
        let control = zil_ir::wire::encode_module_candidate(&original, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&control, &target, &limits).unwrap();
        let mut raw = original;
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let spare = main.locals.iter().find(|l| l.name == "spare").unwrap().id;
        let value = main.locals.iter().find(|l| l.name == "value").unwrap().id;
        let write = main.blocks.iter_mut().flat_map(|b| &mut b.instructions).filter(|i|
            matches!(i.kind, zil_ir::InstructionKind::Store { local, .. } if local == spare)
        ).last().unwrap();
        let zil_ir::InstructionKind::Store { local, .. } = &mut write.kind else {
            unreachable!()
        };
        *local = value;
        // Refresh only byte identity; every candidate borrowing claim stays unchanged.
        plan.module_fingerprint = raw.fingerprint();
        let rejected = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            rejected
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{label}: {rejected:?}"
        );
        let rejected =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            rejected
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{label}: {rejected:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let rejected = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            rejected
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{label}: {rejected:?}"
        );
    }
    let sibling = "struct Refs<life a,life b>{left:&'a i32;right:&'b i32}fn make<life a,life b>(left:&'a i32,right:&'b i32)->Refs<'a,'b>{return Refs::<'a,'b>{left:left,right:right}}fn main()->i32{var left=7;var right=9;let refs=make(&left,&right);let window=&refs;let pointer=window.left;right=11;return *pointer}";
    let report = compile_source(
        &source("ended-window-independent-sibling", sibling),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn qualified_record_window_native_values_remain_complete() {
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in record_window_programs() {
                let directory = std::env::temp_dir().join(format!(
                    "zil-record-window-{}-{label}-{backend:?}-{optimization:?}",
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
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let output = std::process::Command::new(&executable).output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{label}/{backend:?}/{optimization:?}: {output:?}"
                );
                assert!(output.stdout.is_empty(), "{output:?}");
                runs += 1;
            }
        }
    }
    assert_eq!(
        runs,
        record_window_programs().len() * (2 + 2 * usize::from(cfg!(feature = "llvm-api")))
    );
}

#[test]
fn qualified_parent_identity_transfers_actual_resource_manager() {
    let program = whole_package_identity_program();
    let report = compile_source(
        &source("parent-identity-resource-boundary", &program),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

fn whole_package_identity_program() -> String {
    let job = GETTER.split("fn main").next().unwrap();
    format!(
        "{job}fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{{return input}}fn main()->i32{{var value=7;var spare=0;let job=make(&value);let window=identity(&job);spare=11;if *window.refs.pointer==7{{return 0}}return 1}}"
    )
}

fn whole_package_second_input_program() -> String {
    let job = GETTER.split("fn main").next().unwrap();
    format!(
        "{job}fn pick_second<life a,life b>(left:&'a Job<'b>,right:&'a Job<'b>)->&'a Job<'b>{{return right}}fn discard<life b>(job:Job<'b>){{}}fn main()->i32{{var ignored=3;var value=7;var spare=0;let left=make(&ignored);let right=make(&value);let window=pick_second(&left,&right);discard(left);ignored=11;spare=11;if *window.refs.pointer==7{{return 0}}return 1}}"
    )
}
fn whole_package_merged_input_program(flag: bool) -> String {
    let job = GETTER.split("fn main").next().unwrap();
    format!(
        "{job}fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{{return input}}fn flag()->bool{{return {flag}}}fn main()->i32{{var other=3;var value=7;var spare=0;let left=make(&other);let right=make(&value);let chosen=if flag(){{&left}}else{{&right}};let window=identity(chosen);spare=11;let expected=if flag(){{3}}else{{7}};if *window.refs.pointer==expected{{return 0}}return 1}}"
    )
}

#[test]
fn qualified_package_manager_reads_follow_direct_and_returned_windows() {
    let returned = whole_package_identity_program().replace(
        "if *window.refs.pointer==7",
        "if bytes_len(window.bytes)==0 && *window.refs.pointer==7",
    );
    let direct = returned.replace("let window=identity(&job)", "let window=&job");
    for (label, program) in [
        ("direct-manager-read", direct),
        ("returned-manager-read", returned),
    ] {
        let report = compile_source(&source(label, &program), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let invalidated = program.replace("spare=11;", "value=11;");
        let rejected = compile_source(&source(label, &invalidated), &CompileOptions::default());
        assert_eq!(
            rejected.status,
            ResultCategory::SafeReject,
            "{}",
            rejected.to_json()
        );
        assert!(
            rejected
                .diagnostics
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")
                    || d.message.contains("live incompatible loan")),
            "{}",
            rejected.to_json()
        );
    }
}

#[test]
fn qualified_package_return_bridge_keeps_its_whole_input_boundary() {
    let job = GETTER.split("fn main").next().unwrap();
    let multiple = format!(
        "{job}fn choose<life a,life b>(flag:bool,left:&'a Job<'b>,right:&'a Job<'b>)->&'a Job<'b>{{if flag{{return left}}return right}}fn main()->i32{{var a=3;var b=7;let left=make(&a);let right=make(&b);let window=choose(false,&left,&right);if *window.refs.pointer==7{{return 0}}return 1}}"
    );
    let outer = format!(
        "{job}package Outer<life b>{{inner:Job<'b>}}fn wrap<life b>(job:Job<'b>)->Outer<'b>{{return Outer::<'b>{{inner:job}}}}fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{{return input}}fn inspect<life a,life b>(parent:&'a Outer<'b>)->i32{{let input=&parent.inner;let window=identity(input);return *window.refs.pointer}}fn main()->i32{{var value=7;let parent=wrap(make(&value));return inspect(&parent)-7}}"
    );
    // A projection is a legitimate existing address. The limitation is the
    // new whole-input description bridge, rather than its source grammar.
    let control = outer.replace("let window=identity(input)", "let window=input");
    let report = compile_source(
        &source("projected-manager-direct-control", &control),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for (label, program) in [
        ("multiple-callee-inputs", multiple),
        ("projected-caller-manager", outer),
    ] {
        let report = compile_source(&source(label, &program), &CompileOptions::default());
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
                    == "qualified Package return requires one actual whole shared input"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn qualified_package_return_current_request_rechecks_selection_and_conflicts() {
    let program = whole_package_second_input_program();
    let wrong_owner = program.replace("discard(left)", "discard(right)");
    let wrong_source = program.replace("spare=11;", "value=11;");
    let changed_body = program.replace("return right", "return left");
    let mut session = zil_driver::CompilationSession::default();
    for text in [
        &program,
        &wrong_owner,
        &program,
        &wrong_source,
        &program,
        &changed_body,
        &program,
    ] {
        for report in [
            session.compile_source(
                &source("current-manager-selection", text),
                &CompileOptions::default(),
            ),
            compile_source(
                &source("current-manager-selection", text),
                &CompileOptions::default(),
            ),
        ] {
            if text == &program {
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
                    matches!(
                        report.phase,
                        zil_base::Phase::Analyze | zil_base::Phase::Verify
                    ),
                    "{}",
                    report.to_json()
                );
                if text == &wrong_source {
                    assert!(
                        report
                            .diagnostics
                            .iter()
                            .any(|d| d.message.contains("actual stored target/parent loan")
                                || d.message.contains("live incompatible loan")),
                        "{}",
                        report.to_json()
                    );
                } else {
                    assert_eq!(report.phase, zil_base::Phase::Analyze);
                    assert!(report.diagnostics.iter().any(|d|
                        d.code == "E-ANALYZE-IR" && d.message == "stored target proof: stored/address producer has incomplete actual target facts"),
                        "{}", report.to_json());
                }
            }
        }
        assert_eq!(session.active_query_count(), 0);
    }
}

fn package_return_budget_model(width: usize, depth: usize) -> String {
    let fields = (0..width)
        .map(|n| format!("field{n}:i32;"))
        .collect::<String>();
    let values = (0..width)
        .map(|n| format!(",field{n}:0"))
        .collect::<String>();
    let mut program = format!(
        "struct Refs<life b>{{pointer:&'b i32;{fields}}}package Job<life b>{{manages bytes:ByteBuffer;refs:Refs<'b>}}fn make<life b>(input:&'b i32)->Job<'b>{{var bytes=bytes_new();return Job::<'b>{{bytes:give bytes,refs:Refs::<'b>{{pointer:input{values}}}}}}}fn relay0<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{{return input}}"
    );
    for n in 1..=depth {
        program.push_str(&format!(
            "fn relay{n}<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{{return relay{}(input)}}",
            n - 1
        ));
    }
    program.push_str(&format!("fn main()->i32{{let value=7;let job=make(&value);let window=relay{depth}(&job);if *window.refs.pointer==7{{return 0}}return 1}}"));
    program
}

#[test]
fn qualified_package_return_budget_covers_signatures_chains_and_current_requests() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for (width, depth) in [(1, 0), (8, 0), (1, 2), (8, 2)] {
        let program = package_return_budget_model(width, depth);
        let input = source("whole-package-return-budget", &program);
        let raw = typed_source("whole-package-return-budget", &program, &target, &default);
        let plan = zil_analyze::analyze(&raw, &default).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &default).unwrap();
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..default.clone()
        };
        let analyze = exact_record_budget(
            &format!("Package return Analyze width={width} depth={depth}"),
            |n| budget_category(zil_analyze::analyze(&raw, &limits(n))),
        );
        let verify = exact_record_budget(
            &format!("Package return Verify width={width} depth={depth}"),
            |n| {
                budget_category(zil_verify::verify_module(
                    raw.clone(),
                    plan.clone(),
                    &target,
                    &limits(n),
                ))
            },
        );
        let decoded = exact_record_budget(
            &format!("Package return Wire width={width} depth={depth}"),
            |n| budget_category(zil_verify::verify_module_bytes(&wire, &target, &limits(n))),
        );
        let whole = exact_record_budget(
            &format!("Package return Source width={width} depth={depth}"),
            |n| {
                compile_source(
                    &input,
                    &CompileOptions {
                        limits: limits(n),
                        ..CompileOptions::default()
                    },
                )
                .status
            },
        );
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
    for kernel in 0..4 {
        assert!(
            boundaries[1][kernel] > boundaries[0][kernel],
            "wide signatures must cost more: {boundaries:?}"
        );
        assert!(
            boundaries[2][kernel] > boundaries[0][kernel],
            "real return chains must cost more: {boundaries:?}"
        );
        assert!(
            boundaries[3][kernel] > boundaries[1][kernel]
                && boundaries[3][kernel] > boundaries[2][kernel],
            "{boundaries:?}"
        );
    }
}

#[path = "borrowed_byte_queries.rs"]
mod byte_queries;
