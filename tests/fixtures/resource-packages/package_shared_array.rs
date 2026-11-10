//! Owning Package fields retain exact Array sources without new cleanup debt.
use super::*;
const PACKAGE_ARRAY: &str = include_str!("owned-package-shared-address-array.zil");

#[test]
fn source_raw_and_wire_keep_package_field_and_element_ordinals_distinct() {
    let permuted = PACKAGE_ARRAY
        .replacen("items:[first,second]", "items:[second,first]", 1)
        .replacen("first=11", "second=11", 1)
        .replacen("return *pointer-9", "return *pointer-7", 1);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (label, program) in [("ordinary", PACKAGE_ARRAY), ("permuted", permuted.as_str())] {
        let report = compile_source(
            &source(label, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("Package Array {label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = typed_source(label, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
fn package_array_views_preserve_actual_parent_and_each_live_source() {
    let local_escape = PACKAGE_ARRAY.replacen(
        "return Job::<'b>{bytes:give bytes,items:[first,second]}",
        "let local=3;return Job::<'b>{bytes:give bytes,items:[first,&local]}",
        1,
    );
    for (label, program) in [
        (
            "selected source",
            PACKAGE_ARRAY.replacen("first=11", "second=11", 1),
        ),
        (
            "whole window first source",
            PACKAGE_ARRAY.replacen(
                "let pointer=window.items[1]",
                "first=11;let pointer=window.items[1]",
                1,
            ),
        ),
        (
            "whole window second source",
            PACKAGE_ARRAY.replacen(
                "let pointer=window.items[1]",
                "second=11;let pointer=window.items[1]",
                1,
            ),
        ),
        ("short local element", local_escape),
    ] {
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("Package Array negative {label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
}

fn cleanup_programs() -> [(&'static str, String); 2] {
    let definitions = "extern \"C\" fn putchar(c:i32)->i32;resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}enum Fault{Stopped}package Job<life b>{manages bytes:ByteBuffer;manages mark:Mark;items:[&'b i32;2]}fn make<life b>(first:&'b i32,second:&'b i32)->Job<'b>{var bytes=bytes_new();if bytes_reserve(bytes,1){bytes_push(bytes,65u8)}return Job::<'b>{bytes:give bytes,mark:Mark{code:82},items:[first,second]}}fn identity<life a,life b>(input:&'a Job<'b>)->&'a Job<'b>{return input}fn relay<life b>(job:Job<'b>)->Job<'b>{return job}";
    [
        (
            "normal",
            format!(
                "{definitions}fn main()->i32{{var first=7;var second=9;let job=relay(make(&first,&second));let window=identity(&job);let pointer=window.items[1];first=11;return *pointer-9}}"
            ),
        ),
        (
            "error",
            format!(
                "{definitions}fn stop<life b>(first:&'b i32,second:&'b i32)->Fault!i32{{let job=relay(make(first,second));let window=identity(&job);if *window.items[1]!=9{{return 3}}return Fault.Stopped}}fn main()->i32{{var first=7;var second=9;match stop(&first,&second){{.ok(_)=>{{return 1}},.err(_)=>{{}}}}first=11;second=12;return first+second-23}}"
            ),
        ),
    ]
}

#[test]
fn native_package_array_normal_error_and_allocation_failure_cleanup_once() {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let shim = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/composed-store-closures/allocator_probe.c");
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in cleanup_programs() {
                let dir = std::env::temp_dir().join(format!(
                    "zil-package-array-{}-{backend:?}-{optimization:?}-{label}",
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
                    &source(label, &program),
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
                    "{backend:?}/{optimization:?}/{label}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                for fail_at in [0, 1] {
                    let run = std::process::Command::new(&executable)
                        .env("ZIL_TEST_FAIL_AT", fail_at.to_string())
                        .output()
                        .unwrap();
                    assert_eq!(
                        run.status.code(),
                        Some(0),
                        "{backend:?}/{optimization:?}/{label}/{fail_at}: {run:?}"
                    );
                    assert_eq!(run.stdout, b"R");
                    let stats = String::from_utf8(run.stderr).unwrap();
                    let allocations = usize::from(fail_at == 0);
                    for field in [
                        format!("\"allocated\":{allocations}"),
                        format!("\"frees\":{allocations}"),
                        format!("\"failures\":{fail_at}"),
                        "\"live\":0".into(),
                    ] {
                        assert!(
                            stats.contains(&field),
                            "{backend:?}/{optimization:?}/{label}/{fail_at}: {stats}"
                        );
                    }
                    runs += 1;
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
    assert_eq!(runs, 8 + 8 * usize::from(cfg!(feature = "llvm-api")));
    println!("Package Array normal/error/failure native executions: {runs}");
}

#[test]
fn raw_wire_reject_actual_source_writes_and_same_upper_ordinal_substitution() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let baseline = PACKAGE_ARRAY
        .replacen("var first=7", "var spare=0;var first=7", 1)
        .replacen("first=11", "spare=11", 1);
    for mode in [
        "selected write",
        "whole window first",
        "whole window second",
        "swapped ordinal",
    ] {
        let program = if mode.starts_with("whole window") {
            baseline
                .replacen(
                    "let pointer=window.items[1]",
                    "spare=11;let pointer=window.items[1]",
                    1,
                )
                .replacen("\n    spare=11\n", "\n", 1)
        } else if mode == "swapped ordinal" {
            PACKAGE_ARRAY.to_string()
        } else {
            baseline.clone()
        };
        let mut raw = typed_source(mode, &program, &target, &limits);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        if mode == "swapped ordinal" {
            let mut changes = 0;
            for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let zil_ir::InstructionKind::LoadStoredBorrow { path, .. } = &mut i.kind {
                    assert_eq!(path.as_slice(), &[zil_ir::PlaceProjection::Field(1)]);
                    path[0] = zil_ir::PlaceProjection::Field(0);
                    changes += 1;
                }
            }
            assert_eq!(changes, 1);
        } else {
            let name = if mode == "whole window first" {
                "first"
            } else {
                "second"
            };
            let target_local = main.locals.iter().find(|l| l.name == name).unwrap().id;
            let spare = main.locals.iter().find(|l| l.name == "spare").unwrap().id;
            let store = main
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .filter(
                    |i| matches!(i.kind,zil_ir::InstructionKind::Store{local,..} if local==spare),
                )
                .last()
                .unwrap();
            let zil_ir::InstructionKind::Store { local, .. } = &mut store.kind else {
                unreachable!()
            };
            *local = target_local;
        }
        plan.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            analyzed
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{mode}: {analyzed:?}"
        );
        let verified =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            verified
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{mode}: {verified:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            decoded
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{mode}: {decoded:?}"
        );
    }
}

#[test]
fn raw_wire_reject_missing_element_rows_sources_and_metadata_only_elements() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = typed_source("package-array-layout", PACKAGE_ARRAY, &target, &limits);
    let candidate = zil_analyze::analyze(&original, &limits).unwrap();
    for mode in [
        "missing slot",
        "missing constructor source",
        "metadata Element",
    ] {
        let mut raw = original.clone();
        let make = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
        match mode {
            "missing slot" => {
                let Type::BorrowedStorage { life, .. } = &mut make.return_type else {
                    panic!("qualified Package")
                };
                assert_eq!(life.slots.len(), 2);
                life.slots.pop();
            }
            "missing constructor source" => {
                let constructor = make
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, zil_ir::InstructionKind::MakeAggregate { .. }))
                    .unwrap();
                let zil_ir::InstructionKind::MakeAggregate { fields } = &mut constructor.kind
                else {
                    unreachable!()
                };
                assert_eq!(fields.len(), 2);
                fields.pop();
            }
            _ => make.storage_life.as_mut().unwrap().classes.push(
                zil_ir::SignatureLifeClassBinding {
                    position: zil_ir::SignatureLifeClass {
                        base: zil_ir::StorageBase::Result,
                        path: vec![zil_ir::LifeTypeProjection::Element],
                        class: 0,
                    },
                    binder: zil_ir::StorageBinder::Invocation(0),
                },
            ),
        }
        let mut plan = candidate.clone();
        plan.module_fingerprint = raw.fingerprint();
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "{mode}: Analyze"
        );
        assert!(
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err(),
            "{mode}: Verify"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&wire, &target, &limits).is_err(),
            "{mode}: Wire"
        );
    }
}

#[test]
fn raw_wire_rederive_the_whole_package_call_input_and_its_array_sources() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let program = PACKAGE_ARRAY
        .replacen("var first=7", "var ignored=3;var first=7", 1)
        .replacen(
            "let job=relay",
            "let decoy=make(&ignored,&ignored);let unused=&decoy;let job=relay",
            1,
        )
        .replacen("first=11", "ignored=11;first=11", 1);
    let mut raw = typed_source("package-array-actual-input", &program, &target, &limits);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let identity = raw
        .functions
        .iter()
        .find(|f| f.name == "identity")
        .unwrap()
        .id;
    let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let decoy = main.locals.iter().find(|l| l.name == "decoy").unwrap().id;
    let address = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind,zil_ir::InstructionKind::BorrowLocal{local} if local==decoy)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for i in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let zil_ir::InstructionKind::Call {
            function,
            arguments,
        } = &mut i.kind
            && *function == identity
        {
            assert_eq!(arguments.len(), 1);
            arguments[0] = address;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    plan.module_fingerprint = raw.fingerprint();
    let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message.contains("actual stored target/parent loan")),
        "{analyzed:?}"
    );
    let verified =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        verified
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{verified:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap_err()
            .iter()
            .any(|d| d.message.contains("live incompatible loan"))
    );
}

fn package_budget_model(length: usize, width: usize, depth: usize) -> String {
    super::array_budget_model(length, width, depth, true)
        .replacen(
            "struct Batch<life a>{items:",
            "package Batch<life a>{manages bytes:ByteBuffer;items:",
            1,
        )
        .replacen(
            "return Batch::<'a>{items:",
            "var bytes=bytes_new();return Batch::<'a>{bytes:give bytes,items:",
            1,
        )
}

#[test]
fn package_array_slots_pointee_and_parent_snapshot_budgets_are_exact_and_fresh() {
    let default = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for (length, width, depth) in [(2, 1, 0), (8, 1, 0), (2, 8, 0), (2, 1, 3)] {
        let program = package_budget_model(length, width, depth);
        let input = source("package-array-budget", &program);
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
        let label = format!("Package Array len={length} width={width} depth={depth}");
        let types = super::exact_array_budget(&format!("{label} Types"), |n| {
            super::array_budget_category(zil_types::check(&core, &target, &limits(n)))
        });
        let analyze = super::exact_array_budget(&format!("{label} Analyze"), |n| {
            super::array_budget_category(zil_analyze::analyze(&raw, &limits(n)))
        });
        let verify = super::exact_array_budget(&format!("{label} Verify"), |n| {
            super::array_budget_category(zil_verify::verify_module(
                raw.clone(),
                plan.clone(),
                &target,
                &limits(n),
            ))
        });
        let decoded = super::exact_array_budget(&format!("{label} Wire"), |n| {
            super::array_budget_category(zil_verify::verify_module_bytes(
                &wire,
                &target,
                &limits(n),
            ))
        });
        let whole = super::exact_array_budget(&format!("{label} Source"), |n| {
            compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Verify,
                    limits: limits(n),
                    ..CompileOptions::default()
                },
            )
            .status
        });
        let mut session = zil_driver::CompilationSession::default();
        for (n, expected) in [
            (whole, ResultCategory::Success),
            (whole - 1, ResultCategory::ResourceLimit),
            (whole, ResultCategory::Success),
        ] {
            let options = CompileOptions {
                stop_after: Stage::Verify,
                limits: limits(n),
                ..CompileOptions::default()
            };
            assert_eq!(session.compile_source(&input, &options).status, expected);
            assert_eq!(
                zil_driver::CompilationSession::default()
                    .compile_source(&input, &options)
                    .status,
                expected
            );
            assert_eq!(session.active_query_count(), 0);
        }
        assert_eq!(verify, decoded);
        boundaries.push([types, analyze, verify, decoded, whole]);
    }
    // Only actual equation work changes: balanced parent visits and smaller
    // representative components. Keep the previous exact receipt alongside
    // the new N/N-1 and fresh/warm checks; ownership and Types fees are intact.
    let previous = [
        [4018, 50794, 71998, 71998, 71998],
        [5206, 111074, 160717, 160717, 160717],
        [7021, 81557, 113032, 113032, 113032],
        [9484, 87685, 122380, 122380, 122380],
    ];
    for (before, after) in previous.iter().zip(&boundaries) {
        assert_eq!(before[0], after[0]);
        for kernel in 1..5 {
            assert!(after[kernel] < before[kernel]);
        }
    }
    assert_eq!(
        boundaries,
        vec![
            [4018, 40879, 63434, 63434, 63434],
            [5206, 85055, 132989, 132989, 132989],
            [7021, 58776, 90759, 90759, 90759],
            [9484, 71113, 106737, 106737, 106737],
        ]
    );
    for kernel in 0..5 {
        for row in 1..4 {
            assert!(
                boundaries[row][kernel] > boundaries[0][kernel],
                "actual slots/width/depth require more work: {boundaries:?}"
            );
        }
    }
    println!("Package Array complete boundaries: {boundaries:?}");
}

#[test]
fn byte_array_elements_keep_the_existing_relocation_rejection_in_both_kernels() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for operation in [
        "bytes_reserve(bytes,1)",
        "bytes_push(bytes,65u8)",
        "bytes_truncate(bytes,0)",
    ] {
        let model = |pointee: &str| {
            format!(
                "package Job<life b>{{manages bytes:ByteBuffer;items:[&'b {pointee};2]}}fn make<life b>(first:&'b {pointee},second:&'b {pointee})->Job<'b>{{var bytes=bytes_new();{operation};return Job::<'b>{{bytes:give bytes,items:[first,second]}}}}fn main()->i32{{return 0}}"
            )
        };
        let valid = typed_source("package-array-relocation", &model("i32"), &target, &limits);
        let mut plan = zil_analyze::analyze(&valid, &limits).unwrap();
        zil_verify::verify_module(valid, plan.clone(), &target, &limits).unwrap();
        let program = model("u8");
        let report = compile_source(
            &source("package-array-relocation", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{operation}: {}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d
                .message
                .contains("ByteBuffer relocation needs projected stored byte target invalidation")),
            "{}",
            report.to_json()
        );
        let raw = typed_source("package-array-relocation", &program, &target, &limits);
        plan.module_fingerprint = raw.fingerprint();
        let verify =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert!(
            verify.iter().any(|d| d
                .message
                .contains("ByteBuffer relocation needs projected stored byte target invalidation")),
            "{operation}: {verify:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&wire, &target, &limits)
                .unwrap_err()
                .iter()
                .any(|d| d.message.contains(
                    "ByteBuffer relocation needs projected stored byte target invalidation"
                ))
        );
    }
}

const PACKAGE_MAPPING: &str = include_str!("owned-package-shared-array-class-mapping.zil");

#[test]
fn two_nonidentity_lifetime_arguments_keep_their_own_package_array_sources() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let raw = typed_source("package-array-mapping", PACKAGE_MAPPING, &target, &limits);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for (label, program, expected) in [
        (
            "mapped source",
            PACKAGE_MAPPING.to_string(),
            ResultCategory::Success,
        ),
        (
            "actual mapped source write",
            PACKAGE_MAPPING.replacen("first=11", "second=11", 1),
            ResultCategory::SafeReject,
        ),
    ] {
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{label}: {}", report.to_json());
    }
    let mut forged = raw;
    let row = forged
        .functions
        .iter_mut()
        .find(|f| f.name == "make")
        .unwrap()
        .storage_life
        .as_mut()
        .unwrap()
        .classes
        .iter_mut()
        .find(|r| r.position.base == zil_ir::StorageBase::Result)
        .unwrap();
    let zil_ir::StorageBinder::Invocation(actual) = row.binder else {
        panic!("actual invocation lifetime")
    };
    assert!(actual < 2);
    row.binder = zil_ir::StorageBinder::Invocation(1 - actual);
    plan.module_fingerprint = forged.fingerprint();
    assert!(zil_analyze::analyze(&forged, &limits).is_err());
    assert!(zil_verify::verify_module(forged.clone(), plan.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&forged, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

#[test]
fn native_package_array_lifetime_permutation_matches_each_backend() {
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-package-array-mapping-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_source(
                &source("package-array-mapping", PACKAGE_MAPPING),
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
            let run = std::process::Command::new(executable).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
            std::fs::remove_dir_all(dir).unwrap();
            runs += 1;
        }
    }
    assert_eq!(runs, 2 + 2 * usize::from(cfg!(feature = "llvm-api")));
    println!("Package Array life permutation native executions: {runs}");
}

#[test]
fn owning_package_stays_affine_and_its_live_view_pins_the_parent() {
    let duplicate = PACKAGE_ARRAY.replacen(
        "let window=identity(&job)",
        "let duplicate=job;let window=identity(&job)",
        1,
    );
    let moved_parent = PACKAGE_ARRAY.replacen(
        "let pointer=window.items[1]",
        "let moved=relay(job);let pointer=window.items[1]",
        1,
    );
    for (label, program) in [
        ("owning Package cannot Copy", duplicate),
        ("live view pins whole owner", moved_parent),
    ] {
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
    let program = PACKAGE_ARRAY
        .replacen("fn main()", "fn discard<life b>(job:Job<'b>){}fn main()", 1)
        .replacen("first=11", "discard(job);first=11", 1);
    let report = compile_source(
        &source(
            "detached shared leaf survives owning container cleanup",
            &program,
        ),
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
}

fn wide_package_source_raw_and_wire(program: &str, fingerprint: &str, snapshot: bool) {
    let input = source("large-package-array-default-work", program);
    assert_eq!(input.fingerprint().to_hex(), fingerprint);
    let report = compile_source(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    println!("512-address owning Package Source: {}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = typed_source(
        "large-package-array-default-work",
        program,
        &target,
        &limits,
    );
    let make = raw.functions.iter().find(|f| f.name == "make").unwrap();
    let zil_ir::Type::BorrowedStorage { storage, life } = &make.return_type else {
        panic!("the actual owning Package retains its complete qualification");
    };
    assert!(matches!(storage.as_ref(), zil_ir::Type::Package { .. }));
    assert_eq!(
        life.slots.len(),
        512,
        "real address slots rather than scalar padding"
    );
    for (ordinal, slot) in life.slots.iter().enumerate() {
        assert_eq!(
            slot.path,
            [
                zil_ir::PlaceProjection::Field(1),
                zil_ir::PlaceProjection::Field(ordinal as u32)
            ]
        );
        assert_eq!(slot.mode, zil_ir::StoredBorrowMode::SharedAddress);
    }
    if snapshot {
        assert!(raw.functions.iter().any(|f| f.name == "discard"));
        let main = raw.functions.iter().find(|f| f.name == "main").unwrap();
        assert!(
            main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
                matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. })
                    && i.result.is_some_and(|id| {
                        matches!(&main.values[id.0 as usize],
                    zil_ir::Type::Array { element, length: 512 } if matches!(element.as_ref(),
                        zil_ir::Type::Address { mutable: false, .. }))
                    })
            }),
            "the whole field is copied before the owner is discharged"
        );
    }
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("owning Package512 complete Source/raw/Wire passed; full-field snapshot={snapshot}");
}
#[test]
fn owning_wide_package_array_completes_source_raw_and_wire_verification() {
    wide_package_source_raw_and_wire(
        include_str!("owned-package-wide-shared-address-array.zil"),
        "95d6572a129f856a5b073de7cfa2aa9d2b730c0ff44b254fa6f9a3d8830299bb",
        false,
    );
}
#[test]
fn whole_shared_array_field_snapshot_survives_owner_cleanup_with_every_source() {
    wide_package_source_raw_and_wire(
        include_str!("owned-package-shared-address-array-snapshot.zil"),
        "59b8aaa8aa292c1e3921cfee638ef03598e1ffe62a3a4402d1e1542cc793ae66",
        true,
    );
}
