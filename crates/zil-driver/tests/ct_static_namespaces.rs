//! Actual Source successors of the CT host/source consumer. Ordinary methods
//! stay in the input, and runtime qualification remains a separate gate.
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "static-namespace.zil", text)
}
fn assert_source(text: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
#[test]
fn g4462_host_complete_context() {
    assert_source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-full-context.zil"
    ));
}
#[test]
fn g4462_host_member_distinct_lives() {
    assert_source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-member-lives.zil"
    ));
}
#[test]
fn g4462_host_member_nearest_life_shadow() {
    assert_source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-member-life-shadow.zil"
    ));
}
#[test]
fn g3_4_1_life_duplicates_still_reject_within_the_current_declaration() {
    let original = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-member-life-shadow.zil"
    );
    for text in [
        original.replace("fn make<life s>", "fn make<life s,life s>"),
        original.replace("Host<life s>", "Host<life s,life s>"),
        "fn duplicate<life s,life s>()->i32{return 0}fn main()->i32{return 0}".into(),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "E-LIFE-DECLARATION"
                    && diagnostic.message == "duplicate life binder"
            }),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}
#[test]
fn g4462_generated_host_member_distinct_lives() {
    assert_source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generated-host-member-lives.zil"
    ));
}
#[test]
fn g4462_type_const_life_shadow_keeps_member_owner() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-static-host-shadow-all-kinds.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
#[test]
fn g4462_plain_and_unrelated_ct_keep_runtime_methods_gate() {
    for value in ["7", "#(7)"] {
        let text = format!(
            "struct Host<life a>{{fn make()->i32{{return 42}}}}fn main()->i32{{return {value}}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d
                .message
                .contains("generic resource or methods require capability resolution")),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn g4462_real_ct_use_cannot_hide_unused_bad_sibling() {
    for sibling in [
        "fn bad<T:Copy>(value:T)->T{return value+value}",
        "fn bad()->i32{let n=1;let address=&n;return n}",
        "fn bad(self:Self)->i32{return 1}",
    ] {
        let text = format!(
            "struct Host<life a>{{fn make()->i32{{return 42}}{sibling}}}fn query<life a>(p:&'a i32)->i32{{return #(Host::<'a>.make())}}fn main()->i32{{let n=0;return query(&n)}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::ResourceLimit,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Ct),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn g4462_real_ct_use_does_not_authorize_runtime_nominal_instances() {
    for (generic, host_args, field) in [("life a", "'a", ""), ("T:Copy", "i32", "")] {
        let text = format!(
            "struct Host<{generic}>{{fn make()->i32{{return 42}}}}fn query<life a>(p:&'a i32)->i32{{let n:i32=#(Host::<{host_args}>.make());let real:Host<{host_args}>=Host::<{host_args}>{{{field}}};return n}}fn main()->i32{{let n=0;return query(&n)}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d
                .message
                .contains("generic resource or methods require capability resolution")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn g4462_unexecuted_host_call_has_no_successful_namespace_use() {
    let text = "struct Host<life a>{fn make()->i32{return 1/0}}fn query<life a>(p:&'a i32)->i32{return #(if false {Host::<'a>.make()}else{7})}fn main()->i32{let n=0;return query(&n)}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d
            .message
            .contains("generic resource or methods require capability resolution")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g4462_successful_unit_call_uses_its_exact_instruction() {
    assert_source(
        "struct Host<life a>{fn touch(){return}}fn query<life a>(p:&'a i32)->i32{#(Host::<'a>.touch());return 42}fn main()->i32{let n=0;return query(&n)}",
    );
}

#[test]
fn g4462_failed_whole_request_does_not_publish_completed_host_calls() {
    let bad = "struct Host<life a>{fn make()->i32{return 42}}fn fail<life a>()->i32{let n=Host::<'a>.make();return 1/0}fn query<life a>(p:&'a i32)->i32{return #(fail::<'a>())}fn main()->i32{let n=0;return query(&n)}";
    let mut session = zil_driver::CompilationSession::default();
    let report = session.compile_source(&source(bad), &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_ne!(
        report.status,
        ResultCategory::ResourceLimit,
        "{}",
        report.to_json()
    );
    assert_eq!(session.active_query_count(), 0);
    let fresh = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-member-lives.zil"
    );
    let report = session.compile_source(&source(fresh), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(session.active_query_count(), 0);
}
