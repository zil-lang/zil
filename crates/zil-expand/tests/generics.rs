use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::Type;

#[test]
fn generic_records_use_structural_arguments_and_fresh_source_inexpressible_names() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "records.zil",
        r#"
struct Pair<T> { first: T; second: T }
fn first<T>(pair: Pair<T>) -> T => pair.first
fn sample() -> i64 {
    let a: Pair<i64> = Pair::<i64> { first: 1, second: 2 }
    let b: Pair<(i64)> = Pair::<(i64)> { first: 3, second: 4 }
    return first::<i64>(a)
}
"#,
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&source, &parsed).unwrap();
    assert!(expanded.core.unsupported.is_empty());
    assert_eq!(expanded.core.aggregates.len(), 1);
    assert!(
        expanded.core.aggregates[0]
            .name
            .starts_with("$generic_Pair_")
    );
    assert!(
        expanded.core.aggregates[0]
            .fields
            .iter()
            .all(|field| field.ty == Type::I64)
    );
    assert_eq!(expanded.core.functions.len(), 2);
    expanded
        .source_map
        .validate(&[source], &zil_base::Limits::default())
        .unwrap();
}

#[test]
fn unused_generic_declaration_attributes_are_not_erased() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "attrs.zil",
        "#[inline]\nfn id<T>(x:T)->T=>x\nfn sample()->i32=>0",
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let errors = zil_expand::expand_with_map(&source, &parsed).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let diagnostic = &errors[0];
    assert_eq!(diagnostic.phase, Phase::Expand);
    assert_eq!(diagnostic.category, ResultCategory::Unsupported);
    assert_eq!(diagnostic.code, "E-UNSUPPORTED");
    assert_eq!(
        diagnostic.message,
        "unexecuted generic declaration attribute"
    );
    assert!(diagnostic.span.unwrap().is_valid_for(&source));
}

#[test]
fn unused_generic_bounds_require_declared_nominal_traits() {
    for (prefix, bound) in [
        ("", "Copy"),
        ("", "MissingRead"),
        ("struct Read { value: i32 }\n", "Read"),
    ] {
        let text = format!("{prefix}fn id<T: {bound}>(x:T)->T=>x\nfn sample()->i32=>0");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "bounds.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand_with_map(&source, &parsed).unwrap_err();
        assert_eq!(errors.len(), 1, "{bound}: {errors:#?}");
        let diagnostic = &errors[0];
        assert_eq!(diagnostic.phase, Phase::Expand);
        assert_eq!(diagnostic.category, ResultCategory::SafeReject);
        assert_eq!(diagnostic.code, "E-GENERIC-ARGUMENT");
        assert_eq!(
            diagnostic.message,
            format!("generic bound {bound} is not a declared nominal trait")
        );
        let span = diagnostic.span.unwrap();
        assert!(span.is_valid_for(&source));
        assert_eq!(&source.text[span.start..span.end], bound);
    }
}

#[test]
fn unused_generic_with_declared_bound_checks_its_symbolic_method_body() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "declared-bound.zil",
        r#"
trait Read { fn read(self: &Self) -> i32; }
fn get<T: Read>(value: &T) -> i32 => value.read()
fn sample() -> i32 => 0
"#,
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&source, &parsed).unwrap();
    assert!(expanded.core.unsupported.is_empty());
    assert_eq!(expanded.core.functions.len(), 1);
    assert_eq!(expanded.core.functions[0].name, "sample");
    expanded
        .source_map
        .validate(&[source], &zil_base::Limits::default())
        .unwrap();
}
