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

fn rejected(text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "reject252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    if let Ok(core) = zil_expand::expand(&source, &ast) {
        assert!(
            zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).is_err(),
            "accepted {text}"
        );
    }
}

#[test]
fn unused_declarations_cannot_copy_consume_then_return_or_change_the_instance() {
    for definition in [
        "fn bad<F:Fn(i32)->i32>(f:F)->F{let a=f;let b=f;return a}",
        "fn bad<F:FnOnce(i32)->i32>(f:F)->F{f(1);return f}",
        "fn bad<F:Fn(i32)->i32>(f:&F)->F{return f}",
        "fn bad<F:Fn(i32)->i32,G:Fn(i32)->i32>(f:F,g:G)->F{return f}",
        "fn bad<F:Fn(i32)->i32,T:Copy>(f:F,data:T)->F{return f}",
        "fn bad<F:Fn(i32)->i32>(f:F)->F{return (n:i32)=>n}",
        "fn bad<F:Fn()->&i32>(f:F)->F{return f}",
        "fn bad<F:Fn(i32)->i32>(f:F,data:ByteBuffer)->F{return f}",
    ] {
        rejected(&format!("{definition}fn main(){{}}"));
    }
}

#[test]
fn caller_declared_once_or_mutable_position_cannot_upgrade_after_forwarding() {
    let keep = "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}";
    for body in [
        "fn bad<F:FnOnce(i32)->i32>(f:F)->i32{let current=keep(f);return current(1)+current(2)}",
        "fn bad<F:FnOnce(i32)->i32>(f:F)->i32{let current=keep(f);let next=keep(f);return current(1)}",
        "fn bad<F:FnMut(i32)->i32>(f:F)->F{let current=keep(f);current(1);return current}",
    ] {
        rejected(&format!("{keep}{body}fn main(){{}}"));
    }
}

#[test]
fn nonzero_parameter_and_loop_moves_keep_the_actual_role() {
    let module = checked(
        "fn keep<F:FnOnce(i32)->i32>(flag:bool,f:F)->F{return f}fn cycle<F:FnOnce(i32)->i32>(f:F,count:i32)->F{var current=f;var i=0;while i<count{current=keep(true,current);i+=1}return current}fn main()->i32{let f=(n:i32)=>n+3;let current=cycle(keep(true,f),2);return current(4)-7}",
    );
    assert!(
        module
            .functions
            .iter()
            .filter_map(|f| f.callable_return_contract.as_ref())
            .any(|c| c.source == CallableReturnSource::Parameter(1))
    );
}

#[test]
fn address_signature_and_independent_store_and_union_companions_are_closed_data() {
    let head = "package Leaf{manages bytes:ByteBuffer}package Items{manages store:Store<Leaf>}package Fault{items:Items}";
    checked(&format!(
        "{head}fn keep<F:Fn(&mut [i32])->i32>(data:Items,f:F)->F{{return f}}fn main()->i32{{var values=[1,2];let store:Store<Leaf>=store_new();let f=(xs:&mut [i32])->i32=>{{xs[0]+=1;return xs[0]}};let current=keep(Items{{store:give store}},f);return current(&mut values[..])-2}}"
    ));
    checked(&format!(
        "{head}fn keep<F:FnOnce(i32)->i32>(f:F,data:Fault!Items)->F{{return f}}fn main(){{let store:Store<Leaf>=store_new();let data:Fault!Items=Items{{store:give store}};let current=keep((n:i32)=>n,data)}}"
    ));
    rejected(
        "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}fn main(){let n=3;let f=(x:i32)=>x+n;let current=keep(f)}",
    );
}

fn checked_project(main: &str, model: &str, relay: &str) -> TypedProgram {
    let sources = [
        SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
        SourceUnit::new(FileId(1), ModuleId(1), "model.zil", model),
        SourceUnit::new(FileId(2), ModuleId(2), "relay.zil", relay),
    ];
    let ast = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let import = |name: &str, id| CoreImport {
        alias: name.into(),
        module: ModuleId(id),
        span: zil_base::Span::default(),
    };
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &ast.iter().collect::<Vec<_>>(),
        &[
            vec![import("model", 1), import("relay", 2)],
            vec![],
            vec![import("model", 1)],
        ],
        &Limits::default(),
    )
    .unwrap()
    .into_iter()
    .map(|(m, _)| m)
    .collect::<Vec<_>>();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: modules[0].core.source_fingerprint,
        modules,
    };
    zil_types::check_program(&program, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

#[test]
fn frozen_three_module_consumer_reaches_types() {
    checked_project(
        include_str!("../../../tests/impl256/frozen-prep/project/main.zil"),
        include_str!("../../../tests/impl256/frozen-prep/project/model.zil"),
        include_str!("../../../tests/impl256/frozen-prep/project/relay.zil"),
    );
}
#[test]
fn owning_environment_loop_uses_a_real_slot_and_keeps_parameter_origin() {
    let module = checked_project(
        include_str!("../../../tests/impl256/extended/main.zil"),
        include_str!("../../../tests/impl256/extended/model.zil"),
        include_str!("../../../tests/impl256/extended/relay.zil"),
    )
    .module;
    assert!(module.functions.iter().any(|f| {
        f.callable_return_contract
            .as_ref()
            .is_some_and(|c| matches!(c.source, CallableReturnSource::Parameter(_)))
            && f.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
                matches!(
                    i.kind,
                    InstructionKind::TakeOwnedArrayElement { index: 0, .. }
                )
            })
    }));
}
#[test]
fn frozen_same_type_fresh_factory_returns_are_real_safe_rejections() {
    for text in [
        include_str!(
            "../../../tests/impl256/frozen-prep/negative/same_closure_id_different_instance.zil"
        ),
        include_str!(
            "../../../tests/impl256/frozen-prep/negative/same_closure_id_wrong_branch.zil"
        ),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "fresh252.zil", text);
        let ast = zil_syntax::parse(&source).into_result().unwrap();
        let errors = zil_expand::expand(&source, &ast).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == zil_base::ResultCategory::SafeReject
                    && d.message.contains("actual input instance")),
            "{errors:?}"
        );
    }
}

#[test]
fn raw_unused_parameter_definition_checks_mutable_position_before_any_instance() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw252.zil",
        "fn keep<F:FnMut(i32)->i32>(f:F)->F{return f}fn main(){}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let mut core = zil_expand::expand(&source, &ast).unwrap();
    let function = core
        .functions
        .iter_mut()
        .find(|f| f.name == "keep")
        .unwrap();
    let span = function.span;
    let callee = CoreExpr {
        id: zil_base::NodeId(900),
        span,
        kind: CoreExprKind::Name("f".into()),
    };
    let value = CoreExpr {
        id: zil_base::NodeId(901),
        span,
        kind: CoreExprKind::Integer("1".into()),
    };
    let call = CoreExpr {
        id: zil_base::NodeId(902),
        span,
        kind: CoreExprKind::Call {
            callee: Box::new(callee),
            arguments: vec![value],
        },
    };
    function.body.statements.insert(
        0,
        CoreStatement {
            id: zil_base::NodeId(903),
            span,
            kind: CoreStatementKind::Expression(call),
        },
    );
    let errors =
        zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == zil_base::Phase::Types
            && d.category == zil_base::ResultCategory::SafeReject
            && d.code == "E-CLOSURE-MUTABILITY"),
        "{errors:?}"
    );
}

#[test]
fn original_forward_has_exact_fresh_types_and_expand_work_boundaries() {
    let text = include_str!("impl252_fixtures/original.zil");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "front252.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand(&source, &ast).unwrap();
    let expected =
        zil_types::check(&expanded, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let expand = |n| {
        zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
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
    let minimum = |ok: &dyn Fn(usize) -> bool| {
        let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
        assert!(ok(hi));
        while lo + 1 < hi {
            let n = lo + (hi - lo) / 2;
            if ok(n) { hi = n } else { lo = n }
        }
        hi
    };
    let en = minimum(&|n| expand(n).is_ok());
    let tn = minimum(&|n| types(n).is_ok());
    assert!(
        expand(en - 1)
            .unwrap_err()
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert!(
        types(tn - 1)
            .unwrap_err()
            .iter()
            .any(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(expand(en).unwrap().core, expanded);
    assert_eq!(expand(en).unwrap().core, expanded);
    assert_eq!(types(tn).unwrap(), expected);
    assert_eq!(types(tn).unwrap(), expected);
    println!(
        "252 original forward Expand N={en}, Types N={tn}; N-1 ResourceLimit/fresh N identity"
    );
}

#[test]
fn nested_forwarding_accepts_a_stronger_fn_through_mutable_adaptation() {
    checked(
        "fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return current}fn relay<F:Fn(i32)->i32>(f:F)->F{return step(f)}fn main()->i32{let f=(n:i32)=>n+1;let current=relay(f);return current(2)-3}",
    );
}

#[test]
fn in_scope_forward_fnmut_let_call_is_safe_rejected_and_var_call_preserves_state() {
    let prelude = "fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}";
    let bad = format!(
        "{prelude}fn outer<F:FnMut(i32)->i32>(f:F)->F{{let current=keep(f);current(1);return current}}fn main(){{}}"
    );
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "mut252.zil", &bad);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let errors = zil_expand::expand(&unit, &ast).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == zil_base::ResultCategory::SafeReject
                && d.message.contains("mutable binding"))
    );
    checked(&format!(
        "{prelude}fn outer<F:FnMut(i32)->i32>(f:F)->F{{var current=keep(f);current(1);return current}}fn main()->i32{{var count=0;let handler=move(delta:i32)=>{{count+=delta;return count}};var current=outer(handler);return current(1)+current(1)-5}}"
    ));
}
