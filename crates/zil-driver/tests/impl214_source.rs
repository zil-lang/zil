//! Product capture source classification. These tests stop at Types; they do
//! not claim independent ownership verification or machine execution.
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CallableKind, CoreModule, InstructionKind, Type, TypedModule};

const TYPES: &str = r#"
resource R { n:i32 }
package Leaf { manages owner:R; n:i32 }
package Root { child:Leaf; tag:i32 }
fn leaf(n:i32)->Leaf { return Leaf{owner:R{n:n},n:n} }
fn root(n:i32)->Root { return Root{child:leaf(n),tag:n} }
"#;

fn core(source: &str) -> CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "capture214.zil", source);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &syntax).unwrap()
}

fn check(source: &str) -> Result<TypedModule, Vec<Diagnostic>> {
    zil_types::check(
        &core(source),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
}

fn typed(source: &str) -> TypedModule {
    check(source).unwrap_or_else(|errors| panic!("{source}\n{errors:#?}"))
}

fn body(text: &str) -> String {
    format!("{TYPES} fn main()->i32{{var p=root(7);{text}}}")
}

fn kind(source: &str, kind: CallableKind) -> TypedModule {
    let module = typed(source);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, kind, "{source}");
    assert!(matches!(
        module.closures[0].captures[0],
        Type::Package { .. }
    ));
    let entry = &module.functions[module.closures[0].function.0 as usize];
    assert!(
        entry.blocks[entry.entry.0 as usize]
            .instructions
            .iter()
            .any(|i| { matches!(i.kind, InstructionKind::BindPackageCapture { index: 0, .. }) })
    );
    module
}

fn reject(source: &str, message: &str) {
    let errors = check(source).expect_err("unsupported source must not acquire a capture");
    assert!(
        errors.iter().any(|error| error.message.contains(message)),
        "{source}\n{errors:#?}"
    );
}

#[test]
fn explicit_owned_read_is_repeatable_fn() {
    kind(
        &body("let f=move()=>p.tag;return f()+f()"),
        CallableKind::Fn,
    );
}

#[test]
fn original206_child_take_is_once_and_keeps_its_source_bytes() {
    kind(
        include_str!("impl206_independent_fixtures/29_product_capture.zil"),
        CallableKind::FnOnce,
    );
}

#[test]
fn default_whole_and_child_uses_capture_ownership_at_creation() {
    for text in [
        "let f=()=>{let q=p;return q.tag};return f()",
        "let f=()=>{let q=p.child;return q.n};return f()",
    ] {
        let module = kind(&body(text), CallableKind::FnOnce);
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let captures = main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                if let InstructionKind::MakeClosure { captures, .. } = &i.kind {
                    Some(captures)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(captures.len(), 1);
        assert!(
            main.blocks.iter().flat_map(|b| &b.instructions).any(|i| {
                i.result == Some(captures[0]) && matches!(i.kind, InstructionKind::TakeLocal { .. })
            }),
            "ownership must transfer before the environment is constructed"
        );
    }
}

#[test]
fn complete_child_refill_keeps_fnmut() {
    for marker in ["", "move"] {
        kind(
            &body(&format!(
                "var f={marker}()=>{{let q=p.child;let n=q.n;p.child=q;return n}};return f()+f()"
            )),
            CallableKind::FnMut,
        );
    }
}

#[test]
fn child_partial_local_refill_is_ordinary_complete_transfer() {
    kind(
        &body(
            "var f=()=>{var q=p.child;let r=give q.owner;let n=r.n;q.owner=R{n:n+1};p.child=q;return n};return f()+f()",
        ),
        CallableKind::FnMut,
    );
}

#[test]
fn replacement_without_missing_exit_keeps_fnmut() {
    kind(
        &body("var f=move()=>{p.child=leaf(9);return p.tag};return f()+f()"),
        CallableKind::FnMut,
    );
}

#[test]
fn conditional_child_take_records_all_exits() {
    kind(
        &body(
            "let f=(take:bool)=>{if take {let q=p.child;return q.n};return p.tag};return f(true)",
        ),
        CallableKind::FnOnce,
    );
    kind(
        &body(
            "var f=(take:bool)=>{if take {let q=p.child;p.child=q};return p.tag};return f(true)+f(false)",
        ),
        CallableKind::FnMut,
    );
}

#[test]
fn while_break_and_continue_restore_the_child() {
    kind(
        &body(
            "var f=(n:i32)=>{var i=0;while i<n {let q=p.child;p.child=q;i+=1;if i==1 {continue};break};return p.tag};return f(2)+f(0)",
        ),
        CallableKind::FnMut,
    );
}

#[test]
fn try_before_refill_requires_once_even_when_success_restores() {
    let source = format!(
        "{TYPES} fn step(fail:bool)->i64!i32{{if fail {{return i64(9)}};return 3}} fn main()->i64!i32{{var p=root(7);let f=(fail:bool)->i64!i32=>{{let q=p.child;let n=step(fail)?;p.child=q;return n}};return f(false)}}"
    );
    kind(&source, CallableKind::FnOnce);
}

#[test]
fn try_after_complete_refill_keeps_fnmut_on_the_error_exit() {
    let source = format!(
        "{TYPES} fn step(fail:bool)->i64!i32{{if fail {{return i64(9)}};return 3}} fn main()->i64!i32{{var p=root(7);var f=(fail:bool)->i64!i32=>{{let q=p.child;p.child=q;return step(fail)?}};let failed=f(true);return f(false)}}"
    );
    kind(&source, CallableKind::FnMut);
}

#[test]
fn factory_accepts_complete_product_inputs_and_cleanup() {
    for (bound, marker, expression) in [
        ("Fn", "move", "return p.tag"),
        ("FnMut", "", "let q=p.child;let n=q.n;p.child=q;return n"),
        ("FnOnce", "", "let q=p.child;return q.n"),
    ] {
        let source = format!(
            "{TYPES} fn make(input:Root)->impl {bound}()->i32{{var p=input;return {marker}()=>{{{expression}}}}} fn main(){{let f=make(root(7))}}"
        );
        let module = typed(&source);
        let factory = module.functions.iter().find(|f| f.name == "make").unwrap();
        assert_eq!(
            factory.callable_return_contract.as_ref().unwrap().cleanup,
            vec![0]
        );
    }
}

#[test]
fn same_type_siblings_and_three_levels_remain_actual_distinct_values() {
    let source = format!(
        "{TYPES} package Pair{{a:Root;b:Root}} package Wrapper{{pair:Pair}} fn main()->i32{{let p=Wrapper{{pair:Pair{{a:root(3),b:root(5)}}}};let f=()=>{{let pair=p.pair;let a=pair.a;let b=pair.b;return a.tag+b.tag}};return f()}}"
    );
    kind(&source, CallableKind::FnOnce);
}

#[test]
fn nested_default_consumer_transfers_the_outer_product() {
    let source =
        body("let f=()=>{let inner=()=>{let q=p.child;return q.n};return inner()};return f()");
    let module = typed(&source);
    assert_eq!(module.closures.len(), 2);
    assert!(
        module
            .closures
            .iter()
            .all(|c| c.kind == CallableKind::FnOnce)
    );
}

#[test]
fn shadowed_nested_names_do_not_steal_outer_responsibility() {
    let module = typed(&body(
        "let f=move()=>{let child=()=>{let p=root(9);let q=p.child;return q.n};child();return p.tag};return f()",
    ));
    assert_eq!(module.closures.len(), 2);
    assert!(module.closures.iter().all(|c| c.kind == CallableKind::Fn));
    assert_eq!(module.closures[0].captures.len(), 1);
    assert!(module.closures[1].captures.is_empty());
}

#[test]
fn a_product_environment_can_enter_state_and_outer_callback_store() {
    for (bound, call, expression) in [
        ("Fn", "store_at(items,0).callback()", "move()=>p.tag"),
        (
            "FnMut",
            "store_at_mut(items,0).callback()",
            "()=>{let q=p.child;let n=q.n;p.child=q;return n}",
        ),
    ] {
        let source = format!("{TYPES}
package State<F>{{manages bytes:ByteBuffer;callback:F}}
fn make(input:Root)->impl {bound}()->i32{{var p=input;return {expression}}}
fn pack<F:{bound}()->i32>(f:F)->State<F>{{return State::<F>{{bytes:bytes_new(),callback:f}}}}
fn run<F:{bound}()->i32>(state:State<F>)->i32{{let items:Store<State<F>>=store_new();store_reserve(items,2);store_push(items,state);return {call}}}
fn main()->i32{{return run(pack(make(root(7))))}}");
        let module = typed(&source);
        assert!(
            module
                .resources
                .iter()
                .any(|resource| matches!(resource.payload, Type::OwnedStore { .. }))
        );
    }
}

#[test]
fn original_flat_and_recursive_readonly_sources_use_shared_product_captures() {
    // The two original rejection source strings are unchanged. Slice232
    // admits their default shared view without turning either into a snapshot.
    for source in [
        body("let f=()=>p.tag;return f()"),
        format!("{TYPES}fn main()->i32{{let p=leaf(7);let f=()=>p.n;return f()}}"),
    ] {
        let module = typed(&source);
        assert_eq!(module.closures[0].kind, CallableKind::Fn);
        assert!(
            matches!(&module.closures[0].captures[..], [Type::Address { mutable: false, inner }] if matches!(inner.as_ref(), Type::Package { .. }))
        );
    }
}

#[test]
fn source_declared_fn_cannot_hide_child_consumption() {
    reject(
        &format!(
            "{TYPES}fn make(p:Root)->impl Fn()->i32{{return ()=>{{let q=p.child;return q.n}}}}fn main(){{}}"
        ),
        "kind adaptation",
    );
}

#[test]
fn complete_product_store_elements_are_admitted_since_223() {
    let source = format!("{TYPES}fn main(){{let items:Store<Root>=store_new()}}");
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "closed214.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    // The exact old source and source identity remain a Types-level control;
    // 223 now admits this complete Product as a Store element.
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    assert!(module.resources.iter().any(
        |resource| matches!(&resource.payload, Type::OwnedStore { element }
                if matches!(element.as_ref(), Type::Package { name, .. } if name == "@module0:Root"))
    ));
}

#[test]
fn unchanged_flat_types_budget_still_accepts_exact_418() {
    let source = "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}";
    let core = core(source);
    let run = |max_analysis_steps| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps,
                ..Limits::default()
            },
        )
    };
    let expected = run(Limits::default().max_analysis_steps).unwrap();
    assert_eq!(run(418).unwrap(), expected);
    assert!(
        run(417)
            .unwrap_err()
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(418).unwrap(), expected);
}

#[test]
fn new_product_capture_has_exact_recoverable_work_boundary() {
    let source = body("var f=()=>{let q=p.child;let n=q.n;p.child=q;return n};return f()+f()");
    let core = core(&source);
    let run = |max_analysis_steps| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps,
                ..Limits::default()
            },
        )
    };
    let mut lo = 0;
    let mut hi = Limits::default().max_analysis_steps;
    let expected = run(hi).unwrap();
    while lo + 1 < hi {
        let middle = lo + (hi - lo) / 2;
        match run(middle) {
            Ok(_) => hi = middle,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                lo = middle;
            }
        }
    }
    assert_eq!(run(hi).unwrap(), expected);
    assert!(
        run(hi - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(hi).unwrap(), expected);
    println!("214 Product capture Types boundary: {hi}");
}
