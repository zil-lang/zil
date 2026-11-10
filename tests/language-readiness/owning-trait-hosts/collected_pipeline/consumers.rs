//! Borrowed iterator inputs produce owners, grow a collection and return via helpers.
use super::*;

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
fn owned_replies_flow_from_lending_callbacks_into_a_growing_collection() {
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
fn growing_collection_returns_the_incoming_owner_on_every_allocation_failure() {
    owning_trait_native_cleanup(
        &files(MODEL),
        &[
            // Observer fields are attempts:new_allocations:releases. The
            // second collect grows an existing Store buffer with realloc:
            // 13 attempts create 12 allocation identities, all released.
            (None, 0, b"13:12:12\n"),
            (Some("1"), 1, b"1:0:0\n"),
            (Some("2"), 2, b"2:1:1\n"),
            (Some("3"), 3, b"3:2:2\n"),
            (Some("4"), 10, b"4:3:3\n"),
            (Some("5"), 14, b"5:4:4\n"),
            (Some("6"), 15, b"6:5:5\n"),
            (Some("7"), 30, b"7:6:6\n"),
            (Some("8"), 50, b"8:7:7\n"),
            (Some("9"), 20, b"9:8:8\n"),
            (Some("10"), 24, b"10:9:9\n"),
            (Some("11"), 25, b"11:10:10\n"),
            (Some("12"), 40, b"12:11:11\n"),
            (Some("13"), 51, b"13:12:12\n"),
        ],
    );
}

#[test]
fn collecting_fresh_outputs_does_not_release_the_input_iterator_loan() {
    let changed = MODEL.replacen(
        "match library.collect(replies,reply)",
        "library.next_tagged(&mut cursor,9);match library.collect(replies,reply)",
        1,
    );
    assert_ne!(changed, MODEL);
    let report = compile_project(&project(&files(&changed)), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, zil_base::Phase::Analyze);
    assert_eq!(report.verification_runs, 0);
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "E-ANALYZE-IR"
                && diagnostic.message
                    == "stored target proof: simultaneously live access overlaps an actual stored target/parent loan"
        }),
        "{}",
        report.to_json()
    );
}
