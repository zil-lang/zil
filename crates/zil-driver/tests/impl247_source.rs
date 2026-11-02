//! Actual source -> Types -> Analyze. Verify and native execution are accounted separately.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;

fn typed(text: &str) -> TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "247.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

#[test]
fn original_callback_sources_reach_analyze() {
    for text in [
        include_str!("impl246_fixtures/event_reply.zil"),
        include_str!("impl246_fixtures/event_copy_result.zil"),
        include_str!("impl246_fixtures/owned_result.zil"),
    ] {
        let m = typed(text);
        zil_analyze::analyze(&m, &Limits::default()).unwrap();
    }
}

const DATA: &str = "resource R{n:i32} package Event{manages r:R} enum Fault{Cancel} fn event(n:i32)->Event{let r=R{n:n};return Event{r:give r}}";

#[test]
fn owned_data_source_modes_and_cleanup_cfg_reach_analyze() {
    let cases = [
        "fn main()->i32{let h=(e:Event)->Event=>{return e};let a=h(event(2));let b=h(event(3));return a.r.n+b.r.n-5}",
        "fn main()->i32{var count=0;var h=move(e:Event)->Event=>{count+=1;return e};let a=h(event(2));let b=h(event(3));return a.r.n+b.r.n-5}",
        "fn main()->i32{let active=R{n:1};let spare=R{n:2};let h=move(e:Event)->Event=>{let held=give active;let seen=spare.n;return e};let a=h(event(3));return a.r.n-3}",
        "fn prepare(fail:bool)->Fault!Event{if fail{return Fault.Cancel};return event(3)} fn main()->Fault!i32{let active=R{n:1};let spare=R{n:2};let h=move(a:Event,b:Event)->Event=>{let held=give active;let seen=spare.n;return a};let answer=h(prepare(false)?,prepare(true)?);return answer.r.n-3}",
        "enum Packet{Empty,Data(Event)} fn main()->i32{let h=(e:Packet)->Packet=>{return e};let p=h(Packet.Data(event(3)));match p{Packet.Empty()=>{return 1}\n Packet.Data(e)=>{return e.r.n-3}}}",
        "fn main()->Fault!i32{let h=(e:Fault!Event)->Fault!Event=>{return e};let e:Fault!Event=event(3);let a=h(e)?;return a.r.n-3}",
        "package Items{manages values:Store<Event>} fn main()->i32{let values:Store<Event>=store_new();let h=(e:Items)->Items=>{return e};let a=h(Items{values:give values});return i32(store_len(a.values))}",
    ];
    for (index, case) in cases.iter().enumerate() {
        let m = typed(&format!("{DATA} {case}"));
        zil_analyze::analyze(&m, &Limits::default())
            .unwrap_or_else(|e| panic!("source case {index}: {e:?}"));
    }
}

#[test]
fn source_later_argument_failure_cleanup_is_independently_required() {
    let text = format!(
        "{DATA} fn prepare(fail:bool)->Fault!Event{{if fail{{return Fault.Cancel}};return event(3)}} fn main()->Fault!i32{{let h=(a:Event,b:Event)->Event=>{{return a}};let answer=h(prepare(false)?,prepare(true)?);return answer.r.n-3}}"
    );
    let m = typed(&text);
    zil_analyze::analyze(&m, &Limits::default()).unwrap();
    // This is a source-derived IR mutation, not a second accepted source. The
    // early-error edge must clean the already prepared first owning argument.
    let mut missing = m.clone();
    let main = missing
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut removed = 0;
    for block in &mut main.blocks {
        let before = block.instructions.len();
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
        removed += before - block.instructions.len();
    }
    assert!(removed > 0);
    assert!(zil_analyze::analyze(&missing, &Limits::default()).is_err());
}

#[test]
fn source_callback_result_does_not_revive_the_input_loan() {
    let before = "package Event{manages payload:ByteBuffer;tag:i32} fn main()->i32{let b=bytes_new();let e=Event{payload:give b,tag:1};let view=bytes_view(e.payload);let h=(e:Event)->Event=>{return e};let n=i32(view[0]);let a=h(e);return n+a.tag}";
    zil_analyze::analyze(&typed(before), &Limits::default()).unwrap();
    let after = before.replace(
        "let n=i32(view[0]);let a=h(e);",
        "let a=h(e);let n=i32(view[0]);",
    );
    let module = typed(&after);
    let diagnostics = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.phase == zil_base::Phase::Analyze)
    );
    let helper = after
        .replace(
            "fn main()",
            "fn dispatch<F:FnOnce(Event)->Event>(f:F,e:Event)->Event{return f(e)} fn main()",
        )
        .replace("let a=h(e);", "let a=dispatch(h,e);");
    assert!(zil_analyze::analyze(&typed(&helper), &Limits::default()).is_err());
    let sibling = before.replace(
        "let n=i32(view[0]);let a=h(e);",
        "let other=Event{payload:bytes_new(),tag:2};let a=h(other);let n=i32(view[0]);",
    );
    zil_analyze::analyze(&typed(&sibling), &Limits::default()).unwrap();
}

#[test]
fn source_sum_store_and_ordinary_callee_transfers_preserve_actual_windows() {
    let cases = [
        "package Event{manages payload:ByteBuffer} enum Packet{Data(Event)} fn main()->i32{let e=Event{payload:bytes_new()};let view=bytes_view(e.payload);let h=(p:Packet)->Packet=>{return p};let a=h(Packet.Data(e));return i32(view[0])}",
        "package Leaf{manages bytes:ByteBuffer;n:i32} package Items{manages values:Store<Leaf>} fn main()->i32{let values:Store<Leaf>=store_new();let e=Items{values:give values};let view=store_at(e.values,0);let h=(e:Items)->Items=>{return e};let a=h(e);return view.n}",
        "package Event{manages payload:ByteBuffer} fn pass(e:Event)->Event{return e} fn main()->i32{let h=(e:Event)->Event=>{let view=bytes_view(e.payload);let a=pass(e);let n=i32(view[0]);return a};let a=h(Event{payload:bytes_new()});return 0}",
    ];
    for (index, source) in cases.iter().enumerate() {
        let m = typed(source);
        let diagnostics = zil_analyze::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .all(|d| d.phase == zil_base::Phase::Analyze),
            "case {index}: {diagnostics:?}"
        );
    }
}
