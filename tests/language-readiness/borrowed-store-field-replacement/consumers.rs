//! Replace a real managed Store field through its exclusive Package address.
use super::*;

const REFILL: &str = include_str!("refill.zil");
const CALLER_DESCRIPTOR: &str = include_str!("caller-descriptor.zil");
const RECORD_REFILL: &str = include_str!("record-refill.zil");
const RHS_ONCE: &str = include_str!("rhs-once.zil");
const TWO_INPUTS: &str = include_str!("two-inputs.zil");
const PREPARE_ERROR: &str = include_str!("prepare-error.zil");

fn core_source(program: &str) -> zil_ir::CoreModule {
    let input = source(program);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    zil_expand::expand_with_map_limits(&input, &ast, &Limits::default())
        .map(|expanded| std::sync::Arc::unwrap_or_clone(expanded.core))
        .unwrap()
}

fn raw_source(program: &str) -> zil_ir::TypedModule {
    zil_types::check(
        &core_source(program),
        &zil_base::TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap()
}

#[test]
fn borrowed_generic_store_field_refill_reaches_independent_verify() {
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "borrowed-store-refill.zil", REFILL),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    println!("borrowed Store refill: {}", report.to_json());
    if report.status != ResultCategory::Success {
        let raw = raw_source(REFILL);
        for function in &raw.functions {
            if function.name.contains("refill") {
                println!("actual refill TypedFunction: {function:#?}");
            }
        }
    }
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn borrowed_store_field_explicit_dereference_keeps_the_same_actual_receiver() {
    let program = REFILL.replacen("holder.items=give next", "(*holder).items=give next", 1);
    let report = compile_source(
        &source(&program),
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
    let direct = raw_source(REFILL);
    let explicit = raw_source(&program);
    let operations = |module: &zil_ir::TypedModule| {
        let function = module
            .functions
            .iter()
            .find(|f| f.locals.iter().any(|l| l.name == "next"))
            .unwrap();
        function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|i| match &i.kind {
                kind @ zil_ir::InstructionKind::ReservePlaceAddress { .. }
                | kind @ zil_ir::InstructionKind::CommitPlaceReplace { .. } => Some(kind.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let direct = operations(&direct);
    assert_eq!(direct.len(), 2);
    assert_eq!(direct, operations(&explicit));
}

#[test]
fn borrowed_store_field_record_element_preserves_its_complete_store_identity() {
    let report = compile_source(
        &source(RECORD_REFILL),
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

#[test]
fn borrowed_store_field_definition_rejects_shared_description_wrong_type_and_repeated_give() {
    let prefix = "package Holder<T:StoreElement>{manages items:Store<T>}";
    for (label, definition) in [
        (
            "shared receiver",
            "fn bad<T:StoreElement>(holder:&Holder<T>,give next:Store<T>){holder.items=give next}",
        ),
        (
            "description supplied as owner",
            "fn bad<T:StoreElement>(holder:&mut Holder<T>,next:Store<T>){holder.items=next}",
        ),
        (
            "full Store type differs",
            "fn bad<T:StoreElement>(holder:&mut Holder<T>,give next:Store<i64>){holder.items=give next}",
        ),
        (
            "incoming responsibility given twice",
            "fn bad<T:StoreElement>(holder:&mut Holder<T>,give next:Store<T>){holder.items=give next;holder.items=give next}",
        ),
        (
            "old live element borrow",
            "fn bad<T:Copy+StoreElement>(holder:&mut Holder<T>,give next:Store<T>)->T{let old=store_at(holder.items,0);holder.items=give next;return *old}",
        ),
    ] {
        let input = source(&format!("{prefix}{definition}fn main()->i32{{return 0}}"));
        let report = compile_source(
            &input,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("{label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
    }
}

#[test]
fn borrowed_store_field_resolution_preserves_declaring_module_privacy() {
    let input = project(&[
        (
            "main",
            "import provider;fn bad<T:StoreElement>(holder:&mut provider.Holder<T>,give next:Store<T>){holder.items=give next}fn main()->i32{return 0}",
        ),
        (
            "provider",
            "pub package Holder<T:StoreElement>{manages items:Store<T>}",
        ),
    ]);
    let report = compile_project(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
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
            .any(|diagnostic| diagnostic.code == "E-FIELD-PRIVATE"),
        "{}",
        report.to_json()
    );
}

#[test]
fn helper_replacement_retires_the_callers_old_store_descriptor_and_element_borrow() {
    let control = compile_source(
        &source(CALLER_DESCRIPTOR),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        control.status,
        ResultCategory::Success,
        "{}",
        control.to_json()
    );
    assert_eq!(control.verification_runs, 1);
    let old_descriptor = CALLER_DESCRIPTOR.replacen(
        "return i32(*store_at(holder.items,0))-22",
        "return i32(*store_at(old,0))-11",
        1,
    );
    let old_element = REFILL.replacen(
        "refill(&mut holder,give replacement)\n    return i32(*store_at(holder.items,0))-22",
        "let old=store_at(holder.items,0)\n    refill(&mut holder,give replacement)\n    return i32(*old)-11",
        1,
    );
    assert_ne!(old_descriptor, CALLER_DESCRIPTOR);
    assert_ne!(old_element, REFILL);
    let self_give = REFILL.replacen(
        "refill(&mut holder,give replacement)",
        "refill(&mut holder,give holder.items)",
        1,
    );
    assert_ne!(self_give, REFILL);
    for (label, program) in [
        ("caller old descriptor", old_descriptor),
        ("caller old element", old_element),
        ("caller moves the borrowed destination field", self_give),
    ] {
        let report = compile_source(
            &source(&program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("{label}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn raw_and_wire_reject_reusing_a_consumed_store_input_and_a_shared_parent() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for kind in ["duplicate input", "shared parent"] {
        let mut raw = raw_source(if kind == "duplicate input" {
            TWO_INPUTS
        } else {
            REFILL
        });
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let mut edits = 0;
        if kind == "duplicate input" {
            let f = raw
                .functions
                .iter_mut()
                .find(|f| f.locals.iter().any(|l| l.name == "last"))
                .unwrap();
            let mut first = None;
            for block in &mut f.blocks {
                for instruction in &mut block.instructions {
                    if let zil_ir::InstructionKind::CommitPlaceReplace { value, .. } =
                        &mut instruction.kind
                    {
                        if let Some(old) = first {
                            assert_ne!(*value, old);
                            *value = old;
                            edits += 1;
                        } else {
                            first = Some(*value)
                        }
                    }
                }
            }
            assert_eq!(edits, 1);
        } else {
            let package = raw
                .functions
                .iter()
                .find_map(|f| {
                    f.locals.iter().find_map(|l| match &l.ty {
                        zil_ir::Type::Address { inner, .. }
                            if matches!(inner.as_ref(), zil_ir::Type::Package { .. }) =>
                        {
                            Some(inner.as_ref().clone())
                        }
                        _ => None,
                    })
                })
                .unwrap();
            for f in &mut raw.functions {
                for ty in f
                    .values
                    .iter_mut()
                    .chain(f.locals.iter_mut().map(|l| &mut l.ty))
                {
                    if let zil_ir::Type::Address { mutable, inner } = ty
                        && inner.as_ref() == &package
                    {
                        *mutable = false;
                        edits += 1;
                    }
                }
                for block in &mut f.blocks {
                    for instruction in &mut block.instructions {
                        if let zil_ir::InstructionKind::BorrowLocalMut { local } = instruction.kind
                            && f.locals[local.0 as usize].ty == package
                        {
                            instruction.kind = zil_ir::InstructionKind::BorrowLocal { local };
                            edits += 1;
                        }
                    }
                }
            }
            assert!(edits >= 3);
        }
        plan.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
        let verified =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        for (entry, errors) in [
            ("Analyze", analyzed),
            ("Verify", verified),
            ("Wire", decoded),
        ] {
            println!("{kind} {entry}: {errors:?}");
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::SafeReject),
                "{kind}/{entry}: {errors:?}"
            );
            assert!(
                !errors.iter().any(|d| d.message.contains("fingerprint")),
                "{kind}/{entry}: {errors:?}"
            );
            assert!(
                errors.iter().any(|d| {
                    if kind == "duplicate input" {
                        d.message.contains("replacement RHS") && d.message.contains("consumed")
                    } else {
                        d.message.contains("mutable parent")
                            || d.message.contains("exclusive permission")
                    }
                }),
                "{kind}/{entry}: {errors:?}"
            );
        }
    }
}

#[test]
fn named_and_explicit_store_field_replacement_have_exact_fresh_and_warm_budgets() {
    let mut observed_boundaries = Vec::new();
    let explicit = REFILL.replacen("holder.items=give next", "(*holder).items=give next", 1);
    for (label, program) in [("named", REFILL), ("explicit", explicit.as_str())] {
        let input = source(program);
        let mut low = 1;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(
                &input,
                &CompileOptions {
                    stop_after: Stage::Verify,
                    limits: Limits {
                        max_analysis_steps: mid,
                        ..Limits::default()
                    },
                    ..CompileOptions::default()
                },
            );
            match report.status {
                ResultCategory::Success => {
                    assert_eq!(report.verification_runs, 1);
                    high = mid;
                }
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{label}: {}", report.to_json()),
            }
        }
        let mut warm = zil_driver::CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            let options = CompileOptions {
                stop_after: Stage::Verify,
                limits: Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
                ..CompileOptions::default()
            };
            for report in [
                warm.compile_source(&input, &options),
                compile_source(&input, &options),
            ] {
                assert_eq!(report.status, expected, "{label}: {}", report.to_json());
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                }
            }
        }
        observed_boundaries.push((label, "Source Verify", low, 41_124));
        println!("borrowed Store {label}: N={low}, N-1=ResourceLimit, fresh/warm/retry=Pass");
        let core = core_source(program);
        let target = zil_base::TargetProfile::host_llvm();
        let mut low = 1;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            match zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: mid,
                    ..Limits::default()
                },
            ) {
                Ok(_) => high = mid,
                Err(errors)
                    if errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit) =>
                {
                    low = mid + 1
                }
                Err(errors) => panic!("{label} Types: {errors:?}"),
            }
        }
        for n in [low, low - 1, low] {
            let result = zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            );
            if n == low {
                result.unwrap();
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit)
                );
            }
        }
        observed_boundaries.push((
            label,
            "Types",
            low,
            if label == "named" { 2_805 } else { 3_141 },
        ));
        println!("borrowed Store {label} Types: N={low}, N-1=ResourceLimit, retry=Pass");
    }
    assert!(
        observed_boundaries
            .iter()
            .all(|(_, _, actual, expected)| actual == expected),
        "complete named/explicit Source and Types boundary observations: {observed_boundaries:?}"
    );
}

#[test]
fn raw_and_wire_reconstruct_old_descriptor_retirement_after_the_helper() {
    reject_retired_descriptor(CALLER_DESCRIPTOR);
}

fn reject_retired_descriptor(program: &str) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let mut raw = raw_source(program);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let valid_wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&valid_wire, &target, &limits).unwrap();
    let main = raw
        .functions
        .iter_mut()
        .find(|f| f.locals.iter().any(|l| l.name == "old"))
        .unwrap();
    let old = main.locals.iter().find(|l| l.name == "old").unwrap().id;
    let descriptor = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::Store { local, value } if local == old => Some(value),
            _ => None,
        })
        .unwrap();
    let mut changed = 0;
    for block in &mut main.blocks {
        for instruction in &mut block.instructions {
            if let zil_ir::InstructionKind::OwnedStoreBorrow { store, .. } = &mut instruction.kind {
                assert_eq!(
                    main.values[store.0 as usize],
                    main.values[descriptor.0 as usize]
                );
                *store = descriptor;
                changed += 1;
            }
        }
    }
    assert_eq!(
        changed, 1,
        "one actual post-call Store borrow must be redirected"
    );
    plan.module_fingerprint = raw.fingerprint();
    let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
    let verified =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for (entry, errors) in [
        ("Analyze", analyzed),
        ("Verify", verified),
        ("Wire", decoded),
    ] {
        println!("old descriptor {entry}: {errors:?}");
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{entry}: {errors:?}"
        );
        assert!(
            !errors.iter().any(|d| d.message.contains("fingerprint")),
            "{entry}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("retir") && d.message.contains("description")),
            "{entry}: {errors:?}"
        );
    }
}

fn reborrowed_refill(program: &str) -> String {
    let changed = program.replacen(
        "holder.items=give next",
        "let alias=&mut *holder;alias.items=give next",
        1,
    );
    assert_ne!(changed, program);
    changed
}

fn checked_raw_and_wire(program: &str) {
    let report = compile_source(&source(program), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let raw = raw_source(program);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &raw.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &raw.target, &limits).unwrap();
}

#[test]
fn borrowed_store_field_reborrow_preserves_actual_origins_and_retires_old_views() {
    let program = reborrowed_refill(CALLER_DESCRIPTOR);
    checked_raw_and_wire(&program);
    let descriptor = program.replacen(
        "return i32(*store_at(holder.items,0))-22",
        "return i32(*store_at(old,0))-11",
        1,
    );
    let baseline = reborrowed_refill(REFILL);
    let element = baseline.replacen(
        "refill(&mut holder,give replacement)\n    return i32(*store_at(holder.items,0))-22",
        "let old=store_at(holder.items,0)\n    refill(&mut holder,give replacement)\n    return i32(*old)-11",
        1,
    );
    assert_ne!(descriptor, program);
    assert_ne!(element, baseline);
    for invalid in [descriptor, element] {
        let report = compile_source(&source(&invalid), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
    reject_retired_descriptor(&program);
}

#[test]
fn borrowed_store_field_failed_preparation_preserves_the_old_owner() {
    checked_raw_and_wire(PREPARE_ERROR);
    let raw = raw_source(PREPARE_ERROR);
    let helper = raw
        .functions
        .iter()
        .find(|f| f.locals.iter().any(|l| l.name == "next"))
        .unwrap();
    assert!(
        helper
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                zil_ir::InstructionKind::AbortPlaceReservation { .. }
            ))
    );
}

#[cfg(unix)]
#[test]
fn borrowed_store_field_native_transfer_reads_new_payload_and_frees_both_instances_once() {
    use std::os::unix::fs::PermissionsExt;
    use zil_driver::NativeBackend;

    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "borrowed-store-refill-native-{}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    let observer = root.join("allocator.o");
    let compiled = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/composed-store-closures/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{compiled:?}");
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = root.join("link.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
            quote(&clang),
            quote(&observer)
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let reborrow = reborrowed_refill(REFILL);
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program, stdout) in [
                ("scalar", REFILL, &b""[..]),
                ("record", RECORD_REFILL, &b""[..]),
                ("rhs-once", RHS_ONCE, &b"X"[..]),
                ("reborrow", reborrow.as_str(), &b""[..]),
                ("prepare-error", PREPARE_ERROR, &b""[..]),
            ] {
                let dir = root.join(format!("{backend:?}-{optimization:?}-{label}"));
                fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                fs::write(dir.join("compile.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                fs::write(dir.join("execution.txt"), format!("{output:?}")).unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}: {output:?}"
                );
                assert_eq!(output.stdout, stdout, "{output:?}");
                let audit = String::from_utf8(output.stderr).unwrap();
                for field in [
                    "\"allocated\":2",
                    "\"frees\":2",
                    "\"failures\":0",
                    "\"live\":0",
                ] {
                    assert!(
                        audit.contains(field),
                        "{backend:?}/{optimization:?}: {audit}"
                    );
                }
                println!("{backend:?}/{optimization:?}/{label}: exit0, {audit}");
                runs += 1;
            }
        }
    }
    assert_eq!(runs, 10 + 10 * usize::from(cfg!(feature = "llvm-api")));
}
