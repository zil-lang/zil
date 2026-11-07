//! Exact acceptance boundaries use real source, never estimates from AST size.
use std::{fs, path::PathBuf};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};

fn boundary<T: PartialEq + std::fmt::Debug>(
    mut run: impl FnMut(usize) -> Result<T, Vec<Diagnostic>>,
    maximum: usize,
) -> (usize, String) {
    let expected = run(maximum).unwrap();
    let (mut low, mut high) = (0, maximum);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match run(mid) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = mid;
            }
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.phase == Phase::Types
                            && d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = mid;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    let below = run(high - 1).unwrap_err();
    assert!(
        below
            .iter()
            .all(|d| d.phase == Phase::Types && d.category == ResultCategory::ResourceLimit),
        "{below:?}"
    );
    assert_eq!(run(high).unwrap(), expected);
    (high, format!("{below:#?}"))
}

fn measure(
    label: &str,
    source: &str,
    old: Option<(usize, usize)>,
    origin_tables: Option<&[(usize, usize)]>,
) -> (usize, usize) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget202.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let defaults = Limits::default();
    let check = |limits: &Limits| zil_types::check(&core, &TargetProfile::host_llvm(), limits);
    if let Some(recorded_tables) = origin_tables {
        let actual = check(&defaults).unwrap();
        // Each completed helper contract calls flow::contract once, allocating
        // exactly these actual local/SSA provenance cells before reconstruction.
        let tables: Vec<_> = actual
            .functions
            .iter()
            .filter(|function| function.callable_contract.is_some())
            .map(|function| (function.locals.len(), function.values.len()))
            .collect();
        assert_eq!(
            tables, recorded_tables,
            "{label} actual helper origin tables"
        );
        let cells: usize = tables.iter().map(|(locals, values)| locals + values).sum();
        let prior = match label {
            "old-original02" => 6729,
            "old-two-elements" => 30319,
            "old-nested-helper" => 30931,
            _ => unreachable!(),
        };
        if cells != 0 {
            let error = check(&Limits {
                max_analysis_steps: prior,
                ..defaults.clone()
            })
            .unwrap_err();
            assert!(error.iter().all(|d| d.phase == Phase::Types
                && d.category == ResultCategory::ResourceLimit), "{error:?}");
        }
        // Origin's private role usize plus flags replaces each old u8. The
        // unchanged actual table dimensions, not an arbitrary floor, set N.
        let migrated = prior + cells * (std::mem::size_of::<(usize, u8)>() - 1);
        eprintln!(
            "{label}: actual Origin tables={tables:?}, cells={cells}, prior N={prior}, migrated N={migrated}"
        );
        assert_eq!(
            check(&Limits {
                max_analysis_steps: migrated,
                ..defaults.clone()
            })
            .unwrap(),
            actual
        );
    }
    let (work, work_below) = boundary(
        |n| {
            check(&Limits {
                max_analysis_steps: n,
                ..defaults.clone()
            })
        },
        defaults.max_analysis_steps,
    );
    let (locals, locals_below) = boundary(
        |n| {
            check(&Limits {
                max_locals: n,
                ..defaults.clone()
            })
        },
        defaults.max_locals,
    );
    let (functions, functions_below) = boundary(
        |n| {
            check(&Limits {
                max_functions: n,
                ..defaults.clone()
            })
        },
        defaults.max_functions,
    );
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let out = PathBuf::from(root).join(format!("budget202-{label}"));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("source.zil"), source).unwrap();
        fs::write(out.join("boundary.json"), format!("{{\"types_work\":{work},\"types_locals\":{locals},\"types_functions\":{functions},\"old_work_locals\":{},\"checks\":\"default == exact N == fresh N; N-1 Types ResourceLimit\"}}\n", old.map_or("null".into(), |(w,l)|format!("[{w},{l}]")))).unwrap();
        fs::write(out.join("below.txt"), format!("work N-1\n{work_below}\nlocals N-1\n{locals_below}\nfunctions N-1\n{functions_below}")).unwrap();
    }
    eprintln!("{label}: types_work={work}, locals={locals}, functions={functions}");
    (work, locals)
}

#[test]
fn complete_type_instances_have_exact_work_local_and_function_boundaries() {
    measure(
        "original-push",
        include_str!("complete_store_element_binding_fixtures/01_original_push.zil"),
        None,
        None,
    );
    measure(
        "original-factory",
        include_str!("complete_store_element_binding_fixtures/02_original_factory.zil"),
        None,
        None,
    );
}

#[test]
fn pre202_callback_store_paths_keep_their_recorded_types_boundaries() {
    const PAIR: &str = include_str!("callback_store_positions_fixtures/01_pair_store_mut.zil");
    let original = measure(
        "old-original02",
        include_str!("persistent_callback_state_fixtures/02_store_of_callback_state.zil"),
        Some((4241, 16)),
        Some(&[]),
    );
    let pair = measure("old-two-elements", PAIR, Some((11851, 39)), Some(&[]));
    let nested = format!(
        "fn invoke<F:FnMut()->i32>(callback:&mut F)->i32{{return callback()}}\n{}",
        PAIR.replace("first.callback()", "invoke(&mut first.callback)")
            .replace("second.callback()", "invoke(&mut second.callback)")
    );
    let nested = measure(
        "old-nested-helper",
        &nested,
        Some((11792, 40)),
        Some(&[(1, 2)]),
    );
    // 205 checks the old State definitions as well as their concrete instances.
    // Keep the original 201 values in each boundary record above; only the work
    // limits change after measuring these exact sources, including N-1/fresh.
    // See changes/205-legacy-types-budget.md for the original failure and the
    // accepted 204 baseline, whose test and input bytes were identical.
    assert_eq!(
        [original, pair, nested],
        // Prior 205 values: 6667/30137/30706. query work budget preserves the new
        // callable inventory and bounded rich-life/signature snapshot work;
        // see changes/G10.1.5-budget-regressions.md. N-1/fresh remain above.
        // independent callback roles's one actual 1-local/2-SSA Origin table adds 3*(16-1).
        [
            (6729, 16),
            (30319, 39),
            (30931 + 3 * (std::mem::size_of::<(usize, u8)>() - 1), 40)
        ],
        "205 measured State definition checks; original 201 input bytes unchanged"
    );
}
