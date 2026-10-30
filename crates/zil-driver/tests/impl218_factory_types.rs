//! Source-derived front-end checks. Ownership verification and native results
//! belong to the separately published 220/221 combination.
use zil_base::{Diagnostic, FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CallableKind, InstructionKind, Type, TypedModule};

const BASE: &str = "fn base(n:i32)->impl Fn(i32)->i32{return move(x:i32)=>x+n}";

fn typed(text: &str) -> Result<TypedModule, Vec<Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "factory218.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax)?;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default())
}

fn success(text: &str) -> TypedModule {
    typed(text).unwrap_or_else(|errors| panic!("{text}\n{errors:#?}"))
}

#[test]
fn three_declared_modes_create_real_owned_boundaries() {
    for (kind, body, expected) in [
        ("Fn", "return move(x:i32)=>f(x)+bias", CallableKind::Fn),
        ("FnMut", "var inner=f;return move(x:i32)=>inner(x)+bias", CallableKind::FnMut),
        ("FnOnce", "let inner=f;return move(x:i32)=>inner(x)+bias", CallableKind::FnOnce),
    ] {
        let text = format!("{BASE} fn wrap<F:{kind}(i32)->i32>(bias:i32,f:F)->impl {kind}(i32)->i32{{{body}}} fn main()->i32{{var output=wrap(3,base(2));return output(4)}}");
        let module = success(&text);
        let factory = module.functions.iter().find(|function| function.name.contains(":helper")).unwrap();
        assert!(factory.callable_contract.is_none());
        let contract = factory.callable_return_contract.as_ref().unwrap();
        assert_eq!(contract.kind, expected);
        let input = &factory.locals[factory.parameters[1].0 as usize].ty;
        assert!(matches!(input, Type::Closure(_)));
        assert_ne!(input, &factory.return_type);
        let body = &module.functions[module.closures[contract.closure.0 as usize].function.0 as usize];
        if expected == CallableKind::FnOnce {
            assert!(body.blocks.iter().flat_map(|block| &block.instructions)
                .any(|instruction| matches!(instruction.kind, InstructionKind::TakeCapture { .. })),
                "Fn passed as F:FnOnce must consume the actual captured position");
        }
    }
}

#[test]
fn complete_environment_identity_and_runtime_instances_remain_distinct() {
    let wrap = "fn wrap<F:Fn(i32)->i32>(f:F,bias:i32)->impl Fn(i32)->i32{return move(x:i32)=>f(x)+bias}";
    let same = success(&format!("{BASE}{wrap} fn main()->i32{{let a=wrap(base(1),2);let b=wrap(base(3),4);return a(5)+b(6)}}"));
    assert_eq!(same.functions.iter().filter(|function| function.name.contains(":helper")).count(), 1);
    let different = success(&format!("{BASE}{wrap} fn other(n:i32)->impl Fn(i32)->i32{{return move(x:i32)=>n-x}} fn main()->i32{{let a=wrap(base(1),2);let b=wrap(other(3),4);return a(5)+b(6)}}"));
    let results: std::collections::BTreeSet<_> = different.functions.iter()
        .filter(|function| function.name.contains(":helper"))
        .map(|function| function.callable_return_contract.as_ref().unwrap().closure)
        .collect();
    assert_eq!(results.len(), 2);
    success(&format!("{BASE}{wrap} fn main()->i32{{let a=wrap(wrap(base(1),2),3);return a(4)}}"));
}

#[test]
fn input_may_be_consumed_or_dropped_without_capture() {
    for body in ["return move(x:i32)=>x+2", "let seed=f(3);return move(x:i32)=>x+seed"] {
        success(&format!("{BASE} fn wrap<F:FnOnce(i32)->i32>(f:F)->impl Fn(i32)->i32{{{body}}} fn main()->i32{{let output=wrap(base(1));return output(4)}}"));
    }
}

#[test]
fn borrowed_direct_or_nested_input_cannot_become_self_contained() {
    let wrap = "fn wrap<F:Fn(i32)->i32>(f:F)->impl Fn(i32)->i32{return move(x:i32)=>f(x)}";
    for input in ["let n=3;let f=(x:i32)=>x+n;", "let n=3;let address=&n;let f=move(x:i32)=>x+*address;"] {
        let text = format!("{wrap} fn main()->i32{{{input}let output=wrap(f);return output(4)}}");
        let errors = typed(&text).expect_err("external input dependency must be rejected");
        assert!(errors.iter().any(|error| error.message.contains("retained callable") || error.message.contains("self-contained")), "{errors:#?}");
    }
}
