//! Independent 196 review: the attacks change actual argument/SSA origins as
//! well as public claims. Expected event order is derived from these sources.
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
    CallableFlowNext, CallableKind, CallableTraces, CandidatePlan, InstructionKind, TypedModule,
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
            "review196-by197-{label}-{}-{}",
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
        &SourceUnit::new(FileId(0), ModuleId(0), "review196.zil", source),
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

fn native(label: &str, source: &str, expected: &str) {
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
        case.write("native.txt", format!("optimization={optimization:?}\nexit={:?}\nexpected_exit=0\nexpected_stdout={expected:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review196.zil", source);
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

const SPARSE: &str = r#"
extern "C" fn putchar(n:i32)->i32;
fn make(code:i32)->impl FnMut(i32)->i32 {
    var total=0;
    return move(n:i32)=>{total+=n;unsafe{putchar(code)};return total}
}
fn leaf<F:FnMut(i32)->i32>(dummy:i32,a:&mut F,gap:bool,b:&mut F,n:i32)->i32 {
    let x=a(n);return x+b(n)
}
fn parent<F:FnMut(i32)->i32>(ignored:bool,a:&mut F,n:i32,b:&mut F)->i32 {
    let first=leaf(n,a,true,b,n);
    let second=leaf(n,b,false,a,n+1);
    return first+second
}
fn main()->i32 {
    var a=make(65);var b=make(66);
    return parent(false,&mut a,2,&mut b)-14
}
"#;

fn parent(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| {
            f.callable_contract.as_ref().is_some_and(|c| {
                c.traces
                    .as_finite()
                    .is_some_and(|paths| paths.iter().any(|p| p.calls.len() == 4))
            })
        })
        .unwrap()
}

#[test]
fn sparse_same_type_roles_keep_distinct_runtime_state_and_each_child_mapping() {
    let (module, _) = baseline("sparse-control", SPARSE);
    let contract = module.functions[parent(&module)]
        .callable_contract
        .as_ref()
        .unwrap();
    assert_eq!(
        contract
            .roles
            .iter()
            .map(|r| r.parameter)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(contract.roles[0].closure, contract.roles[1].closure);
    assert_eq!(
        contract.traces.as_finite().unwrap()[0]
            .calls
            .iter()
            .map(|e| e.parameter)
            .collect::<Vec<_>>(),
        [1, 3, 3, 1]
    );
    native("sparse-runtime", SPARSE, "ABBA");
}

#[test]
fn swapping_actual_arguments_at_only_the_second_child_site_invalidates_the_claim() {
    let (mut module, candidate) = baseline("actual-child-map-control", SPARSE);
    let index = parent(&module);
    let calls: Vec<_> = module.functions[index]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .filter_map(|i| match &mut i.kind {
            InstructionKind::Call { arguments, .. } => Some(arguments),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    let second = calls.into_iter().nth(1).unwrap();
    assert_ne!(second[1], second[3]);
    second.swap(1, 3);
    reject(
        "actual-second-child-map",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
        true,
    );
}

#[test]
fn two_role_entries_cannot_name_one_actual_parameter_local() {
    let (mut module, candidate) = baseline("duplicate-local-control", SPARSE);
    let index = parent(&module);
    let function = &mut module.functions[index];
    assert_ne!(function.parameters[1], function.parameters[3]);
    function.parameters[3] = function.parameters[1];
    reject(
        "duplicate-actual-local",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-PARAMETER",
        true,
    );
}

fn captured_window(nested: bool, alias: bool) -> String {
    let helper = if nested {
        "fn child<F:Fn()->i32,G:Fn()->i32>(writer:&G,reader:&F)->i32{return reader()+writer()}fn pair<F:Fn()->i32,G:Fn()->i32>(reader:&F,writer:&G)->i32{return child(writer,reader)}"
    } else {
        "fn pair<F:Fn()->i32,G:Fn()->i32>(reader:&F,writer:&G)->i32{return reader()+writer()}"
    };
    let written = if alias { "other" } else { "origin" };
    format!(
        "resource R{{n:i32}}{helper}fn main()->i32{{let origin=R{{n:1}};let other=R{{n:8}};let edit={written};let held=&other;let view=&held.n;let reader=()=>*view;let writer=()=>{{edit.n+=1;return edit.n}};return pair(&reader,&writer)-10}}"
    )
}

fn actual_desc_alias(module: &mut TypedModule) {
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let origin = main.locals.iter().find(|l| l.name == "origin").unwrap().id;
    let other = main.locals.iter().find(|l| l.name == "other").unwrap().id;
    let mut changed = 0;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::DescribeLocal { local } = &mut instruction.kind
            && *local == origin
        {
            *local = other;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
}

fn source_borrow_reject(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn second_role_descriptor_write_conflicts_with_the_first_roles_current_capture() {
    for nested in [false, true] {
        let source = captured_window(nested, false);
        let (mut module, candidate) = baseline("capture-window-control", &source);
        native("capture-window-native", &source, "");
        source_borrow_reject(
            "capture-window-source-alias",
            &captured_window(nested, true),
        );
        actual_desc_alias(&mut module);
        reject(
            "capture-window-raw-alias",
            module,
            candidate,
            ResultCategory::SafeReject,
            "E-BORROW-CONFLICT",
            false,
        );
    }
}

fn ordinary_window(alias: bool) -> String {
    let written = if alias { "other" } else { "origin" };
    format!(
        "fn pair<F:Fn()->i32,G:Fn()->i32>(reader:&F,writer:&G,edit:ByteBuffer)->i32{{bytes_reserve(edit,2);return reader()+writer()+i32(bytes_len(edit))}}fn main()->i32{{let origin=bytes_new();let other=bytes_new();bytes_reserve(other,1);bytes_push(other,65u8);let edit={written};let view=bytes_view(other);let p=&view[0];let reader=()=>i32(*p);let writer=move()=>1;return pair(&reader,&writer,edit)-66}}"
    )
}

#[test]
fn ordinary_desc_parameter_cannot_use_any_callback_own_window_exemption() {
    let source = ordinary_window(false);
    let (mut module, candidate) = baseline("ordinary-window-control", &source);
    native("ordinary-window-native", &source, "");
    source_borrow_reject("ordinary-window-source-alias", &ordinary_window(true));
    actual_desc_alias(&mut module);
    // Retain the original stale-provenance attack, then let the producer state
    // the new actual provenance. Neither candidate grants borrowing permission.
    reject(
        "ordinary-window-stale-hold",
        module.clone(),
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-HOLD",
        false,
    );
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject(
        "ordinary-window-raw-alias",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
        false,
    );
}

const MERGED: &str = r#"
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn select<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32 {
    var selected=a;let alternate=b;
    if flag {selected=a}else{selected=a};
    return selected()+b()
}
fn main()->i32{let a=make(1);let b=make(2);return select(&a,&b,true)-3}
"#;

#[test]
fn a_real_branch_store_merging_two_same_type_origins_is_not_one_role() {
    let (mut module, candidate) = baseline("merged-origin-control", MERGED);
    let helper = module
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    let selected = helper
        .locals
        .iter()
        .find(|l| l.name == "selected")
        .unwrap()
        .id;
    let alternate = helper
        .locals
        .iter()
        .find(|l| l.name == "alternate")
        .unwrap()
        .id;
    let alternate_value = helper
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::Store { local, value } if local == alternate => Some(value),
            _ => None,
        })
        .unwrap();
    let stores: Vec<_> = helper
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .filter_map(|i| match &mut i.kind {
            InstructionKind::Store { local, value } if *local == selected => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(stores.len(), 3);
    *stores.into_iter().last().unwrap() = alternate_value;
    reject(
        "merged-actual-origins",
        module,
        candidate,
        ResultCategory::Unsupported,
        "mixed",
        true,
    );
}

const BRANCH: &str = r#"
extern "C" fn putchar(n:i32)->i32;
fn make(n:i32)->impl Fn()->i32{return move()=>{unsafe{putchar(n)};return 1}}
fn choose<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32 {
    if flag {return a()+b()}else{return b()+a()}
}
fn main()->i32{let a=make(65);let b=make(66);return choose(&a,&b,true)+choose(&a,&b,false)-4}
"#;

#[test]
fn actual_branch_role_swap_cannot_keep_a_second_correlated_trace() {
    let (mut module, candidate) = baseline("branch-origin-control", BRANCH);
    native("branch-correlated-runtime", BRANCH, "ABBA");
    let helper = module
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    let a = helper.parameters[0];
    let b = helper.parameters[1];
    let contract = helper.callable_contract.as_ref().unwrap();
    assert_eq!(
        contract
            .traces
            .as_finite()
            .unwrap()
            .iter()
            .map(|p| p.calls.iter().map(|e| e.parameter).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [vec![0, 1], vec![1, 0]]
    );
    let block = helper
        .blocks
        .iter_mut()
        .rev()
        .find(|block| {
            block
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }))
                .count()
                == 2
        })
        .unwrap();
    let mut changed = 0;
    for instruction in &mut block.instructions {
        if let InstructionKind::Load(local) = &mut instruction.kind {
            if *local == a {
                *local = b;
                changed += 1
            } else if *local == b {
                *local = a;
                changed += 1
            }
        }
    }
    assert_eq!(changed, 2);
    reject(
        "actual-branch-origins",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-VERIFY-CALLABLE-CONTRACT",
        true,
    );
}

const FLOW: &str = r#"
extern "C" fn putchar(n:i32)->i32;
fn make(n:i32)->impl Fn()->i32{return move()=>{unsafe{putchar(n)};return 1}}
fn walk<F:Fn()->i32>(a:&F,unused:bool,b:&F,n:i32)->i32 {
    let before=a();var i=0;var sum=0;
    while i<n{sum+=b()+a();i+=1};
    return before+sum+b()
}
fn main()->i32{let a=make(65);let b=make(66);return walk(&a,false,&b,1)-4}
"#;

#[test]
fn flow_keeps_prefix_loop_order_and_return_suffix_by_actual_role_position() {
    let (module, candidate) = baseline("flow-order-control", FLOW);
    native("flow-order-runtime", FLOW, "ABAB");
    let index = module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some())
        .unwrap();
    let CallableTraces::Flow(flow) = &module.functions[index]
        .callable_contract
        .as_ref()
        .unwrap()
        .traces
    else {
        panic!("expected Flow")
    };
    assert_eq!(
        flow.nodes
            .iter()
            .map(|n| n.calls.iter().map(|e| e.parameter).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [vec![0], vec![], vec![2, 0], vec![2]]
    );
    assert_eq!(flow.nodes[2].next, CallableFlowNext::Edges(vec![1]));
    for attack in 0..2 {
        let mut changed = module.clone();
        let CallableTraces::Flow(flow) = &mut changed.functions[index]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
        else {
            unreachable!()
        };
        if attack == 0 {
            flow.nodes[2].calls.swap(0, 1)
        } else {
            let prefix = flow.nodes[0].calls[0];
            flow.nodes[0].calls[0] = flow.nodes[3].calls[0];
            flow.nodes[3].calls[0] = prefix
        }
        reject(
            &format!("ordered-flow-{attack}"),
            changed,
            candidate.clone(),
            ResultCategory::SafeReject,
            "E-VERIFY-CALLABLE-CONTRACT",
            true,
        );
    }
}

#[test]
fn an_unused_owned_once_role_is_closed_on_a_zero_runtime_loop_in_raw_and_wire() {
    let source = "fn make(n:i32)->impl Fn()->i32{return move()=>n}fn repeat<F:Fn()->i32,G:Fn()->i32>(idle:F,active:&G,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=active();i+=1};return sum}fn main()->i32{let a=make(1);let b=make(2);return repeat(a,&b,0)}";
    let (mut module, candidate) = baseline("unused-owned-loop-control", source);
    let contract = module
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    assert_eq!(contract.roles[0].parameter, 0);
    let CallableTraces::Flow(flow) = &contract.traces else {
        panic!("expected Flow")
    };
    assert!(
        flow.nodes
            .iter()
            .flat_map(|n| &n.calls)
            .all(|e| e.parameter != 0)
    );
    contract.roles[0].kind = CallableKind::FnOnce;
    reject(
        "zero-runtime-loop-unused-once",
        module,
        candidate,
        ResultCategory::Unsupported,
        "cyclic FnOnce",
        true,
    );
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let mut high = Limits::default().max_analysis_steps;
    assert!(run(high).is_ok());
    let mut low = 0;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match run(mid) {
            Ok(()) => high = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = mid;
            }
        }
    }
    assert!(run(high).is_ok());
    let errors = run(high - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert!(run(high).is_ok());
    high
}

#[test]
fn joint_flow_charges_default_budgets_and_fresh_requests_recover() {
    let (module, candidate) = baseline("flow-budget-control", FLOW);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review196.zil", FLOW);
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
    Case::new("flow-budget-boundaries").write("boundaries.json",format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}

const NESTED_CAPTURE: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn make(code:i32)->impl FnMut(i32)->i32 {
    let ticket=Ticket{code:code};var total=0;
    return move(n:i32)=>{ticket.code;total+=n;return total}
}
fn apply<G:FnMut(i32)->i32,F:Fn(i32)->i32>(tag:i32,passive:&F,active:&mut G,n:i32)->i32 {
    return passive(n)+active(n)
}
fn wrap()->impl FnMut()->i32 {
    var active=make(90);
    return move()=>{let passive=(n:i32)=>n;return apply(7,&passive,&mut active,2)}
}
fn main()->i32{var f=wrap();let first=f();let second=f();return first+second-10}
"#;

#[test]
fn nested_second_role_mutates_a_captured_callback_and_cleans_its_owner_once() {
    let (module, _) = baseline("nested-captured-callback-control", NESTED_CAPTURE);
    let contract = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    assert_eq!(
        contract
            .roles
            .iter()
            .map(|r| (r.parameter, r.kind))
            .collect::<Vec<_>>(),
        [(1, CallableKind::Fn), (2, CallableKind::FnMut)]
    );
    native("nested-captured-callback-runtime", NESTED_CAPTURE, "Z");
    let case = Case::new("nested-captured-callback-wrong-outer-mode");
    let wrong = NESTED_CAPTURE.replace("fn wrap()->impl FnMut()->i32", "fn wrap()->impl Fn()->i32");
    let report = compile(&case, &wrong, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review196.zil", NESTED_CAPTURE);
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
    Case::new("nested-capture-budget-boundaries").write("boundaries.json",format!("{{\"types\":{types},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
