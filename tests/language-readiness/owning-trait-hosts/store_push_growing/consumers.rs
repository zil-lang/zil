//! Ordinary growth returns its actual input owner; fixed push retains its contract.
use super::*;

const MAIN: &str = include_str!("main.zil");
const CAPTURE: &str = include_str!("capture.zil");
const BORROW: &str = include_str!("borrow.zil");
const STORE: &str = include_str!("../../../../lib/std/store.zil");

fn sources(main: &str) -> zil_driver::ProjectSources {
    let mut sources = project(&[("main", main), ("store", STORE)]);
    sources.paths =
        std::collections::BTreeMap::from([(vec!["std".into(), "store".into()], ModuleId(1))]);
    sources
}

#[test]
fn ordinary_store_growth_preserves_owning_rhs_capture_and_last_use() {
    for (label, source) in [("ordinary", MAIN), ("capture", CAPTURE), ("borrow", BORROW)] {
        let report = compile_project(&sources(source), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            zil_base::Phase::Verify,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{label}: {}", report.to_json());
        assert_eq!(
            report.source_files.len(),
            2,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn ordinary_store_growth_keeps_consumed_owner_and_live_borrow_rejections() {
    let reused = MAIN
        .replacen(
            "match store.push_growing(items,item(expected,created))",
            "let incoming=item(expected,created)\n    match store.push_growing(items,incoming)",
            1,
        )
        .replacen(
            "if *created!=count+1 || !prefix(items,length+1)",
            "if incoming.id!=expected {return 16}\n    if *created!=count+1 || !prefix(items,length+1)",
            1,
        );
    let live = BORROW
        .replacen("let snapshot=*held", "let snapshot:i64=10", 1)
        .replacen("if snapshot!=10", "if *held!=10 || snapshot!=10", 1);
    assert_ne!(reused, MAIN);
    assert_ne!(live, BORROW);
    for (label, source) in [("consumed-owner", reused), ("live-borrow", live)] {
        // Compare the entire actual semantic diagnostic to the same misuse of
        // the existing fixed-capacity API, not to a phase-only rejection.
        let fixed = source.replace("store.push_growing(", "store.push(");
        assert_ne!(fixed, source);
        let control = compile_project(&sources(&fixed), &CompileOptions::default());
        let report = compile_project(&sources(&source), &CompileOptions::default());
        for result in [&control, &report] {
            assert_eq!(
                result.status,
                ResultCategory::SafeReject,
                "{label}: {}",
                result.to_json()
            );
            assert_eq!(
                result.phase,
                zil_base::Phase::Analyze,
                "{label}: {}",
                result.to_json()
            );
            assert_eq!(result.verification_runs, 0, "{label}: {}", result.to_json());
            assert!(
                !result.artifacts.contains_key("executable"),
                "{label}: {}",
                result.to_json()
            );
        }
        let diagnostics = |result: &zil_driver::CompileReport| {
            result
                .diagnostics
                .iter()
                .map(|d| (d.phase, d.category, d.code.clone(), d.message.clone()))
                .collect::<Vec<_>>()
        };
        assert!(
            !control.diagnostics.is_empty(),
            "{label}: {}",
            control.to_json()
        );
        assert!(control.diagnostics.iter().any(|d| match label {
            "consumed-owner" => d.code == "E-UNINITIALIZED",
            _ => d.code == "E-ANALYZE-IR" && d.message == "actual borrowed resource access conflicts with a live child or unrelated address window",
        }), "{label}: {}", control.to_json());
        assert_eq!(
            diagnostics(&report),
            diagnostics(&control),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn ordinary_store_growth_retries_each_incoming_owner_and_captured_prefix() {
    owning_trait_project_native_cleanup(
        &sources(MAIN),
        &[
            // abcd is the direct RHS trace; DCBA is explicit pop/drop.
            (None, 0, b"abcdDCBA5:2:2\n"),
            (Some("1"), 0, b"abcdDCBA6:2:2\n"),
            (Some("2"), 0, b"abcdDCBA6:2:2\n"),
            (Some("3"), 0, b"abcdDCBA6:2:2\n"),
            (Some("4"), 0, b"abcdDCBA6:2:2\n"),
            (Some("5"), 0, b"abcdDCBA6:2:2\n"),
        ],
    );
    owning_trait_project_native_cleanup(
        &sources(CAPTURE),
        &[(None, 0, b"2:1:1\n"), (Some("2"), 0, b"3:1:1\n")],
    );
}
