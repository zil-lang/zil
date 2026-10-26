use zil_base::{FileId, ModuleId, SourceUnit};
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
fn unused_generic_attributes_and_bounds_are_not_erased() {
    for text in [
        "#[inline]\nfn id<T>(x:T)->T=>x\nfn sample()->i32=>0",
        "fn id<T: Copy>(x:T)->T=>x\nfn sample()->i32=>0",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "attrs.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand_with_map(&source, &parsed).unwrap_err();
        assert!(errors.iter().any(|error| error.code == "E-UNSUPPORTED"));
    }
}
