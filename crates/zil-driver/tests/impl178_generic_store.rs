//! Generic Store helpers preserve concrete element identity and real ownership.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "generic-store178-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
        assert_eq!(out.stdout, stdout);
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;n:i32}
package Other{manages r:R;wide:i64}
fn item(n:i32)->Item=>Item{r:R{code:n},n:n}
fn other(n:i32)->Other=>Other{r:R{code:n},wide:i64(n)}
"#;
const HELPERS: &str = r#"
fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
fn reserve<T:StoreElement>(s:Store<T>,n:i64)->bool=>store_reserve(s,n)
fn push<T:StoreElement>(s:Store<T>,v:T)->T!void=>store_push(s,v)
fn pop<T:StoreElement>(s:Store<T>)->void!T=>store_pop(s)
fn len<T:StoreElement>(s:Store<T>)->i64=>store_len(s)
fn relay<T:StoreElement>(give s:Store<T>)->Store<T> contract{result(give)}=>give s
"#;

#[test]
fn two_element_shapes_use_the_same_actual_generic_helpers() {
    let source = format!(
        "{MODEL}{HELPERS}fn main()->void!i32{{let s:Store<Item>=empty();reserve(s,2);push(s,item(65));push(s,item(66));let b:Store<Other>=empty();reserve(b,1);push(b,other(67));let old=pop(s)?;let next=relay(give b);return old.n+i32(len(s))+i32(len(next))-68}}"
    );
    native(&[("main", &source)], b"CBA", 0);
}

#[test]
fn unused_templates_reject_unproven_store_element_or_owner() {
    for body in [
        "fn bad<T>(s:Store<T>)->i64=>store_len(s)",
        "trait Marker{}fn bad<T:Marker>(s:Store<T>)->i64=>store_len(s)",
        "fn bad<T:StoreElement>(s:Store<T>)->Store<T>contract{result(give)}=>give s",
        "fn bad<T:StoreElement>(give s:Store<T>)->Store<T>contract{result(give)}{let d=s;return give d}",
        "fn bad<T:StoreElement>(give s:Store<T>)->Store<T>contract{result(give)}{let a=give s;return give s}",
        "fn bad<T:StoreElement>(s:Store<T>,v:T){store_push(s,v);store_push(s,v)}",
    ] {
        let source = format!("{MODEL}{body};fn main(){{}}");
        let report = check(&[("main", &source)]);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{body}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn cross_module_helpers_preserve_two_nominal_elements_and_failure_returns() {
    let model = MODEL
        .replace("resource R", "pub resource R")
        .replace("package Item", "pub package Item")
        .replace("package Other", "pub package Other")
        .replace("fn item", "pub fn item")
        .replace("fn other", "pub fn other")
        .replace(";n:i32", ";pub n:i32")
        .replace(";wide:i64", ";pub wide:i64");
    let helper = HELPERS.replace("fn ", "pub fn ")
        + r#"
pub fn two<T:StoreElement>(s:Store<T>,a:T,b:T)->T!void{push(s,a)?;push(s,b)?;return ()}
pub fn observe<T:StoreElement>(s:Store<T>)->i64{return len(s)}
"#;
    let main = r#"import model;import collection;
fn main()->void!i32{
 let a:Store<model.Item>=collection.empty();collection.reserve(a,1);
 collection.two(a,model.item(65),model.item(66));
 let b:Store<model.Other>=collection.empty();collection.reserve(b,1);collection.push(b,model.other(67));
 let popped=collection.pop(a)?;let moved=collection.relay(give b);
 return popped.n+i32(collection.observe(a))+i32(collection.observe(moved))-66
}"#;
    native(
        &[("main", main), ("model", &model), ("collection", &helper)],
        b"BCA",
        0,
    );
}

#[test]
fn element_bound_checks_real_shapes_and_never_grants_copy_or_members() {
    for bad in [
        "let s:Store<i32>=empty();",
        "let s:Store<R>=empty();",
        "let s:Store<OnlyCopy>=empty();",
        "let s:Store<Zero>=empty();",
        "let s:Store<Nested>=empty();",
    ] {
        let source = format!(
            "{MODEL}{HELPERS}struct V{{n:i32}}package OnlyCopy{{n:i32}}resource Z{{}}package Zero{{manages z:Z}}package Nested{{manages items:Store<Item>}}fn main(){{{bad}}}"
        );
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{bad}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    for bad in [
        "fn bad<T:StoreElement>(a:&T)->T=>*a",
        "fn bad<T:StoreElement>(a:T)->i32=>a.n",
        "package Bad<T>{manages s:Store<T>}",
        "package Bad<T:StoreElement>{manages item:T}",
        "package Batch<T:StoreElement>{manages s:Store<T>}fn bad<T>(b:Batch<T>)->Batch<T>=>b",
        "trait StoreElement{}",
        "impl StoreElement for Item{}",
        "fn bad<T:StoreElement>(s:Store<T>,v:T)->void!void{store_push(s,v)?;return ()}",
        "fn bad<T:StoreElement>(s:Store<T>)->T!T{let p=store_pop(s)?;return p}",
    ] {
        let source = format!("{MODEL}{bad};fn main(){{}}");
        let report = check(&[("main", &source)]);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{bad}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn descriptor_aliases_do_not_revive_after_give_or_replacement() {
    for body in [
        "let s:Store<T>=store_new();let old=s;let next=give s;return store_len(old)",
        "let s:Store<T>=store_new();let old=s;if condition{let next=give s;}return store_len(old)",
    ] {
        let source = format!("fn bad<T:StoreElement>(condition:bool)->i64{{{body}}}fn main(){{}}");
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand
                    && d.message.contains("descriptor depends")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_owning_package_and_concrete_callback_state_compose() {
    let source = format!(
        r#"{MODEL}{HELPERS}
 package Batch<T:StoreElement>{{manages items:Store<T>}}
 fn batch<T:StoreElement>(v:T)->Batch<T>{{let s:Store<T>=empty();reserve(s,1);push(s,v);return Batch::<T>{{items:give s}}}}
 fn take<T:StoreElement>(b:Batch<T>)->Store<T>contract{{result(give)}}{{return give b.items}}
 package State<F>{{manages items:Store<Item>;callback:F;n:i32}}
 fn make()->impl FnMut()->i32{{var n=0;return move()=>{{n+=1;return n}}}}
 fn pack<F:FnMut()->i32>(f:F)->State<F>{{let s:Store<Item>=empty();reserve(s,1);push(s,item(65));return State::<F>{{items:give s,callback:f,n:0}}}}
 fn update<F:FnMut()->i32>(state:State<F>)->State<F>{{var p=state;let old=give p.items;p.items=empty();reserve(p.items,1);push(p.items,item(66));let f=&mut p.callback;p.n=f();return p}}
 fn main()->i32{{let a=take(batch(other(67)));let p=update(pack(make()));return p.n+i32(len(a))+i32(store_len(p.items))-3}}
 "#
    );
    native(&[("main", &source)], b"ABC", 0);
}

#[test]
fn unused_package_fields_keep_mode_and_instance_identity() {
    for body in [
        "fn bad<T:StoreElement>(s:Store<T>)->Batch<T> =>Batch::<T>{items:s}",
        "fn bad<T:StoreElement>(b:&Batch<T>)->Store<T>contract{result(give)}=>give b.items",
        "fn bad<T:StoreElement>(b:Batch<T>)->i64{var p=b;let old=p.items;p.items=store_new();return store_len(old)}",
        "fn bad<T:StoreElement>(b:Batch<T>,flag:bool)->i64{var p=b;let old=p.items;if flag{p.items=store_new();}return store_len(old)}",
        "fn bad<T:StoreElement>(v:T)->T!T=>v",
    ] {
        let source =
            format!("package Batch<T:StoreElement>{{manages items:Store<T>}}{body};fn main(){{}}");
        let report = check(&[("main", &source)]);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{body}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand),
            "{body}: {}",
            report.to_json()
        );
    }
}

#[test]
fn generic_prefix_drain_repeats_with_real_item_cleanup() {
    let source = format!(
        "{MODEL}{HELPERS}fn clear<T:StoreElement>(s:Store<T>)->i64{{var n:i64=0;while store_len(s)>0{{store_pop(s);n+=1}}return n}}fn main()->i32{{let s:Store<Item>=empty();reserve(s,3);push(s,item(65));push(s,item(66));push(s,item(67));return i32(clear(s))+i32(len(s))-3}}"
    );
    native(&[("main", &source)], b"CBA", 0);
    for body in [
        "fn bad<T:StoreElement>(s:Store<T>,v:T){while store_len(s)<2{store_push(s,v)}}",
        "fn bad<T:StoreElement>(give s:Store<T>){while store_len(s)<2{let moved=give s;}}",
    ] {
        let report = check(&[("main", &format!("{body};fn main(){{}}"))]);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand
                    && d.message.contains("loop must preserve")),
            "{}",
            report.to_json()
        );
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw178.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn rejected_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    category: ResultCategory,
    code: &str,
    message: &str,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join(format!("{label}.bin")), &bytes).unwrap();
    for (path, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            dir.join(format!("{label}-{path}.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.category == category && d.code == code && d.message.contains(message)),
            "{label}: {errors:?}"
        );
    }
}

#[test]
fn generic_source_still_requires_independent_raw_and_wire_ownership() {
    use zil_ir::{InstructionKind as I, Type};
    let source = format!(
        "{MODEL}{HELPERS}fn main()->i32{{let s:Store<Item>=empty();reserve(s,1);push(s,item(65));return i32(len(s))-1}}"
    );
    let (original, plan) = candidate(&source);
    let mut forged = original.clone();
    let i = forged
        .resources
        .iter()
        .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    forged.resources[i].name = "@builtin:OwnedStore<forged-proof>".into();
    rejected_candidate(
        "generic-wrong-element-identity",
        forged,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
        "canonical element identity",
    );
    let mut forged = original.clone();
    let instruction = forged
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::OwnedStorePush { .. }))
        .unwrap();
    if let I::OwnedStorePush { store, value } = &mut instruction.kind {
        *value = *store;
    }
    rejected_candidate(
        "generic-desc-is-not-item",
        forged,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
        "complete nominal Package element",
    );
    let mut forged = original;
    let mut removed = false;
    for f in &mut forged.functions {
        if let Some((b,i))=f.blocks.iter().enumerate().find_map(|(b,block)|block.instructions.iter().position(|i|matches!(i.kind,I::DropManaged{local} if matches!(&f.locals[local.0 as usize].ty,Type::Managed(p) if matches!(p.as_ref(),Type::OwnedStore{..})))).map(|i|(b,i))){
   f.blocks[b].instructions.remove(i);removed=true;break;
  }
    }
    assert!(removed);
    rejected_candidate(
        "generic-unpaid-prefix",
        forged,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
        "outstanding",
    );
}

#[test]
fn bounded_element_shape_work_has_an_exact_definition_time_boundary() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "budget178.zil",
        format!(
            "{MODEL}{HELPERS}fn main()->i32{{let s:Store<Other>=empty();reserve(s,1);push(s,other(65));return i32(len(s))-1}}"
        ),
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand_with_map_limits(&source, &ast, &Limits::default()).is_ok());
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: mid,
                ..Limits::default()
            },
        ) {
            Ok(_) => high = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| e.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = mid;
            }
        }
    }
    assert!(
        zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let errors = zil_expand::expand_with_map_limits(
        &source,
        &ast,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.category == ResultCategory::ResourceLimit)
    );
    let dir = directory();
    fs::write(
        dir.join("boundary.json"),
        format!("{{\"pass_steps\":{high},\"reject_steps\":{}}}\n", high - 1),
    )
    .unwrap();
}
