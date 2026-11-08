//! Ordinary owned results retain complete nominal identity and cleanup.
use super::*;

const PROTOCOL: &str = include_str!("protocol.zil");
const LIBRARY: &str = include_str!("library.zil");
const MODEL: &str = include_str!("model.zil");
const MAIN: &str = include_str!("main.zil");

fn files() -> [(&'static str, &'static str); 4] {
    [("main", MAIN), ("protocol", PROTOCOL), ("library", LIBRARY), ("model", MODEL)]
}

#[test]
fn owning_trait_results_reach_concrete_and_symbolic_consumers() {
    for main in [MAIN.to_owned(), MAIN.replace("library.duplicate(&original)", "original.duplicate()").replace("library.try_duplicate(&original,", "original.try_duplicate(")] {
        let report = compile_project(&project(&[("main", &main), ("protocol", PROTOCOL), ("library", LIBRARY), ("model", MODEL)]), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
#[cfg(target_os = "linux")]
fn owning_trait_results_native_values_and_cleanup_on_every_exit() {
    owning_trait_native_cleanup(&files(), &[
        (None, 0, b"4:4:4\n"),
        (Some("1"), 1, b"1:0:0\n"),
        (Some("2"), 2, b"2:1:1\n"),
        (Some("3"), 4, b"3:2:2\n"),
        (Some("4"), 6, b"4:3:3\n"),
    ]);
}
