//! Retained concrete identities, inline sub-environments and transactional slot replacement.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CallableKind, InstructionKind, Type};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(name: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "retained168-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep.is_some())
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), data).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

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
fn compile(
    input: &ProjectSources,
    case: &Case,
    optimization: Option<Optimization>,
) -> CompileReport {
    let native_main = input
        .modules
        .iter()
        .find(|unit| unit.module_id == input.root)
        .unwrap()
        .text
        .contains("fn main(");
    for unit in &input.modules {
        case.write(&format!("module{}.zil", unit.module_id.0), &unit.text);
    }
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            entry: if native_main {
                None
            } else {
                Some("value".into())
            },
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn native(input: &ProjectSources, label: &str, expected: &str, code: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(&format!("{label}-{opt:?}"));
        let report = compile(input, &case, Some(opt));
        success(&report);
        let path = &report.artifacts["executable"];
        case.write(
            "native-command.json",
            format!("[{}]\n", json_string(&path.to_string_lossy())),
        );
        let output = Command::new(path).output().unwrap();
        case.write("native.stdout", &output.stdout);
        case.write("native.stderr", &output.stderr);
        case.write("native.exit", format!("{:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(code), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

const DROP: &str = r#"extern "C" fn putchar(x:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}"#;
const SHARED: &str = "fn make(n:i32)->impl Fn()->i32{let r=R{code:n};return move ()=>r.code}";
const MUTABLE: &str =
    "fn make(n:i32)->impl FnMut()->i32{let r=R{code:n};return move ()=>{r.code+=1;return r.code}}";
const ONCE: &str = "fn eat(give r:R)->i32{return r.code}fn make(n:i32)->impl FnOnce()->i32{let r=R{code:n};return move ()=>eat(give r)}";
fn run(source: &str, label: &str, stdout: &str) {
    native(&project(&[("main", source)]), label, stdout, 0);
}
fn rejected(source: &str, code: &str) {
    let case = Case::new("reject");
    let report = compile(&project(&[("main", source)]), &case, None);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "unexpected acceptance: {source}"
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
}
#[test]
fn forward_factories_resolve_reverse_declarations_and_cleanup_once() {
    run(
        &format!(
            "{DROP} fn outer(n:i32)->impl Fn()->i32{{return middle(n)}}fn middle(n:i32)->impl Fn()->i32{{return make(n)}}{SHARED}fn main()->i32{{let f=outer(65);return f()+f()-130}}"
        ),
        "forward",
        "A",
    );
}
#[test]
fn cross_module_forwarding_preserves_public_opaque_result() {
    let main = "import a;fn main()->i32{let f=a.outer(21);return f()+f()-42}";
    let a = "import b;pub fn outer(n:i32)->impl Fn()->i32{return b.make(n)}";
    let b = "pub fn make(n:i32)->impl Fn()->i32{return move ()=>n}";
    native(
        &project(&[("main", main), ("a", a), ("b", b)]),
        "module-forward",
        "",
        0,
    );
}
#[test]
fn same_factory_slot_replacement_drops_previous_after_new_is_ready() {
    run(
        &format!(
            "{DROP}{SHARED}fn main()->i32{{var f=make(65);let a=f();f=make(66);return a+f()-131}}"
        ),
        "replace",
        "AB",
    );
}
#[test]
fn failed_replacement_keeps_original_slot_responsibility() {
    let source = format!(
        "{DROP}enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}{SHARED}fn main()->E!i32{{var f=make(65);f=make(fail()?);return f()}}"
    );
    native(&project(&[("main", &source)]), "replace-error", "A", 1);
}
#[test]
fn owned_shared_subenvironment_is_borrowed_and_dropped_recursively() {
    run(
        &format!(
            "{DROP}{SHARED}fn wrap(n:i32)->impl Fn()->i32{{let f=make(n);return move ()=>f()}}fn outer(n:i32)->impl Fn()->i32{{let f=wrap(n);return move ()=>f()}}fn main()->i32{{let f=outer(65);return f()+f()-130}}"
        ),
        "shared-deep",
        "A",
    );
}
#[test]
fn owned_mutable_subenvironment_updates_real_inline_storage() {
    run(
        &format!(
            "{DROP}{MUTABLE}fn wrap(n:i32)->impl FnMut()->i32{{var f=make(n);return move ()=>f()}}fn main()->i32{{var f=wrap(63);return f()+f()-129}}"
        ),
        "mutable",
        "A",
    );
}
#[test]
fn consuming_subenvironment_is_taken_and_called_exactly_once() {
    run(
        &format!(
            "{DROP}{ONCE}fn wrap(n:i32)->impl FnOnce()->i32{{let f=make(n);return move ()=>f()}}fn main()->i32{{let f=wrap(65);return f()-65}}"
        ),
        "once",
        "A",
    );
}
#[test]
fn uncalled_subenvironment_and_siblings_follow_capture_cleanup_order() {
    run(
        &format!(
            "{DROP}{SHARED}fn wrap()->impl Fn()->i32{{let a=make(65);let b=make(66);return move ()=>a()+b()}}fn main()->i32{{let f=wrap();return 0}}"
        ),
        "uncalled",
        "AB",
    );
}
#[test]
fn nested_error_exit_cleans_inner_activation_then_recursive_environment() {
    let source = format!(
        "{DROP}enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn make()->impl Fn()->E!i32{{let a=R{{code:65}};return move ()->E!i32=>{{let b=R{{code:66}};let n=a.code;return fail()?}}}}fn wrap()->impl Fn()->E!i32{{let f=make();return move ()->E!i32=>f()}}fn main()->E!i32{{let f=wrap();return f()?}}"
    );
    native(&project(&[("main", &source)]), "nested-error", "BA", 1);
}
#[test]
fn nonmove_inner_borrow_keeps_outer_owned_callback_live() {
    run(
        &format!("{DROP}{SHARED}fn main()->i32{{let f=make(65);let g=()=>f();return g()+f()-130}}"),
        "shared-borrow",
        "A",
    );
}
#[test]
fn nested_move_creation_transfers_ownership_before_any_call() {
    run(
        &format!(
            "{DROP}{SHARED}fn main()->i32{{let f=make(65);let g=()=>{{let h=move()=>f();return 0}};return g()}}"
        ),
        "deferred-move",
        "A",
    );
}
#[test]
fn distinct_source_opaque_factories_cannot_mix_even_when_forwarded() {
    rejected(
        "fn make()->impl Fn()->i32{return ()=>1}fn wrap()->impl Fn()->i32{return make()}fn main()->i32{var f=make();f=wrap();return f()}",
        "E-TYPE-MISMATCH",
    );
    rejected(
        "fn a()->impl Fn()->i32{return ()=>1}fn b()->impl Fn()->i32{return a()}fn choose(yes:bool)->impl Fn()->i32{if yes{return a()}return b()}fn main()->i32{return 0}",
        "E-TYPE-MISMATCH",
    );
}
#[test]
fn recursive_factory_identity_and_escaping_nested_borrow_are_rejected() {
    rejected(
        "fn a()->impl Fn()->i32{return b()}fn b()->impl Fn()->i32{return a()}fn main()->i32{return 0}",
        "E-UNSUPPORTED",
    );
    rejected(
        "fn make()->impl Fn()->i32{var n=1;let f=()=>n;return move ()=>f()}fn main()->i32{return 0}",
        "E-UNSUPPORTED",
    );
}
#[test]
fn retained_alias_and_consuming_callback_misuse_are_rejected() {
    rejected(
        &format!(
            "{DROP}{SHARED}fn main()->i32{{var f=make(65);let g=()=>f();f=make(66);return g()}}"
        ),
        "E-BORROW-CONFLICT",
    );
    let source = format!(
        "{DROP}{ONCE}fn wrap()->impl FnOnce()->i32{{let f=make(65);return move ()=>f()}}fn main()->i32{{let f=wrap();return f()+f()}}"
    );
    let case = Case::new("repeat-once");
    let report = compile(&project(&[("main", &source)]), &case, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

fn checked_module(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw168.zil", source);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}
fn raw_rejected(module: zil_ir::TypedModule, mut plan: zil_ir::CandidatePlan, expected: &str) {
    let limits = Limits::default();
    let target = TargetProfile::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result.expect_err("forged raw/wire module accepted");
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && (d.message.contains(expected) || d.code == expected)),
            "{errors:?}"
        );
    }
}
#[test]
fn independent_raw_and_wire_reject_cycles_wrong_capture_indices_and_permissions() {
    let text = format!(
        "{DROP}{MUTABLE}fn wrap()->impl FnMut()->i32{{var f=make(64);return move ()=>f()}}fn main()->i32{{var f=wrap();return f()-65}}"
    );
    let (module, plan) = checked_module(&text);
    let outer = module
        .closures
        .iter()
        .find(|c| c.captures.iter().any(|t| matches!(t, Type::Closure(_))))
        .unwrap();
    let id = outer.id;
    let mut cycle = module.clone();
    cycle.closures[id.0 as usize].captures[0] = Type::Closure(id);
    raw_rejected(cycle, plan.clone(), "E-VERIFY-CLOSURE-LAYOUT");
    let mut wrong = module.clone();
    let f = &mut wrong.functions[outer.function.0 as usize];
    let op = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::BorrowCapture { .. }))
        .unwrap();
    op.kind = InstructionKind::BorrowCapture {
        index: u32::MAX,
        mutable: true,
    };
    raw_rejected(wrong, plan.clone(), "E-VERIFY-CAPTURE");
    let mut shared = module.clone();
    shared.closures[id.0 as usize].kind = CallableKind::Fn;
    for f in &mut shared.functions {
        if let Some(contract) = &mut f.callable_return_contract
            && contract.closure == id
        {
            contract.kind = CallableKind::Fn;
        }
    }
    raw_rejected(shared, plan.clone(), "exclusive capture borrow");
    let mut copied = module.clone();
    let f = &mut copied.functions[outer.function.0 as usize];
    let op = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::BorrowCapture { .. }))
        .unwrap();
    f.values[op.result.unwrap().0 as usize] = Type::Closure(zil_ir::ClosureId(0));
    op.kind = InstructionKind::ReadCapture { index: 0 };
    raw_rejected(copied, plan, "never copying ReadCapture");
}
#[test]
fn independent_raw_and_wire_reject_repeated_nested_take_and_factory_cycles() {
    use zil_ir::{ValueHold, ValueId};
    let text =
        format!("{DROP}{ONCE}fn main()->i32{{let f=make(65);let g=move()=>f();return g()-65}}");
    let (module, mut plan) = checked_module(&text);
    let outer = module
        .closures
        .iter()
        .find(|c| c.captures.iter().any(|t| matches!(t, Type::Closure(_))))
        .unwrap();
    let mut duplicate = module.clone();
    let f = &mut duplicate.functions[outer.function.0 as usize];
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
        })
        .unwrap();
    let pos = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeCapture { .. }))
        .unwrap();
    let mut second = block.instructions[pos].clone();
    let result = ValueId(f.values.len() as u32);
    f.values
        .push(f.values[second.result.unwrap().0 as usize].clone());
    second.result = Some(result);
    block.instructions.insert(pos + 1, second);
    plan.functions
        .iter_mut()
        .find(|p| p.function == outer.function)
        .unwrap()
        .holds
        .push(ValueHold {
            value: result,
            roots: vec![],
        });
    raw_rejected(
        duplicate,
        plan,
        "captured responsibility was already transferred",
    );
    let (mut forward, proof) = checked_module(
        "fn wrap()->impl Fn()->i32{return make()}fn make()->impl Fn()->i32{return ()=>42}fn main()->i32{let f=wrap();return f()-42}",
    );
    let wrap = forward
        .functions
        .iter_mut()
        .find(|f| f.name == "wrap")
        .unwrap();
    let id = wrap.id;
    let op = wrap
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    op.kind = InstructionKind::Call {
        function: id,
        arguments: vec![],
    };
    raw_rejected(forward, proof, "recursive callable factory dependencies");
}

#[test]
fn independent_raw_and_wire_bound_repeated_child_layout_expansion() {
    use zil_ir::{ClosureId, FunctionId};
    let (mut module, mut plan) = checked_module(
        "fn make()->impl Fn()->i32{return ()=>42}fn main()->i32{let f=make();return f()-42}",
    );
    let entry = module.functions[module.closures[0].function.0 as usize].clone();
    let entry_plan = plan
        .functions
        .iter()
        .find(|p| p.function == entry.id)
        .unwrap()
        .clone();
    for index in 1..18 {
        let mut definition = module.closures[0].clone();
        definition.id = ClosureId(index);
        definition.function = FunctionId(module.functions.len() as u32);
        definition.captures = vec![Type::Closure(ClosureId(index - 1)); 2];
        let mut function = entry.clone();
        function.id = definition.function;
        function.name = format!("$raw{index}");
        function.environment = Some(definition.id);
        let mut function_plan = entry_plan.clone();
        function_plan.function = function.id;
        plan.functions.push(function_plan);
        module.functions.push(function);
        module.closures.push(definition);
    }
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits {
        max_analysis_steps: 10_000,
        ..Limits::default()
    };
    let target = TargetProfile::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for outcome in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&bytes, &target, &limits),
    ] {
        let errors = outcome.expect_err("exponential inline DAG accepted");
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit
                    && d.message.contains("closure layout expansion")),
            "{errors:?}"
        );
    }
}

#[test]
fn forwarding_keeps_exact_mutable_and_consuming_factory_kinds() {
    run(
        &format!(
            "{DROP}{MUTABLE}fn wrap(n:i32)->impl FnMut()->i32{{return make(n)}}fn main()->i32{{var f=wrap(63);return f()+f()-129}}"
        ),
        "forward-mut",
        "A",
    );
    run(
        &format!(
            "{DROP}{ONCE}fn wrap(n:i32)->impl FnOnce()->i32{{return make(n)}}fn main()->i32{{let f=wrap(65);return f()-65}}"
        ),
        "forward-once",
        "A",
    );
}
#[test]
fn consumed_callee_is_cleaned_when_later_argument_fails() {
    let text = format!(
        "{DROP}enum E{{Fail}}fn fail()->E!i32{{return E.Fail}}fn eat(give r:R,n:i32)->i32{{return r.code+n}}fn make()->impl FnOnce(i32)->i32{{let r=R{{code:65}};return move (n:i32)=>eat(give r,n)}}fn wrap()->impl FnOnce()->E!i32{{let f=make();return move ()->E!i32=>f(fail()?)}}fn main()->E!i32{{let f=wrap();return f()?}}"
    );
    native(&project(&[("main", &text)]), "callee-error", "A", 1);
}

#[test]
fn independent_ownership_checks_run_without_nominal_resources() {
    let (mut module, plan) = checked_module(
        "fn make(n:i32)->impl FnMut()->i32{var count=n;return move ()=>{count+=1;return count}}fn wrap(n:i32)->impl FnMut()->i32{var f=make(n);return move ()=>f()}fn main()->i32{var f=wrap(0);f=wrap(1);return f()-2}",
    );
    assert!(module.resources.is_empty());
    assert!(
        module
            .closures
            .iter()
            .all(|c| c.kind != CallableKind::FnOnce)
    );
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = main
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
        .unwrap();
    block.instructions.remove(position);
    raw_rejected(module, plan, "unpaid");
}
