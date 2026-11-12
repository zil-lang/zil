//! Quality regression from the actual large closed-workload CGU. This does
//! not replace the original 425-function performance fixture or its flags.
use super::*;

const SOURCE: &str = include_str!("../../tests/fixtures/spill-native-products.ll");

fn config() -> Config<'static> {
    Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O2,
    }
}

fn ordinary(source: &str) -> Object {
    compile_input(
        Input::Text(source),
        &config(),
        FunctionMerging::Disabled,
        &|| false,
        false,
        None,
    )
    .expect("the unchanged full O2 pipeline is the native byte oracle")
    .0
}

fn products(
    source: &str,
    prior: Option<&crate::NativeWorkProducts>,
) -> (Object, crate::NativeWorkProducts, crate::NativeProductStats) {
    let mut attempt = NativeAttempt {
        prior,
        stats: crate::NativeProductStats::default(),
        replay_failed: false,
        products: None,
        timings: AttemptTimings::default(),
    };
    let (object, _) = compile_input(
        Input::Text(source),
        &config(),
        FunctionMerging::Disabled,
        &|| false,
        false,
        Some(&mut attempt),
    )
    .expect("the actual spill bodies must succeed in the current native context");
    eprintln!("closed spill work: {:?}", attempt.stats);
    assert!(!attempt.replay_failed);
    assert_eq!(attempt.stats.available, 1);
    assert_eq!(attempt.stats.failed_replays, 0);
    assert_eq!(attempt.stats.fresh_context_fallbacks, 0);
    assert_eq!(attempt.stats.asm_printer_runs, 2);
    assert!(object.receipt.verify_before_after);
    (
        object,
        attempt
            .products
            .expect("successful real spill work products"),
        attempt.stats,
    )
}

fn exercise(source: &str) {
    let (cold, prior, cold_work) = products(source, None);
    assert_eq!(cold_work.machine_function_executions, 2);
    assert_eq!(cold_work.machine_function_reuses, 0);
    assert!(cold_work.mir_snapshot_bytes > 0);
    assert_eq!(cold.bytes, ordinary(source).bytes);
    let (warm, _, warm_work) = products(source, Some(&prior));
    assert_eq!(warm_work.machine_function_reuses, 2);
    assert_eq!(warm_work.machine_function_executions, 0);
    assert!(warm_work.machine_pass_skips > 0);
    assert!(warm_work.mir_replay_bytes > 0);
    assert_eq!(warm.bytes, cold.bytes);

    // Edit the exact first m0 leaf's checked arithmetic. The other 20-leaf
    // root still has its original function and complete dependency inputs.
    let old = "  %v.9 = add i32 0, 1\n";
    assert!(source.contains(old));
    let edited = source.replacen(old, "  %v.9 = add i32 0, 2\n", 1);
    assert_ne!(source, edited);
    let (changed, changed_prior, changed_work) = products(&edited, Some(&prior));
    assert_eq!(changed_work.machine_function_reuses, 1);
    assert_eq!(changed_work.machine_function_executions, 1);
    assert_ne!(changed.bytes, cold.bytes);
    assert_eq!(changed.bytes, ordinary(&edited).bytes);
    let (changed_warm, _, changed_warm_work) = products(&edited, Some(&changed_prior));
    assert_eq!(changed_warm_work.machine_function_reuses, 2);
    assert_eq!(changed_warm_work.machine_function_executions, 0);
    assert_eq!(changed_warm.bytes, changed.bytes);
}

#[test]
fn actual_closed_workload_spills_reuse_current_frames_and_match_full_o2() {
    exercise(SOURCE);
}

#[test]
fn actual_spill_bodies_with_unwind_tables_replay_current_cfi() {
    // Retain the entire actual function bodies and optimization pipeline, and
    // explicitly request unwind tables on these two exported test functions.
    // This additional quality case makes the native AsmPrinter consume CFI.
    let source = SOURCE
        .replace(
            "define i32 @zil.fn.25(i32 %arg.0) {",
            "define i32 @zil.fn.25(i32 %arg.0) uwtable {",
        )
        .replace(
            "define i32 @zil.fn.361(i32 %arg.0) {",
            "define i32 @zil.fn.361(i32 %arg.0) uwtable {",
        );
    assert_eq!(source.matches(" uwtable {").count(), 2);
    exercise(&source);
}
