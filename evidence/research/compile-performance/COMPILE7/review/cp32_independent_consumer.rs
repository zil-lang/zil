//! Non-author public API checks for production-cap partial epoch transitions.
//! This is correctness evidence only; no elapsed time is used as performance.
#[path = "cp29_independent_consumer.rs"]
mod cp29_regression;

use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CoreProgram, ProgramModule};
use zil_types::{ProgramMemo, check_program, check_program_with_memo};

fn parse_module(index: usize, text: &str) -> ProgramModule {
    let source = SourceUnit::new(
        FileId(index as u32), ModuleId(index as u32), format!("partial{index}.zil"), text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_program_module(&source, &parsed, vec![]).unwrap().0
}

fn leaf_text(public: bool) -> String {
    let mut s = String::new();
    for i in 0..8 {
        s.push_str(&format!("fn f{i}(x:i64)->i64 {{return (x*{}+{})%251;}}\n", i + 2, i * 3 + 1));
    }
    if public { s.push_str("pub "); }
    s.push_str("fn run(x:i64)->i64 {var y=x;");
    for i in 0..8 { s.push_str(&format!("y=f{i}(y);")); }
    s.push_str("return y;}");
    s
}

fn root_text(extra: usize) -> String {
    let mut s = String::from("fn main()->i32 {");
    for _ in 0..extra { s.push_str("1;"); }
    s.push_str("return 1;}");
    s
}

fn project() -> CoreProgram {
    let leaf = leaf_text(true);
    CoreProgram {
        root: ModuleId(0), source_fingerprint: Fingerprint::ZERO,
        modules: (0..=1024).map(|i| {
            parse_module(i, if i == 0 { "fn main()->i32 {return 1;}" } else { &leaf })
        }).collect(),
    }
}

fn checked(program: &CoreProgram, old: &ProgramMemo) -> ProgramMemo {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let cold = check_program(program, &target, &limits).unwrap();
    let (warm, next) = check_program_with_memo(
        program, &target, &limits, Fingerprint::ZERO, old).unwrap();
    assert_eq!(warm, cold);
    assert!(next.stats().eligible);
    assert!(next.stats().retained_weight <= 64 * 1024 * 1024);
    next
}

#[test]
fn expanding_a_retained_module_evicts_then_reacquires_later_snapshots_without_stale_ids() {
    let mut program = project();
    let original = checked(&program, &ProgramMemo::default());
    let initial = original.stats();
    assert!(initial.retained_modules > 1 && initial.retained_modules < program.modules.len());
    program.modules[0] = parse_module(0, &root_text(500));
    let expanded = checked(&program, &original);
    let small = expanded.stats();
    assert!(small.retained_modules > 1 && small.retained_modules < initial.retained_modules);
    assert_eq!(small.source_modules_reused, initial.retained_modules - 1);
    assert_eq!(small.reused_functions, (initial.retained_modules - 1) * 9);
    assert_eq!(small.snapshot_cloned_functions, 1);

    let repeat = checked(&program, &expanded);
    assert_eq!(repeat.stats().retained_modules, small.retained_modules);
    assert_eq!(repeat.stats().snapshot_cloned_functions, 0);
    assert_eq!(repeat.stats().reused_functions, 1 + (small.retained_modules - 1) * 9);

    program.modules[0] = parse_module(0, &root_text(0));
    let restored = checked(&program, &repeat);
    assert_eq!(restored.stats().retained_modules, initial.retained_modules);
    assert_eq!(restored.stats().reused_functions, (small.retained_modules - 1) * 9);
    assert_eq!(restored.stats().snapshot_cloned_functions,
               1 + (initial.retained_modules - small.retained_modules) * 9);
    // Adjacent epochs share immutable entries: growing/shrinking the new one
    // cannot alter a still-live earlier epoch or its charge receipts.
    assert_eq!(original.stats(), initial);
    println!("retention_transition initial={initial:?} expanded={small:?} restored={:?}", restored.stats());
}

#[test]
fn omitted_module_visibility_and_late_semantic_error_remain_live_for_the_whole_environment() {
    let mut program = project();
    let original = checked(&program, &ProgramMemo::default());
    assert!(original.stats().retained_modules < program.modules.len());
    let last = program.modules.len() - 1;
    // Stable-order production selection omitted the last equal-sized module.
    // Its exported declaration still belongs to the complete environment.
    program.modules[last] = parse_module(last, &leaf_text(false));
    let changed = checked(&program, &original);
    assert_eq!(changed.stats().reused_functions, 0);
    assert_eq!(changed.stats().source_modules_reused, 0);
    assert!(changed.stats().snapshot_cloned_functions > 0);

    let retained_stats = changed.stats();
    let invalid = leaf_text(false).replace("return y;", "return absent_name;");
    program.modules[last] = parse_module(last, &invalid);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let cold = check_program(&program, &target, &limits).unwrap_err();
    let warm = check_program_with_memo(
        &program, &target, &limits, Fingerprint::ZERO, &changed).err().unwrap();
    assert_eq!(warm, cold);
    assert_eq!(cold[0].span.unwrap().file, FileId(last as u32));
    assert_eq!(changed.stats(), retained_stats);
    program.modules[last] = parse_module(last, &leaf_text(false));
    let restored = checked(&program, &changed);
    assert_eq!(restored.stats().retained_modules, retained_stats.retained_modules);
    assert_eq!(restored.stats().source_modules_reused, retained_stats.retained_modules);
    assert_eq!(restored.stats().snapshot_cloned_functions, 0);
    println!("omitted_module_live visibility_invalidation={retained_stats:?} first_error={:?}", cold[0]);
}
