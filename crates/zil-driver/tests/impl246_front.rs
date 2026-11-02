//! Author source-to-Types evidence; independent kernels/native consumers are separate.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn checked(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front246.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
#[test]
fn original_three_callback_sources_reach_types_without_changes() {
    for text in [include_str!("impl246_fixtures/event_reply.zil"),
        include_str!("impl246_fixtures/event_copy_result.zil"),
        include_str!("impl246_fixtures/owned_result.zil")] {
        checked(text);
    }
}
