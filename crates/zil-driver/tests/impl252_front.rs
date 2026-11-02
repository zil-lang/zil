//! Source-to-Types author tests; independent kernels and native evidence separate.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn checked(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
#[test]
fn original_forward_source_reaches_types_without_changes() {
    let typed = checked(include_str!("impl252_fixtures/original.zil"));
    let helper = typed
        .functions
        .iter()
        .find(|f| {
            f.callable_return_contract
                .as_ref()
                .is_some_and(|c| matches!(c.source, CallableReturnSource::Parameter(_)))
        })
        .unwrap();
    assert_eq!(
        helper.callable_return_contract.as_ref().unwrap().source,
        CallableReturnSource::Parameter(0)
    );
    assert_eq!(
        helper.callable_contract.as_ref().unwrap().roles[0].cleanup,
        CallableCleanup::Returned
    );
}
#[test]
fn original_concrete_copy_and_generic_declared_modes_remain_distinct() {
    checked(
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn twice<F:Fn(i32)->i32>(f:F)->i32{let current=keep(f);return current(1)+current(2)}fn main()->i32{let offset=3;let f=move(n:i32)=>n+offset;let current=keep(f);return current(1)+current(2)+f(3)+twice(f)-24}",
    );
}
#[test]
fn mutable_owned_environment_can_be_called_then_forwarded() {
    checked(
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return keep(current)}fn main()->i32{var count=0;var f=move(n:i32)=>{count+=n;return count};var current=step(f);return current(2)-3}",
    );
}
