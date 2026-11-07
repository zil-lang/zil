use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

const MIXED: &str = "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{var p=P{r:R{n:5},n:7};let P{r:moved,n:ref mut n}=p;*n=9;return moved.n+*n}fn main()->i32{return result()-14}";

#[test]
fn owned_package_named_pattern_moves_and_borrows_distinct_fields() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owned_package_pattern.zil", MIXED);
    let report = compile_source(&source, &CompileOptions::default());
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
}
