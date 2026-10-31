//! Non-author public API controls over parsed and expanded real source.
//! This executable is correctness evidence only, never timing evidence.
use zil_base::{FileId, Fingerprint, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::CoreProgram;
use zil_types::{check_program, check_program_with_memo, ProgramMemo};

fn core(texts: &[&str]) -> CoreProgram {
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: texts.iter().enumerate().map(|(i, text)| {
            let source = SourceUnit::new(FileId(i as u32), ModuleId(i as u32),
                                         format!("review{i}.zil"), *text);
            let parsed = zil_syntax::parse(&source).into_result().unwrap();
            zil_expand::expand_program_module(&source, &parsed, vec![]).unwrap().0
        }).collect(),
    }
}

#[test]
fn repeated_mixed_scalar_and_live_foreign_epochs_equal_cold() {
    let io = r#"extern "C" fn getchar()->i32;
fn read()->i32{var x=0;unsafe{x=getchar()}return x}"#;
    let last = "fn stable(x:i32)->i32 { var y=x; while y<10 { y=y+1; } return y; }";
    let bodies = [
        "fn edit()->i32=>1",
        "fn edit()->i32 { let x=1; if x==1 { return 7; } else { return 2; } }",
        "fn edit()->i32 { var x=0; while x<3 { x=x+1; } return x; }",
        "fn edit()->i32=>9",
    ];
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let mut previous = ProgramMemo::default();
    for turn in 0..12 {
        let io = io.replace("var x=0", if turn % 2 == 0 {"var x=0"} else {"var x=1"});
        let source = core(&[&io, bodies[turn % bodies.len()], last]);
        let expected = check_program(&source, &target, &limits).unwrap();
        let (actual, next) = check_program_with_memo(
            &source, &target, &limits, Fingerprint::ZERO, &previous).unwrap();
        assert_eq!(actual, expected, "epoch {turn}");
        assert!(next.stats().eligible);
        assert_eq!(next.stats().retained_modules, 2);
        if turn > 0 {
            assert_eq!(next.stats().reused_functions, 1, "epoch {turn}");
            assert_eq!(next.stats().checked_functions, 3, "epoch {turn}");
        }
        previous = next;
    }
}

#[test]
fn unchanged_later_builder_quota_failure_is_identical_in_four_dimensions() {
    let first = "fn a()->i32=>1";
    let enlarged = "fn a()->i32 { let p=1; if true { return p; } else { return 0; } }";
    let last = "fn b()->i32 { let x=1; let y=2; let z=3; if true { return x+y+z; } else { return 0; } }";
    let source = core(&[first, last]);
    let changed = core(&[enlarged, last]);
    let prefix = core(&[enlarged]);
    let target = TargetProfile::host_llvm();
    for dimension in 0..4 {
        let limits = (1..256).find_map(|n| {
            let mut limits = Limits::default();
            match dimension {
                0 => limits.max_blocks = n,
                1 => limits.max_instructions = n,
                2 => limits.max_locals = n,
                _ => limits.max_values = n,
            }
            if check_program(&source, &target, &limits).is_err()
                || check_program(&prefix, &target, &limits).is_err() { return None; }
            let error = check_program(&changed, &target, &limits).err()?;
            (error.first()?.span?.file == FileId(1)).then_some(limits)
        }).expect("a limit that admits the changed first body but fails in unchanged second body");
        let (_, previous) = check_program_with_memo(
            &source, &target, &limits, Fingerprint::ZERO, &ProgramMemo::default()).unwrap();
        let cold = check_program(&changed, &target, &limits).unwrap_err();
        let warm = check_program_with_memo(&changed, &target, &limits, Fingerprint::ZERO, &previous)
            .err().unwrap();
        assert_eq!(cold[0].category, ResultCategory::ResourceLimit);
        assert_eq!(cold[0].span.unwrap().file, FileId(1));
        assert_eq!(warm, cold, "dimension {dimension}");
        let (restored, retained) = check_program_with_memo(
            &source, &target, &limits, Fingerprint::ZERO, &previous).unwrap();
        assert_eq!(restored, check_program(&source, &target, &limits).unwrap());
        assert_eq!(retained.stats().reused_functions, 2);
        println!("quota dimension={dimension} cold_and_warm_first_error={:?}", cold[0]);
    }
}

#[test]
fn intervening_semantic_rejection_cannot_poison_a_successful_epoch() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = core(&["fn a()->i32=>1", "fn b()->i32=>2"]);
    let (_, previous) = check_program_with_memo(
        &original, &target, &limits, Fingerprint::ZERO, &ProgramMemo::default()).unwrap();
    for text in ["fn a()->i32=>unknown", "fn a()->i32=>true", "fn a()->i32 { let x=true; return x; }"] {
        let changed = core(&[text, "fn b()->i32=>2"]);
        let cold = check_program(&changed, &target, &limits).unwrap_err();
        let warm = check_program_with_memo(&changed, &target, &limits, Fingerprint::ZERO, &previous)
            .err().unwrap();
        assert_eq!(warm, cold);
        let (actual, next) = check_program_with_memo(
            &original, &target, &limits, Fingerprint::ZERO, &previous).unwrap();
        assert_eq!(actual, check_program(&original, &target, &limits).unwrap());
        assert_eq!(next.stats().reused_functions, 2);
    }
}
