//! Independent 204 review. A raw attack is derived only after its real source
//! control has passed Types, Analyze, raw Verify and Wire Verify. Earlier
//! qualification failures are saved and cannot count as loan-safety evidence.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::*;

const ORIGINALS: [(&str, &str); 4] = [
    (
        "original00",
        include_str!("impl204_review_fixtures/00_package_owned_control.zil"),
    ),
    (
        "original01",
        include_str!("impl204_review_fixtures/01_direct_store_capture.zil"),
    ),
    (
        "original02",
        include_str!("impl204_review_fixtures/02_original201_package_store_capture.zil"),
    ),
    (
        "original03",
        include_str!("impl204_review_fixtures/03_mutating_store_capture.zil"),
    ),
];

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "review204-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn compile(case: &Case, source: &str, optimization: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "review204.zil", source),
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}

fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        case.write("stdout.log", &result.stdout);
        case.write("stderr.log", &result.stderr);
        case.write(
            "native.txt",
            format!(
                "optimization={optimization:?}\nactual_exit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n",
                result.status.code()
            ),
        );
        assert_eq!(result.status.code(), Some(exit), "{label}: {result:?}");
        assert_eq!(result.stdout, stdout.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty(), "{label}: {result:?}");
    }
}

fn source_reject(label: &str, source: &str, phase: Phase, code: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

fn verify_pair(
    case: &Case,
    module: &TypedModule,
    plan: &CandidatePlan,
    rejection: Option<(ResultCategory, &str)>,
) {
    let bytes = zil_ir::wire::encode_module_candidate(module, plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &bytes);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    );
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={wire:#?}"));
    for result in [raw, wire] {
        if let Some((category, code)) = rejection {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
                "{errors:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review204.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let typed = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.expect("the real source control must reach TypedIR before any attack");
    let plan = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    verify_pair(&case, &typed, &plan, None);
    (typed, plan)
}

fn attack(
    label: &str,
    module: TypedModule,
    original: &CandidatePlan,
    require_fresh: bool,
    category: ResultCategory,
    code: &str,
) {
    let case = Case::new(label);
    case.write("typed.txt", format!("{module:#?}"));
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{analyzed:#?}"));
    case.write(
        "candidate-origin.txt",
        if analyzed.is_ok() {
            "fresh Analyze result\n"
        } else {
            "Analyze rejected; original control plan with attacked module fingerprint, not a fresh proof\n"
        },
    );
    let mut plan = if require_fresh {
        analyzed.expect("this particular attack must reach Verify with a fresh Analyze candidate")
    } else {
        analyzed.unwrap_or_else(|_| original.clone())
    };
    plan.module_fingerprint = module.fingerprint();
    verify_pair(&case, &module, &plan, Some((category, code)));
}

const PREFIX: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages owner:Ticket;value:i32}
fn emit(n:i32){unsafe{putchar(n)}}
fn empty()->Store<Item> contract{result(create)}{return store_new()}
"#;

const TWO_STORES: &str = r#"
fn make()->impl Fn()->i32{
    let left=empty();store_reserve(left,1);store_push(left,Item{owner:Ticket{code:65},value:10});
    let right=empty();store_reserve(right,1);store_push(right,Item{owner:Ticket{code:66},value:100});
    let side=Ticket{code:83};
    return move()=>{let n=store_len(right);side.code;return i32(n+store_len(left))}
}
fn main()->i32{let callback=make();let n=callback();emit(67);return n-2}
"#;

const PACKAGE: &str = r#"
package Bag{manages left:Store<Item>;manages side:Ticket;manages right:Store<Item>}
fn make()->impl Fn()->i32{
    let left=empty();store_reserve(left,1);store_push(left,Item{owner:Ticket{code:65},value:10});
    let right=empty();store_reserve(right,1);store_push(right,Item{owner:Ticket{code:66},value:100});
    let bag=Bag{right:give right,side:Ticket{code:83},left:give left};
    return move()=>{return i32(store_len(bag.right)+store_len(bag.left))}
}
fn main()->i32{let callback=make();let n=callback();emit(67);return n-2}
"#;

// Both free Managed variables are given in the body. No outer `move` supplies
// ownership, and the branch before either give must still own both at creation.
const ONCE: &str = r#"
fn make()->impl FnOnce(i32)->i32{
    let items=empty();store_reserve(items,1);store_push(items,Item{owner:Ticket{code:65},value:10});
    let side=Ticket{code:83};
    return (mode:i32)=>{
        emit(69);if mode==0{return 0}
        let taken=give items;emit(84);
        if mode==2{let capacity:i64=9223372036854775807;if !store_reserve(taken,capacity){emit(70);return 0}return 9}
        let marker=give side;marker.code;
        return i32(store_len(taken))-1
    }
}
fn main()->i32{let callback=make();return callback(1)}
"#;

const WINDOW: &str = r#"
fn make()->impl FnMut()->i32{
    let items=empty();store_reserve(items,1);store_push(items,Item{owner:Ticket{code:65},value:10});
    return move()=>{
        let view=store_at(items,0);let answer=view.value;
        store_reserve(items,2);
        return answer
    }
}
fn main()->i32{var callback=make();return callback()-10}
"#;

const OUTER: &str = r#"
package Pair<F>{first:F;manages marker:Ticket;second:F}
package Cell<F>{manages owner:Ticket;callback:F}
fn make(label:i32,seed:i32)->impl FnMut(i32)->i32{
    let items=empty();store_reserve(items,1);store_push(items,Item{owner:Ticket{code:label},value:seed});
    return move(delta:i32)=>{let view=store_at_mut(items,0);view.value+=delta;return view.value}
}
fn pack<F:FnMut(i32)->i32>(first:F,second:F)->Pair<F>{return Pair::<F>{first:first,marker:Ticket{code:90},second:second}}
fn run<F:FnMut(i32)->i32>(pair:Pair<F>)->void!i32{
    let outer:Store<Cell<F>>=store_new();if !store_reserve(outer,2){return ()}
    store_push(outer,Cell::<F>{owner:Ticket{code:80},callback:pair.first});
    store_push(outer,Cell::<F>{owner:Ticket{code:81},callback:pair.second});
    let left=store_at_mut(outer,0);let x=left.callback(1);let y=left.callback(2);
    let right=store_at_mut(outer,1);let z=right.callback(3);
    if !store_reserve(outer,4){return ()}
    var tail=store_pop(outer)?;let t=tail.callback(4);
    return x+y+z+t-234
}
fn main()->void!i32{return run(pack(make(65,10),make(66,100)))}
"#;

#[test]
fn original_four_sources_keep_bytes_and_reach_real_o0_o2_execution() {
    for (label, source) in ORIGINALS {
        native(label, source, "", 0);
    }
}

#[test]
fn equal_store_types_keep_independent_capture_and_package_field_cleanup_order() {
    native(
        "two-owned-stores",
        &format!("{PREFIX}{TWO_STORES}"),
        "CBSA",
        0,
    );
    native(
        "package-owned-stores",
        &format!("{PREFIX}{PACKAGE}"),
        "CASB",
        0,
    );
    native(
        "two-owned-stores-never-called",
        &format!("{PREFIX}{TWO_STORES}")
            .replace("let n=callback();emit(67);return n-2", "return 0"),
        "BSA",
        0,
    );
}

#[test]
fn implicit_body_give_and_once_exit_points_keep_the_actual_owner() {
    let source = format!("{PREFIX}{ONCE}");
    for (label, text, expected, exit) in [
        ("once-after-take", source.clone(), "ETSA", 0),
        ("once-before-take", source.replace("callback(1)", "callback(0)"), "EAS", 0),
        ("once-not-called", source.replace("return callback(1)", "return 0"), "AS", 0),
        // This is an arithmetic-overflow rejection, not an allocator-fault claim.
        ("once-local-reserve-overflow", source.replace("callback(1)", "callback(2)"), "ETFAS", 0),
        ("once-argument-fails", source.replace(
            "fn main()->i32{let callback=make();return callback(1)}",
            "enum E{Fail} fn arg()->E!i32{emit(80);return E.Fail} fn main()->E!i32{let callback=make();return callback(arg()?)}",
        ), "PAS", 1),
    ] {
        native(label, &text, expected, exit);
    }
    source_reject(
        "once-cannot-repeat",
        &source.replace("return callback(1)", "callback(1);return callback(1)"),
        Phase::Analyze,
        "E-UNINITIALIZED",
    );
}

#[test]
fn package_field_give_moves_its_store_but_keeps_sibling_flags() {
    let source = format!("{PREFIX}{PACKAGE}")
        .replace("impl Fn()->i32", "impl FnOnce()->i32")
        .replace(
            "return move()=>{return i32(store_len(bag.right)+store_len(bag.left))}",
            "return ()=>{let taken=give bag.left;emit(84);return i32(store_len(taken))-1}",
        )
        .replace("let n=callback();emit(67);return n-2", "return callback()");
    native("package-once-field-take", &source, "TASB", 0);
}

#[test]
fn captured_store_windows_keep_the_parent_until_the_last_real_use() {
    let source = format!("{PREFIX}{WINDOW}");
    native("capture-window-last-use-control", &source, "A", 0);
    source_reject(
        "capture-window-live-across-reserve",
        &source.replace("return answer\n", "return view.value\n"),
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn outer_store_moves_two_same_type_environments_without_merging_inner_stores() {
    native(
        "outer-store-two-inner-stores",
        &format!("{PREFIX}{OUTER}"),
        "QBPAZ",
        0,
    );
}

#[test]
fn old_copy_callback_fields_remain_valid_outside_the_captured_inner_leaf() {
    let ordinary = format!(
        r#"{PREFIX}
package FnItem{{manages owner:Ticket;handler:fn(i32)->i32}}
fn pass(n:i32)->i32{{return n}}
fn main()->i32{{let items:Store<FnItem>=store_new();store_reserve(items,1);store_push(items,FnItem{{owner:Ticket{{code:65}},handler:pass}});return i32(store_len(items))-1}}
"#
    );
    native("ordinary-store-keeps-copy-fnptr-field", &ordinary, "A", 0);
    let captured = ordinary.replace(
        "fn main()->i32{let items:Store<FnItem>=store_new();store_reserve(items,1);store_push(items,FnItem{owner:Ticket{code:65},handler:pass});return i32(store_len(items))-1}",
        "fn make()->impl Fn()->i32{let items:Store<FnItem>=store_new();store_reserve(items,1);store_push(items,FnItem{owner:Ticket{code:65},handler:pass});return move()=>i32(store_len(items))-1}fn main()->i32{let f=make();return f()}",
    );
    assert_ne!(ordinary, captured);
    source_reject(
        "captured-inner-leaf-closes-fnptr-field",
        &captured,
        Phase::Types,
        "E-UNSUPPORTED",
    );

    let companion = format!("{PREFIX}{PACKAGE}\nfn pass(n:i32)->i32{{return n}}")
        .replace(
            "manages right:Store<Item>}",
            "manages right:Store<Item>;handler:fn(i32)->i32}",
        )
        .replace(
            "side:Ticket{code:83},left:give left}",
            "side:Ticket{code:83},left:give left,handler:pass}",
        );
    native(
        "captured-package-keeps-copy-fnptr-companion",
        &companion,
        "CASB",
        0,
    );
}

#[test]
fn declared_once_field_of_actual_fn_keeps_its_own_captured_store_cleanup() {
    let source = format!(
        r#"{PREFIX}
package State<F,G>{{first:F;manages side:Ticket;second:G}}
fn make(code:i32)->impl Fn()->i32{{let items=empty();store_reserve(items,1);store_push(items,Item{{owner:Ticket{{code:code}},value:code}});return move()=>i32(store_len(items))}}
fn pack<F:Fn()->i32,G:FnOnce()->i32>(first:F,second:G)->State<F,G>{{return State::<F,G>{{first:first,side:Ticket{{code:83}},second:second}}}}
fn identity<T:StoreElement>(state:T)->T{{return state}}
fn run<F:Fn()->i32,G:FnOnce()->i32>(state:State<F,G>)->i32{{let ready=identity(state);let a=ready.first();let b=ready.second();emit(67);return a+b+ready.first()-3}}
fn main()->i32{{return run(pack(make(65),make(66)))}}
"#
    );
    native(
        "same-actual-fn-declared-once-store-field",
        &source,
        "BCAS",
        0,
    );
    source_reject(
        "declared-once-store-field-cannot-repeat",
        &source.replace(
            "let b=ready.second();emit(67)",
            "let b=ready.second();ready.second();emit(67)",
        ),
        Phase::Types,
        "E-CLOSURE-MODE",
    );
}

#[test]
fn move_keyword_does_not_turn_a_store_description_into_an_owned_capture() {
    let owned = format!(
        r#"{PREFIX}
fn make(give items:Store<Item>)->impl Fn()->i32{{return move()=>i32(store_len(items))}}
fn main()->i32{{let items=empty();store_reserve(items,1);store_push(items,Item{{owner:Ticket{{code:65}},value:10}});let f=make(give items);return f()-1}}
"#
    );
    native("given-store-parameter-is-owned", &owned, "A", 0);
    let borrowed = owned
        .replace("make(give items:Store<Item>)", "make(items:Store<Item>)")
        .replace("make(give items)", "make(items)");
    source_reject(
        "move-of-desc-is-still-desc",
        &borrowed,
        Phase::Types,
        "E-UNSUPPORTED",
    );
}

#[test]
fn direct_child_environment_propagates_the_new_copy_result_requirement() {
    let leaf = format!("{PREFIX}{}", ORIGINALS[1].1)
        .replace("package Item { manages bytes: ByteBuffer; value: i32 }", "")
        .replace(
            "Item { bytes: bytes_new(), value: 42 }",
            "Item { owner: Ticket{code:65}, value: 42 }",
        )
        .replace("fn make()", "fn leaf()");
    let good = leaf.replace(
        "fn main()",
        "fn make()->impl Fn()->i32{let inner=leaf();return move()=>inner()}\nfn main()",
    );
    native("child-store-owned-copy-result", &good, "A", 0);

    let lending = format!(
        r#"{PREFIX}
fn leaf()->impl Fn()->i32{{let items=empty();return move()=>i32(store_len(items))}}
fn make()->impl Fn()->&[u8]{{let inner=leaf();let b=bytes_new();bytes_reserve(b,1);bytes_push(b,7u8);return move()=>{{inner();return bytes_view(b)}}}}
fn main()->i32{{let callback=make();let view=callback();return i32(view[0])-7}}
"#
    );
    let old_control = lending.replace(
        "let items=empty();return move()=>i32(store_len(items))",
        "let count=0;return move()=>count",
    );
    native("old-child-without-store-still-lends", &old_control, "", 0);
    native(
        "unrelated-owned-store-does-not-taint-old-lending",
        &format!(
            "{old_control}\nfn unrelated()->impl Fn()->i32{{let items=empty();return move()=>i32(store_len(items))}}"
        ),
        "",
        0,
    );
    source_reject(
        "child-store-cannot-hide-lending-result",
        &lending,
        Phase::Types,
        "E-UNSUPPORTED",
    );
}

#[test]
fn source_produced_unused_capture_still_requires_exact_canonical_resource() {
    let source = format!("{PREFIX}{TWO_STORES}").replace(
        "let callback=make();let n=callback();emit(67);return n-2",
        "return 0",
    );
    let (module, plan) = baseline("unused-capture-real-source", &source);
    let index = module
        .resources
        .iter()
        .position(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .unwrap();
    let mut missing = module.clone();
    missing.resources.remove(index);
    attack(
        "unused-missing-canonical",
        missing,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
    );
    let mut renamed = module.clone();
    renamed.resources[index].name.push_str("-forged");
    attack(
        "unused-renamed-canonical",
        renamed,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
    );
    let mut custom = module.clone();
    custom.resources[index].teardown = module.resources.iter().find_map(|r| r.teardown);
    assert!(custom.resources[index].teardown.is_some());
    attack(
        "unused-custom-store-teardown",
        custom,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
    );
}

#[test]
fn source_produced_make_closure_cannot_duplicate_one_actual_store_value() {
    let (module, plan) = baseline(
        "duplicate-store-capture-control",
        &format!("{PREFIX}{TWO_STORES}"),
    );
    let definition = module.closures.iter().find(|c| c.captures.iter().filter(|ty| matches!(ty, Type::Managed(p) if matches!(p.as_ref(),Type::OwnedStore{..}))).count()==2).unwrap();
    let closure = definition.id;
    let slots: Vec<_> = definition
        .captures
        .iter()
        .enumerate()
        .filter_map(|(i, ty)| {
            matches!(ty,Type::Managed(p) if matches!(p.as_ref(),Type::OwnedStore{..})).then_some(i)
        })
        .collect();
    assert_eq!(definition.captures[slots[0]], definition.captures[slots[1]]);
    let mut bad = module.clone();
    let mut changed = 0;
    for f in &mut bad.functions {
        for b in &mut f.blocks {
            for i in &mut b.instructions {
                if let InstructionKind::MakeClosure {
                    closure: id,
                    captures,
                } = &mut i.kind
                    && *id == closure
                {
                    assert_ne!(captures[slots[0]], captures[slots[1]]);
                    captures[slots[1]] = captures[slots[0]];
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(changed, 1);
    attack(
        "same-type-distinct-runtime-capture-duplicate",
        bad,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
    );
}

#[test]
fn raw_store_mutation_keeps_the_current_capture_window_alive() {
    let (mut module, plan) = baseline(
        "raw-window-order-source-control",
        &format!("{PREFIX}{WINDOW}"),
    );
    let function = module
        .closures
        .iter()
        .find(|c| c.kind == CallableKind::FnMut && c.captures.iter().any(is_owned_store))
        .unwrap()
        .function;
    let f = &mut module.functions[function.0 as usize];
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::OwnedStoreReserve { .. }))
        })
        .unwrap();
    let read = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
        .unwrap();
    let reserve = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::OwnedStoreReserve { .. }))
        .unwrap();
    assert!(read < reserve);
    let InstructionKind::OwnedStoreReserve { store, capacity } = block.instructions[reserve].kind
    else {
        unreachable!()
    };
    let mut move_indices = vec![reserve];
    for value in [store, capacity] {
        let definition = block
            .instructions
            .iter()
            .position(|i| i.result == Some(value))
            .unwrap();
        if definition > read {
            assert!(
                matches!(
                    block.instructions[definition].kind,
                    InstructionKind::DescribeCapture { .. }
                        | InstructionKind::Constant(_)
                        | InstructionKind::Load(_)
                ),
                "only side-effect-free preparation may move with the existing reserve"
            );
            if let InstructionKind::Load(local) = block.instructions[definition].kind {
                assert!(
                    matches!(&f.locals[local.0 as usize].ty,Type::Desc(payload) if matches!(payload.as_ref(),Type::OwnedStore{..}))
                );
                assert!(block.instructions[..read].iter().any(
                    |i| matches!(i.kind,InstructionKind::Store{local:defined,..} if defined==local)
                ));
            }
            move_indices.push(definition);
        }
    }
    move_indices.sort_unstable();
    move_indices.dedup();
    let moved: Vec<_> = move_indices
        .iter()
        .map(|i| block.instructions[*i].clone())
        .collect();
    let old = std::mem::take(&mut block.instructions);
    for (index, instruction) in old.into_iter().enumerate() {
        if index == read {
            block.instructions.extend(moved.iter().cloned());
        }
        if !move_indices.contains(&index) {
            block.instructions.push(instruction);
        }
    }
    attack(
        "reserve-before-last-element-read",
        module,
        &plan,
        true,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn raw_equal_typed_capture_indices_do_not_make_two_stores_the_same_parent() {
    let source = format!(
        r#"{PREFIX}
fn make()->impl FnMut()->i32{{
    let left=empty();store_reserve(left,1);store_push(left,Item{{owner:Ticket{{code:65}},value:10}});
    let right=empty();store_reserve(right,1);store_push(right,Item{{owner:Ticket{{code:66}},value:100}});
    return move()=>{{let view=store_at(right,0);store_reserve(left,2);return view.value}}
}}
fn main()->i32{{var f=make();return f()-100}}
"#
    );
    let (mut module, plan) = baseline("distinct-runtime-capture-parents", &source);
    let definition = module
        .closures
        .iter()
        .find(|c| {
            c.kind == CallableKind::FnMut
                && c.captures.iter().filter(|t| is_owned_store(t)).count() == 2
        })
        .unwrap();
    assert_eq!(definition.captures[0], definition.captures[1]);
    let f = &mut module.functions[definition.function.0 as usize];
    let borrowed = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::OwnedStoreBorrow { store, .. } => Some(store),
            _ => None,
        })
        .unwrap();
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::OwnedStoreReserve { store, .. } = &mut i.kind {
            assert_ne!(*store, borrowed);
            *store = borrowed;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    attack(
        "reserve-repointed-to-other-same-type-capture",
        module,
        &plan,
        true,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn actual_capture_writes_and_transfers_cannot_be_hidden_by_mode_or_read_metadata() {
    let (module, plan) = baseline("shared-write-source-control", &format!("{PREFIX}{WINDOW}"));
    let mut shared = module.clone();
    let definition = shared
        .closures
        .iter_mut()
        .find(|c| c.kind == CallableKind::FnMut)
        .unwrap();
    assert!(definition.captures.iter().any(is_owned_store));
    let closure = definition.id;
    definition.kind = CallableKind::Fn;
    let mut contracts = 0;
    for function in &mut shared.functions {
        if let Some(contract) = &mut function.callable_return_contract
            && contract.closure == closure
        {
            contract.kind = CallableKind::Fn;
            contracts += 1;
        }
    }
    assert_eq!(
        contracts, 1,
        "synchronize the untrusted factory claim so the real write must be checked"
    );
    attack(
        "owned-store-write-forged-as-shared",
        shared,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
    );

    let (module, plan) = baseline("once-transfer-source-control", &format!("{PREFIX}{ONCE}"));
    let definition = module
        .closures
        .iter()
        .find(|c| c.kind == CallableKind::FnOnce && c.captures.iter().any(is_owned_store))
        .unwrap();
    let capture = definition.captures.iter().position(is_owned_store).unwrap() as u32;
    let function = definition.function;
    let mut read = module.clone();
    let mut changed = 0;
    for block in &mut read.functions[function.0 as usize].blocks {
        for instruction in &mut block.instructions {
            if matches!(instruction.kind,InstructionKind::TakeCapture{index} if index==capture) {
                instruction.kind = InstructionKind::ReadCapture { index: capture };
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    attack(
        "owned-store-take-replaced-with-read",
        read,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-CAPTURE",
    );

    let mut repeated = module.clone();
    let f = &mut repeated.functions[function.0 as usize];
    let (block, index) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(
                    |i| matches!(i.kind,InstructionKind::TakeCapture{index} if index==capture),
                )
                .map(|i| (b, i))
        })
        .unwrap();
    let mut second = f.blocks[block].instructions[index].clone();
    let ty = f.values[second.result.unwrap().0 as usize].clone();
    second.result = Some(ValueId(f.values.len() as u32));
    f.values.push(ty);
    f.blocks[block].instructions.insert(index + 1, second);
    attack(
        "actual-store-capture-taken-twice",
        repeated,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
    );
}

fn is_owned_store(ty: &Type) -> bool {
    matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::OwnedStore{..}))
}

/// First build and verify an actual source module, then remove only unused
/// producer/body operations using an existing source-produced scalar body.
/// The resulting positive raw control retains the real two capture types and
/// their canonical definitions, but contains no Store operation or MakeClosure.
/// This is a source-derived metadata control, not an unchanged source execution.
fn unused_capture_control() -> (TypedModule, CandidatePlan, ClosureId) {
    let source = format!(
        r#"{PREFIX}
resource Empty{{}}
fn seven()->i32{{return 7}}
fn make(give first:Store<Item>,give second:Store<Item>)->impl Fn()->i32{{
    return move()=>{{let a=first;let b=second;return 7}}
}}
fn main()->i32{{return 0}}
"#
    );
    let (mut module, _) = baseline("unused-metadata-original-source", &source);
    let definition = module
        .closures
        .iter()
        .find(|c| c.captures.iter().filter(|t| is_owned_store(t)).count() == 2)
        .unwrap()
        .clone();
    let scalar = module
        .functions
        .iter()
        .find(|f| f.name == "seven")
        .unwrap()
        .clone();
    for f in &mut module.functions {
        if f.name == "make" || f.name == "empty" || f.id == definition.function {
            let (id, name, environment) = (f.id, f.name.clone(), f.environment);
            *f = scalar.clone();
            f.id = id;
            f.name = name;
            f.environment = environment;
        }
    }
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .all(|i| !matches!(
                i.kind,
                InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::OwnedStoreReserve { .. }
                    | InstructionKind::OwnedStorePush { .. }
                    | InstructionKind::OwnedStorePop { .. }
                    | InstructionKind::OwnedStoreBorrow { .. }
                    | InstructionKind::OwnedStoreBorrowMut { .. }
                    | InstructionKind::OwnedStoreLen { .. }
                    | InstructionKind::OwnedStoreCapacity { .. }
                    | InstructionKind::MakeClosure { .. }
            ))
    );
    let case = Case::new("unused-metadata-derived-positive");
    case.write("source.zil", source);
    case.write("typed.txt", format!("{module:#?}"));
    let plan = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    verify_pair(&case, &module, &plan, None);
    (module, plan, definition.id)
}

#[test]
fn unused_capture_table_without_store_operations_checks_the_second_actual_leaf() {
    let (module, plan, root) = unused_capture_control();
    let slots: Vec<_> = module.closures[root.0 as usize]
        .captures
        .iter()
        .enumerate()
        .filter_map(|(i, t)| is_owned_store(t).then_some(i))
        .collect();
    assert_eq!(slots.len(), 2);
    let original_payload = match &module.closures[root.0 as usize].captures[slots[1]] {
        Type::Managed(p) => p.as_ref().clone(),
        _ => unreachable!(),
    };
    let resource = module
        .resources
        .iter()
        .position(|r| r.payload == original_payload)
        .unwrap();
    let mut absent = module.clone();
    absent.resources.remove(resource);
    attack(
        "capture-only-no-canonical",
        absent,
        &plan,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-OWNED-STORE",
    );

    let Type::OwnedStore {
        element: original_element,
    } = original_payload
    else {
        unreachable!()
    };
    let Type::Package { name, fields } = original_element.as_ref() else {
        unreachable!()
    };
    let header = Type::Record {
        name: "CallbackHeader".into(),
        fields: vec![(
            "function".into(),
            Type::FunctionPointer {
                parameters: vec![Type::I32],
                result: Box::new(Type::I32),
            },
        )],
    };
    let header_definition = ResourceDefinition {
        name: "CallbackHeader".into(),
        payload: header.clone(),
        teardown: None,
    };
    let mut header_fields = fields.clone();
    header_fields.push(("header".into(), Type::Managed(Box::new(header.clone()))));
    let mut ordinary = module.clone();
    ordinary.resources.push(header_definition.clone());
    ordinary.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<OrdinaryHeader>".into(),
        payload: Type::OwnedStore {
            element: Box::new(Type::Package {
                name: "OrdinaryHeader".into(),
                fields: header_fields.clone(),
            }),
        },
        teardown: None,
    });
    let case = Case::new("ordinary-store-retains-managed-copy-function-header");
    case.write("typed.txt", format!("{ordinary:#?}"));
    let ordinary_plan = zil_analyze::analyze(&ordinary, &Limits::default());
    case.write("analyze.txt", format!("{ordinary_plan:#?}"));
    verify_pair(&case, &ordinary, &ordinary_plan.unwrap(), None);
    for (label, fields) in [
        (
            "zero-leaf",
            vec![(
                "owner".to_owned(),
                Type::Managed(Box::new(Type::Record {
                    name: "Empty".into(),
                    fields: vec![],
                })),
            )],
        ),
        ("inner-callback-cycle", {
            let mut fields = fields.clone();
            fields.push(("hidden".into(), Type::Closure(root)));
            fields
        }),
        ("inner-store", {
            let mut fields = fields.clone();
            fields.push((
                "hidden".into(),
                module.closures[root.0 as usize].captures[slots[0]].clone(),
            ));
            fields
        }),
        ("hidden-address", {
            let mut fields = fields.clone();
            fields.push((
                "hidden".into(),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            ));
            fields
        }),
        ("hidden-function-pointer", {
            let mut fields = fields.clone();
            fields.push((
                "hidden".into(),
                Type::Tuple(vec![Type::Array {
                    element: Box::new(Type::FunctionPointer {
                        parameters: vec![Type::I32],
                        result: Box::new(Type::I32),
                    }),
                    length: 1,
                }]),
            ));
            fields
        }),
        ("hidden-foreign-callback", {
            let mut fields = fields.clone();
            fields.push((
                "hidden".into(),
                Type::Record {
                    name: "CallbackRecord".into(),
                    fields: vec![(
                        "callback".into(),
                        Type::ForeignCallback {
                            parameters: vec![Type::I32],
                            result: Box::new(Type::I32),
                        },
                    )],
                },
            ));
            fields
        }),
        ("managed-header-callback", header_fields),
    ] {
        let mut bad = module.clone();
        let name = format!("{name}-{label}");
        let element = Type::Package {
            name: name.clone(),
            fields,
        };
        let payload = Type::OwnedStore {
            element: Box::new(element),
        };
        bad.resources.push(ResourceDefinition {
            name: format!("@builtin:OwnedStore<{name}>"),
            payload: payload.clone(),
            teardown: None,
        });
        if label == "managed-header-callback" {
            bad.resources.push(header_definition.clone());
        }
        if label == "zero-leaf"
            && !bad
                .resources
                .iter()
                .any(|r| matches!(&r.payload,Type::Record{name,..} if name=="Empty"))
        {
            bad.resources.push(ResourceDefinition {
                name: "Empty".into(),
                payload: Type::Record {
                    name: "Empty".into(),
                    fields: vec![],
                },
                teardown: None,
            });
        }
        bad.closures[root.0 as usize].captures[slots[1]] = Type::Managed(Box::new(payload));
        attack(
            label,
            bad,
            &plan,
            false,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        );
    }
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    run(high).unwrap();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match run(n) {
            Ok(()) => high = n,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                low = n;
            }
        }
    }
    run(high).unwrap();
    let errors = run(high - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit),
        "{errors:#?}"
    );
    run(high).unwrap();
    high
}

fn phase_budgets(label: &str, source: &str) -> [usize; 4] {
    let (module, plan) = baseline(label, source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review204.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let values = [
        boundary(|n| {
            zil_expand::expand_with_map_limits(
                &unit,
                &syntax,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .map(|_| ())
        }),
        boundary(|n| {
            zil_types::check(
                &core,
                &module.target,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .map(|typed| assert_eq!(typed, module))
        }),
        boundary(|n| {
            zil_analyze::analyze(
                &module,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .map(|candidate| assert_eq!(candidate, plan))
        }),
        boundary(|n| {
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .map(|_| ())
        }),
    ];
    let case = Case::new(&format!("{label}-boundaries"));
    case.write("source.zil", source);
    case.write("boundaries.json",format!("{{\"expand\":{},\"types\":{},\"analyze\":{},\"verify\":{},\"method\":\"each exact N, N-1 ResourceLimit, fresh N; fresh Types/Analyze output equals control\"}}\n",values[0],values[1],values[2],values[3]));
    values
}

#[test]
fn legacy_byte_capture_has_exact_fresh_budget_control() {
    let source = "fn make()->impl Fn()->i32{let b=bytes_new();return move()=>i32(bytes_len(b))}fn main()->i32{let callback=make();return callback()}";
    // Measured at a94bd81d before any 204 production merge. That complete
    // production domain equals ee0; retain its old source and fees exactly.
    let actual = phase_budgets("legacy-byte-capture", source);
    assert_eq!(actual, [2, 196, 2230, 2490]);
}

#[test]
fn new_owned_store_capture_has_exact_recoverable_phase_budgets() {
    phase_budgets("new-two-store-capture", &format!("{PREFIX}{TWO_STORES}"));
}

#[test]
fn unused_capture_qualification_has_exact_fresh_raw_and_wire_work_limits() {
    let (module, plan, _) = unused_capture_control();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|actual| assert_eq!(actual, plan))
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let wire = boundary(|n| {
        zil_verify::verify_module_bytes(
            &bytes,
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let case = Case::new("unused-capture-definition-boundaries");
    case.write("typed.txt", format!("{module:#?}"));
    case.write("candidate.wire", bytes);
    case.write("boundaries.json",format!("{{\"analyze\":{analyze},\"verify_raw\":{verify},\"verify_wire\":{wire},\"store_operations\":0,\"make_closure_operations\":0,\"method\":\"N, N-1 ResourceLimit, fresh N; source-derived positive metadata control\"}}\n"));
}
