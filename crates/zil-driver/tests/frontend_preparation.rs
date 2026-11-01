//! Current-input preparation reuse must agree with the ordinary raw entry point.
use std::collections::BTreeMap;
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, Stage};
use zil_ir::{CoreProgram, ProgramModule};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "preparation.zil", text)
}
fn core(text: &str) -> CoreProgram {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    }
}
fn ct(program: &CoreProgram) -> CoreProgram {
    let mut result = program.clone();
    for module in &mut result.modules {
        module.core = zil_ct::evaluate_source_constants(
            &module.core,
            &TargetProfile::default(),
            &Limits::default(),
        )
        .unwrap();
    }
    result
}

#[test]
fn ordinary_source_borrows_current_input_and_matches_raw_types_after_ct() {
    let program = core("fn twice(x:i32)->i32{return x*2;} fn result()->i32{return twice(7);}");
    let limits = Limits::default();
    let inferred = zil_types::infer_program_for_ct(&program, &limits).unwrap();
    assert!(std::ptr::eq(inferred.program(), &program));
    let evaluated = ct(inferred.program());
    assert_eq!(evaluated, program);
    let expected =
        zil_types::check_program(&evaluated, &TargetProfile::default(), &limits).unwrap();
    let prepared = inferred.after_evaluation(evaluated);
    let actual =
        zil_types::check_prepared_program(&prepared, &TargetProfile::default(), &limits).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn changed_ct_and_closure_inference_still_run_the_ordinary_types_path() {
    for (text, borrowed) in [
        ("fn result()->i64{return #(1+2);}", true),
        ("fn result()->i32{let add=x=>x+1;return add(4);}", false),
    ] {
        let program = core(text);
        let limits = Limits::default();
        let inferred = zil_types::infer_program_for_ct(&program, &limits).unwrap();
        assert_eq!(std::ptr::eq(inferred.program(), &program), borrowed);
        let evaluated = ct(inferred.program());
        assert_ne!(evaluated, program);
        let expected =
            zil_types::check_program(&evaluated, &TargetProfile::default(), &limits).unwrap();
        let prepared = inferred.after_evaluation(evaluated);
        assert_eq!(
            zil_types::check_prepared_program(&prepared, &TargetProfile::default(), &limits)
                .unwrap(),
            expected
        );
        let report = CompilationSession::default().compile_project(
            &ProjectSources {
                root: ModuleId(0),
                modules: vec![source(text)],
                paths: BTreeMap::new(),
            },
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
}

#[test]
fn changed_limits_or_declarations_cannot_retain_the_previous_preparation() {
    let program = core("fn a()->i32{return 7;} fn b()->i32{return a();}");
    let limits = Limits::default();
    let prepared = zil_types::infer_program_for_ct(&program, &limits)
        .unwrap()
        .after_evaluation(program.clone());
    for changed in [
        Limits {
            max_nodes: 1,
            ..limits.clone()
        },
        Limits {
            max_functions: 1,
            ..limits.clone()
        },
        Limits {
            max_analysis_steps: 1,
            ..limits.clone()
        },
        Limits {
            max_call_depth: 0,
            ..limits.clone()
        },
    ] {
        let expected =
            zil_types::check_program(&program, &TargetProfile::default(), &changed).unwrap_err();
        assert_eq!(
            zil_types::check_prepared_program(&prepared, &TargetProfile::default(), &changed)
                .unwrap_err(),
            expected
        );
    }
    let mut changed = program.clone();
    changed.modules[0].core.functions[1].name = "a".into();
    changed.modules[0].core.functions[1].span.start += 1;
    let expected =
        zil_types::check_program(&changed, &TargetProfile::default(), &limits).unwrap_err();
    let prepared = zil_types::infer_program_for_ct(&program, &limits)
        .unwrap()
        .after_evaluation(changed);
    assert_eq!(
        zil_types::check_prepared_program(&prepared, &TargetProfile::default(), &limits)
            .unwrap_err(),
        expected
    );
}

#[test]
fn unchanged_preparation_retains_the_current_shared_owner() {
    let program = std::sync::Arc::new(core("fn result()->i32{return 7;}"));
    let limits = Limits::default();
    let inferred = zil_types::infer_shared_program_for_ct(&program, &limits).unwrap();
    assert!(std::ptr::eq(inferred.program(), program.as_ref()));
    let prepared = inferred.into_prepared();
    assert!(std::ptr::eq(prepared.program(), program.as_ref()));
    assert_eq!(
        zil_types::check_prepared_program(&prepared, &TargetProfile::default(), &limits).unwrap(),
        zil_types::check_program(&program, &TargetProfile::default(), &limits).unwrap()
    );
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[test]
fn plain_prefix_then_ct_fallback_restores_the_exact_project_allowance() {
    let dir = std::env::temp_dir().join(format!("zil-preparation-mixed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                dir.join("main.zil").to_string_lossy(),
                "import dep\nfn result()->i32{return dep.value();}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                dir.join("dep.zil").to_string_lossy(),
                "pub fn value()->i32{return #(2+3);}",
            ),
        ],
        paths: BTreeMap::from([(vec!["dep".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        backend: zil_driver::NativeBackend::LlvmApi,
        optimization: zil_driver::Optimization::O2,
        incremental_o2: true,
        stop_after: Stage::Build,
        entry: Some("result".into()),
        output: Some(dir.join("program")),
        // The fast prefix must not consume this allowance a second time when
        // the following CT module switches the whole request to the old path.
        limits: Limits {
            max_functions: 2,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    for expected in [5, 5, 9] {
        if expected == 9 {
            project.modules[1].text = "pub fn value()->i32{return #(4+5);}".into();
        }
        let report = session.compile_project(&project, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = std::process::Command::new(dir.join("program"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
        assert!(output.stderr.is_empty());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
