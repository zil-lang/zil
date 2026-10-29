//! Real source producers and independent Analyze/raw/Wire checks for callback
//! Store elements. Native execution belongs to the separate consumer suite.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CallableKind, CandidatePlan, HoldRoot, InstructionKind as I, Type, TypedModule};

const ORIGINAL: &str = include_str!("impl200_fixtures/02_store_of_callback_state.zil");
// Byte-identical to preflight201/analyze/cases/01_split_pair_two_elements.zil.
// This retains the original two-element probe rather than replacing it with
// a new two-State-parameter helper outside the 201 entry profile.
const SPLIT: &str = r#"package Pair<T,F> { manages bytes: ByteBuffer; first: F; second: F; value: T }
package Item<T,F> { manages bytes: ByteBuffer; callback: F; value: T }

fn make_callback(seed: i32) -> impl Fn() -> i32 {
    let log = bytes_new()
    return move () => { bytes_len(log); return seed }
}

fn pair<T: Copy, F: Fn() -> i32>(first: F, second: F, value: T) -> Pair<T,F> {
    return Pair::<T,F> {
        bytes: bytes_new(), first: first, second: second, value: value
    }
}

fn split_and_consume<F: Fn() -> i32>(state: Pair<i32,F>) -> void!i32 {
    let items: Store<Item<i32,F>> = store_new()
    if !store_reserve(items, 2) { return () }
    let first = Item::<i32,F> {
        bytes: bytes_new(), callback: state.first, value: state.value
    }
    let second = Item::<i32,F> {
        bytes: bytes_new(), callback: state.second, value: state.value
    }
    store_push(items, first)
    store_push(items, second)
    let tail = store_pop(items)?
    let head = store_pop(items)?
    return head.callback() + tail.callback() + head.value + tail.value - 42
}

fn main() -> void!i32 {
    return split_and_consume(pair(make_callback(17), make_callback(23), 1))
}
"#;
const WINDOW: &str = r#"package State<F>{manages storage:ByteBuffer;callback:F}
fn make()->impl FnMut()->i32{let b=bytes_new();var n=0;return move()=>{bytes_len(b);n+=1;return n}}
fn pack<F:FnMut()->i32>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback}}
fn run<F:FnMut()->i32>(state:State<F>)->i32{
    let items:Store<State<F>>=store_new()
    if !store_reserve(items,1){return 1}
    store_push(items,state)
    let item=store_at_mut(items,0)
    return item.callback()-1
}
fn main()->i32{return run(pack(make()))}
"#;
const LENDING: &str = r#"package State<F>{manages storage:ByteBuffer;callback:F}
fn make()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}
fn pack<F:FnMut()->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback}}
fn run<F:FnMut()->&[u8]>(state:State<F>)->i32{
    let items:Store<State<F>>=store_new()
    if !store_reserve(items,1){return 1}
    store_push(items,state)
    let item=store_at_mut(items,0)
    let view=item.callback()
    return i32(view[0])-7
}
fn main()->i32{return run(pack(make()))}
"#;
// Byte-identical to preflight201/analyze/cases/02_package_store_capture.zil.
const PACKAGE_STORE_CAPTURE: &str = r#"package Item { manages bytes: ByteBuffer; value: i32 }
package Bag { manages items: Store<Item>; manages bytes: ByteBuffer }

fn make() -> impl Fn() -> i32 {
    let items: Store<Item> = store_new()
    store_reserve(items, 1)
    store_push(items, Item { bytes: bytes_new(), value: 42 })
    let bag = Bag { items: give items, bytes: bytes_new() }
    return move () => { return i32(store_len(bag.items)) }
}

fn main() -> i32 {
    let callback = make()
    return callback() - 1
}
"#;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str, source: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "analyze201-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("source.zil"), source).unwrap();
        Self(path, saved.is_some())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), contents).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn candidate(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label, source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "store201.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let module = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("types.txt", format!("{module:#?}"));
    let module = module.unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    let verified = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    );
    case.write("verify.txt", format!("{verified:#?}"));
    verified.unwrap();
    (module, plan)
}
fn pair(
    label: &str,
    source: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    reject: Option<(ResultCategory, &str, &str)>,
) {
    let case = Case::new(label, source);
    let limits = Limits::default();
    case.write("module.txt", format!("{module:#?}"));
    let analyzed = zil_analyze::analyze(&module, &limits);
    case.write("analyze.txt", format!("{analyzed:#?}"));
    match reject {
        None => {
            plan = analyzed.unwrap();
        }
        Some((category, message, _)) => {
            let errors = analyzed.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Analyze
                    && d.category == category
                    && d.message.contains(message)),
                "{label}: {errors:?}"
            );
        }
    }
    plan.module_fingerprint = module.fingerprint();
    case.write("candidate.txt", format!("{plan:#?}"));
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    case.write("candidate.wire", &bytes);
    for (route, checked) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
        ),
    ] {
        case.write(&format!("{route}.txt"), format!("{checked:#?}"));
        match reject {
            None => {
                checked.unwrap();
            }
            Some((category, _, code)) => {
                let errors = checked.unwrap_err();
                assert!(
                    errors.iter().any(|d| d.phase == Phase::Verify
                        && d.category == category
                        && d.code == code),
                    "{label}/{route}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn original_sources_reach_analyze_and_both_independent_verifier_routes() {
    assert_eq!(ORIGINAL.len(), 626);
    for (label, source) in [("original02", ORIGINAL), ("original-split-pair", SPLIT)] {
        let (module, plan) = candidate(label, source);
        pair(label, source, module, plan, None);
    }
}

#[test]
fn actual_borrowed_callback_and_lending_result_keep_the_store_local() {
    for (label, source, lending) in [
        ("fnmut-window", WINDOW, false),
        ("fnmut-lending-window", LENDING, true),
    ] {
        let (module, plan) = candidate(label, source);
        let f = module
            .functions
            .iter()
            .find(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, I::OwnedStoreBorrowMut { .. }))
            })
            .unwrap();
        let fp = &plan.functions[f.id.0 as usize];
        let owner = f.locals.iter().find(|local| matches!(&local.ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore {..}))).unwrap().id;
        let call = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find(|i| matches!(i.kind, I::CallClosureBorrowed { .. }))
            .unwrap();
        let I::CallClosureBorrowed { callee, .. } = call.kind else {
            unreachable!()
        };
        assert_eq!(
            fp.holds[callee.0 as usize].roots,
            vec![HoldRoot::Local(owner)]
        );
        if lending {
            assert_eq!(
                fp.holds[call.result.unwrap().0 as usize].roots,
                vec![HoldRoot::Local(owner)]
            );
        }
        pair(label, source, module, plan, None);
    }
}

#[test]
fn second_push_cannot_reuse_the_first_runtime_element() {
    let (original, plan) = candidate("push-control", SPLIT);
    pair("push-control", SPLIT, original.clone(), plan.clone(), None);
    let mut module = original;
    let mut first = None;
    let mut changed = false;
    for f in &mut module.functions {
        for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let I::OwnedStorePush { value, .. } = &mut instruction.kind {
                if let Some(previous) = first {
                    *value = previous;
                    changed = true;
                } else {
                    first = Some(*value);
                }
            }
        }
    }
    assert!(changed);
    pair(
        "duplicate-push-value",
        SPLIT,
        module,
        plan,
        Some((ResultCategory::SafeReject, "resource", "E-VERIFY-RESOURCE")),
    );
}

#[test]
fn removing_parent_prefix_cleanup_is_a_resource_failure() {
    let (original, plan) = candidate("prefix-cleanup-control", ORIGINAL);
    let mut module = original;
    let mut changed = false;
    for f in &mut module.functions {
        for block in &mut f.blocks {
            block.instructions.retain(|i| {
                if let I::DropManaged { local } = i.kind
                    && matches!(&f.locals[local.0 as usize].ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore {..}))
                { changed = true; false } else { true }
            });
        }
    }
    assert!(changed);
    pair(
        "missing-prefix-cleanup",
        ORIGINAL,
        module,
        plan,
        Some((ResultCategory::SafeReject, "resource", "E-VERIFY-RESOURCE")),
    );
}

#[test]
fn forged_self_containment_and_borrowed_modes_are_not_authority() {
    let (original, plan) = candidate("self-contained-control", ORIGINAL);
    let mut module = original;
    let capture = module
        .closures
        .iter_mut()
        .flat_map(|c| &mut c.captures)
        .find(|t| matches!(t, Type::Managed(inner) if **inner == Type::ByteBuffer))
        .unwrap();
    *capture = Type::Desc(Box::new(Type::ByteBuffer));
    pair(
        "forged-self-containment",
        ORIGINAL,
        module,
        plan,
        Some((
            ResultCategory::Unsupported,
            "self-contained environment",
            "E-UNSUPPORTED",
        )),
    );
    let (original, plan) = candidate("borrowed-mode-control", WINDOW);
    for kind in [CallableKind::FnOnce, CallableKind::FnMut] {
        let mut module = original.clone();
        let mut changed = false;
        for f in &mut module.functions {
            let mut shared_callees = vec![];
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let I::CallClosureBorrowed {
                    callee,
                    kind: actual,
                    ..
                } = &mut i.kind
                {
                    *actual = kind;
                    if kind == CallableKind::FnMut {
                        let Type::Address { mutable, .. } = &mut f.values[callee.0 as usize] else {
                            unreachable!()
                        };
                        *mutable = false;
                        shared_callees.push(*callee);
                    }
                    changed = true;
                }
            }
            for callee in shared_callees {
                let reborrow = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| i.result == Some(callee))
                    .unwrap();
                let I::ReborrowAddress { mutable, .. } = &mut reborrow.kind else {
                    panic!("callee must be an actual reborrow");
                };
                // Change the actual producer too. This valid shared reborrow
                // must reach the FnMut calling-permission check.
                *mutable = false;
            }
        }
        assert!(changed);
        pair(
            &format!("borrowed-mode-{kind:?}"),
            WINDOW,
            module,
            plan.clone(),
            Some((
                ResultCategory::SafeReject,
                "exclusive FnMut address",
                "E-VERIFY-CLOSURE-CALL",
            )),
        );
    }
}

#[test]
fn original_package_store_capture_is_owned_since_204() {
    assert_eq!(
        PACKAGE_STORE_CAPTURE,
        include_str!("impl204_fixtures/02_original201_package_store_capture.zil")
    );
    let case = Case::new("original-package-store-capture", PACKAGE_STORE_CAPTURE);
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "store201-capture.zil",
        PACKAGE_STORE_CAPTURE,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let result = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("types.txt", format!("{result:#?}"));
    let module = result.unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    pair(
        "original-package-store-capture-204",
        PACKAGE_STORE_CAPTURE,
        module,
        plan.unwrap(),
        None,
    );
}

#[test]
fn source_derived_analyze_budgets_have_exact_fresh_boundaries() {
    for (label, source) in [
        ("original02", ORIGINAL),
        ("split", SPLIT),
        ("lending", LENDING),
    ] {
        let (module, expected) = candidate(label, source);
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            match zil_analyze::analyze(
                &module,
                &Limits {
                    max_analysis_steps: mid,
                    ..Limits::default()
                },
            ) {
                Ok(_) => high = mid,
                Err(errors) => {
                    assert_eq!(errors.len(), 1);
                    assert_eq!(errors[0].phase, Phase::Analyze);
                    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                    low = mid + 1;
                }
            }
        }
        let exact = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: low,
                ..Limits::default()
            },
        )
        .unwrap();
        let below = zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: low - 1,
                ..Limits::default()
            },
        )
        .unwrap_err();
        assert_eq!(below.len(), 1);
        assert_eq!(below[0].phase, Phase::Analyze);
        assert_eq!(below[0].category, ResultCategory::ResourceLimit);
        assert_eq!(exact, expected);
        assert_eq!(
            zil_analyze::analyze(&module, &Limits::default()).unwrap(),
            expected
        );
        let case = Case::new(&format!("budget-{label}"), source);
        case.write(
            "boundary.txt",
            format!("Analyze N={low}\nN-1={below:#?}\nexact/fresh/default candidates equal\n"),
        );
        println!("source {label}: Analyze N={low}");
    }
}
