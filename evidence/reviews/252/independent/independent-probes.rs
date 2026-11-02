use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CallableCleanup, CallableReturnSource, InstructionKind, TypedModule};

fn types(text: &str) -> Result<TypedModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "independent252.zil", text);
    let syntax = zil_syntax::parse(&source).into_result()?;
    let core = zil_expand::expand(&source, &syntax)?;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default())
}

#[test]
fn two_normal_returns_preserve_nonzero_actual_parameter() {
    let m = types("fn choose<F:FnOnce(i32)->i32>(flag:bool,f:F)->F{if flag{return f}return f}fn main()->i32{let f=(x:i32)=>x+2;let g=choose(false,f);return g(3)-5}").unwrap();
    let helper = m.functions.iter().find(|f| f.callable_return_contract.is_some()).unwrap();
    let contract = helper.callable_return_contract.as_ref().unwrap();
    assert_eq!(contract.source, CallableReturnSource::Parameter(1));
    assert_eq!(helper.callable_contract.as_ref().unwrap().roles[0].cleanup, CallableCleanup::Returned);
    assert!(helper.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i.kind, InstructionKind::TakeLocal { .. })));
}

#[test]
fn generic_copy_shaped_f_moves_while_caller_can_copy_concrete_value() {
    let m = types("fn keep<F:Fn(i32)->i32>(f:F)->F{return f}fn main()->i32{let f=(x:i32)=>x+1;let g=keep(f);return f(2)+g(2)-6}").unwrap();
    let helper = m.functions.iter().find(|f| f.callable_return_contract.is_some()).unwrap();
    assert!(helper.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i.kind, InstructionKind::TakeLocal { .. })));
}

#[test]
fn moved_old_position_and_wrong_branch_cannot_return() {
    for source in [
        "fn bad<F:FnOnce(i32)->i32>(f:F)->F{let current=f;return f}fn main(){}",
        "fn bad<F:FnOnce(i32)->i32>(f:F)->F{if true{return f}let current=f;return f}fn main(){}",
    ] {
        assert!(types(source).is_err(), "accepted {source}");
    }
}

#[test]
fn readonly_mutable_forward_call_is_safe_reject() {
    let e = types("fn bad<F:FnMut(i32)->i32>(f:F)->F{let current=f;current(1);return current}fn main(){}").unwrap_err();
    assert!(e.iter().any(|d| d.category == ResultCategory::SafeReject && d.message.contains("mutable binding")), "{e:?}");
}
