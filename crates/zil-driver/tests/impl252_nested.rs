use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
fn typed(text: &str) -> zil_ir::TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "nested252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
#[test]
fn original_nested_owned_callback_creates_moves_refills_and_calls() {
    let module = typed(include_str!("impl252_fixtures/nested_original.zil"));
    assert!(module.functions.iter().filter(|f|f.environment.is_some()).any(|f|f.locals.iter().any(|l|matches!(&l.ty,zil_ir::Type::Array{element,length:1} if matches!(element.as_ref(),zil_ir::Type::Closure(_))))));
}
