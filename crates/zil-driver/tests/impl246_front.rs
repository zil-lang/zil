//! Author source-to-Types evidence; independent kernels/native consumers are separate.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn core(text: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front246.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}
fn checked(text: &str) -> TypedModule {
    zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap()
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
fn wrapper_data_and_callback_signatures_are_checked_independently() {
    for wrapper in [
        "fn wrapper<F:FnOnce(Event)->Reply>(f:F,event:Event,extra:bool)->i32{let reply=f(event);if extra{return reply.tag}return 0}",
        "package Output{manages payload:ByteBuffer;count:i32}fn wrapper<F:FnOnce(Event)->Reply>(f:F,event:Event,extra:bool)->Output{let reply=f(event);return Output{payload:give reply.payload,count:reply.tag}}",
        "fn wrapper<F:FnOnce(i32)->i32>(f:F,event:Event)->Reply{let n=f(event.tag);return Reply{payload:give event.payload,tag:n}}",
    ] {
        checked(&format!("{DATA}{wrapper}fn main(){{}}"));
    }
    checked(&format!(
        "{DATA}fn wrapper<F:FnOnce(i32)->i32>(f:F,event:Event)->Reply{{let n=f(event.tag);return Reply{{payload:give event.payload,tag:n}}}}fn main()->i32{{let reply=wrapper((n:i32)=>n+1,Event{{payload:bytes_new(),tag:7}});return reply.tag-8}}"
    ));
}

#[test]
fn closed_owned_dispatch_crosses_real_model_and_helper_modules() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import model;import dispatch;fn main()->i32{let handler=(event:model.Event)->model.Reply=>{return model.reply(event)};let reply=dispatch.run(handler,model.make());return reply.tag-8}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "model.zil",
            "pub package Event{manages bytes:ByteBuffer;pub tag:i32}pub package Reply{manages bytes:ByteBuffer;pub tag:i32}pub fn make()->Event{return Event{bytes:bytes_new(),tag:7}}pub fn reply(event:Event)->Reply{return Reply{bytes:give event.bytes,tag:event.tag+1}}",
        ),
        SourceUnit::new(
            FileId(2),
            ModuleId(2),
            "dispatch.zil",
            "import model;pub fn run<F:FnOnce(model.Event)->model.Reply>(handler:F,event:model.Event)->model.Reply{return handler(event)}",
        ),
    ];
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let import = |alias: &str, module| CoreImport {
        alias: alias.into(),
        module: ModuleId(module),
        span: zil_base::Span::default(),
    };
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &[
            vec![import("model", 1), import("dispatch", 2)],
            vec![],
            vec![import("model", 1)],
        ],
        &Limits::default(),
    )
    .unwrap();
    let modules = expanded
        .into_iter()
        .map(|(module, _)| module)
        .collect::<Vec<_>>();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: modules[0].core.source_fingerprint,
        modules,
    };
    let typed = zil_types::check_program(&program, &TargetProfile::host_llvm(), &Limits::default())
        .unwrap();
    assert!(typed.module.functions.iter().any(|f| {
        f.callable_contract
            .as_ref()
            .is_some_and(|contract| contract.roles.len() == 1)
    }));
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
            "{DATA}fn main(){{let p=Event{{payload:bytes_new(),tag:1}};let view=&p;let f=(e:Event)->Reply=>{{let n=view.tag;return Reply{{payload:give e.payload,tag:n}}}}}}"
        ),
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

#[test]
fn a_later_argument_failure_keeps_the_callee_and_first_argument_in_real_cleanup_slots() {
    let t = checked(&format!(
        "{DATA}fn prepare()->void!Event{{return ()}}fn main()->void!Reply{{let active=bytes_new();let spare=bytes_new();let f=move(a:Event,b:Event)->Reply=>{{let used=give active;bytes_len(spare);return Reply{{payload:give a.payload,tag:a.tag}}}};return f(Event{{payload:bytes_new(),tag:7}},prepare()?)}}"
    ));
    let closure = &t.closures[0];
    assert_eq!(closure.kind, CallableKind::FnOnce);
    assert_eq!(closure.captures.len(), 2);
    let main = t.functions.iter().find(|f| f.name == "main").unwrap();
    let error = main
        .blocks
        .iter()
        .find(|block| {
            block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
                && !block
                    .instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
        })
        .unwrap();
    let package = error
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
        .unwrap();
    let callee = error
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
        .unwrap();
    assert!(
        package < callee,
        "successful argument preparation cleans before callee"
    );
    assert!(
        !error
            .instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
    );
    let body = t
        .functions
        .iter()
        .find(|f| f.id == closure.function)
        .unwrap();
    assert!(
        body.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
    );
}

#[test]
fn raw_unused_helper_signature_cannot_forge_a_data_or_passing_qualification() {
    let source = format!(
        "{DATA}fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply{{return f(event)}}fn main(){{}}"
    );
    let original = core(&source);
    for change in 0..6 {
        let mut forged = original.clone();
        let dispatch = forged
            .functions
            .iter_mut()
            .find(|f| f.name == "dispatch")
            .unwrap();
        match change {
            0 => dispatch.callable_parameters[0].kind = CallableKind::Fn,
            1 => dispatch.parameters[0].mode = PassingMode::Copy,
            2 => dispatch.parameters[1].mode = PassingMode::Copy,
            3 => {
                dispatch.callable_parameters[0].parameters[0] =
                    Type::Managed(Box::new(Type::ByteBuffer))
            }
            4 => {
                dispatch.callable_parameters[0].result = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }
            }
            _ => {
                dispatch.callable_parameters[0].result = Type::Package {
                    name: "HiddenCallbackStore".into(),
                    fields: vec![(
                        "values".into(),
                        Type::Managed(Box::new(Type::OwnedStore {
                            element: Box::new(Type::Package {
                                name: "Item".into(),
                                fields: vec![
                                    ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                                    ("callback".into(), Type::Closure(ClosureId(0))),
                                ],
                            }),
                        })),
                    )],
                }
            }
        }
        assert!(
            zil_types::check(&forged, &TargetProfile::host_llvm(), &Limits::default()).is_err(),
            "raw mutation {change} accepted"
        );
    }
}

#[test]
fn dispatch_has_fresh_exact_types_and_expand_work_boundaries() {
    let source = include_str!("impl246_fixtures/event_reply.zil");
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "front246.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let expanded = core(source);
    let expected = checked(source);
    let types = |n| {
        zil_types::check(
            &expanded,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let expand = |n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let minimum = |succeeds: &dyn Fn(usize) -> bool| {
        let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
        assert!(succeeds(hi));
        while lo + 1 < hi {
            let n = lo + (hi - lo) / 2;
            if succeeds(n) { hi = n } else { lo = n }
        }
        hi
    };
    let types_n = minimum(&|n| types(n).is_ok());
    let expand_n = minimum(&|n| expand(n).is_ok());
    assert!(
        types(types_n - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert!(
        expand(expand_n - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(types(types_n).unwrap(), expected);
    assert_eq!(types(types_n).unwrap(), expected);
    assert_eq!(expand(expand_n).unwrap().core, expanded);
    assert_eq!(expand(expand_n).unwrap().core, expanded);
    println!(
        "owned dispatch Types N={types_n}, Expand N={expand_n}; N-1 ResourceLimit and fresh N identity"
    );
}

#[test]
fn registered_copy_helper_costs_and_old_flat_control_have_exact_fresh_boundaries() {
    for (source, expand_n, types_n) in [
        (
            "fn apply<T:Copy,F:Fn(i32)->i32>(f:&F,value:T)->T{f(1);return value}fn main()->i32{let f=(n:i32)=>n+1;return apply(&f,2)-2}",
            489,
            420,
        ),
        (
            include_str!("impl246_fixtures/copy_enum_helper.zil"),
            665,
            610,
        ),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget246.zil", source);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &ast).unwrap();
        let expected =
            zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
        let expand = |n| {
            zil_expand::expand_with_map_limits(
                &unit,
                &ast,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        let types = |n| {
            zil_types::check(
                &core,
                &TargetProfile::host_llvm(),
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
        };
        assert!(
            expand(expand_n - 1)
                .unwrap_err()
                .iter()
                .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(expand(expand_n).unwrap().core, core);
        assert_eq!(expand(expand_n).unwrap().core, core);
        assert!(
            types(types_n - 1)
                .unwrap_err()
                .iter()
                .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(types(types_n).unwrap(), expected);
        assert_eq!(types(types_n).unwrap(), expected);
        println!("registered Copy helper Expand {expand_n}/Types {types_n}; N-1 and fresh N");
    }
    // The legacy flat control is also asserted at Types418 by impl240_front
    // and at Types418/Analyze8673 by impl206_budget in the final affected set.
}
