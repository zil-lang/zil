//! Real source and the unchanged production capacity, not a reduced test cap.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, QueryDisposition};
use zil_ir::CoreProgram;
use zil_types::{ProgramMemo, check_program, check_program_with_memo};

fn source(index: usize, value: u32) -> SourceUnit {
    let text = if index == 0 {
        format!("fn main()->i32=>{value}")
    } else {
        let mut text = String::new();
        for n in 0..8 {
            text.push_str(&format!(
                "fn f{n}(x:i64)->i64 {{return (x*{}+{})%251;}}\n",
                n + 2,
                n * 3 + 1
            ));
        }
        text.push_str("pub fn run(x:i64)->i64 {var y=x;");
        for n in 0..8 {
            text.push_str(&format!("y=f{n}(y);"));
        }
        text.push_str("return y;}");
        text
    };
    SourceUnit::new(
        FileId(index as u32),
        ModuleId(index as u32),
        format!("m{index}.zil"),
        text,
    )
}

fn expanded(source: &SourceUnit) -> zil_ir::ProgramModule {
    let parsed = zil_syntax::parse(source).into_result().unwrap();
    zil_expand::expand_program_module(source, &parsed, vec![])
        .unwrap()
        .0
}

#[test]
fn production_64mib_partially_retains_1024_real_scalar_modules_and_matches_cold_typed() {
    let mut program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: (0..=1024).map(|i| expanded(&source(i, 1))).collect(),
    };
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let (first, memo) = check_program_with_memo(
        &program,
        &target,
        &limits,
        Fingerprint::ZERO,
        &ProgramMemo::default(),
    )
    .unwrap();
    assert_eq!(first, check_program(&program, &target, &limits).unwrap());
    drop(first);
    let stats = memo.stats();
    assert!(stats.eligible && stats.retained_modules > 1);
    assert!(stats.retained_modules < program.modules.len(), "{stats:?}");
    assert!(stats.capacity_skipped_modules > 0);
    assert!(stats.retained_weight <= 64 * 1024 * 1024);
    assert_eq!(
        stats.snapshot_cloned_functions,
        1 + (stats.retained_modules - 1) * 9
    );
    program.modules[0] = expanded(&source(0, 9));
    let (edited, next) =
        check_program_with_memo(&program, &target, &limits, Fingerprint::ZERO, &memo).unwrap();
    assert_eq!(edited, check_program(&program, &target, &limits).unwrap());
    drop(edited);
    assert_eq!(
        next.stats().source_modules_reused,
        stats.retained_modules - 1
    );
    assert_eq!(next.stats().snapshot_cloned_functions, 1);
    assert_eq!(
        next.stats().reused_functions,
        (stats.retained_modules - 1) * 9
    );
    assert!(next.stats().retained_weight <= 64 * 1024 * 1024);
    program.modules[0] = expanded(&source(0, 1));
    let (restored, last) =
        check_program_with_memo(&program, &target, &limits, Fingerprint::ZERO, &next).unwrap();
    assert_eq!(restored, check_program(&program, &target, &limits).unwrap());
    assert_eq!(last.stats().snapshot_cloned_functions, 1);
    eprintln!(
        "CP32 production-cap cold={stats:?}, edit={:?}, restore={:?}",
        next.stats(),
        last.stats()
    );
}

#[test]
fn source_receipt_reuse_does_not_reuse_analysis_or_verification_for_an_edit() {
    let project = |value| ProjectSources {
        root: ModuleId(0),
        paths: Default::default(),
        modules: (0..40).map(|i| source(i, value)).collect(),
    };
    let options = CompileOptions::default();
    let mut session = CompilationSession::default();
    let cold = session.compile_project(&project(1), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    let edited = session.compile_project(&project(9), &options);
    let fresh = zil_driver::compile_project(&project(9), &options);
    assert_eq!(
        edited.status,
        ResultCategory::Success,
        "{}",
        edited.to_json()
    );
    assert_eq!(edited.scalar_interfaces, fresh.scalar_interfaces);
    assert_eq!(edited.diagnostics, fresh.diagnostics);
    assert_eq!(edited.verification_runs, 1);
    assert!(
        edited
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Analyze && e.disposition == QueryDisposition::Executed)
    );
    assert_eq!(session.type_memo_stats().source_modules_reused, 39);
    assert_eq!(session.type_memo_stats().source_modules_checked, 1);
    assert_eq!(session.type_memo_stats().snapshot_cloned_functions, 1);
    session.clear();
    assert_eq!(session.retained_type_memo_stats().retained_weight, 0);
}
