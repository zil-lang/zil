use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::{CallableKind, CoreModule, Type};

const SAME: &str = include_str!("impl203_fixtures/01_same_factory_distinct_modes.zil");
const TWICE: &str = include_str!("impl203_fixtures/02_once_repeated.zil");
const REVERSED: &str = include_str!("impl203_fixtures/03_reversed_declared_modes.zil");
const MARKERS: &str = include_str!("impl203_fixtures/04_mode_cleanup_markers.zil");

fn expand(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state203.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let result = zil_expand::expand_with_map(&source, &syntax).unwrap();
    result
        .source_map
        .validate(std::slice::from_ref(&source), &Limits::default())
        .unwrap();
    assert!(result.core.unsupported.is_empty());
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let out = std::path::PathBuf::from(root).join(source.fingerprint().to_string());
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("source.zil"), text).unwrap();
        std::fs::write(out.join("core.txt"), format!("{:#?}", result.core)).unwrap();
        std::fs::write(
            out.join("source-map.txt"),
            format!("{:#?}", result.source_map),
        )
        .unwrap();
    }
    result.core
}

fn positive(text: &str) {
    let core = expand(text);
    let state = core
        .functions
        .iter()
        .find(|f| !f.state_parameters.is_empty())
        .unwrap();
    assert!(state.callable_parameters.is_empty());
    assert!(state.type_parameters.is_empty());
    assert_eq!(state.state_parameters.len(), 1);
    let owner = &state.state_parameters[0];
    assert_eq!(owner.index, 0);
    assert_eq!(owner.bindings.len(), 2);
    let first = &owner.bindings[0];
    let second = &owner.bindings[1];
    assert_eq!((first.generic_index, second.generic_index), (0, 1));
    assert_eq!((&*first.generic_name, &*second.generic_name), ("F", "G"));
    assert_eq!(
        (first.kind, second.kind),
        (CallableKind::Fn, CallableKind::FnOnce)
    );
    assert_eq!(
        (&first.fields[..], &second.fields[..]),
        (&[1][..], &[2][..])
    );
    assert_eq!((&first.result, &second.result), (&Type::I32, &Type::I32));
    let constructor = core
        .functions
        .iter()
        .find(|f| f.callable_parameters.len() == 2)
        .unwrap();
    assert!(constructor.state_parameters.is_empty());
    assert_eq!(constructor.callable_parameters[0].index, 0);
    assert_eq!(constructor.callable_parameters[1].index, 1);
}

fn reject_once(text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state203-reject.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let errors = zil_expand::expand(&source, &syntax).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Expand
            && d.category == ResultCategory::SafeReject
            && d.code == "E-GENERIC-ARGUMENT"
            && d.message.contains("callback field used after")),
        "{errors:?}"
    );
}

#[test]
fn original_same_factory_has_two_declared_state_bindings() {
    positive(SAME);
}

#[test]
fn original_once_repetition_is_checked_before_instantiation() {
    reject_once(TWICE);
}

#[test]
fn original_reversed_modes_do_not_follow_one_representative_bound() {
    reject_once(REVERSED);
}

#[test]
fn original_cleanup_markers_keep_the_same_state_mapping() {
    positive(MARKERS);
}

fn unused(text: &str) -> String {
    format!(
        "{}fn main() -> i32 {{ return 0 }}\n",
        text.split("fn main()").next().unwrap()
    )
}

fn rejection(text: &str) -> Vec<zil_base::Diagnostic> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state203-negative.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &syntax).unwrap_err()
}

#[test]
fn marker_foreign_calls_require_unsafe_even_in_an_unused_definition() {
    let changed = MARKERS.replace("unsafe { putchar(77) }", "putchar(77)");
    for source in [&changed, &unused(&changed)] {
        let errors = rejection(source);
        assert!(
            errors.iter().any(|d| d.phase == Phase::Expand
                && d.category == ResultCategory::SafeReject
                && d.message == "foreign call requires an explicit unsafe block"),
            "{errors:?}"
        );
    }
    expand(&unused(MARKERS));
}

#[test]
fn marker_foreign_arguments_and_result_follow_the_real_declaration() {
    for changed in [
        MARKERS.replace("putchar(77)", "putchar(state.first)"),
        MARKERS.replace("putchar(77)", "putchar(true)"),
        MARKERS.replace(
            "unsafe { putchar(77) }",
            "unsafe { let marker: i64 = putchar(77) }",
        ),
    ] {
        let errors = rejection(&unused(&changed));
        assert!(errors.iter().any(|d| d.category == ResultCategory::SafeReject
            && d.code == "E-GENERIC-ARGUMENT"), "{errors:?}");
    }
}

#[test]
fn marker_unsafe_scope_retains_consumption_and_hides_its_locals() {
    reject_once(&MARKERS.replace("unsafe { putchar(77) }", "unsafe { state.second() }"));
    let source = MARKERS
        .replace(
            "unsafe { putchar(77) }",
            "unsafe { let local_marker = putchar(77) }",
        )
        .replace("first + again + second - 40", "local_marker");
    assert!(
        rejection(&source)
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject)
    );
}

#[test]
fn field_selectors_follow_aggregate_positions_not_function_binder_order() {
    let source = format!(
        "{}\n{}",
        SAME,
        r#"
fn reverse<G: FnOnce() -> i32, F: Fn() -> i32>(state: State<i32,F,G>) -> i32 {
    let first = state.first()
    let second = state.second()
    return first + second
}
fn swapped<F: Fn() -> i32, G: FnOnce() -> i32>(state: State<i32,G,F>) -> i32 {
    let first = state.first()
    let second = state.second()
    return first + second
}
"#
    );
    let core = expand(&source);
    for name in ["reverse", "swapped"] {
        let function = core.functions.iter().find(|f| f.name == name).unwrap();
        let bindings = &function.state_parameters[0].bindings;
        assert_eq!(
            (&bindings[0].fields[..], &bindings[1].fields[..]),
            (&[2][..], &[1][..])
        );
        let Type::Named(nominal) = &function.parameters[0].ty else {
            panic!("nominal State required")
        };
        assert!(nominal.contains("_$data"));
        let declaration = core.aggregates.iter().find(|a| a.name == *nominal).unwrap();
        for binding in bindings {
            assert_eq!(
                declaration.fields[binding.fields[0] as usize].ty,
                Type::Named(binding.generic_name.clone())
            );
        }
    }
}

#[test]
fn whole_t_store_identity_accepts_two_declared_callable_fields() {
    let source = format!(
        "{}\n{}",
        SAME,
        r#"
fn identity<T: StoreElement>(value: T) -> T { return value }
fn relay<F: Fn() -> i32, G: FnOnce() -> i32>(state: State<i32,F,G>) -> State<i32,F,G> {
    return identity(state)
}
"#
    );
    let core = expand(&source);
    let identity = core
        .functions
        .iter()
        .find(|f| f.name == "identity")
        .unwrap();
    assert_eq!(identity.type_parameters.len(), 1);
    assert!(identity.state_parameters.is_empty());
    assert!(identity.callable_parameters.is_empty());
    assert!(format!("{core:#?}").contains("TypeApply"));
}

#[test]
fn a_state_owner_is_not_two_runtime_arguments_and_has_only_two_direct_fields() {
    for changed in [
        SAME.replace(
            "(state: State<T,F,G>)",
            "(state: State<T,F,G>, other: State<T,F,G>)",
        ),
        SAME.replace("second: G; tag: T", "second: G; extra: F; tag: T")
            .replace(
                "second: second, tag: tag",
                "second: second, extra: first, tag: tag",
            ),
        SAME.replace("second: G; tag: T", "second: (G,); tag: T")
            .replace("second: second, tag: tag", "second: (second,), tag: tag"),
    ] {
        let errors = rejection(&changed);
        assert!(
            errors.iter().all(|d| d.phase == Phase::Expand),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|d| matches!(
                d.category,
                ResultCategory::Unsupported | ResultCategory::SafeReject
            )),
            "{errors:?}"
        );
    }
}

#[test]
fn two_declared_callback_results_cannot_gain_lending_from_concrete_arguments() {
    let changed = SAME.replace("F: Fn() -> i32", "F: Fn() -> &[u8]");
    let errors = rejection(&unused(&changed));
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
}

#[test]
fn dual_state_metadata_and_marker_work_have_fresh_exact_budget_boundaries() {
    for (label, text) in [("dual", SAME), ("markers", MARKERS)] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "state203-budget.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let check = |n| {
            zil_expand::expand_with_map_limits(
                &source,
                &syntax,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        check(high).unwrap();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match check(n) {
                Ok(_) => high = n,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = n;
                }
            }
        }
        check(high).unwrap();
        assert!(
            check(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        check(high).unwrap();
        eprintln!("state203_{label}_expand_n={high}");
    }
}

#[test]
fn a_single_root_child_family_keeps_its_old_identity_inside_a_dual_scope() {
    let source = format!(
        "{}\n{}",
        SAME,
        r#"
package Item<H> { manages marker: Ticket; callback: H }
fn single<H: Fn() -> i32>(value: H) -> Item<H> {
    return Item::<H> { marker: Ticket { code: 90 }, callback: value }
}
fn extract<F: Fn() -> i32, G: FnOnce() -> i32>(state: State<i32,F,G>) -> Item<F> {
    return Item::<F> { marker: Ticket { code: 90 }, callback: state.first }
}
"#
    );
    let core = expand(&source);
    let mut families = Vec::new();
    for name in ["single", "extract"] {
        let function = core.functions.iter().find(|f| f.name == name).unwrap();
        let Type::Named(nominal) = &function.return_type else {
            panic!("complete Item required")
        };
        assert!(!nominal.contains("_$data"), "{nominal}");
        families.push(nominal.rsplit_once('_').unwrap().0);
    }
    assert_eq!(families[0], families[1]);
}

#[test]
fn an_owned_dual_state_can_remain_complete_on_an_error_union_side() {
    let original = include_str!("impl203_fixtures/05_independent_error_side.zil");
    let core = expand(original);
    expand(&original.replace("    return 0\n}", "    return -1\n}"));
    let function = core
        .functions
        .iter()
        .find(|f| !f.state_parameters.is_empty())
        .unwrap();
    assert_eq!(function.state_parameters[0].bindings.len(), 2);
    assert!(matches!(function.return_type, Type::ErrorUnion { .. }));
    for changed in [
        original.replace(
            "error_route::<State<T,F,G>>(full)",
            "error_route::<State<T,G,F>>(full)",
        ),
        original.replace(
            "let routed:State<T,F,G>!void",
            "let routed:void!State<T,F,G>",
        ),
        original.replace("->State<T,F,G>!i32", "->(State<T,F,G>, i32)"),
        original.replace("    return 0\n}", "    return true\n}"),
        original.replace("    return 0\n}", "    return i64(0)\n}"),
    ] {
        let errors = rejection(&changed);
        assert!(
            errors.iter().any(|d| matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
            "{errors:?}"
        );
    }
}
