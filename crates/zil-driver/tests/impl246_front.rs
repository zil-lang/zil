//! Author source-to-Types evidence; independent kernels/native consumers are separate.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn checked(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front246.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "reject246.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    if let Ok(core) = zil_expand::expand(&source, &ast) {
        assert!(
            zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).is_err(),
            "accepted: {text}"
        );
    }
}
const DATA: &str = "package Event{manages payload:ByteBuffer;tag:i32}package Reply{manages payload:ByteBuffer;tag:i32}";
#[test]
fn original_three_callback_sources_reach_types_without_changes() {
    for text in [
        include_str!("impl246_fixtures/event_reply.zil"),
        include_str!("impl246_fixtures/event_copy_result.zil"),
        include_str!("impl246_fixtures/owned_result.zil"),
    ] {
        checked(text);
    }
}

#[test]
fn fresh_owned_arguments_do_not_consume_the_fn_or_fnmut_environment() {
    for (declaration, prelude, body, kind) in [
        ("let", "", "", CallableKind::Fn),
        ("var", "var count=0;", "count+=1;", CallableKind::FnMut),
    ] {
        let t = checked(&format!(
            "{DATA}fn main()->i32{{{prelude}{declaration} handler=(event:Event)->Reply=>{{{body}return Reply{{payload:give event.payload,tag:event.tag+1}}}};let a=handler(Event{{payload:bytes_new(),tag:1}});let b=handler(Event{{payload:bytes_new(),tag:2}});return a.tag+b.tag-5}}"
        ));
        assert_eq!(t.closures.len(), 1);
        assert_eq!(t.closures[0].kind, kind);
    }
}

#[test]
fn concrete_sum_error_union_and_store_product_signatures_are_real_data() {
    for (head, ty, input, body, result) in [
        (
            "enum Packet{Empty,Data(Event)}",
            "Packet",
            "Packet.Data(Event{payload:bytes_new(),tag:3})",
            "return event",
            "Packet",
        ),
        (
            "enum Fault{NoMemory}",
            "Fault!Event",
            "Event{payload:bytes_new(),tag:3}",
            "let value=event?;return value",
            "Fault!Event",
        ),
        (
            "package Leaf{manages bytes:ByteBuffer}package Items{manages values:Store<Leaf>}",
            "Items",
            "Items{values:store_new()}",
            "return event",
            "Items",
        ),
    ] {
        checked(&format!(
            "{DATA}{head}fn dispatch<F:FnOnce({ty})->{result}>(f:F,event:{ty})->{result}{{return f(event)}}fn main(){{let handler=(event:{ty})->{result}=>{{{body}}};let input:{ty}={input};let reply=dispatch(handler,input)}}"
        ));
    }
}

#[test]
fn a_closure_in_an_ordinary_owned_error_result_function_preserves_both_branches() {
    checked(&format!(
        "{DATA}enum Packet{{Empty,Data(Event)}}package Fault{{packet:Packet}}fn dispatch<F:FnOnce(Event)->Fault!Reply>(f:F,event:Event)->Fault!Reply{{return f(event)}}fn main()->Fault!i32{{let handler=(event:Event)->Fault!Reply=>{{return Fault{{packet:Packet.Data(event)}}}};let reply=dispatch(handler,Event{{payload:bytes_new(),tag:3}})?;return reply.tag}}"
    ));
}

#[test]
fn unused_owned_dispatch_definitions_are_checked_and_can_discard_the_callback() {
    checked(&format!(
        "{DATA}fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply{{return Reply{{payload:give event.payload,tag:event.tag}}}}fn main(){{}}"
    ));
    for declaration in [
        "fn dispatch<F:Fn(Event)->Reply>(f:F,event:Event)->Reply{return f(event)}",
        "fn dispatch<F:FnMut(Event)->Reply>(f:&mut F,event:Event)->Reply{return f(event)}",
        "fn dispatch<F:FnOnce(Event)->Reply>(f:&F,event:Event)->Reply{return f(event)}",
        "fn dispatch<F:FnOnce(Event)->Reply,T:Copy>(f:F,event:Event,extra:T)->Reply{return f(event)}",
        "fn dispatch<F:FnOnce(Event)->Reply,G:Fn()->i32>(f:F,g:G,event:Event)->Reply{return f(event)}",
        "fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply{let a=f(event);return f(event)}",
        "fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply{let a=f(event);return event}",
    ] {
        rejected(&format!("{DATA}{declaration}fn main(){{}}"));
    }
}

#[test]
fn owned_signature_does_not_open_addresses_managed_roots_or_capture_profiles() {
    for source in [
        format!(
            "{DATA}fn main(){{let f=(e:Event,n:&i32)->Reply=>{{return Reply{{payload:give e.payload,tag:e.tag}}}}}}"
        ),
        format!(
            "{DATA}fn main(){{let f=(bytes:ByteBuffer)->Reply=>{{return Reply{{payload:give bytes,tag:1}}}}}}"
        ),
        format!("{DATA}fn main(){{let f=(e:Event)->&i32=>{{return &e.tag}}}}"),
        format!(
            "{DATA}fn main(){{let p=Event{{payload:bytes_new(),tag:1}};let f=(e:Event)->Reply=>{{let n=p.tag;return Reply{{payload:give e.payload,tag:n}}}}}}"
        ),
        format!(
            "{DATA}fn main(){{let values:Store<i32>=store_new();let f=move(e:Event)->Reply=>{{store_len(values);return Reply{{payload:give e.payload,tag:e.tag}}}}}}"
        ),
        format!(
            "{DATA}enum Packet{{Data(Event)}}fn main(){{let p=Packet.Data(Event{{payload:bytes_new(),tag:1}});let f=move(e:Event)->Reply=>{{let q=p;return Reply{{payload:give e.payload,tag:e.tag}}}}}}"
        ),
        format!(
            "{DATA}fn make()->impl Fn(Event)->Reply{{return (e:Event)->Reply=>{{return Reply{{payload:give e.payload,tag:e.tag}}}}}}fn main(){{}}"
        ),
        format!(
            "{DATA}fn main(){{let f:fn(Event)->Reply=(e:Event)->Reply=>{{return Reply{{payload:give e.payload,tag:e.tag}}}}}}"
        ),
    ] {
        rejected(&source);
    }
}
