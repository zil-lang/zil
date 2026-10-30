//! Real source controls for borrowed CT argument hints and repeated shape use.
use std::collections::BTreeMap;
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};

fn project(root: &str, library: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "root.zil", root),
            SourceUnit::new(FileId(1), ModuleId(1), "library.zil", library),
        ],
        paths: BTreeMap::from([(vec!["library".into()], ModuleId(1))]),
    }
}

const LIBRARY: &str = "pub fn sum(p: [[i64;2];2])->i64 { return p[0][0]+p[0][1]+p[1][0]+p[1][1]; }";
const SOURCE: &str = "import library as wide\nfn value()->i64 { let a=wide.sum([[#(2147483647+1),#(2)],[#(3),#(4)]]); let b=wide.sum([[#(5),#(6)],[#(7),#(8)]]); return a+b; }";

#[test]
fn repeated_cross_module_compound_hints_keep_ct_context_and_current_verify() {
    let report = compile_project(
        &project(SOURCE, LIBRARY),
        &CompileOptions {
            stop_after: Stage::Lower,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}

#[test]
fn borrowed_hints_do_not_grant_visibility_or_hide_argument_errors() {
    for (source, library, code) in [
        (SOURCE.to_string(), LIBRARY.replacen("pub ", "", 1), "E-MODULE-PRIVATE"),
        (SOURCE.replace("#(2)", "true"), LIBRARY.to_string(), "E-TYPE-MISMATCH"),
        (
            "import library as wide\nfn value()->i64 { return wide.sum([[1i64,2i64],[3i64,4i64]],0i64); }".into(),
            LIBRARY.to_string(),
            "E-CALL-ARITY",
        ),
    ] {
        let report = compile_project(&project(&source, &library), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    }
}

#[test]
fn compound_parameter_hints_preserve_narrow_ct_overflow_rejection() {
    let report = compile_project(
        &project(
            "import library\nfn value()->i32 { return library.sum([#(255+1),0u8]); }",
            "pub fn sum(p:[u8;2])->i32=>i32(p[0])",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        "E-CT-ARITHMETIC",
        "{}",
        report.to_json()
    );
}

#[test]
fn real_source_repeated_tuple_shapes_keep_complete_core_and_exact_node_boundary() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "repeated-tuples.zil",
        "fn sum(p:((i64,i64),(i64,i64),(i64,i64),(i64,i64)))->i64 { return p.0.0+p.3.1; } fn value()->i64 { let p=((1i64,2i64),(3i64,4i64),(5i64,6i64),(7i64,8i64)); let a=sum(p); let b=sum(p); let c=sum(p); let d=sum(p); return a+b+c+d; }",
    );
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let input = zil_expand::expand(&source, &parsed).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let (output, usage) =
        zil_ct::evaluate_source_constants_with_usage(&input, &target, &limits).unwrap();
    assert_eq!(output, input, "the entire untrusted Core remains unchanged");
    assert_eq!(
        usage.steps, 0,
        "runtime-only source executes no CT VM operations"
    );
    let exact = Limits {
        max_nodes: usage.nodes,
        ..limits.clone()
    };
    assert_eq!(
        zil_ct::evaluate_source_constants_with_usage(&input, &target, &exact).unwrap(),
        (output, usage)
    );
    let below = Limits {
        max_nodes: usage.nodes - 1,
        ..limits
    };
    let errors = zil_ct::evaluate_source_constants_with_usage(&input, &target, &below).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Ct);
    let report = zil_driver::compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Lower,
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
