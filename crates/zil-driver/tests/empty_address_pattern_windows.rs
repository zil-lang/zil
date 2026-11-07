//! A tagged Store result still needs pattern checks when there are no Address values.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{InstructionKind, PlaceProjection, Type};

const SOURCE: &str = r#"
resource Ticket{code:i32}
package Item{manages ticket:Ticket}
fn main()->i32{
 let values:Store<Item>=store_new();
 let pushed=store_push(values,Item{ticket:Ticket{code:65}});
 match pushed{.err(failed)=>{return failed.ticket.code-65},.ok(_)=>{return i32(store_len(values))-1}}
}
"#;

fn source() -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "empty_address_pattern_windows.zil",
        SOURCE,
    )
}

#[test]
fn tagged_store_result_checks_pattern_with_no_live_address_table() {
    let report = compile_source(
        &source(),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn malformed_pattern_projection_still_rejects_without_address_table() {
    let source = source();
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let mut raw = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap();
    assert!(
        !function
            .values
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
    );
    let projection = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match &mut instruction.kind {
            InstructionKind::TakeVariantField { path, .. } => Some(path),
            _ => None,
        })
        .unwrap();
    projection.push(PlaceProjection::Field(999));
    let diagnostics = zil_analyze::analyze(&raw, &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.phase == Phase::Analyze),
        "{diagnostics:#?}"
    );
}
