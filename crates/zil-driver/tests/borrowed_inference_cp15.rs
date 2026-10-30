//! Real source through parsing/expansion; inference results remain untrusted Core.
use std::borrow::Cow;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::{CoreExprKind, CoreProgram, CoreStatementKind, Type};

fn project(sources: &[&str]) -> CoreProgram {
    let modules = sources
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let source = SourceUnit::new(
                FileId(index as u32),
                ModuleId(index as u32),
                format!("m{index}.zil"),
                *text,
            );
            let parsed = zil_syntax::parse(&source).into_result().unwrap();
            zil_expand::expand_program_module(&source, &parsed, Vec::new())
                .unwrap()
                .0
        })
        .collect();
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules,
    }
}

#[test]
fn unchanged_scalar_project_borrows_the_exact_original_storage() {
    let input = project(&["fn first(x: i32)->i32=>x+1", "fn second(x: i64)->i64=>x-1"]);
    let result = zil_types::infer_program_closures_borrowed(&input, &Limits::default()).unwrap();
    let Cow::Borrowed(borrowed) = result else {
        panic!("unchanged inference must not clone the project");
    };
    assert!(std::ptr::eq(borrowed, &input));
    assert!(std::ptr::eq(
        &borrowed.modules[1].core.functions[0],
        &input.modules[1].core.functions[0]
    ));
    assert_eq!(
        zil_types::infer_program_closures(&input, &Limits::default()).unwrap(),
        input
    );
}

#[test]
fn rewritten_function_preserves_both_unchanged_neighbors_and_source() {
    let input = project(&[
        "fn before()->i32=>1\nfn middle()->i32 { let f=(x)=>x; return f(7); }\nfn after()->i32=>2",
    ]);
    let original = input.clone();
    let result = zil_types::infer_program_closures_borrowed(&input, &Limits::default()).unwrap();
    assert!(matches!(result, Cow::Owned(_)));
    let functions = &result.modules[0].core.functions;
    assert_eq!(functions.len(), 3);
    assert_eq!(functions[0], input.modules[0].core.functions[0]);
    assert_eq!(functions[2], input.modules[0].core.functions[2]);
    assert_eq!(functions[1].id, input.modules[0].core.functions[1].id);
    let CoreStatementKind::Binding {
        initializer: Some(initializer),
        ..
    } = &functions[1].body.statements[0].kind
    else {
        panic!("expected the source binding");
    };
    let CoreExprKind::Closure {
        parameters,
        return_type,
        ..
    } = &initializer.kind
    else {
        panic!("expected the inferred source closure");
    };
    assert_eq!(parameters[0].ty, Some(Type::I32));
    assert_eq!(*return_type, Some(Type::I32));
    assert_eq!(input, original);
}

#[test]
fn normalized_project_keeps_changes_in_a_module_without_closures() {
    let input = project(&[
        "struct Cell { value: i32 }\nfn plain(x: Cell)->i32=>x.value",
        "fn use_closure()->i32 { let f=(x)=>x; return f(7); }",
    ]);
    let result = zil_types::infer_program_closures_borrowed(&input, &Limits::default()).unwrap();
    assert!(matches!(result, Cow::Owned(_)));
    let Type::Record { name, fields } = &result.modules[0].core.functions[0].parameters[0].ty
    else {
        panic!("normalization in an otherwise unchanged module must be retained");
    };
    assert_eq!(name, "@module0:Cell");
    assert_eq!(fields, &vec![("value".into(), Type::I32)]);
    assert_eq!(
        input.modules[0].core.functions[0].parameters[0].ty,
        Type::Named("Cell".into())
    );
}

#[test]
fn borrowing_does_not_skip_metadata_or_zero_work_failures() {
    let mut input = project(&["fn first()->i32=>1", "fn second()->i32=>2"]);
    let limits = Limits {
        max_analysis_steps: 0,
        ..Limits::default()
    };
    let errors = zil_types::infer_program_closures_borrowed(&input, &limits).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Types);
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(
        errors[0].message,
        "source closure constraint work budget exceeded"
    );
    assert_eq!(
        errors[0].span,
        Some(input.modules[0].core.functions[0].span)
    );
    input.modules[1].core.module_id = ModuleId(0);
    let errors =
        zil_types::infer_program_closures_borrowed(&input, &Limits::default()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Types);
    assert_eq!(errors[0].code, "E-MODULE-ID");
    assert_eq!(errors[0].message, "duplicate module identity");
}
