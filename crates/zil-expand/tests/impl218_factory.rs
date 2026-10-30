use zil_base::{Diagnostic, FileId, ModuleId, SourceUnit};
use zil_ir::{CallableKind, CoreModule};

fn expand(text: &str) -> Result<CoreModule, Vec<Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "factory218.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map(&source, &syntax).map(|result| result.core)
}

#[test]
fn unused_public_factory_definitions_keep_their_real_bound_and_result() {
    for (kind, body, wanted) in [
        ("Fn", "return move (x:i32)=>f(x)+bias", CallableKind::Fn),
        ("FnMut", "var inner=f; return move (x:i32)=>inner(x)+bias", CallableKind::FnMut),
        ("FnOnce", "let inner=f; return move (x:i32)=>inner(x)+bias", CallableKind::FnOnce),
    ] {
        let text = format!("pub fn wrap<F:{kind}(i32)->i32>(bias:i32,f:F)->impl {kind}(i32)->i32{{{body}}} fn main(){{}}");
        let core = expand(&text).unwrap_or_else(|error| panic!("{text}\n{error:#?}"));
        assert!(core.unsupported.is_empty(), "{:#?}", core.unsupported);
        let factory = core.functions.iter().find(|function| function.name == "wrap").unwrap();
        assert_eq!(factory.callable_parameters.len(), 1);
        assert_eq!(factory.callable_parameters[0].index, 1);
        assert_eq!(factory.callable_parameters[0].kind, wanted);
        assert_eq!(factory.callable_return.as_ref().unwrap().kind, wanted);
    }
}

#[test]
fn unused_factory_checks_capture_consumption_and_hidden_identity() {
    let bodies = [
        "return move (x:i32)=>f(x)+f(x)",
        "let a=f; return move (x:i32)=>f(x)",
        "let c=move (x:i32)=>f(x); let d=move (x:i32)=>f(x); return c",
        "return move (x:i32)=>{while x>0 {f(x)} return 0}",
        "return f",
        "if true {return move (x:i32)=>f(x)} else {return move (x:i32)=>f(x)}",
    ];
    for body in bodies {
        let text = format!("fn bad<F:FnOnce(i32)->i32>(f:F)->impl FnOnce(i32)->i32{{{body}}} fn main(){{}}");
        let error = expand(&text).expect_err(&text);
        assert!(!error.is_empty());
    }
}

#[test]
fn boundary_keeps_borrowed_multiple_generic_and_lending_inputs_closed() {
    for signature in [
        "fn bad<F:Fn(i32)->i32>(f:&F)->impl Fn(i32)->i32",
        "fn bad<F:Fn(i32)->i32>(f:F,g:F)->impl Fn(i32)->i32",
        "fn bad<T:Copy,F:Fn(i32)->i32>(f:F,x:T)->impl Fn(i32)->i32",
        "fn bad<F:Fn(i32)->&i32>(f:F)->impl Fn(i32)->i32",
        "fn bad<F:Fn(i32)->i32>(f:F)->impl Fn(i32)->&i32",
    ] {
        let text = format!("{signature}{{return move (x:i32)=>f(x)}} fn main(){{}}");
        assert!(expand(&text).is_err(), "{text}");
    }
}

#[test]
fn input_can_be_settled_without_being_retained_in_the_result() {
    for body in [
        "return move (x:i32)=>x+1",
        "let value=f(4); return move (x:i32)=>x+value",
        "let c=move (x:i32)=>f(x); return c",
    ] {
        let result = if body.contains("=>f(x)") { "FnOnce" } else { "Fn" };
        let text = format!("fn wrap<F:FnOnce(i32)->i32>(f:F)->impl {result}(i32)->i32{{{body}}} fn main(){{}}");
        let core = expand(&text).unwrap_or_else(|error| panic!("{text}\n{error:#?}"));
        assert!(core.unsupported.is_empty(), "{:#?}", core.unsupported);
    }
}
