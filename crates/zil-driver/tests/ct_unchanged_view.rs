//! The borrowing CT view must retain the old evaluator's exact work and errors.
use zil_base::{
    CompileResult, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ct::{SourceEvaluationUsage, SourceProgramContext};
use zil_ir::{CoreExprKind, CoreImport, CoreModule, CoreProgram, CoreStatementKind, Type};

fn core(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "unchanged.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}

fn old(
    core: &CoreModule,
    limits: &Limits,
    context: Option<&SourceProgramContext<'_>>,
) -> CompileResult<(CoreModule, SourceEvaluationUsage)> {
    let target = TargetProfile::default();
    match context {
        Some(context) => {
            zil_ct::evaluate_source_constants_with_context(core, &target, limits, context)
        }
        None => zil_ct::evaluate_source_constants_with_usage(core, &target, limits),
    }
}

/// `None` leaves the old evaluator in charge. Every selected result, including
/// a failure, is checked against the complete old result rather than its category.
fn compare(
    core: &CoreModule,
    limits: &Limits,
    context: Option<&SourceProgramContext<'_>>,
    require_selected: bool,
) -> bool {
    let expected = old(core, limits, context);
    let actual =
        zil_ct::validate_unchanged_source(core, &TargetProfile::default(), limits, context);
    if require_selected {
        assert!(actual.is_some(), "ordinary source unexpectedly declined");
    }
    match actual {
        Some(Ok(usage)) => {
            let (evaluated, old_usage) = expected.unwrap();
            assert_eq!(&evaluated, core, "a selected borrowing path changed Core");
            assert_eq!(usage, old_usage);
            true
        }
        Some(Err(errors)) => {
            assert_eq!(errors, expected.unwrap_err());
            true
        }
        None => false,
    }
}

const ORDINARY: &str = r#"
fn add(x:i32)->i32{return x+1;}
fn discard(x:())->(){}
extern "C" fn putchar(x:i32)->i32;
fn result()->i32{
    let empty=();discard(empty);
    var i:i32=0;var sum:i32=7;
    while i<4{
        i=i+1;
        if i==2{continue;}
        sum=sum+add(i);
        if i==4{break;}
    }
    unsafe{putchar(sum);}
    let sign=if !(sum<0){1}else{2};
    return i32(u16(sum))+sign;
}
"#;

#[test]
fn ordinary_loops_calls_hints_and_foreign_unsafe_are_exactly_unchanged() {
    let core = core(ORDINARY);
    assert!(compare(&core, &Limits::default(), None, true));
    let (_, usage) = old(&core, &Limits::default(), None).unwrap();
    assert_eq!(usage.steps, 0);
    assert!(usage.nodes > core.functions.len());
}

#[test]
fn imported_scalar_call_hints_use_the_same_current_context() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import dep\nfn result()->i64{let x=dep.plus(9);return x+1;}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "dep.zil",
            "pub fn plus(x:i64)->i64{return x+3;}",
        ),
    ];
    let ast = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = [
        vec![CoreImport {
            alias: "dep".into(),
            module: ModuleId(1),
            span: ast[0].items[0].span,
        }],
        vec![],
    ];
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &ast.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: expanded[0].0.core.source_fingerprint,
        modules: expanded.into_iter().map(|(module, _)| module).collect(),
    };
    let context = SourceProgramContext::new(&program, &Limits::default()).unwrap();
    for module in &program.modules {
        assert!(compare(
            &module.core,
            &Limits::default(),
            Some(&context),
            true
        ));
    }
}

#[derive(Clone, Copy)]
enum Budget {
    Nodes,
    Work,
    Depth,
}
fn limits(budget: Budget, n: usize) -> Limits {
    let mut limits = Limits::default();
    match budget {
        Budget::Nodes => limits.max_nodes = n,
        Budget::Work => limits.max_analysis_steps = n,
        Budget::Depth => limits.max_call_depth = n,
    }
    limits
}

#[test]
fn exact_node_work_and_depth_boundaries_keep_old_usage_and_first_error() {
    let core = core(ORDINARY);
    for budget in [Budget::Nodes, Budget::Work, Budget::Depth] {
        let mut low = 0;
        let mut high = match budget {
            Budget::Nodes => Limits::default().max_nodes,
            Budget::Work => Limits::default().max_analysis_steps,
            Budget::Depth => Limits::default().max_call_depth,
        };
        assert!(old(&core, &limits(budget, low), None).is_err());
        assert!(old(&core, &limits(budget, high), None).is_ok());
        // Find the old evaluator's boundary independently of the new validator.
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            if old(&core, &limits(budget, middle), None).is_ok() {
                high = middle;
            } else {
                low = middle;
            }
        }
        for n in [high - 1, high, high + 1] {
            let limits = limits(budget, n);
            let result = old(&core, &limits, None);
            if n < high {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit)
                );
            } else {
                assert_eq!(result.unwrap().0, core);
            }
            // A bounded eligibility scan may decline tiny node/depth limits;
            // then the unchanged old evaluator remains the driver fallback.
            let selected = compare(&core, &limits, None, false);
            if matches!(budget, Budget::Work) {
                assert!(
                    selected,
                    "work boundary must exercise the new checker's exact error path"
                );
            }
        }
    }
}

#[test]
fn raw_schema_and_conversion_errors_match_and_transforming_shapes_fall_back() {
    let mut invalid_schema = core("fn result()->i32{return 3;}");
    invalid_schema.schema_version += 1;
    assert!(compare(&invalid_schema, &Limits::default(), None, true));
    assert!(old(&invalid_schema, &Limits::default(), None).is_err());

    let mut invalid_conversion = core("fn result()->i32{return i32(3);}");
    let CoreStatementKind::Return(Some(expression)) =
        &mut invalid_conversion.functions[0].body.statements[0].kind
    else {
        panic!("actual source return");
    };
    let CoreExprKind::IntegerConvert { target, .. } = &mut expression.kind else {
        panic!("actual source integer conversion");
    };
    *target = Type::Bool;
    assert!(old(&invalid_conversion, &Limits::default(), None).is_err());
    assert!(compare(&invalid_conversion, &Limits::default(), None, true));

    for text in [
        "fn result()->i32{return #(1+2);}",
        "fn result()->i32{let add=x=>x+1;return add(4);}",
        "fn take(x:(i32,i32))->i32{return x.0;} fn result()->i32{return take((1,2));}",
    ] {
        let core = core(text);
        assert!(
            zil_ct::validate_unchanged_source(
                &core,
                &TargetProfile::default(),
                &Limits::default(),
                None
            )
            .is_none(),
            "{text}"
        );
    }
}
