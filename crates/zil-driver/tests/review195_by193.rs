//! Non-author review of 195 at 1a890dabc4eb04b26570439718b0a9432511fa95.
//! Oracles below come from the source and resource rules, not a Flow producer.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::{CallableTraces, CandidatePlan, CleanupExitKind, InstructionKind, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "review195-by193-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(label: &str, source: &str) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "review195.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    report
}

fn accepted(label: &str, source: &str) {
    let report = check(label, source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

fn rejected_source(label: &str, source: &str, category: ResultCategory, message: &str) {
    let report = check(label, source);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn nested_match_distinguishes_returned_value_from_break_and_continue_debts() {
    let source = "fn keep<T>(value:T,mode:i32)->T{for outer in 0..3{var inner=0;while inner<2{inner+=1;match mode{0=>{return value},1=>{break},_=>{continue}}}break}return value}fn main()->i32{return keep(7,1)-7}";
    accepted("nested-match-positive", source);
    // The concrete Copy call cannot validate the generic definition. The
    // changed branch either survives the loop or is a genuine backedge.
    rejected_source(
        "nested-match-break-move",
        &source.replace("1=>{break}", "1=>{let taken=value;break}"),
        ResultCategory::SafeReject,
        "generic value used after move; Copy cannot be inferred from a concrete instance",
    );
    rejected_source(
        "nested-match-continue-move",
        &source.replace("_=>{continue}", "_=>{let taken=value;continue}"),
        ResultCategory::Unsupported,
        "generic loop must preserve outer carrier and non-Copy consumption state",
    );
}

#[test]
fn nested_break_carries_invalidated_origins_but_excludes_returning_paths() {
    let source = "fn inspect<T:StoreElement>(give incoming:Store<T>,mode:i32)->i64{var current:Store<T>=store_new();let safe:Store<T>=store_new();let alias=safe;for outer in 0..2{while mode>0{if mode==1{return 0}else{current=give incoming;break}}break}return store_len(alias)}fn main(){}";
    accepted("nested-store-unrelated-alias", source);
    rejected_source(
        "nested-store-break-invalidates-alias",
        &source.replace("let alias=safe", "let alias=current"),
        ResultCategory::SafeReject,
        "Store descriptor depends on a carrier or field that was moved",
    );
    // If replacement exits the function, it must not taint the path which
    // only breaks and then observes the original current carrier.
    accepted(
        "nested-store-return-excluded",
        "fn inspect<T:StoreElement>(give incoming:Store<T>,mode:i32)->i64{var current:Store<T>=store_new();let alias=current;for outer in 0..2{while mode>0{if mode==1{current=give incoming;return 0}else{break}}break}return store_len(alias)}fn main(){}",
    );
}

const NATIVE: &str = r#"
extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
fn emit(n:i32){unsafe{putchar(n)}}
resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}
fn run<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{
 let root=R{code:82};var total=0;
 for outer in 0..n{
  let outer_guard=R{code:79+outer};var inner=0;
  while inner<3{
   let inner_guard=R{code:97+inner};inner+=1;
   match (outer==1,inner==2){
    (true,true)=>{return total+f(outer*10+inner)},
    (false,true)=>{break},
    (_,false)=>{if inner==1{continue}}
   }
   total+=f(inner)
  }
  total+=f(9);
  if outer==0{continue}
  emit(88)
 }
 return total
}
fn main()->i32{
 var n=0;unsafe{n=getchar()-48};var calls=0;
 var f=move(value:i32)=>{calls+=1;emit(48+calls);return value+calls};
 return run(&mut f,n)
}
"#;

#[test]
fn nearest_mixed_loop_targets_match_the_manual_native_cleanup_oracle() {
    // n=0 has only the helper root. The first outer iteration continues its
    // while once, breaks at inner=2, calls f(9), then continues the for.
    // The second outer iteration returns f(12) before unwinding b/P/R.
    let cases = [
        (0, "R", 0),
        (1, "ab1OR", 10),
        (2, "ab1Oa2bPR", 24),
        (4, "ab1Oa2bPR", 24),
    ];
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("native-mixed-targets");
        fs::write(dir.join("source.zil"), NATIVE).unwrap();
        let program = dir.join("program");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "native195.zil", NATIVE),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(program.clone()),
                optimization,
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert!(
            fs::read_to_string(dir.join("phases/typed.txt"))
                .unwrap()
                .contains("traces: Flow(")
        );
        for (input, expected, code) in cases {
            let case = dir.join(format!("input-{input}"));
            fs::create_dir(&case).unwrap();
            fs::write(case.join("stdin.txt"), [b'0' + input]).unwrap();
            let mut child = Command::new(&program)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&[b'0' + input])
                .unwrap();
            let started = Instant::now();
            let mut timed_out = false;
            while child.try_wait().unwrap().is_none() {
                if started.elapsed() > Duration::from_secs(10) {
                    timed_out = true;
                    child.kill().unwrap();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            fs::write(case.join("stdout.log"), &output.stdout).unwrap();
            fs::write(case.join("stderr.log"), &output.stderr).unwrap();
            fs::write(case.join("native.json"), format!("{{\"optimization\":\"{optimization:?}\",\"input\":{input},\"expected_stdout\":{expected:?},\"expected_exit\":{code},\"exit_code\":{:?},\"timed_out\":{timed_out}}}\n", output.status.code().unwrap_or(-1))).unwrap();
            assert!(!timed_out);
            assert_eq!(output.status.code(), Some(code), "{output:?}");
            assert_eq!(output.stdout, expected.as_bytes());
            assert!(output.stderr.is_empty());
        }
    }
}

fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw195.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    fs::write(dir.join("accepted.wire"), wire).unwrap();
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    (module, candidate)
}

fn rejected_raw(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    code: &str,
    message: &str,
) {
    // The hostile raw module is not rechecked by Types or Analyze. A fresh
    // fingerprint is identity only and supplies neither a loan nor cleanup proof.
    candidate.module_fingerprint = module.fingerprint();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    )
    .unwrap_err();
    let decoded =
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err();
    let dir = directory(label);
    fs::write(dir.join("rejected.wire"), wire).unwrap();
    fs::write(
        dir.join("diagnostics.txt"),
        format!("raw={raw:#?}\nwire={decoded:#?}\n"),
    )
    .unwrap();
    for diagnostics in [&raw, &decoded] {
        assert!(
            diagnostics.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code
                && d.message.contains(message)),
            "{label}: {diagnostics:?}"
        );
    }
}

#[test]
fn duplicate_cleanup_on_each_real_loop_exit_is_independently_rejected() {
    let (module, candidate) = baseline("raw-mixed-targets", NATIVE);
    let f = module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some())
        .unwrap();
    assert!(matches!(
        module.functions[f]
            .callable_contract
            .as_ref()
            .unwrap()
            .traces,
        CallableTraces::Flow(_)
    ));
    for exit in [CleanupExitKind::Break, CleanupExitKind::Continue] {
        let mut changed = module.clone();
        let block = changed.functions[f]
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::ScopeExit{kind,..} if kind == exit))
                    && b.instructions
                        .iter()
                        .any(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
            })
            .unwrap();
        let at = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
            .unwrap();
        let duplicate = block.instructions[at].clone();
        block.instructions.insert(at + 1, duplicate);
        rejected_raw(
            &format!("double-cleanup-{exit:?}"),
            changed,
            candidate.clone(),
            "E-VERIFY-RESOURCE",
            "duplicate automatic cleanup",
        );
    }
}

#[test]
fn control_edges_cannot_hide_a_forged_actual_captured_description_origin() {
    let source = "resource R{n:i32}fn walk<F:Fn()->i32>(f:&F,stop:i32)->i32{var seen=0;for index in 0..4{if index==0{continue}seen=f();if index==stop{break}}return seen}fn main()->i32{let origin=R{n:1};let other=R{n:8};let edit=origin;let held=&other;let view=&held.n;let f=()=>{edit.n+=1;return edit.n};let answer=walk(&f,2);return answer+*view-11}";
    let (mut module, candidate) = baseline("desc-independent-source", source);
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
    rejected_raw(
        "forged-loop-desc-alias",
        module,
        candidate,
        "E-BORROW-CONFLICT",
        "",
    );
}

#[test]
fn nested_exit_snapshots_with_shadowed_moved_ids_keep_exact_existing_limits() {
    let mut source = "fn keep<T>(value:T,".to_owned();
    for index in 0..8 {
        source.push_str(&format!("spare{index}:T,"));
    }
    source.push_str("mode:i32)->T{");
    // Moved symbolic positions remain real even when their names are hidden.
    for index in 0..8 {
        source.push_str(&format!(
            "let hidden=spare{index};let lost=hidden;let hidden=mode;let lost=mode;"
        ));
    }
    source.push_str("let held:Store<i32>=store_new();let owned=give held;let held=0;let owned=0;");
    source.push_str("let a_long_visible_local_name_to_charge_before_loop_snapshot_copies=mode;");
    for depth in 0..4 {
        source.push_str(&format!("for index{depth} in 0..2{{"));
    }
    source.push_str("if mode==0{continue}else{break}");
    for _ in 0..3 {
        source.push_str("}if mode==1{continue}else{break}");
    }
    source.push_str("}return value}fn main(){}");
    // Pilot01 incorrectly asked this budget control to accept Store<i32>.
    // Keep that complete original input and its real nominal-element gate;
    // the budget control uses the supported complete Package element below.
    rejected_source(
        "shadowed-id-scalar-store-remains-closed",
        &source,
        ResultCategory::SafeReject,
        "StoreElement requires a complete nominal Package or declared resource",
    );
    let resource_only = format!(
        "resource Item{{value:i32}}{}",
        source.replace("Store<i32>", "Store<Item>")
    );
    // Pilot02 also preserved the direct-resource category boundary. A
    // declared resource is not itself the Package Store element profile.
    rejected_source(
        "shadowed-id-resource-store-remains-closed",
        &resource_only,
        ResultCategory::SafeReject,
        "StoreElement requires the declared Package/resource category and type arguments",
    );
    let source = format!(
        "resource R{{value:i32}}package Item{{manages r:R}}{}",
        source.replace("Store<i32>", "Store<Item>")
    );
    accepted("shadowed-id-budget-default", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget195.zil", &source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    for metric in ["nodes", "work", "depth"] {
        let dir = directory(metric);
        fs::write(dir.join("source.zil"), &source).unwrap();
        let mut observations = Vec::new();
        let mut attempt = |limit: usize| {
            let mut limits = Limits::default();
            match metric {
                "nodes" => limits.max_nodes = limit,
                "work" => limits.max_analysis_steps = limit,
                "depth" => limits.max_call_depth = limit,
                _ => unreachable!(),
            }
            let result = zil_expand::expand_with_map_limits(&unit, &ast, &limits);
            let diagnostics = match &result {
                Ok(_) => "[]".to_owned(),
                Err(ds) => format!(
                    "[{}]",
                    ds.iter().map(|d| d.to_json()).collect::<Vec<_>>().join(",")
                ),
            };
            observations.push(format!(
                "{{\"limit\":{limit},\"success\":{},\"diagnostics\":{diagnostics}}}",
                result.is_ok()
            ));
            if let Err(ds) = &result {
                assert!(
                    ds.iter().any(|d| d.phase == Phase::Expand
                        && d.category == ResultCategory::ResourceLimit
                        && d.code == "E-RESOURCE-LIMIT"),
                    "{ds:?}"
                );
            }
            result.is_ok()
        };
        let full = match metric {
            "nodes" => Limits::default().max_nodes,
            "work" => Limits::default().max_analysis_steps,
            "depth" => Limits::default().max_call_depth,
            _ => unreachable!(),
        };
        assert!(attempt(full));
        assert!(!attempt(0));
        let (mut low, mut high) = (0usize, full);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            if attempt(middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        assert!(high > 0);
        assert!(!attempt(high - 1));
        assert!(attempt(high));
        assert!(attempt(full));
        fs::write(dir.join("frontier.json"), format!("{{\"metric\":\"{metric}\",\"minimum\":{high},\"n_minus_one_rejects\":true,\"fresh_default_retry\":true,\"observations\":[{}]}}\n", observations.join(","))).unwrap();
    }
}
