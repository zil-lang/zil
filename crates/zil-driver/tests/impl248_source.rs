//! Original source bytes through the independent Verify boundary. Native
//! execution and failure cleanup are measured by the consumer slice.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::InstructionKind;

#[test]
fn original_owned_callback_sources_reach_verify() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (name, text) in [
        (
            "event_reply",
            include_str!("impl246_fixtures/event_reply.zil"),
        ),
        (
            "event_copy_result",
            include_str!("impl246_fixtures/event_copy_result.zil"),
        ),
        (
            "owned_result",
            include_str!("impl246_fixtures/owned_result.zil"),
        ),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed, candidate, &target, &limits).unwrap();
    }
}

#[test]
fn owning_dispatch_can_have_independent_copy_input_and_result() {
    let text = "package Event{manages payload:ByteBuffer;tag:i32}package Reply{manages payload:ByteBuffer;tag:i32}fn dispatch<F:FnOnce(Event)->Reply>(h:F,e:Event,flag:bool)->i32{let r=h(e);if flag{return r.tag}return 0}fn main()->i32{let b=bytes_new();let h=(e:Event)->Reply=>{return Reply{payload:give e.payload,tag:e.tag+1}};return dispatch(h,Event{payload:give b,tag:6},true)-7}";
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "extra.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, candidate, &target, &limits).unwrap();
}

#[test]
fn copy_callback_can_feed_an_owned_helper_result() {
    let text = "package Reply{manages payload:ByteBuffer;tag:i32}fn make_reply(tag:i32)->Reply{let b=bytes_new();return Reply{payload:give b,tag:tag}}fn wrap<F:FnOnce(i32)->i32>(h:F,x:i32)->Reply{return make_reply(h(x))}fn main()->i32{let h=(x:i32)->i32=>{return x+1};let r=wrap(h,6);return r.tag-7}";
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "copy_to_owned.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, candidate, &target, &limits).unwrap();
}

#[test]
fn repeated_fn_and_fnmut_calls_each_consume_a_new_owned_argument() {
    let declaration = "package Event{manages payload:ByteBuffer;tag:i32}package Reply{manages payload:ByteBuffer;tag:i32}";
    let bodies = [
        "fn main()->i32{let h=(e:Event)->Reply=>{return Reply{payload:give e.payload,tag:e.tag+1}};let a=bytes_new();let first=h(Event{payload:give a,tag:1});let b=bytes_new();let second=h(Event{payload:give b,tag:2});return first.tag+second.tag-5}",
        "fn main()->i32{var count=0;var h=move(e:Event)->Reply=>{count+=1;return Reply{payload:give e.payload,tag:e.tag+count}};let a=bytes_new();let first=h(Event{payload:give a,tag:1});let b=bytes_new();let second=h(Event{payload:give b,tag:2});return first.tag+second.tag-6}",
    ];
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (index, body) in bodies.iter().enumerate() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "repeat.zil",
            format!("{declaration}{body}"),
        );
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed, candidate, &target, &limits)
            .unwrap_or_else(|errors| panic!("case {index}: {errors:?}"));
    }
}

#[test]
fn callback_return_does_not_revive_a_borrowed_input_window() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let bytes = include_bytes!("../../../evidence/impl/248/loan_revival.wire");
    let (typed, candidate) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(typed, candidate, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap_err(),
    ] {
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
}

#[test]
fn callback_transfer_keeps_equal_typed_sibling_roots_distinct() {
    let text = "package Event{manages payload:ByteBuffer;tag:i32}fn main()->i32{let b=bytes_new();let e=Event{payload:give b,tag:1};let view=bytes_view(e.payload);let h=(e:Event)->Event=>{return e};let other=Event{payload:bytes_new(),tag:2};let a=h(other);let n=i32(view[0]);return n+a.tag}";
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "sibling.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, candidate, &target, &limits).unwrap();
}

#[test]
fn later_argument_failure_retains_prepared_argument_cleanup() {
    let text = "resource R{n:i32}package Event{manages r:R}enum Fault{Cancel}fn event(n:i32)->Event{let r=R{n:n};return Event{r:give r}}fn prepare(fail:bool)->Fault!Event{if fail{return Fault.Cancel};return event(3)}fn main()->Fault!i32{let h=(a:Event,b:Event)->Event=>{return a};let answer=h(prepare(false)?,prepare(true)?);return answer.r.n-3}";
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "later.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits).unwrap();
    let mut tampered = typed;
    let main = tampered
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap();
    let mut removed = 0;
    for block in &mut main.blocks {
        let old = block.instructions.len();
        block
            .instructions
            .retain(|instruction| !matches!(instruction.kind, InstructionKind::DropPackage { .. }));
        removed += old - block.instructions.len();
    }
    assert!(removed > 0);
    let mut candidate = candidate;
    candidate.module_fingerprint = tampered.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&tampered, &candidate, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(tampered, candidate, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    ] {
        assert_eq!(errors[0].code, "E-VERIFY-RESOURCE", "{errors:?}");
    }
}
