//! Independent development review of ordinary Product Store composition.
//! These checks do not constitute a review of an uncommitted candidate.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, PassingMode, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const HEAD: &str = r#"
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Bag{manages items:Store<Item>}
package Pair{left:Bag;right:Bag}
fn bag(code:i32)->Bag{
 let items:Store<Item>=store_new();
 if store_reserve(items,1){store_push(items,Item{ticket:Ticket{code:code},value:code})}
 return Bag{items:give items}
}
fn pair()->Pair{return Pair{left:bag(65),right:bag(66)}}
"#;
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!(
        "review235-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn unit(source: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review235.zil", source)
}
fn typed(source: &str) -> TypedModule {
    let u = unit(source);
    let syntax = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn reject(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) {
    let errors = match result {
        Ok(_) => panic!("{label}: incorrect acceptance"),
        Err(e) => e,
    };
    assert!(
        errors.iter().any(|d| matches!(
            d.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:#?}"
    );
    assert!(
        errors.iter().all(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: only candidate claims rejected: {errors:#?}"
    );
}
fn raw_wire(label: &str, module: TypedModule, mut plan: CandidatePlan, accept: bool) {
    let out = dir(label);
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("input.wire"), &wire).unwrap();
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    for (route, result) in [("raw", raw), ("wire", bytes)] {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        if accept {
            result.unwrap_or_else(|e| panic!("{label}/{route}: {e:#?}"));
        } else {
            reject(result, &format!("{label}/{route}"));
        }
    }
}
fn verified(source: &str) -> (TypedModule, CandidatePlan) {
    let module = typed(source);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    raw_wire("control", module.clone(), plan.clone(), true);
    (module, plan)
}
fn source(label: &str, text: &str, accept: bool) {
    let out = dir(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let report = compile_source(
        &unit(text),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    if accept {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
    } else {
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
    }
    assert!(!report.artifacts.contains_key("executable"));
}
fn native(label: &str, text: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend,
                emit_dir: Some(out.join("phases")),
                output: Some(out.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &output.stdout).unwrap();
        fs::write(out.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn unused_product_parameters_still_require_exact_canonical_store_metadata() {
    // No constructor, Store operation or callback is present. The only Store
    // carrier occurs inside the unused ordinary parameter's recursive type.
    let declarations = HEAD.split("fn bag").next().unwrap();
    let text = format!("{declarations}fn unused(value:Pair){{}}fn main(){{}}");
    let (module, plan) = verified(&text);
    for mode in [
        "missing",
        "alias",
        "wrong-element",
        "user-teardown",
        "duplicate",
    ] {
        let mut altered = module.clone();
        let index = altered
            .resources
            .iter()
            .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
            .unwrap();
        match mode {
            "missing" => {
                altered.resources.remove(index);
            }
            "alias" => altered.resources[index].name.push_str(".alias"),
            "wrong-element" => {
                let Type::OwnedStore { element } = &mut altered.resources[index].payload else {
                    unreachable!()
                };
                let Type::Package { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                fields[1].1 = Type::I64;
            }
            "user-teardown" => {
                altered.resources[index].teardown = Some(
                    altered
                        .functions
                        .iter()
                        .find(|f| f.name == "main")
                        .unwrap()
                        .id,
                )
            }
            "duplicate" => altered.resources.push(altered.resources[index].clone()),
            _ => unreachable!(),
        }
        reject(zil_analyze::analyze(&altered, &Limits::default()), mode);
        raw_wire(mode, altered, plan.clone(), false);
    }
}

fn birth(incoming: bool) -> String {
    let target = if incoming { "left" } else { "right" };
    format!(
        "{HEAD}fn origin(give old:Store<Item>)->Store<Item> contract{{result(create)}}{{let new:Store<Item>=store_new();let value=Pair{{left:Bag{{items:give old}},right:Bag{{items:give new}}}};let moved=value;let child=moved.{target};return give child.items}}fn main()->i32{{let old:Store<Item>=store_new();let made=origin(give old);return i32(store_len(made))}}"
    )
}
#[test]
fn same_typed_store_leaves_preserve_incoming_and_created_through_child_transfer() {
    let good = birth(false);
    verified(&good);
    native("created-right", &good, "", 0);
    let module = typed(&birth(true));
    let errors = zil_analyze::analyze(&module, &Limits::default())
        .expect_err("incoming Store cannot become create");
    assert!(
        errors
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.message.contains("incoming responsibility")),
        "{errors:#?}"
    );
    let mut ordinary = module.clone();
    ordinary
        .functions
        .iter_mut()
        .find(|f| f.name == "origin")
        .unwrap()
        .result_mode = PassingMode::Give;
    let plan = zil_analyze::analyze(&ordinary, &Limits::default()).unwrap();
    raw_wire("incoming-left", module, plan, false);
}

#[test]
fn ordinary_composition_does_not_open_capture_or_containers() {
    for (label, tail) in [
        (
            "move-capture",
            "fn main(){let value=pair();let action=move()=>{let child=value.left;return i32(store_len(child.items))}}",
        ),
        (
            "shared-default-capture",
            "fn main(){let value=pair();let action=()=>{let child=value.left;return i32(store_len(child.items))}}",
        ),
        (
            "nested-element",
            "fn unused(values:Store<Pair>){}fn main(){}",
        ),
        ("array", "fn unused(values:[Pair;2]){}fn main(){}"),
        (
            "sum",
            "fn unused(value:Pair)->void!Pair{return value}fn main(){}",
        ),
        ("shared", "fn unused(value:&Pair){}fn main(){}"),
    ] {
        source(label, &format!("{HEAD}{tail}"), false);
    }
}

#[test]
fn remembered_header_description_blocks_parent_transfer_until_its_actual_last_use() {
    source(
        "header-live-move",
        &format!(
            "{HEAD}fn main(){{let value=pair();let child=value.left;let header=child.items;let moved=child;let length=store_len(header)}}"
        ),
        false,
    );
    source(
        "header-ended-move",
        &format!(
            "{HEAD}fn main(){{let value=pair();let child=value.left;let header=child.items;let length=store_len(header);let moved=child}}"
        ),
        true,
    );
}

#[test]
fn pop_and_failed_full_push_then_parent_drop_settle_only_actual_prefix() {
    let text = format!(
        "{HEAD}fn main()->i32{{var value=pair();var child=value.left;let popped=store_pop(child.items);store_push(child.items,Item{{ticket:Ticket{{code:67}},value:67}});store_push(child.items,Item{{ticket:Ticket{{code:68}},value:68}});value.left=child;return 0}}"
    );
    verified(&text);
    native("pop-full-push", &text, "DACB", 0);
}

#[test]
fn generic_store_element_binder_survives_a_non_element_product_wrapper() {
    let text = r#"
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item<F>{manages ticket:Ticket;callback:F;value:i32}
package Packet<F>{manages marker:Ticket;callback:F}
package Bag<T:StoreElement>{manages items:Store<T>}
package Root<T:StoreElement>{child:Bag<T>}
fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}
fn make()->impl Fn()->i32{let count=8;return move()=>count}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:90},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{
 let items:Store<Item<F>>=store_new();
 if !store_reserve(items,1){return 1}
 store_push(items,Item::<F>{ticket:Ticket{code:65},callback:packet.callback,value:3});
 let root=wrap(give items);let child=root.child;
 let element=store_at_mut(child.items,0);
 if element.callback()!=8{return 2}
 if element.callback()!=8{return 3}
 return 0
}
fn main()->i32{return run(pack(make()))}
"#;
    source("generic-callback-element", text, true);
    verified(text);
    native("generic-callback-element", text, "AZ", 0);
}

#[test]
fn direct_callback_store_and_noncallback_generic_wrapper_controls() {
    let callback = r#"
resource Ticket{code:i32}
package Item<F>{manages ticket:Ticket;callback:F;value:i32}
package Packet<F>{manages marker:Ticket;callback:F}
fn make()->impl Fn()->i32{let count=8;return move()=>count}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:90},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{
 let items:Store<Item<F>>=store_new();store_reserve(items,1);
 store_push(items,Item::<F>{ticket:Ticket{code:65},callback:packet.callback,value:3});
 let element=store_at_mut(items,0);return element.callback()-8
}
fn main()->i32{return run(pack(make()))}
"#;
    source("direct-callback-control", callback, true);
    let wrapper = format!(
        "{HEAD}package GenericBag<T:StoreElement>{{manages items:Store<T>}}package GenericRoot<T:StoreElement>{{child:GenericBag<T>}}fn wrap<T:StoreElement>(give items:Store<T>)->GenericRoot<T>{{return GenericRoot::<T>{{child:GenericBag::<T>{{items:give items}}}}}}fn main()->i32{{let items:Store<Item>=store_new();let root=wrap(give items);let child=root.child;return i32(store_len(child.items))}}"
    );
    source("noncallback-wrapper-control", &wrapper, true);
}
