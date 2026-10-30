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
        (
            "FnMut",
            "var inner=f;return move(x:i32)=>inner(x)+bias",
            CallableKind::FnMut,
        ),
        (
            "FnOnce",
            "let inner=f;return move(x:i32)=>inner(x)+bias",
            CallableKind::FnOnce,
        ),
    ] {
        let text = format!(
            "{BASE} fn wrap<F:{kind}(i32)->i32>(bias:i32,f:F)->impl {kind}(i32)->i32{{{body}}} fn main()->i32{{var output=wrap(3,base(2));return output(4)}}"
        );
        let module = success(&text);
        let factory = module
            .functions
            .iter()
            .find(|function| function.name.contains(":helper"))
            .unwrap();
        assert!(factory.callable_contract.is_none());
        let contract = factory.callable_return_contract.as_ref().unwrap();
        assert_eq!(contract.kind, expected);
        let input = &factory.locals[factory.parameters[1].0 as usize].ty;
        assert!(matches!(input, Type::Closure(_)));
        assert_ne!(input, &factory.return_type);
        let body =
            &module.functions[module.closures[contract.closure.0 as usize].function.0 as usize];
        if expected == CallableKind::FnOnce {
            assert!(
                body.blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| matches!(
                        instruction.kind,
                        InstructionKind::TakeCapture { .. }
                    )),
                "Fn passed as F:FnOnce must consume the actual captured position"
            );
        }
    }
}

#[test]
fn complete_environment_identity_and_runtime_instances_remain_distinct() {
    let wrap =
        "fn wrap<F:Fn(i32)->i32>(f:F,bias:i32)->impl Fn(i32)->i32{return move(x:i32)=>f(x)+bias}";
    let same = success(&format!(
        "{BASE}{wrap} fn main()->i32{{let a=wrap(base(1),2);let b=wrap(base(3),4);return a(5)+b(6)}}"
    ));
    assert_eq!(
        same.functions
            .iter()
            .filter(|function| function.name.contains(":helper"))
            .count(),
        1
    );
    let different = success(&format!(
        "{BASE}{wrap} fn other(n:i32)->impl Fn(i32)->i32{{return move(x:i32)=>n-x}} fn main()->i32{{let a=wrap(base(1),2);let b=wrap(other(3),4);return a(5)+b(6)}}"
    ));
    let results: std::collections::BTreeSet<_> = different
        .functions
        .iter()
        .filter(|function| function.name.contains(":helper"))
        .map(|function| function.callable_return_contract.as_ref().unwrap().closure)
        .collect();
    assert_eq!(results.len(), 2);
    success(&format!(
        "{BASE}{wrap} fn main()->i32{{let a=wrap(wrap(base(1),2),3);return a(4)}}"
    ));
}

#[test]
fn input_may_be_consumed_or_dropped_without_capture() {
    for body in [
        "return move(x:i32)=>x+2",
        "let seed=f(3);return move(x:i32)=>x+seed",
    ] {
        success(&format!(
            "{BASE} fn wrap<F:FnOnce(i32)->i32>(f:F)->impl Fn(i32)->i32{{{body}}} fn main()->i32{{let output=wrap(base(1));return output(4)}}"
        ));
    }
}

#[test]
fn borrowed_direct_or_nested_input_cannot_become_self_contained() {
    let wrap = "fn wrap<F:Fn(i32)->i32>(f:F)->impl Fn(i32)->i32{return move(x:i32)=>f(x)}";
    for input in [
        "let n=3;let f=(x:i32)=>x+n;",
        "let n=3;let address=&n;let f=move(x:i32)=>x+*address;",
    ] {
        let text = format!("{wrap} fn main()->i32{{{input}let output=wrap(f);return output(4)}}");
        let errors = typed(&text).expect_err("external input dependency must be rejected");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("retained callable")
                    || error.message.contains("self-contained")),
            "{errors:#?}"
        );
    }
}

#[test]
fn factory_body_can_forward_a_grounded_new_factory_result() {
    for (kind, body) in [
        ("Fn", "return move(x:i32)=>f(x)+bias"),
        ("FnMut", "var inner=f;return move(x:i32)=>inner(x)+bias"),
        ("FnOnce", "return move(x:i32)=>f(x)+bias"),
    ] {
        let text = format!(
            "{BASE} fn wrap<F:{kind}(i32)->i32>(f:F,bias:i32)->impl {kind}(i32)->i32{{{body}}} fn outer<G:{kind}(i32)->i32>(g:G)->impl {kind}(i32)->i32{{return wrap(g,2)}} fn main()->i32{{var output=outer(base(3));return output(4)}}"
        );
        let module = success(&text);
        let helpers: Vec<_> = module
            .functions
            .iter()
            .filter(|function| function.name.contains(":helper"))
            .collect();
        assert_eq!(helpers.len(), 2);
        assert!(
            helpers
                .iter()
                .all(|function| function.callable_contract.is_none())
        );
        assert_eq!(helpers[0].return_type, helpers[1].return_type);
    }
}

#[test]
fn same_concrete_type_does_not_share_declaration_mode() {
    let text = "fn base()->impl Fn()->i32{return move()=>3} fn wrap<F:FnOnce()->i32>(f:F)->impl FnOnce()->i32{let other=base();return move()=>other()+other()+f()} fn main()->i32{let output=wrap(base());return output()}";
    let module = success(text);
    let output = module
        .functions
        .iter()
        .find(|function| function.name.contains(":helper"))
        .unwrap()
        .callable_return_contract
        .as_ref()
        .unwrap()
        .closure;
    let definition = &module.closures[output.0 as usize];
    assert_eq!(definition.captures.len(), 2);
    assert_eq!(definition.captures[0], definition.captures[1]);
    let body = &module.functions[definition.function.0 as usize];
    assert_eq!(
        body.blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(
                instruction.kind,
                InstructionKind::CallClosureOnce { .. }
            ))
            .count(),
        1
    );
    assert_eq!(
        body.blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(
                instruction.kind,
                InstructionKind::CallClosureBorrowed {
                    kind: CallableKind::Fn,
                    ..
                }
            ))
            .count(),
        2
    );
}

#[test]
fn declared_once_survives_nested_capture_and_actual_mutable_input() {
    success(
        "fn base()->impl Fn()->i32{return move()=>3} fn wrap<F:FnOnce()->i32>(f:F)->impl FnOnce()->i32{return move()=>{let inner=move()=>f();return inner()}} fn main()->i32{let output=wrap(base());return output()}",
    );
    success(
        "fn base(n:i32)->impl FnMut(i32)->i32{var total=n;return move(x:i32)=>{total+=x;return total}} fn wrap<F:FnOnce(i32)->i32>(f:F)->impl FnOnce(i32)->i32{return move(x:i32)=>f(x)} fn main()->i32{let output=wrap(base(3));return output(4)}",
    );
}

#[test]
fn copy_actual_argument_retains_the_callers_original_position() {
    success(&format!(
        "{BASE} fn wrap<F:Fn(i32)->i32>(f:F)->impl Fn(i32)->i32{{return move(x:i32)=>f(x)}} fn main()->i32{{let input=base(2);let output=wrap(input);return input(3)+output(4)}}"
    ));
}
