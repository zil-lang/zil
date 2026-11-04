use super::*;
const HIGHER_RANK: &str = include_str!("higher_rank.zil");
#[test]
fn terminal342_borrowed_pointer_invocation_retains_source_relation() {
    let report = compile_source(&source(HIGHER_RANK),&CompileOptions::default());
    assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
}
