//! Black-box Source consumers for ordinary mutable POD Store elements.
use super::*;

pub(super) fn native_all_backends(label: &str, source: &str, expected: &[u8], imported: bool) {
    native_entry_all_backends(label, source, expected, imported, None);
}
pub(super) fn native_entry_all_backends(
    label: &str,
    source: &str,
    expected: &[u8],
    imported: bool,
    entry: Option<&str>,
) {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("g933-{label}-{backend:?}-{optimization:?}"));
            let input = dir.join("main.zil");
            fs::write(&input, source).unwrap();
            let exe = dir.join("program");
            let options = CompileOptions {
                backend,
                entry: entry.map(str::to_owned),
                stop_after: Stage::Build,
                optimization,
                output: Some(exe.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            };
            let report = if imported {
                zil_driver::compile_path(&input, &options)
            } else {
                let unit = SourceUnit::new(FileId(0), ModuleId(0), "g933.zil", source);
                compile_source(&unit, &options)
            };
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert!(report.verification_runs > 0, "{}", report.to_json());
            if imported {
                assert!(report.source_files.len() >= 2, "{}", report.to_json());
            }
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none(), "{}", report.to_json());
            let output = Command::new(exe).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, expected, "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn g933_direct_write_helper_reborrow_last_use_and_distinct_root() {
    native_all_backends("direct", include_str!("direct.zil"), b"MUT\n", false);
}

#[test]
fn g933_std_store_helpers_resolve_across_real_module_boundary() {
    native_all_backends("std-store", include_str!("std-store.zil"), b"STDM\n", true);
}

#[test]
fn g933_live_same_root_windows_and_structural_changes_are_rejected() {
    const PREFIX: &str = "struct Cell{value:i32}fn consume(give items:Store<Cell>){}fn destroy(give items:Store<Cell>){}fn main()->i32{let items:Store<Cell>=store_new();store_reserve(items,3);store_push(items,Cell{value:41});store_push(items,Cell{value:42});";
    // Both shared and exclusive views have a legal isolated baseline. The
    // return reads the original view, so each inserted operation is live.
    let mut mismatches = Vec::new();
    for (label, view, change) in [
        (
            "shared-other-index",
            "store_at_mut(items,0)",
            "let other=store_at(items,1);let n=other.value;",
        ),
        (
            "mut-other-index",
            "store_at_mut(items,0)",
            "let other=store_at_mut(items,1);let n=other.value;",
        ),
        (
            "mut-after-shared",
            "store_at(items,0)",
            "let other=store_at_mut(items,1);let n=other.value;",
        ),
        (
            "reserve",
            "store_at_mut(items,0)",
            "store_reserve(items,4);",
        ),
        (
            "push",
            "store_at_mut(items,0)",
            "store_push(items,Cell{value:43});",
        ),
        ("pop", "store_at_mut(items,0)", "store_pop(items);"),
        ("give", "store_at_mut(items,0)", "consume(give items);"),
        ("drop", "store_at_mut(items,0)", "destroy(give items);"),
    ] {
        let baseline = format!("{PREFIX}let view={view};return view.value}}");
        let accepted = check(&format!("g933-{label}-baseline"), &baseline, Stage::Verify);
        assert_eq!(
            accepted.status,
            ResultCategory::Success,
            "{}",
            accepted.to_json()
        );
        assert!(accepted.verification_runs > 0);
        let source = format!("{PREFIX}let view={view};{change}return view.value}}");
        let report = check(&format!("g933-{label}"), &source, Stage::Verify);
        let diagnosis = report.diagnostics.first();
        eprintln!(
            "g933 {label}: status={:?} phase={:?} code={:?} message={:?}",
            report.status,
            report.phase,
            diagnosis.map(|d| &d.code),
            diagnosis.map(|d| &d.message)
        );
        let (code, message) = if label.contains("index") || label == "mut-after-shared" {
            (
                "E-ANALYZE-IR",
                "actual borrowed resource access conflicts with a live child or unrelated address window",
            )
        } else {
            (
                "E-BORROW-CONFLICT",
                "owned storage transition invalidates a live address window in main",
            )
        };
        let precise_conflict = report.phase == Phase::Analyze
            && diagnosis.is_some_and(|d| d.code == code && d.message == message);
        if report.status != ResultCategory::SafeReject
            || !precise_conflict
            || report.artifacts.contains_key("executable")
        {
            mismatches.push(format!("{label}: {}", report.to_json()));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn g933_source_derived_raw_wire_actual_receiver_conflict() {
    source_derived_receiver_conflict(
        "struct Cell{value:i32}fn main()->i32{let first:Store<Cell>=store_new();let second:Store<Cell>=store_new();store_reserve(first,1);store_reserve(second,1);store_push(first,Cell{value:20});store_push(second,Cell{value:22});let a=store_at_mut(first,0);let b=store_at_mut(second,0);b.value=23;return a.value+b.value}",
    );
}
pub(super) fn source_derived_receiver_conflict(text: &str) {
    use std::collections::BTreeSet;

    let (mut module, mut plan) = candidate(text);
    let mut changed_borrow = None;
    for (function_index, function) in module.functions.iter_mut().enumerate() {
        let mut first = None;
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let I::OwnedStoreBorrowMut { store, .. } = &mut instruction.kind {
                    let result = instruction.result.expect("Store borrow has address result");
                    if let Some((first_result, first_store)) = first {
                        let old_store = *store;
                        *store = first_store;
                        changed_borrow = Some((function_index, first_result, result, old_store));
                    } else {
                        first = Some((result, *store));
                    }
                }
            }
        }
    }
    let (function_index, first_result, second_result, old_store) =
        changed_borrow.expect("source has two mutable element borrows");
    let function = &module.functions[function_index];
    let function_plan = plan
        .functions
        .iter()
        .find(|row| row.function == function.id)
        .expect("Source plan contains the function");
    let new_roots = function_plan.holds[first_result.0 as usize].roots.clone();
    let old_roots = function_plan.holds[second_result.0 as usize].roots.clone();
    assert_ne!(new_roots, old_roots, "baseline uses distinct actual Stores");
    assert_eq!(
        function_plan.holds[old_store.0 as usize].roots, old_roots,
        "second Store descriptor is the source of the original view"
    );

    // A forged receiver with the original candidate proves the independent
    // Hold gate notices stale provenance. Wire and raw use the same re-bound
    // transport fingerprint, with no producer or verifier proof function.
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let stale_wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
            .unwrap_err(),
        zil_verify::verify_module_bytes(&stale_wire, &module.target, &limits).unwrap_err(),
    ] {
        assert_eq!(errors[0].code, "E-VERIFY-HOLD", "{errors:?}");
    }

    // Rebind only results actually derived from the changed second borrow.
    // Local copies of the address are tracked syntactically; unrelated second
    // Store descriptions keep their original roots. No index is treated apart.
    let mut derived = BTreeSet::from([second_result.0]);
    let mut address_locals = BTreeSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                I::Store { local, value } => {
                    if derived.contains(&value.0) {
                        address_locals.insert(local.0);
                    } else {
                        address_locals.remove(&local.0);
                    }
                }
                I::Load(local) if address_locals.contains(&local.0) => {
                    derived.insert(instruction.result.expect("Load has result").0);
                }
                I::AddressField { address, .. }
                | I::ReborrowAddress { address, .. }
                | I::BorrowPlaceAddress { address, .. }
                | I::DescribeAddress { address }
                    if derived.contains(&address.0) =>
                {
                    derived.insert(instruction.result.expect("address derivation has result").0);
                }
                _ => {}
            }
        }
    }
    assert!(
        derived.len() > 1,
        "field use must derive from the second view"
    );
    let function_plan = plan
        .functions
        .iter_mut()
        .find(|row| row.function == function.id)
        .expect("Source plan contains the function");
    for value in derived {
        if matches!(
            function.values[value as usize],
            Type::Address { .. } | Type::Desc(_)
        ) {
            let row = &mut function_plan.holds[value as usize];
            assert_eq!(
                row.roots, old_roots,
                "only actual second-view roots are rebound"
            );
            row.roots.clone_from(&new_roots);
        }
    }

    // Analyze independently rejects the executable alias. Verify now receives
    // a candidate whose claimed Hold rows match the mutated actual source and
    // must reach its private live-loan conflict on both raw and Wire paths.
    let analyze_errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(
        analyze_errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
        "{analyze_errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap_err(),
    ] {
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
}

#[path = "allocation.rs"]
mod allocation;
