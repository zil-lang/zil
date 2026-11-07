//! Integration regression found by the root-authored 173 composed consumer.
//! Independent approval is recorded separately; this is not a reviewer claim.
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

#[test]
fn existing_owned_error_helper_abi_survives_unrelated_retained_callback_inference() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "inference173.zil",
        r#"
resource R{n:i32}
package P{manages r:R}
fn failed()->P!void{return P{r:R{n:7}}}
fn relay(value:P!void)->P!void{return value}
fn kept()->impl Fn()->i32{let n=7;return move()=>n}
fn main()->i32{let callback=kept();relay(failed());return callback()-7}
"#,
    );
    let report = compile_source(&source, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
