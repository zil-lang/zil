//! Replace a real managed Store field through its exclusive Package address.
use super::*;

const REFILL: &str = include_str!("refill.zil");
const CALLER_DESCRIPTOR: &str = include_str!("caller-descriptor.zil");
const RECORD_REFILL: &str = include_str!("record-refill.zil");

fn raw_source(program: &str) -> zil_ir::TypedModule {
    let input = source(program);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits(&input, &ast, &Limits::default())
        .unwrap()
        .core;
    zil_types::check(
        &core,
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
    for (label, program) in [
        ("caller old descriptor", old_descriptor),
        ("caller old element", old_element),
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
fn raw_and_wire_reconstruct_old_descriptor_retirement_after_the_helper() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let mut raw = raw_source(CALLER_DESCRIPTOR);
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
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program) in [("scalar", REFILL), ("record", RECORD_REFILL)] {
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
                assert!(output.stdout.is_empty(), "{output:?}");
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
    assert_eq!(runs, 4 + 4 * usize::from(cfg!(feature = "llvm-api")));
}
