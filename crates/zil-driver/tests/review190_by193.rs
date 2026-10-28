//! Independent review of 190 at source 7491c212. The expected graphs and native
//! output below follow the source's control flow, not a production normalizer.
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
use zil_ir::{
    CallableExit, CallableFlow, CallableFlowNext, CallableFlowNode, CallableKind, CallableTrace,
    CallableTraces, CandidatePlan, Constant, InstructionKind, Terminator, TypedModule,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = root.join(format!(
        "review190-by193-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn typed(source: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review190.zil", source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn accept(label: &str, module: &TypedModule) -> CandidatePlan {
    let candidate = zil_analyze::analyze(module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    let dir = directory(label);
    fs::write(dir.join("accepted.wire"), wire).unwrap();
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    fs::write(
        dir.join("result.json"),
        "{\"analyze\":\"Pass\",\"verify_raw\":\"Pass\",\"verify_wire\":\"Pass\"}\n",
    )
    .unwrap();
    candidate
}

fn rejected(label: &str, module: TypedModule, mut candidate: CandidatePlan, code: &str) {
    // Rebinding identity supplies no proof. In particular, Analyze is not
    // rerun to manufacture an oracle for the hostile module or candidate.
    candidate.module_fingerprint = module.fingerprint();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let errors = [
        zil_verify::verify_module(
            module.clone(),
            candidate,
            &module.target,
            &Limits::default(),
        )
        .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err(),
    ];
    let dir = directory(label);
    fs::write(dir.join("rejected.wire"), wire).unwrap();
    fs::write(dir.join("diagnostics.txt"), format!("{errors:#?}")).unwrap();
    for diagnostics in errors {
        assert!(
            diagnostics.iter().any(|d| {
                d.phase == Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.code == code
            }),
            "{label}: {diagnostics:?}"
        );
    }
}

fn row(calls: usize, edges: &[u32]) -> CallableFlowNode {
    CallableFlowNode {
        calls: vec![CallableKind::Fn; calls],
        next: CallableFlowNext::Edges(edges.to_vec()),
    }
}

fn returning(calls: usize) -> CallableFlowNode {
    CallableFlowNode {
        calls: vec![CallableKind::Fn; calls],
        next: CallableFlowNext::Exit(CallableExit::Return),
    }
}

#[test]
fn exact_four_node_loop_distinguishes_prefix_body_and_return_suffix() {
    let source = "fn walk<F:Fn()->i32>(f:&F,n:i32)->i32{let before=f();while n>0{f()}return before+f()}fn main()->i32{let f=move()=>3;return walk(&f,0)-6}";
    let module = typed(source);
    let helper = module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some())
        .unwrap();
    let expected = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![row(1, &[1]), row(0, &[2, 3]), row(1, &[1]), returning(1)],
    });
    assert_eq!(
        module.functions[helper]
            .callable_contract
            .as_ref()
            .unwrap()
            .traces,
        expected
    );
    let candidate = accept("manual-four-nodes", &module);
    for (label, prefix, body, suffix) in [
        ("prefix-moved-to-loop", 0, 2, 1),
        ("body-moved-to-return", 1, 0, 2),
        ("return-moved-to-prefix", 2, 1, 0),
    ] {
        let mut changed = module.clone();
        changed.functions[helper]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces = CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: vec![
                row(prefix, &[1]),
                row(0, &[2, 3]),
                row(body, &[1]),
                returning(suffix),
            ],
        });
        rejected(
            label,
            changed,
            candidate.clone(),
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
}

const NONRETURN: &str = r#"
resource R{n:i32}
fn lock<F:Fn()->i32>(f:&F,n:i32)->i32{let seen=f();while n>0{}return seen}
fn outer<F:Fn()->i32>(f:&F,n:i32)->i32{let seen=lock(f,n);return seen+f()}
fn main()->i32{
    let origin=R{n:1};let other=R{n:8};let edit=origin;
    let held=&other;let view=&held.n;
    let f=()=>{edit.n+=1;return edit.n};
    let answer=outer(&f,0);return answer+*view-13
}
"#;

fn nonreturning_module() -> (TypedModule, usize, usize) {
    let mut module = typed(NONRETURN);
    let child = module
        .functions
        .iter()
        .position(|f| {
            f.callable_contract.is_some()
                && f.blocks
                    .iter()
                    .any(|b| matches!(b.terminator, Terminator::Branch { .. }))
        })
        .unwrap();
    let parent = module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some() && f.id != module.functions[child].id)
        .unwrap();
    let function = &mut module.functions[child];
    let header = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, Terminator::Branch { .. }))
        .unwrap();
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = header.terminator
    else {
        unreachable!()
    };
    header.terminator = Terminator::Jump(then_block);
    // A disconnected block must still have valid SSA definitions. Its old
    // scope exits are no longer active and cannot manufacture a reachable exit.
    for instruction in &mut function.blocks[else_block.0 as usize].instructions {
        if matches!(instruction.kind, InstructionKind::Load(_)) {
            instruction.kind = InstructionKind::Constant(Constant::I32(0));
        }
    }
    function.blocks[else_block.0 as usize]
        .instructions
        .retain(|i| i.result.is_some());
    function.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![row(1, &[1]), row(0, &[2]), row(0, &[1])],
    });
    // Hand-derived: parent entry enters this one child copy. The child has no
    // Return, so the actual later parent callback and Return are not reachable.
    module.functions[parent]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![row(0, &[1]), row(1, &[2]), row(0, &[3]), row(0, &[2])],
    });
    (module, child, parent)
}

#[test]
fn no_return_child_keeps_its_prefix_but_never_reaches_the_parent_suffix() {
    let (module, child, parent) = nonreturning_module();
    let candidate = accept("manual-nonreturn-child-and-parent", &module);
    for index in [child, parent] {
        let mut changed = module.clone();
        let CallableTraces::Flow(flow) = &mut changed.functions[index]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
        else {
            unreachable!()
        };
        for node in &mut flow.nodes {
            node.calls.clear();
        }
        rejected(
            &format!("nonreturn-prefix-erased-{index}"),
            changed,
            candidate.clone(),
            "E-VERIFY-CALLABLE-CONTRACT",
        );
    }
    let mut changed = module;
    let CallableTraces::Flow(flow) = &mut changed.functions[parent]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    else {
        unreachable!()
    };
    flow.nodes[3].next = CallableFlowNext::Edges(vec![2, 4]);
    flow.nodes.push(returning(1));
    rejected(
        "nonreturn-child-given-parent-continuation",
        changed,
        candidate,
        "E-VERIFY-CALLABLE-CONTRACT",
    );
}

#[test]
fn a_nonreturning_prefix_still_writes_its_actual_desc_source() {
    let (mut module, _, _) = nonreturning_module();
    let candidate = accept("nonreturn-desc-different-source-control", &module);
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let origin = main.locals.iter().find(|l| l.name == "origin").unwrap().id;
    let other = main.locals.iter().find(|l| l.name == "other").unwrap().id;
    let mut count = 0;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::DescribeLocal { local } = &mut instruction.kind
            && *local == origin
        {
            *local = other;
            count += 1;
        }
    }
    assert_eq!(count, 1);
    rejected(
        "nonreturn-prefix-now-aliases-live-shared-source",
        module,
        candidate,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn finite_once_zero_or_one_calls_keeps_its_original_trace_domain() {
    let source = "fn choose<F:FnOnce()->i32>(f:F,n:i32)->i32{for index in 0..n{return f()}return 0}fn main()->i32{let f=move()=>5;return choose(f,4)-5}";
    let module = typed(source);
    let contract = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    assert_eq!(
        contract.traces,
        CallableTraces::Finite(vec![
            CallableTrace {
                calls: vec![],
                exit: CallableExit::Return
            },
            CallableTrace {
                calls: vec![CallableKind::FnOnce],
                exit: CallableExit::Return
            },
        ])
    );
    accept("finite-once-exact-traces", &module);
    let source = "fn idle<F:FnOnce()->i32>(f:F,n:i32)->i32{while n>0{}return 0}fn main()->i32{let f=move()=>5;return idle(f,0)}";
    let dir = directory("cyclic-once-zero-runtime-control");
    fs::write(dir.join("source.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "once.zil", source),
        &CompileOptions::default(),
    );
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.status, ResultCategory::Unsupported);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
}

fn chain(count: usize) -> String {
    let mut source = "fn level0<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}\n".to_owned();
    for i in 1..count {
        source.push_str(&format!(
            "fn level{i}<F:Fn()->i32>(f:&F,n:i32)->i32=>level{}(f,n)\n",
            i - 1
        ));
    }
    source.push_str(&format!(
        "fn main()->i32{{let f=move()=>1;return level{}(&f,20000)-20000}}",
        count - 1
    ));
    source
}

fn resource_limit(errors: &[Diagnostic]) -> bool {
    errors
        .iter()
        .any(|d| d.category == ResultCategory::ResourceLimit)
}

#[test]
fn all_three_phases_enforce_twenty_four_static_helpers_at_the_exact_depth() {
    let count = 24;
    let source = chain(count);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "depth.zil", source.clone());
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let module = typed(&source);
    let candidate = accept("depth24-default-control", &module);
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    let dir = directory("depth24-boundary");
    fs::write(dir.join("source.zil"), source).unwrap();
    for depth in [count - 1, count] {
        let limits = Limits {
            max_call_depth: depth,
            ..Limits::default()
        };
        let attempts = [
            (
                "Types",
                zil_types::check(&core, &module.target, &limits).map(|_| ()),
            ),
            (
                "Analyze",
                zil_analyze::analyze(&module, &limits).map(|_| ()),
            ),
            (
                "VerifyRaw",
                zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ()),
            ),
            (
                "VerifyWire",
                zil_verify::verify_module_bytes(&wire, &module.target, &limits).map(|_| ()),
            ),
        ];
        for (phase, result) in attempts {
            fs::write(
                dir.join(format!("{phase}-{depth}.txt")),
                format!("{result:#?}"),
            )
            .unwrap();
            if depth == count {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert!(resource_limit(&errors), "{phase}: {errors:?}");
                let expected = if phase == "Types" {
                    "callable helper nesting exceeds selected bound"
                } else {
                    "helper dependency call depth exhausted"
                };
                assert!(
                    errors
                        .iter()
                        .any(|d| d.code == "E-RESOURCE-LIMIT" && d.message == expected),
                    "{phase}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn exact_work_frontiers_are_independent_and_fresh_requests_recover() {
    let source = chain(4);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "work.zil", source.clone());
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let module = typed(&source);
    let candidate = accept("work-default-control", &module);
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    let dir = directory("work-frontiers");
    fs::write(dir.join("source.zil"), source).unwrap();
    for phase in ["Types", "Analyze", "VerifyRaw", "VerifyWire"] {
        let attempt = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            match phase {
                "Types" => zil_types::check(&core, &module.target, &limits).map(|_| ()),
                "Analyze" => zil_analyze::analyze(&module, &limits).map(|_| ()),
                "VerifyRaw" => zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ()),
                _ => zil_verify::verify_module_bytes(&wire, &module.target, &limits).map(|_| ()),
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        attempt(high).unwrap();
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match attempt(mid) {
                Ok(()) => high = mid,
                Err(errors) => {
                    assert!(resource_limit(&errors), "{phase}: {errors:?}");
                    low = mid;
                }
            }
        }
        let errors = attempt(high - 1).unwrap_err();
        assert!(resource_limit(&errors));
        attempt(high).unwrap();
        attempt(Limits::default().max_analysis_steps).unwrap();
        fs::write(dir.join(format!("{phase}.json")), format!("{{\"phase\":\"{phase}\",\"pass_N\":{high},\"reject_N_minus_1\":{},\"fresh_default_pass\":true}}\n",high-1)).unwrap();
        fs::write(
            dir.join(format!("{phase}-rejection.txt")),
            format!("{errors:#?}"),
        )
        .unwrap();
    }
}

const NATIVE: &str = r#"
extern "C" fn getchar()->i32;
extern "C" fn putchar(n:i32)->i32;
fn emit(n:i32){unsafe{putchar(n)}}
resource R{code:i32}
impl Drop for R{fn drop(self:Self){emit(.code)}}
fn piece<F:FnMut(i32)->i32>(f:&mut F,n:i32,tag:i32)->i32{
    var sum=0;for index in 0..n{sum+=f(tag+index)}return sum
}
fn route<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{
    var sum=0;for index in 0..n{
        sum+=piece(f,index,65);sum+=f(120);sum+=piece(f,n-index,97)
    }return sum
}
fn main()->i32{
    var n=0;unsafe{n=getchar()};n-=48;
    let owned=R{code:72};var step=0;
    var callback=move(value:i32)=>{owned.code;step+=1;emit(value);return step};
    let answer=route(&mut callback,n);let calls=n*(n+1);let after=callback(90);
    return answer-calls*(calls+1)/2+after-calls-1
}
"#;

#[test]
fn two_loop_call_sites_preserve_argument_order_environment_and_single_cleanup() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(&format!("native-{optimization:?}"));
        let program = dir.join("program");
        fs::write(dir.join("source.zil"), NATIVE).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "native.zil", NATIVE),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(program.clone()),
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
        for n in 0..=4u8 {
            let mut expected = Vec::new();
            for index in 0..n {
                expected.extend((0..index).map(|i| b'A' + i));
                expected.push(b'x');
                expected.extend((0..n - index).map(|i| b'a' + i));
            }
            expected.extend_from_slice(b"ZH");
            let mut child = Command::new(&program)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(&[b'0' + n]).unwrap();
            let start = Instant::now();
            let mut timed_out = false;
            while child.try_wait().unwrap().is_none() {
                if start.elapsed() > Duration::from_secs(5) {
                    timed_out = true;
                    child.kill().unwrap();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            let case = dir.join(format!("input-{n}"));
            fs::create_dir(&case).unwrap();
            fs::write(case.join("stdout.log"), &output.stdout).unwrap();
            fs::write(case.join("stderr.log"), &output.stderr).unwrap();
            fs::write(case.join("expected.stdout"), &expected).unwrap();
            fs::write(case.join("result.json"), format!("{{\"input\":{n},\"optimization\":\"{optimization:?}\",\"timeout\":{timed_out},\"exit_code\":{},\"expected_exit\":0}}\n",output.status.code().map_or("null".to_owned(),|n|n.to_string()))).unwrap();
            assert!(!timed_out);
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty());
        }
    }
}
