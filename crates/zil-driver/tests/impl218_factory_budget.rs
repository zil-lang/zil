//! Limited front-end budget probes. The three legacy inputs are byte-for-byte
//! shared with 221; no Analyze, Verify or native acceptance is inferred here.
use std::{fmt::Debug, fs, path::PathBuf};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};

fn boundary<T: PartialEq + Debug>(
    mut run: impl FnMut(usize) -> Result<T, Vec<Diagnostic>>,
    maximum: usize,
    phase: Phase,
) -> usize {
    let expected = run(maximum).unwrap();
    let (mut low, mut high) = (0, maximum);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = middle;
            }
            Err(errors) => {
                assert!(!errors.is_empty());
                assert!(
                    errors.iter().all(|error| error.phase == phase
                        && error.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = middle;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    let below = run(high - 1).unwrap_err();
    assert!(!below.is_empty());
    assert!(
        below
            .iter()
            .all(|error| error.phase == phase && error.category == ResultCategory::ResourceLimit),
        "{below:#?}"
    );
    assert_eq!(run(high).unwrap(), expected);
    high
}

fn probe(label: &str, legacy: bool) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/218/budget-sources")
        .join(format!("{label}.zil"));
    let text = fs::read_to_string(fixture).unwrap();
    let name = format!("budget{}-{label}.zil", if legacy { 221 } else { 218 });
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let expand = boundary(
        |n| {
            zil_expand::expand_with_map_limits(
                &source,
                &syntax,
                &Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                },
            )
            .map(|expanded| expanded.core)
        },
        defaults.max_analysis_steps,
        Phase::Expand,
    );
    let types = boundary(
        |n| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                },
            )
        },
        defaults.max_analysis_steps,
        Phase::Types,
    );
    let locals = boundary(
        |n| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_locals: n,
                    ..defaults.clone()
                },
            )
        },
        defaults.max_locals,
        Phase::Types,
    );
    let functions = boundary(
        |n| {
            zil_types::check(
                &core,
                &target,
                &Limits {
                    max_functions: n,
                    ..defaults.clone()
                },
            )
        },
        defaults.max_functions,
        Phase::Types,
    );
    let summary = format!(
        "{{\"case\":\"{label}\",\"expand_work\":{expand},\"types_work\":{types},\"types_locals\":{locals},\"types_functions\":{functions},\"checks\":\"default == N == fresh N; N-1 phase-specific ResourceLimit\"}}\n"
    );
    eprintln!("{summary}");
    if let Some(root) = std::env::var_os("ZIL_218_BUDGET_OUT") {
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(format!("{label}.json")), summary).unwrap();
        fs::write(root.join(format!("{label}.core.txt")), format!("{core:#?}")).unwrap();
        fs::write(
            root.join(format!("{label}.typed.txt")),
            format!("{typed:#?}"),
        )
        .unwrap();
    }
}

#[test]
fn legacy_helper_state_and_opaque_exact_boundaries() {
    for label in ["helper", "state", "opaque"] {
        probe(label, true);
    }
}

#[test]
fn owning_factory_and_forwarded_nested_capture_exact_boundaries() {
    for label in ["owning", "forwarding"] {
        probe(label, false);
    }
}
