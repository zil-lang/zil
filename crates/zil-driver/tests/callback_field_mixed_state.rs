//! Independent 199 review of concrete field positions, evaluation order,
//! declared modes and the source of each actual callback window.
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
use zil_ir::{CallableKind, CandidatePlan, InstructionKind, Type, TypedModule};

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
            "review199-by197-{label}-{}-{}",
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
        &SourceUnit::new(FileId(0), ModuleId(0), "review199.zil", source),
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

fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review199.zil", source);
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
    let module = typed.unwrap();
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
    case.write("verify.txt", format!("raw={raw:#?}\nwire={decoded:#?}\n"));
    raw.unwrap();
    decoded.unwrap();
    (module, candidate)
}

fn reject(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    category: ResultCategory,
    clue: &str,
    analyze_too: bool,
) {
    let case = Case::new(label);
    candidate.module_fingerprint = module.fingerprint();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    case.write("attacked.wire", &wire);
    case.write("attacked-typed.txt", format!("{module:#?}"));
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    case.write(
        "diagnostics.txt",
        format!("raw={raw:#?}\nwire={decoded:#?}\nanalyze={analyzed:#?}\n"),
    );
    for result in [raw, decoded] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == category
                && (d.code == clue || d.message.contains(clue))),
            "{label}: {errors:#?}"
        );
    }
    if analyze_too {
        let errors = analyzed.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == category),
            "{label}: {errors:#?}"
        );
    }
}

const PREFIX: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn emit(n:i32){unsafe{putchar(n)}}
"#;

const NESTED: &str = r#"
package State<T,F>{manages guard:Ticket;value:T;callback:F}
fn pack<T:Copy,F:FnMut(i32)->i32>(callback:F,value:T,code:i32)->State<T,F> {
    return State::<T,F>{guard:Ticket{code:code},value:value,callback:callback}
}
fn make(code:i32)->impl FnMut(i32)->i32 {
    let ticket=Ticket{code:code};var total=0;
    return move(n:i32)=>{emit(ticket.code);total+=n;return total}
}
fn next(value:&mut i32)->i32{emit(80);*value+=1;return *value}
fn main()->i32 {
    var a=pack(make(65),1,97);var b=pack(make(66),3,98);
    let result=a.callback(b.callback(next(&mut a.value)));
    let later=a.callback(1)+b.callback(1);
    return result+later+a.value-10
}
"#;

fn main_function(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap()
}

fn source_rejected(label: &str, source: &str, phase: Phase, category: ResultCategory, code: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn distinct_same_closure_states_nest_calls_while_borrowing_a_real_sibling() {
    let source = format!("{PREFIX}{NESTED}");
    let (module, _) = baseline("nested-state-control", &source);
    let main = &module.functions[main_function(&module)];
    let a = main.locals.iter().find(|l| l.name == "a").unwrap();
    let b = main.locals.iter().find(|l| l.name == "b").unwrap();
    assert_eq!(a.ty, b.ty);
    let Type::Package { fields, .. } = &a.ty else {
        panic!("State package")
    };
    assert!(matches!(fields[2].1, Type::Closure(_)));
    let instructions = main
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .collect::<Vec<_>>();
    assert!(
        !instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::TakePackageField { .. }))
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|i| matches!(
                i.kind,
                InstructionKind::CallClosureBorrowed {
                    kind: CallableKind::FnMut,
                    ..
                }
            ))
            .count(),
        4
    );
    native("nested-state-runtime", &source, "PBAABbBaA", 0);
    source_rejected(
        "same-field-nested-argument",
        &source.replace(
            "a.callback(b.callback(next(&mut a.value)))",
            "a.callback(a.callback(next(&mut a.value)))",
        ),
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn retargeting_the_inner_callee_to_the_same_state_cannot_create_another_window() {
    let source = format!("{PREFIX}{NESTED}");
    let (mut module, _) = baseline("retarget-state-control", &source);
    let index = main_function(&module);
    let main = &mut module.functions[index];
    let a = main.locals.iter().find(|l| l.name == "a").unwrap().id;
    let b = main.locals.iter().find(|l| l.name == "b").unwrap().id;
    let instruction = main
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,InstructionKind::BorrowPackageField{local,index:2,..}if local==b))
        .unwrap();
    let InstructionKind::BorrowPackageField { local, .. } = &mut instruction.kind else {
        unreachable!()
    };
    *local = a;
    // Give the independent checker a provenance-matching, still untrusted
    // candidate. The attack must reach permission checks, not a stale hash.
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject(
        "retarget-inner-same-state",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
        false,
    );
}

const MODES: &str = r#"
package State<F>{manages guard:Ticket;callback:F}
fn pack<F:Fn()->i32>(callback:F,code:i32)->State<F>{return State::<F>{guard:Ticket{code:code},callback:callback}}
fn make(code:i32)->impl Fn()->i32{let ticket=Ticket{code:code};return move()=>ticket.code}
fn consume<G:FnOnce()->i32>(state:State<G>)->i32{return state.callback()}
fn observe<G:Fn()->i32>(state:State<G>)->i32{return state.callback()+state.callback()}
fn main()->i32{let a=consume(pack(make(65),83));emit(88);let b=observe(pack(make(66),84));return a+b-197}
"#;

#[test]
fn one_concrete_closure_id_does_not_share_declared_modes_between_state_helpers() {
    let source = format!("{PREFIX}{MODES}");
    let (module, _) = baseline("declared-mode-control", &source);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::Fn);
    let mut once = 0;
    let mut shared = 0;
    let mut take = 0;
    for instruction in module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
    {
        match instruction.kind {
            InstructionKind::CallClosureOnce { .. } => once += 1,
            InstructionKind::CallClosureBorrowed {
                kind: CallableKind::Fn,
                ..
            } => shared += 1,
            InstructionKind::TakePackageField { .. } => take += 1,
            _ => {}
        }
    }
    assert_eq!((once, shared, take), (1, 2, 1));
    native("declared-mode-runtime", &source, "ASXTB", 0);
}

const PREPARATION: &str = r#"
enum E{Fail}
package State<F>{manages guard:Ticket;callback:F}
fn pack<F:FnOnce(i32)->i32>(callback:F)->State<F>{return State::<F>{guard:Ticket{code:84},callback:callback}}
fn make()->impl Fn(i32)->i32{let ticket=Ticket{code:65};return move(n:i32)=>{emit(67);return ticket.code+n}}
fn argument(fail:bool)->E!i32{emit(80);if fail{return E.Fail};return 2}
fn consume<G:FnOnce(i32)->i32>(state:State<G>,fail:bool)->E!i32{
    let answer=state.callback(argument(fail)?);emit(88);return answer
}
fn main()->E!i32{return consume(pack(make()),false)?-67}
"#;

#[test]
fn fn_to_once_takes_the_callee_before_arguments_and_cleans_failed_preparation() {
    let source = format!("{PREFIX}{PREPARATION}");
    let (module, _) = baseline("once-preparation-control", &source);
    let consume = module
        .functions
        .iter()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
        })
        .unwrap();
    let arguments = module
        .functions
        .iter()
        .find(|f| f.name == "argument")
        .unwrap()
        .id;
    let entry = &consume.blocks[consume.entry.0 as usize];
    let taken = entry
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakePackageField { .. }))
        .unwrap();
    let prepared = entry
        .instructions
        .iter()
        .position(|i| matches!(i.kind,InstructionKind::Call{function,..} if function==arguments))
        .unwrap();
    assert!(taken < prepared);
    native("once-preparation-success", &source, "PCAXT", 0);
    native(
        "once-preparation-failure",
        &source.replace("consume(pack(make()),false)", "consume(pack(make()),true)"),
        "PAT",
        1,
    );
}

const LENDING: &str = r#"
package State<F>{manages storage:ByteBuffer;callback:F;count:i32}
fn make()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}
fn pack<F:FnMut()->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback,count:0}}
fn main()->i32{
    var a=pack(make());var b=pack(make());
    let first=a.callback();let other=b.callback();
    a.count+=1;
    return i32(first[0])+i32(other[0])+a.count-15
}
"#;

#[test]
fn lending_windows_distinguish_two_state_instances_with_the_same_closure_type() {
    let (mut module, _) = baseline("lending-instances-control", LENDING);
    native("lending-instances-runtime", LENDING, "", 0);
    source_rejected(
        "lending-same-instance",
        &LENDING.replace("let other=b.callback()", "let other=a.callback()"),
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
    let index = main_function(&module);
    let main = &mut module.functions[index];
    let a = main.locals.iter().find(|l| l.name == "a").unwrap().id;
    let b = main.locals.iter().find(|l| l.name == "b").unwrap().id;
    let instruction = main
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,InstructionKind::BorrowPackageField{local,index:1,..}if local==b))
        .unwrap();
    let InstructionKind::BorrowPackageField { local, .. } = &mut instruction.kind else {
        unreachable!()
    };
    *local = a;
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject(
        "lending-retargeted-instance",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
        false,
    );
}

#[test]
fn a_conditional_argument_cannot_invalidate_its_already_borrowed_parent() {
    let source = format!(
        "{PREFIX}{}fn discard<T:Copy,F:FnMut(i32)->i32>(state:State<T,F>)->i32{{return 1}}fn main()->i32{{var a=pack(make(65),1,97);return a.callback(if false{{discard(a)}}else{{1}})}}",
        NESTED.split("fn main()").next().unwrap()
    );
    source_rejected(
        "conditional-parent-move",
        &source,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let mut high = Limits::default().max_analysis_steps;
    let mut low = 0;
    run(high).unwrap();
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match run(mid) {
            Ok(()) => high = mid,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                low = mid
            }
        }
    }
    run(high).unwrap();
    let errors = run(high - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    run(high).unwrap();
    high
}

#[test]
fn field_lookup_and_nested_argument_windows_have_exact_fresh_phase_budgets() {
    let source = format!("{PREFIX}{NESTED}");
    let (module, candidate) = baseline("nested-budget-control", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review199.zil", &source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
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
    Case::new("nested-budget-boundaries").write("boundaries.json",format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
