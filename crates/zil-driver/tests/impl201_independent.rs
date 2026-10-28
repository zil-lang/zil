//! Independent 201 review. All controls start at source; raw attacks retain
//! their actual inputs and distinguish Analyze rejection from fresh Verify.
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
use zil_ir::{
    CandidatePlan, Constant, Instruction, InstructionKind as I, Type, TypedModule, ValueId,
};

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
            "independent201-{label}-{}-{}",
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
fn compile(case: &Case, source: &str, native: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent201.zil", source),
        &CompileOptions {
            stop_after: if native.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: native.unwrap_or(Optimization::O0),
            output: native.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn native(label: &str, source: &str, expected: &str, expected_exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={expected_exit}\nexpected_stdout={expected:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(expected_exit));
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}
fn rejected(label: &str, source: &str, category: ResultCategory, phases: &[Phase], clue: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert!(phases.contains(&report.phase), "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == clue || d.message.contains(clue)),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent201.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let module = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("typed.txt", format!("{module:#?}"));
    let module = module.unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{candidate:#?}"));
    let candidate = candidate.unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    case.write("baseline.wire", &wire);
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={decoded:#?}"));
    raw.unwrap();
    decoded.unwrap();
    (module, candidate)
}
fn raw_reject(
    label: &str,
    module: TypedModule,
    original: CandidatePlan,
    fresh: bool,
    category: ResultCategory,
    clue: &str,
) {
    let case = Case::new(label);
    case.write("typed.txt", format!("{module:#?}"));
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{analyzed:#?}"));
    let mut candidate = if fresh {
        analyzed.expect("this attack must reach independent Verify with a fresh candidate")
    } else {
        analyzed.unwrap_or(original)
    };
    candidate.module_fingerprint = module.fingerprint();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    case.write("attacked.wire", &bytes);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    );
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default());
    case.write(
        "verify.txt",
        format!("raw={raw:#?}\nwire={wire:#?}\nfresh_required={fresh}\n"),
    );
    for route in [raw, wire] {
        let ds = route.unwrap_err();
        assert!(
            ds.iter().any(|d| d.phase == Phase::Verify
                && d.category == category
                && (d.code == clue || d.message.contains(clue))),
            "{label}: {ds:#?}"
        );
    }
}

const PREFIX: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn emit(n:i32){unsafe{putchar(n)}}
"#;
const MULTIPLE: &str = r#"
package Pair<F>{first:F;manages root:Ticket;second:F}
package Item<F>{callback:F;pad:(u8,i64,u16);manages owner:Ticket;value:i32}
fn pack<F:FnMut(i32)->i32>(first:F,second:F)->Pair<F>{
    return Pair::<F>{first:first,root:Ticket{code:90},second:second}
}
fn make(label:i32,seed:i32)->impl FnMut(i32)->i32{
    let ticket=Ticket{code:label};var count=seed;
    return move(delta:i32)=>{ticket.code;count+=delta;return count}
}
fn run<F:FnMut(i32)->i32>(pair:Pair<F>)->void!i32{
    let items:Store<Item<F>>=store_new();
    if !store_reserve(items,2){return ()}
    let first=Item::<F>{callback:pair.first,pad:(1u8,i64(2),3u16),owner:Ticket{code:80},value:7};
    store_push(items,first);
    let second=Item::<F>{callback:pair.second,pad:(4u8,i64(5),6u16),owner:Ticket{code:81},value:9};
    store_push(items,second);
    let a=store_at_mut(items,0);let x=a.callback(1);let y=a.callback(2);
    let b=store_at_mut(items,1);let z=b.callback(3);
    if !store_reserve(items,4){return ()}
    var tail=store_pop(items)?;let t=tail.callback(4);
    let c=store_at_mut(items,0);let u=c.callback(5);
    return x+y+z+t+u-252
}
fn main()->void!i32{return run(pack(make(65,10),make(66,100)))}
"#;

#[test]
fn original_store_probe_executes_without_source_changes() {
    native(
        "original-02",
        include_str!("impl200_fixtures/02_store_of_callback_state.zil"),
        "",
        0,
    );
}

#[test]
fn same_closure_type_has_two_actual_elements_and_exact_pop_prefix_cleanup() {
    let source = format!("{PREFIX}{MULTIPLE}");
    native("two-elements-aligned-captures", &source, "BQAPZ", 0);
    let start = "let a=store_at_mut(items,0);let x=a.callback(1);let y=a.callback(2);";
    let end = "return x+y+z+t+u-252";
    let left = source.find(start).unwrap();
    let right = source.find(end).unwrap() + end.len();
    let unused = format!("{}return 0{}", &source[..left], &source[right..]);
    native("two-elements-zero-calls", &unused, "APBQZ", 0);
    // Capacity one returns the uninserted second complete value to immediate
    // error cleanup. It does not create a second prefix element.
    let full = unused.replace("store_reserve(items,2)", "store_reserve(items,1)");
    native("full-push-owns-error-payload", &full, "BQAPZ", 0);
}

const WINDOW: &str = r#"
package State<F>{manages root:Ticket;callback:F;value:i32}
fn pack<F:FnMut(i32)->i32>(callback:F)->State<F>{return State::<F>{root:Ticket{code:84},callback:callback,value:7}}
fn make()->impl FnMut(i32)->i32{var count=0;return move(delta:i32)=>{count+=delta;return count}}
fn run<F:FnMut(i32)->i32>(state:State<F>)->i32{
    let items:Store<State<F>>=store_new();store_reserve(items,1);store_push(items,state);
    let delta=if store_reserve(items,2){1}else{2};
    let view=store_at_mut(items,0);return view.callback(delta)-1
}
fn main()->i32{return run(pack(make()))}
"#;

#[test]
fn actual_callee_window_precedes_argument_preparation_and_keeps_parent_authority() {
    let source = format!("{PREFIX}{WINDOW}");
    native("prepare-argument-before-window", &source, "T", 0);
    let bad = source
        .replace("let delta=if store_reserve(items,2){1}else{2};", "")
        .replace(
            "view.callback(delta)",
            "view.callback(if store_reserve(items,2){1}else{2})",
        );
    rejected(
        "callee-before-argument-resize",
        &bad,
        ResultCategory::SafeReject,
        &[Phase::Verify],
        "E-BORROW-CONFLICT",
    );
    let child = source.replace(
        "return view.callback(delta)-1",
        "let child=&mut view.callback;view.value+=1;return child(delta)-1",
    );
    rejected(
        "actual-child-blocks-parent-write",
        &child,
        ResultCategory::SafeReject,
        &[Phase::Verify],
        "E-BORROW-CONFLICT",
    );
    let twice = source.replace(
        "return view.callback(delta)-1",
        "let second=store_at_mut(items,0);return view.callback(delta)+second.callback(delta)-3",
    );
    rejected(
        "same-element-two-exclusive-windows",
        &twice,
        ResultCategory::SafeReject,
        &[Phase::Types],
        "E-STATE-ALIAS",
    );
}

const LENDING: &str = r#"
package State<F>{callback:F;manages bytes:ByteBuffer;value:i32}
fn pack<F:FnMut()->&[u8]>(callback:F)->State<F>{return State::<F>{callback:callback,bytes:bytes_new(),value:7}}
fn make(seed:u8)->impl FnMut()->&[u8]{
    let log=bytes_new();return move()=>{bytes_reserve(log,bytes_len(log)+1);bytes_push(log,seed);return bytes_view(log)}
}
fn run<F:FnMut()->&[u8]>(state:State<F>)->i32{
    let items:Store<State<F>>=store_new();store_reserve(items,1);store_push(items,state);
    let view=store_at_mut(items,0);let bytes=view.callback();let value=i32(bytes[0]);
    store_reserve(items,2);return value-7
}
fn main()->i32{return run(pack(make(7u8)))}
"#;

#[test]
fn lending_result_keeps_actual_element_and_parent_until_last_use() {
    native("lending-last-use-before-reserve", LENDING, "", 0);
    for (label, invalidation) in [
        ("reserve", "store_reserve(items,2);"),
        ("reserve-with-enough-capacity", "store_reserve(items,1);"),
        ("reserve-that-would-fail", "store_reserve(items,-1);"),
        ("pop", "store_pop(items);"),
        ("recall", "view.callback();"),
        ("parent-copy-write", "view.value+=1;"),
    ] {
        let bad = LENDING.replace(
            "let value=i32(bytes[0]);",
            &format!("{invalidation}let value=i32(bytes[0]);"),
        );
        rejected(
            &format!("live-lending-{label}"),
            &bad,
            ResultCategory::SafeReject,
            &[Phase::Verify],
            "E-BORROW-CONFLICT",
        );
    }
}

const ONCE: &str = r#"
package Pair<F>{first:F;manages root:Ticket;second:F}
fn pack<F:FnOnce()->i32>(first:F,second:F)->Pair<F>{return Pair::<F>{first:first,root:Ticket{code:90},second:second}}
fn make(label:i32)->impl FnOnce()->i32{let ticket=Ticket{code:label};return move()=>{let taken=give ticket;return taken.code}}
fn run<F:FnOnce()->i32>(pair:Pair<F>)->void!i32{
    let items:Store<Pair<F>>=store_new();store_reserve(items,1);store_push(items,pair);
    var owned=store_pop(items)?;let a=owned.first();emit(88);let b=owned.second();emit(89);return a+b-131
}
fn main()->void!i32{return run(pack(make(65),make(66)))}
"#;

#[test]
fn two_fn_once_fields_are_consumed_only_after_pop_owns_the_whole_element() {
    let source = format!("{PREFIX}{ONCE}");
    native("pop-two-fnonce-fields", &source, "AXBYZ", 0);
    let strengthened = source.replace("impl FnOnce()->i32{let ticket=Ticket{code:label};return move()=>{let taken=give ticket;return taken.code}}", "impl Fn()->i32{let ticket=Ticket{code:label};return move()=>ticket.code}");
    assert_ne!(strengthened, source);
    native(
        "pop-fn-adapted-to-declared-fnonce",
        &strengthened,
        "AXBYZ",
        0,
    );
    let borrowed = source.replace(
        "var owned=store_pop(items)?",
        "let owned=store_at_mut(items,0)",
    );
    rejected(
        "fnonce-borrowed-element",
        &borrowed,
        ResultCategory::SafeReject,
        &[Phase::Expand],
        "FnOnce requires an owned Store element obtained by pop",
    );
}

const ZERO: &str = r#"
resource Empty{}
package State<F>{manages empty:Empty;callback:F}
fn pack<F:Fn()->i32>(callback:F)->State<F>{return State::<F>{empty:Empty{},callback:callback}}
fn make()->impl Fn()->i32{return ()=>0}
fn run<F:Fn()->i32>(state:State<F>)->i32{let items:Store<State<F>>=store_new();store_reserve(items,1);store_push(items,state);return 0}
fn main()->i32{return run(pack(make()))}
"#;

#[test]
fn zero_sized_actual_environment_is_not_a_nonzero_store_element() {
    rejected(
        "zero-managed-and-empty-environment",
        ZERO,
        ResultCategory::Unsupported,
        &[Phase::Verify],
        "nonzero actual inline layout",
    );
    let sized = ZERO.replace(
        "fn make()->impl Fn()->i32{return ()=>0}",
        "fn make()->impl Fn()->i32{let n=1;return move()=>n}",
    );
    native("nonzero-real-capture-layout-control", &sized, "", 0);
}

#[test]
fn captured_zero_sized_package_keeps_its_existing_flagged_environment_layout() {
    let source = r#"
extern "C" fn putchar(n:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(65)}}}
package Token{manages empty:Empty}
package State<F>{manages bytes:ByteBuffer;callback:F}
fn make()->impl Fn()->i32{
    let held=Token{empty:Empty{}};return move()=>{held.empty;return 7}
}
fn pack<F:Fn()->i32>(callback:F)->State<F>{return State::<F>{bytes:bytes_new(),callback:callback}}
fn run<F:Fn()->i32>(state:State<F>)->i32{return state.callback()-7}
fn main()->i32{return run(pack(make()))}
"#;
    native(
        "zero-package-capture-existing-state-control",
        source,
        "A",
        0,
    );
    let stored = source.replace("return state.callback()-7", "let items:Store<State<F>>=store_new();store_reserve(items,1);store_push(items,state);let view=store_at(items,0);return view.callback()-7");
    native("zero-package-capture-in-store", &stored, "A", 0);
}

#[test]
fn raw_live_lending_parent_reserve_is_rejected_with_a_fresh_candidate() {
    let (mut module, original) = baseline("raw-lending-control", LENDING);
    let f = module
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, I::OwnedStoreBorrowMut { .. }))
        })
        .unwrap();
    let store = module.functions[f]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::OwnedStoreBorrowMut { store, .. } => Some(store),
            _ => None,
        })
        .unwrap();
    let (block, call) = module.functions[f]
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(i, instruction)| {
                    matches!(instruction.kind, I::CallClosureBorrowed { .. }).then_some((b, i))
                })
        })
        .unwrap();
    let span = module.functions[f].blocks[block].instructions[call].span;
    let capacity = ValueId(module.functions[f].values.len() as u32);
    module.functions[f].values.push(Type::I64);
    let result = ValueId(module.functions[f].values.len() as u32);
    module.functions[f].values.push(Type::Bool);
    module.functions[f].blocks[block].instructions.splice(
        call + 1..call + 1,
        [
            Instruction {
                result: Some(capacity),
                kind: I::Constant(Constant::I64(2)),
                span,
            },
            Instruction {
                result: Some(result),
                kind: I::OwnedStoreReserve { store, capacity },
                span,
            },
        ],
    );
    raw_reject(
        "live-lending-parent-reserve",
        module,
        original,
        true,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn raw_same_id_element_values_cannot_duplicate_a_pushed_responsibility() {
    let source = format!("{PREFIX}{MULTIPLE}");
    let (mut module, original) = baseline("raw-two-elements-control", &source);
    let f = module
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter(|i| matches!(i.kind, I::OwnedStorePush { .. }))
                .count()
                == 2
        })
        .unwrap();
    let first = module.functions[f]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::OwnedStorePush { value, .. } => Some(value),
            _ => None,
        })
        .unwrap();
    let second = module.functions[f]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .filter(|i| matches!(i.kind, I::OwnedStorePush { .. }))
        .nth(1)
        .unwrap();
    let I::OwnedStorePush { value, .. } = &mut second.kind else {
        unreachable!()
    };
    assert_ne!(*value, first);
    *value = first;
    raw_reject(
        "second-push-duplicates-first-value",
        module,
        original,
        false,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
    );
}

#[test]
fn raw_store_capture_cycle_is_closed_before_environment_layout() {
    let source = include_str!("impl200_fixtures/02_store_of_callback_state.zil");
    let (module, original) = baseline("actual-store-capture-control", source);
    let store = module
        .resources
        .iter()
        .find_map(|r| matches!(r.payload, Type::OwnedStore { .. }).then_some(r.payload.clone()))
        .unwrap();
    let closure = module
        .closures
        .iter()
        .position(|c| {
            c.captures
                .iter()
                .any(|t| matches!(t, Type::Managed(inner) if **inner == Type::ByteBuffer))
        })
        .unwrap();
    let capture = module.closures[closure]
        .captures
        .iter()
        .position(|t| matches!(t, Type::Managed(inner) if **inner == Type::ByteBuffer))
        .unwrap();
    for (label, field) in [
        (
            "direct-store-capture-cycle",
            Type::Managed(Box::new(store.clone())),
        ),
        (
            "package-store-capture-cycle",
            Type::Package {
                name: "HiddenStore".into(),
                fields: vec![("items".into(), Type::Managed(Box::new(store)))],
            },
        ),
    ] {
        let mut attacked = module.clone();
        // The real element already contains this exact ClosureId. Following
        // this forged capture through the real Store therefore closes a cycle;
        // no producer self-contained bit or fabricated display name proves it.
        attacked.closures[closure].captures[capture] = field;
        raw_reject(
            label,
            attacked,
            original.clone(),
            false,
            ResultCategory::Unsupported,
            "finite owning environment profile",
        );
    }
}

#[test]
fn raw_unused_store_definition_checks_the_real_callback_shape() {
    let source = include_str!("impl200_fixtures/02_store_of_callback_state.zil");
    let (mut module, _) = baseline("unused-store-source-control", source);
    let mut payload = module
        .resources
        .iter()
        .find_map(|r| matches!(r.payload, Type::OwnedStore { .. }).then_some(r.payload.clone()))
        .unwrap();
    let Type::OwnedStore { element } = &mut payload else {
        unreachable!()
    };
    let Type::Package { name, fields } = element.as_mut() else {
        unreachable!()
    };
    *name = format!("{name}:review-unused");
    let callback = fields
        .iter()
        .find_map(|(_, ty)| matches!(ty, Type::Closure(_)).then_some(ty.clone()))
        .unwrap();
    let name = format!("@builtin:OwnedStore<{name}>");
    module.resources.push(zil_ir::ResourceDefinition {
        name,
        payload,
        teardown: None,
    });
    // The extra definition is never used by any function or instruction.
    // Its valid, source-derived concrete callback metadata must first pass
    // both independent routes, before only its field count is corrupted.
    let case = Case::new("unused-store-valid-definition");
    case.write("source.zil", source);
    case.write("typed.txt", format!("{module:#?}"));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    case.write("baseline.wire", &bytes);
    let raw = zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    );
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={wire:#?}"));
    raw.unwrap();
    wire.unwrap();
    let Type::OwnedStore { element } = &mut module.resources.last_mut().unwrap().payload else {
        unreachable!()
    };
    let Type::Package { fields, .. } = element.as_mut() else {
        unreachable!()
    };
    fields.push(("extra_callback_1".into(), callback.clone()));
    fields.push(("extra_callback_2".into(), callback));
    raw_reject(
        "unused-definition-has-three-callbacks",
        module,
        plan,
        false,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

#[test]
fn raw_redirect_to_same_typed_parent_cannot_borrow_another_stores_authority() {
    let start = MULTIPLE.find("fn run<").unwrap();
    let end = MULTIPLE.find("fn main()").unwrap();
    let source = format!(
        r#"{PREFIX}{}
fn run<F:FnMut(i32)->i32>(pair:Pair<F>)->void!i32{{
    let left:Store<Item<F>>=store_new();let right:Store<Item<F>>=store_new();
    store_reserve(left,1);store_reserve(right,1);
    let first=Item::<F>{{callback:pair.first,pad:(1u8,i64(2),3u16),owner:Ticket{{code:80}},value:7}};
    store_push(left,first);
    let second=Item::<F>{{callback:pair.second,pad:(4u8,i64(5),6u16),owner:Ticket{{code:81}},value:9}};
    store_push(right,second);
    let view=store_at_mut(left,0);store_reserve(right,2);return view.callback(1)-11
}}
{}"#,
        &MULTIPLE[..start],
        &MULTIPLE[end..]
    );
    native("same-type-two-stores-control", &source, "BQAPZ", 0);
    let (mut module, original) = baseline("same-type-two-stores-raw-control", &source);
    let f = module
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter(|i| matches!(i.kind, I::OwnedStoreNew { .. }))
                .count()
                == 2
        })
        .unwrap();
    let left = module.functions[f]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            I::OwnedStoreBorrowMut { store, .. } => Some(store),
            _ => None,
        })
        .unwrap();
    let position = module.functions[f]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(i, instruction)| {
                    matches!(instruction.kind, I::OwnedStoreReserve { .. }).then_some((b, i))
                })
        })
        .last()
        .unwrap();
    let right = match module.functions[f].blocks[position.0].instructions[position.1].kind {
        I::OwnedStoreReserve { store, .. } => store,
        _ => unreachable!(),
    };
    assert_ne!(
        left, right,
        "control must use two distinct actual descriptor values"
    );
    assert_eq!(
        module.functions[f].values[left.0 as usize],
        module.functions[f].values[right.0 as usize]
    );
    let I::OwnedStoreReserve { store, .. } =
        &mut module.functions[f].blocks[position.0].instructions[position.1].kind
    else {
        unreachable!()
    };
    *store = left;
    raw_reject(
        "same-type-right-reserve-retargeted-to-live-left",
        module,
        original,
        true,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
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
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    run(high).unwrap();
    high
}

#[test]
fn callback_store_has_bounded_fresh_phase_work_without_raising_defaults() {
    let (module, candidate) = baseline("budget-store-control", LENDING);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent201.zil", LENDING);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let expand = boundary(|n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    Case::new("exact-budgets").write("boundaries.json", format!("{{\"expand\":{expand},\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
