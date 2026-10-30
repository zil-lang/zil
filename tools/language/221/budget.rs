//! Limited baseline/combined budget harness. The compiler crates come from a
//! separately identified checkout; this file is never inserted into that tree.
use std::{fs, path::Path};
use zil_base::{
    CompileResult, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile,
};

fn boundary(
    label: &str,
    directory: &Path,
    mut attempt: impl FnMut(usize) -> CompileResult<()>,
) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    attempt(high).unwrap_or_else(|errors| panic!("{label}: {errors:#?}"));
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match attempt(n) {
            Ok(()) => high = n,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit),
                    "{label}: {errors:#?}"
                );
                low = n;
            }
        }
    }
    attempt(high).unwrap();
    let below = attempt(high - 1).unwrap_err();
    assert!(
        below
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    attempt(high).unwrap();
    fs::write(
        directory.join(format!("{label}-N-1.txt")),
        format!("{below:#?}"),
    )
    .unwrap();
    high
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    assert_eq!(
        arguments.len(),
        3,
        "expected SOURCE_DIRECTORY OUTPUT_DIRECTORY"
    );
    let sources = Path::new(&arguments[1]);
    let out = Path::new(&arguments[2]);
    let mut rows = Vec::new();
    for label in ["helper", "state", "opaque"] {
        let directory = out.join(label);
        fs::create_dir_all(&directory).unwrap();
        let text = fs::read_to_string(sources.join(format!("{label}.zil"))).unwrap();
        fs::write(directory.join("source.zil"), &text).unwrap();
        let unit = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            format!("budget221-{label}.zil"),
            text,
        );
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &syntax).unwrap();
        let limits = Limits::default();
        let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits)
            .unwrap_or_else(|errors| panic!("{label}: {errors:#?}"));
        let candidate = zil_analyze::analyze(&module, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
        let expected =
            zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits)
                .unwrap()
                .fingerprint();
        fs::write(directory.join("typed.txt"), format!("{module:#?}")).unwrap();
        fs::write(directory.join("candidate.txt"), format!("{candidate:#?}")).unwrap();
        fs::write(directory.join("input.wire"), &wire).unwrap();
        let selected = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let analyze = boundary("analyze", &directory, |n| {
            zil_analyze::analyze(&module, &selected(n)).map(|actual| assert_eq!(actual, candidate))
        });
        let verify = boundary("verify", &directory, |n| {
            zil_verify::verify_module(
                module.clone(),
                candidate.clone(),
                &module.target,
                &selected(n),
            )
            .map(|actual| assert_eq!(actual.fingerprint(), expected))
        });
        let wire_verify = boundary("verify-wire", &directory, |n| {
            zil_verify::verify_module_bytes(&wire, &module.target, &selected(n))
                .map(|actual| assert_eq!(actual.fingerprint(), expected))
        });
        println!(
            "{label}: Analyze N={analyze}; Verify N={verify}; Wire N={wire_verify}; N-1 ResourceLimit; fresh N identical"
        );
        rows.push(format!("{{\"source\":\"{label}\",\"analyze\":{analyze},\"verify\":{verify},\"verify_wire\":{wire_verify},\"typed_fingerprint\":\"{}\",\"verified_fingerprint\":\"{}\"}}", module.fingerprint(), expected));
    }
    fs::write(out.join("boundaries.json"), format!("{{\"status\":\"Pass\",\"checks\":\"N, N-1 ResourceLimit, fresh N; Candidate and Verified identity equal default\",\"rows\":[{}]}}\n", rows.join(","))).unwrap();
}
