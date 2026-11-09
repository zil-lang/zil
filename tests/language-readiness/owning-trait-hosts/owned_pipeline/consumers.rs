//! A Store loan remains real while ordinary trait calls produce owned results.
use super::*;
use zil_base::Phase;

const MAIN: &str = include_str!("main.zil");
const PROTOCOL: &str = include_str!("protocol.zil");
const LIBRARY: &str = include_str!("library.zil");
const MODEL: &str = include_str!("model.zil");

fn files(model: &str) -> [(&str, &str); 4] {
    [
        ("main", MAIN),
        ("protocol", PROTOCOL),
        ("library", LIBRARY),
        ("model", model),
    ]
}

#[test]
fn owning_pipeline_composes_multiple_bounds_lending_and_self_results() {
    let report = compile_project(&project(&files(MODEL)), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
#[cfg(target_os = "linux")]
fn owning_pipeline_observes_values_and_once_cleanup_on_every_allocation_exit() {
    owning_trait_native_cleanup(
        &files(MODEL),
        &[
            (None, 0, b"11:11:11\n"),
            (Some("1"), 1, b"1:0:0\n"),
            (Some("2"), 2, b"2:1:1\n"),
            (Some("3"), 3, b"3:2:2\n"),
            (Some("4"), 10, b"4:3:3\n"),
            (Some("5"), 14, b"5:4:4\n"),
            (Some("6"), 15, b"6:5:5\n"),
            (Some("7"), 30, b"7:6:6\n"),
            (Some("8"), 20, b"8:7:7\n"),
            (Some("9"), 24, b"9:8:8\n"),
            (Some("10"), 25, b"10:9:9\n"),
            (Some("11"), 40, b"11:10:10\n"),
        ],
    );
}

#[test]
fn owning_pipeline_keeps_the_parent_loan_and_affine_fresh_results() {
    let report = compile_project(&project(&files(MODEL)), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    for action in [
        "library.next_tagged(&mut cursor,9)",
        "store_reserve(cursor.items,3)",
        "let moved=cursor",
    ] {
        let changed = MODEL.replacen(
            "let copy:Item=original.duplicate()",
            &format!("let copy:Item=original.duplicate();{action}"),
            1,
        );
        assert_ne!(changed, MODEL);
        let report = compile_project(&project(&files(&changed)), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{action}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Analyze,
            "{action}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    let changed = MODEL.replacen(
        "if consume(copy)!=21 {return 12}",
        "if consume(copy)!=21 {return 12};consume(copy)",
        1,
    );
    let report = compile_project(&project(&files(&changed)), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn owning_pipeline_raw_and_wire_recover_actual_store_roots() {
    let library = LIBRARY
        .replace("import protocol", "")
        .replace("protocol.", "");
    let model=MODEL.replace("import protocol","").replace("import library","").replace("protocol.","").replace("library.","").replace("let index=self.index","let local=Item{data:bytes_new(),tag:90};let unrelated:Store<Item>=store_new();let capacity=store_len(unrelated);let index=self.index");
    let text = format!("{PROTOCOL}\n{library}\n{model}\nfn main()->i32{{return run()}}");
    let input = SourceUnit::new(FileId(0), ModuleId(0), "owning-pipeline-roots.zil", text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for manager in [false, true] {
        let mut changed = typed.clone();
        let provider = changed
            .functions
            .iter_mut()
            .find(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. }))
            })
            .unwrap();
        let local = provider
            .locals
            .iter()
            .find(|l| l.name == "local")
            .unwrap()
            .id;
        let unrelated = provider
            .locals
            .iter()
            .find(|l| l.name == "unrelated")
            .unwrap()
            .id;
        let other = provider
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::DescribeLocal { local } if local == unrelated => i.result,
                _ => None,
            })
            .unwrap();
        let borrow = provider
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. }))
            .unwrap();
        if manager {
            let InstructionKind::OwnedStoreBorrow { store, .. } = &mut borrow.kind else {
                unreachable!()
            };
            assert_eq!(
                provider.values[store.0 as usize],
                provider.values[other.0 as usize]
            );
            *store = other;
        } else {
            borrow.kind = InstructionKind::BorrowLocal { local };
        }
        let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::SafeReject
                && d.message.contains("stored target proof")),
            "{errors:?}"
        );
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(changed.clone(), candidate, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|d| d.phase == Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.code == "E-VERIFY-STORED"),
                "{errors:?}"
            );
        }
    }
}
