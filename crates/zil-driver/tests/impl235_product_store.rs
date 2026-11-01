//! 235 source and independently checked raw/Wire ordinary ownership.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{InstructionKind as I, Type};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages r:R;mark:i32}
package Set{manages items:Store<Leaf>}
package Session{left:Set;right:Set;ready:bool}
fn set()->Set{return Set{items:store_new()}}
fn session()->Session{return Session{left:set(),right:set(),ready:true}}
fn pass(s:Session)->Session{return s}
"#;
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let d = root.join(format!(
        "product235-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&d).unwrap();
    d
}
fn unit(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "product235.zil", s)
}
fn source(label: &str, s: &str, expected: ResultCategory) {
    let d = dir(label);
    fs::write(d.join("source.zil"), s).unwrap();
    let r = compile_source(
        &unit(s),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(d.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(d.join("compile.json"), r.to_json()).unwrap();
    assert_eq!(r.status, expected, "{label}: {}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
fn native(label: &str, s: &str, stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let d = dir(label);
        fs::write(d.join("source.zil"), s).unwrap();
        let exe = d.join("program");
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let r = compile_source(
            &unit(s),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(exe.clone()),
                emit_dir: Some(d.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(d.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "{label}: {}",
            r.to_json()
        );
        let o = Command::new(exe).output().unwrap();
        fs::write(d.join("stdout"), &o.stdout).unwrap();
        fs::write(d.join("stderr"), &o.stderr).unwrap();
        assert_eq!(o.status.code(), Some(exit), "{label}: {o:?}");
        assert_eq!(o.stdout, stdout, "{label}: {o:?}");
        assert!(o.stderr.is_empty());
    }
}
fn candidate(s: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let u = unit(s);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let m = zil_types::check(&core, &target, &limits).unwrap();
    let p = zil_analyze::analyze(&m, &limits).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &target, &limits).unwrap();
    (m, p)
}
fn raw_reject(label: &str, m: zil_ir::TypedModule, mut p: zil_ir::CandidatePlan) {
    let l = Limits::default();
    let t = TargetProfile::host_llvm();
    p.module_fingerprint = m.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&m, &p, &l).unwrap();
    let d = dir(label);
    fs::write(d.join("candidate.wire"), &wire).unwrap();
    let errors = zil_analyze::analyze(&m, &l).expect_err(label);
    assert!(
        errors.iter().all(|d| d.phase == Phase::Analyze),
        "{label}: {errors:?}"
    );
    fs::write(d.join("analyze.txt"), format!("{errors:#?}")).unwrap();
    for (kind, r) in [
        ("raw", zil_verify::verify_module(m, p, &t, &l)),
        ("wire", zil_verify::verify_module_bytes(&wire, &t, &l)),
    ] {
        let e = r.expect_err(label);
        assert!(e.iter().all(|d| d.phase == Phase::Verify), "{label}: {e:?}");
        assert!(
            e.iter().any(|d| matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
            "{label}: {e:?}"
        );
        fs::write(d.join(format!("{kind}.txt")), format!("{e:#?}")).unwrap();
    }
}
#[test]
fn original206_inner_store_is_promoted_without_rewriting() {
    source(
        "original206",
        include_str!("impl206_independent_fixtures/22_inner_store.zil"),
        ResultCategory::Success,
    );
}
#[test]
fn two_same_typed_stores_move_refill_and_drop_their_actual_prefixes() {
    native(
        "twins",
        &format!(
            "{MODEL} fn main()->i32{{var s=pass(session());var a=s.left;var b=s.right;if !store_reserve(a.items,2){{return 1}};if !store_reserve(b.items,2){{return 2}};store_push(a.items,Leaf{{r:R{{code:65}},mark:1}});store_push(b.items,Leaf{{r:R{{code:66}},mark:2}});s.left=a;s.right=b;return 0}}"
        ),
        b"AB",
        0,
    );
    native(
        "replace",
        &format!(
            "{MODEL} fn main()->i32{{var s=session();var a=s.left;store_reserve(a.items,1);store_push(a.items,Leaf{{r:R{{code:65}},mark:1}});s.left=a;let fresh=set();s.left=fresh;return 0}}"
        ),
        b"A",
        0,
    );
}
#[test]
fn a_complete_taken_store_field_can_be_refilled_and_replaced() {
    native(
        "store-refill",
        &format!(
            "{MODEL} fn main()->i32{{var s=session();var a=s.left;store_reserve(a.items,1);store_push(a.items,Leaf{{r:R{{code:65}},mark:1}});let old=give a.items;a.items=store_new();store_reserve(a.items,1);store_push(a.items,Leaf{{r:R{{code:66}},mark:2}});s.left=a;return 0}}"
        ),
        b"AB",
        0,
    );
    native(
        "store-replace",
        &format!(
            "{MODEL} fn main()->i32{{var s=session();var a=s.left;store_reserve(a.items,1);store_push(a.items,Leaf{{r:R{{code:65}},mark:1}});let replacement:Store<Leaf>=store_new();store_reserve(replacement,1);store_push(replacement,Leaf{{r:R{{code:66}},mark:2}});a.items=give replacement;s.left=a;return 0}}"
        ),
        b"AB",
        0,
    );
}

#[test]
fn partial_and_live_windows_reject_but_last_use_and_other_store_are_allowed() {
    for (label, body) in [
        (
            "partial-return",
            "var s=session();let a=s.left;let t=pass(s)",
        ),
        (
            "partial-refill",
            "var s=session();var a=s.left;let items=give a.items;s.left=a",
        ),
        (
            "window-take",
            "var s=session();var a=s.left;let w=store_at(a.items,0);let items=give a.items;let x=w.mark",
        ),
        (
            "window-replace",
            "var s=session();var a=s.left;let w=store_at(a.items,0);a.items=store_new();let x=w.mark",
        ),
        (
            "window-reserve",
            "var s=session();var a=s.left;let w=store_at(a.items,0);store_reserve(a.items,2);let x=w.mark",
        ),
    ] {
        source(
            label,
            &format!("{MODEL} fn main(){{{body}}}"),
            ResultCategory::SafeReject,
        )
    }
    source(
        "distinct-stores",
        &format!(
            "{MODEL} fn main(){{var s=session();var a=s.left;var b=s.right;let w=store_at(a.items,0);store_reserve(b.items,2);let x=w.mark;s.left=a;s.right=b}}"
        ),
        ResultCategory::Success,
    );
    source(
        "last-use",
        &format!(
            "{MODEL} fn main(){{var s=session();var a=s.left;let w=store_at(a.items,0);let x=w.mark;store_reserve(a.items,2);s.left=a}}"
        ),
        ResultCategory::Success,
    );
}
#[test]
fn new_owned_products_do_not_gain_other_shape_permissions() {
    for (label, body) in [
        ("element", "fn unused(s:Store<Session>){} fn main(){}"),
        ("array", "fn unused(s:[Session;2]){} fn main(){}"),
        (
            "sum",
            "fn unused(s:Session)->void!Session{return s} fn main(){}",
        ),
        ("shared", "fn unused(s:&Session){} fn main(){}"),
        (
            "capture",
            "fn main(){let s=session();let f=move ()=>i32(if s.ready{1}else{0})}",
        ),
    ] {
        source(
            label,
            &format!("{MODEL} {body}"),
            ResultCategory::Unsupported,
        )
    }
}
#[test]
fn raw_and_wire_rebuild_exact_store_definitions_for_new_products() {
    let (m, p) = candidate(&format!("{MODEL} fn main(){{let s=session()}}"));
    for kind in [
        "missing",
        "wrong-name",
        "teardown",
        "duplicate",
        "wrong-element",
    ] {
        let mut m = m.clone();
        let index = m
            .resources
            .iter()
            .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
            .unwrap();
        match kind {
            "missing" => {
                m.resources.remove(index);
            }
            "wrong-name" => m.resources[index].name.push('x'),
            "teardown" => {
                m.resources[index].teardown =
                    Some(m.functions.iter().find(|f| f.name == "main").unwrap().id)
            }
            "duplicate" => m.resources.push(m.resources[index].clone()),
            "wrong-element" => {
                let Type::OwnedStore { element } = &mut m.resources[index].payload else {
                    unreachable!()
                };
                let Type::Package { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                fields[1].1 = Type::I64;
            }
            _ => unreachable!(),
        }
        raw_reject(kind, m, p.clone());
    }
}
#[test]
fn raw_and_wire_reject_duplicate_child_ssa() {
    let (m, p) = candidate(&format!("{MODEL} fn main(){{let s=session()}}"));
    let mut altered = m.clone();
    let f = altered
        .functions
        .iter_mut()
        .find(|f| f.name == "session")
        .unwrap();
    let make = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(&i.kind,I::MakePackage{fields} if fields.len()==3))
        .unwrap();
    let I::MakePackage { fields } = &mut make.kind else {
        unreachable!()
    };
    fields[1] = fields[0];
    raw_reject("duplicate-child", altered, p.clone());
}
#[test]
fn same_typed_store_leaves_keep_created_and_incoming_origins() {
    let prefix = format!(
        "{MODEL} fn origin(give old:Store<Leaf>)->Store<Leaf> contract{{result(create)}}{{let fresh:Store<Leaf>=store_new();var s=Session{{left:Set{{items:give old}},right:Set{{items:give fresh}},ready:true}};var b=s.right;s.right=b;let next=s;let result=next.right;return give result.items}} fn main(){{}}"
    );
    source("actual-created", &prefix, ResultCategory::Success);
    source(
        "actual-incoming",
        &prefix.replace("let result=next.right", "let result=next.left"),
        ResultCategory::SafeReject,
    );
    let (mut m, p) = candidate(&prefix);
    let f = m.functions.iter_mut().find(|f| f.name == "origin").unwrap();
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::TakePackageField { local, index } = &mut i.kind
            && f.locals[local.0 as usize].name == "next"
        {
            assert_eq!(*index, 1);
            *index = 0;
            changed = true;
        }
    }
    assert!(changed);
    raw_reject("wrong-twin-origin", m, p);
}
#[test]
fn raw_and_wire_recover_the_actual_store_root_of_an_element_window() {
    let text = format!(
        "{MODEL} fn main(){{var s=session();var a=s.left;var b=s.right;let w=store_at(a.items,0);store_reserve(b.items,2);let x=w.mark;s.left=a;s.right=b}}"
    );
    let (mut m, p) = candidate(&text);
    let f = m.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let borrowed = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::OwnedStoreBorrow { store, .. } => Some(store),
            _ => None,
        })
        .unwrap();
    let mut changed = false;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::OwnedStoreReserve { store, .. } = &mut i.kind {
            *store = borrowed;
            changed = true;
        }
    }
    assert!(changed);
    raw_reject("same-store-invalidates-window", m, p);
}
