//! Stage localization for the unchanged 24-level independent helper input.
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

#[test]
fn finite_forward_chain_finishes_each_frontend_stage_on_the_default_test_stack() {
    let mut text = String::new();
    for i in 0..24 {
        if i == 23 {
            text.push_str(&format!("fn h{i}<F:Fn()->i32>(f:&F)->i32{{return f()}}"));
        } else {
            text.push_str(&format!(
                "fn h{i}<F:Fn()->i32>(f:&F)->i32{{return h{}(f)+1}}",
                i + 1
            ));
        }
    }
    text.push_str("fn main()->i32{let value=5;let f=()=>value;return h0(&f)-28}");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text);
    for stop_after in [Stage::Parse, Stage::Core, Stage::Types] {
        eprintln!("review188 depth24 entering {stop_after:?}");
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after,
                ..CompileOptions::default()
            },
        );
        eprintln!(
            "review188 depth24 returned {stop_after:?}: {:?}",
            report.status
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}
