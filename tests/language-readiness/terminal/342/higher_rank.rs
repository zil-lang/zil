//! Universally declared callback operations and invocation-local returned loans.
use super::*;
use zil_driver::{NativeBackend, Optimization};
const PAIR: &str = "fn pair<F:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'a i64,G:for<life a> Fn(&'a i64,&'a i64)->&'a i64>(f:&F,g:&G,p:&i64,q:&i64)->i64{let left=f(p,q);let right=g(p,q);return *left+*right}fn result()->i64{let n:i64=21;let other:i64=41;let f=(p:&i64,q:&i64)=>p;return pair(&f,&f,&n,&other)}";

const STORAGE: &str = "fn invoke<F:for<life a> Fn(&'a [u8])->i32>(f:&F,v:&[u8])->i32{return f(v)}fn result()->i32{let read=bytes_new();let edit=bytes_new();bytes_reserve(read,1);bytes_reserve(edit,1);bytes_push(read,42u8);bytes_push(edit,1u8);let f=(v:&[u8])->i32=>{bytes_reserve(edit,8);return i32(v[0])};return invoke(&f,bytes_view(read))}";
const OWNED: &str = "fn invoke<F:for<life a> FnMut(&'a [u8])->i32>(f:&mut F,v:&[u8])->i32{return f(v)}fn result()->i32{let a=bytes_new();let b=bytes_new();bytes_reserve(a,1);bytes_reserve(b,1);bytes_push(a,42u8);bytes_push(b,1u8);let view=bytes_view(a);var f=move(v:&[u8])->i32=>{bytes_reserve(b,8);return i32(v[0])};return invoke(&mut f,view)}";
const PARENT: &str = "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var n:i64=40;let parent=&mut n;var f=(p:&mut i64)=>p;let child=apply(&mut f,&mut *parent);let answer=*child;*parent=42;return *parent}";
const MULTI: &str = "fn apply<F:for<life a> FnMut(&'a mut i64,&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64,q:&mut i64)->&mut i64{return f(p,q)}fn result()->i64{var a:i64=40;var b:i64=2;var f=(p:&mut i64,q:&mut i64)=>{*p+=*q;return p};let answer=apply(&mut f,&mut a,&mut b);return *answer}";

#[test]
fn terminal342_hrtb_callbacks_use_actual_fresh_arguments_and_fixed_environment() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for text in [
        "fn apply<F:for<life a> Fn(&'a i64)->i64>(f:&F,p:&i64)->i64{return f(p)}fn result()->i64{let n:i64=40;let offset:i64=2;let f=(p:&i64)=>*p+offset;return apply(&f,&n)}",
        "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;let p=apply(&f,&n);return *p}",
        "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var n:i64=40;var f=(p:&mut i64)=>{*p+=2;return p};let p=apply(&mut f,&mut n);return *p}",
        "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn lift(p:&i64)->&i64{let offset:i64=0;let f=(q:&i64)=>{let seen=offset;return q};return apply(&f,p)}fn result()->i64{let n:i64=42;return *lift(&n)}",
        "fn apply<F:for<life a> FnOnce(&'a i64)->&'a i64>(f:F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;let p=apply(f,&n);return *p}",
        "fn apply<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->&mut i64{return f(p)}fn result()->i64{var a:i64=20;var b:i64=20;var f=(p:&mut i64)=>{*p+=1;return p};let left=apply(&mut f,&mut a);let right=apply(&mut f,&mut b);return *left+*right}",
        "fn apply<T:Copy,F:for<life a> Fn(&'a T)->&'a T>(f:&F,p:&T)->&T{return f(p)}fn result()->i64{let n:i64=42;let f=(p:&i64)=>p;return *apply(&f,&n)}",
        "fn apply<F:for<life a> Fn(&'a [i64])->&'a [i64]>(f:&F,p:&[i64])->&[i64]{return f(p)}fn result()->i64{let n:[i64;1]=[42];let f=(p:&[i64])=>p;let answer=apply(&f,&n[..]);return answer[0]}",
        PAIR,
        STORAGE,
        OWNED,
        PARENT,
        MULTI,
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{text}: {}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
}

#[test]
fn terminal342_unused_hrtb_definitions_cannot_invent_invocation_loans() {
    for text in [
        "fn bad<F:for<life a> FnMut(&'a mut i64)->&'a mut i64>(f:&mut F,p:&mut i64)->i64{let first=f(&mut *p);let second=f(&mut *p);return *first+*second}fn main(){}",
        "fn bad<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F)->&i64{let n:i64=0;return f(&n)}fn main(){}",
        "fn bad<F:for<life a> FnOnce(&'a i64)->i64>(f:F,p:&i64)->i64{let one=f(p);return one+f(p)}fn main(){}",
        "fn narrow<H:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'a i64>(h:&H,p:&i64,q:&i64)->&i64{return h(p,q)}fn bad<F:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'b i64>(f:&F,p:&i64,q:&i64)->&i64{return narrow(f,p,q)}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}
#[test]
fn terminal342_each_hrtb_role_checks_actual_callback_roots_on_raw_and_wire() {
    let module = typed(PAIR);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    for fault in [0, 1, 2] {
        let mut changed = module.clone();
        let roles = &mut changed
            .functions
            .iter_mut()
            .find(|f| {
                f.callable_contract
                    .as_ref()
                    .is_some_and(|c| c.roles.len() == 2)
            })
            .unwrap()
            .callable_contract
            .as_mut()
            .unwrap()
            .roles;
        assert_eq!(roles[0].closure, roles[1].closure);
        let life = roles[1].life.as_mut().unwrap();
        match fault {
            0 => {
                *life = zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 2,
                    parameter_lives: vec![Some(0), Some(1)],
                    result_life: Some(1),
                })
                .unwrap();
            }
            1 => {
                *life = zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 2,
                    parameter_lives: vec![Some(1), Some(1)],
                    result_life: Some(1),
                })
                .unwrap();
            }
            _ => {
                life.invocation_binder_count = u32::MAX;
            }
        }
        assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
        let mut fake = plan.clone();
        fake.module_fingerprint = changed.fingerprint();
        let raw = zil_verify::verify_module(
            changed.clone(),
            fake.clone(),
            &changed.target,
            &Limits::default(),
        )
        .unwrap_err();
        if fault == 0 {
            assert!(
                raw.iter().any(|d| d.code == "E-VERIFY-CALLABLE-LIFE"),
                "{raw:?}"
            );
        }
        if fault == 2 {
            assert!(
                raw.iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{raw:?}"
            );
            continue;
        }
        let wire =
            zil_ir::wire::encode_module_candidate(&changed, &fake, &Limits::default()).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default())
            .unwrap_err();
        assert!(
            decoded
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{decoded:?}"
        );
    }
}

#[test]
fn terminal342_fixed_capture_cannot_become_an_invocation_result() {
    let text = "fn apply<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn result()->i64{let n:i64=42;let outside:i64=7;let f=(p:&i64)=>{let note=outside;return p};return *apply(&f,&n)}";
    let module = typed(text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let mut changed = module.clone();
    let function = changed
        .functions
        .iter_mut()
        .find(|f| f.environment.is_some())
        .unwrap();
    let capture = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::ReadCapture { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    assert!(matches!(
        function.values[capture.0 as usize],
        zil_ir::Type::Address { .. }
    ));
    for block in &mut function.blocks {
        if matches!(block.terminator, zil_ir::Terminator::Return(_)) {
            block.terminator = zil_ir::Terminator::Return(Some(capture));
        }
    }
    assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
    let mut fake = plan;
    fake.module_fingerprint = changed.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&changed, &fake, &Limits::default()).unwrap();
    for errors in [
        zil_verify::verify_module(changed.clone(), fake, &changed.target, &Limits::default())
            .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-CALLABLE-LIFE"),
            "{errors:?}"
        );
    }
    let report = compile_source(
        &source(&text.replace("return p};", "return &outside};")),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

fn raw_reject(module: TypedModule, mut plan: zil_ir::CandidatePlan) {
    let analyze = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        analyze
            .iter()
            .all(|d| d.category == ResultCategory::SafeReject),
        "{analyze:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for error in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
            .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            error.iter().any(|d| d.code.starts_with("E-BORROW-")),
            "{error:?}"
        );
    }
}
#[test]
fn terminal342_hrtb_actual_mutable_arguments_and_returned_children_keep_windows() {
    for text in [MULTI, PARENT] {
        let module = typed(text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let mut changed = module.clone();
        let helper = changed
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap()
            .id;
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "result")
            .unwrap();
        let mut candidate = plan;
        if text == MULTI {
            let local = f.locals.iter().find(|l| l.name == "a").unwrap().id;
            let mut value = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match &i.kind {
                    InstructionKind::Call {
                        function,
                        arguments,
                    } if *function == helper => Some(arguments[2]),
                    _ => None,
                })
                .unwrap();
            loop {
                candidate.functions[f.id.0 as usize].holds[value.0 as usize].roots =
                    vec![zil_ir::HoldRoot::Local(local)];
                let def = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| i.result == Some(value))
                    .unwrap();
                match def.kind {
                    InstructionKind::BorrowLocalMut { .. } => {
                        def.kind = InstructionKind::BorrowLocalMut { local };
                        break;
                    }
                    InstructionKind::ReborrowAddress { address, .. } => value = address,
                    _ => panic!("unexpected prepared exclusive source: {:?}", def.kind),
                }
            }
        } else {
            let scalar = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    matches!(i.kind, InstructionKind::Constant(zil_ir::Constant::I64(40)))
                        .then_some(i.result)
                        .flatten()
                })
                .unwrap();
            let local = f.locals.iter().find(|l| l.name == "parent").unwrap().id;
            let n = f.locals.iter().find(|l| l.name == "n").unwrap().id;
            let parent = zil_ir::ValueId(f.values.len() as u32);
            f.values.push(f.locals[local.0 as usize].ty.clone());
            candidate.functions[f.id.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value: parent,
                    roots: vec![zil_ir::HoldRoot::Local(n)],
                });
            for block in &mut f.blocks {
                if let Some(at) = block.instructions.iter().position(
                    |i| matches!(i.kind,InstructionKind::Call{function,..} if function==helper),
                ) {
                    let span = block.instructions[at].span;
                    block.instructions.insert(
                        at + 1,
                        zil_ir::Instruction {
                            result: Some(parent),
                            kind: InstructionKind::Load(local),
                            span,
                        },
                    );
                    block.instructions.insert(
                        at + 2,
                        zil_ir::Instruction {
                            result: None,
                            kind: InstructionKind::WriteAddress {
                                address: parent,
                                value: scalar,
                            },
                            span,
                        },
                    );
                    break;
                }
            }
        }
        raw_reject(changed, candidate);
    }
}
#[test]
fn terminal342_hrtb_capture_paths_substitute_actual_storage_through_helpers() {
    assert_eq!(zil_base::SCHEMA_VERSION, 40);
    assert_eq!(zil_ir::wire::WIRE_VERSION, 40);
    for (valid, invalid) in [
        (
            STORAGE,
            STORAGE.replace("bytes_view(read)", "bytes_view(edit)"),
        ),
        (
            OWNED,
            OWNED.replace("let view=bytes_view(a)", "let view=bytes_view(b)"),
        ),
    ] {
        let module = typed(valid);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let mut changed = module.clone();
        let function = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "result")
            .unwrap();
        let resource = if valid == STORAGE {
            function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match &i.kind {
                    InstructionKind::MakeClosure { captures, .. } => captures.first().copied(),
                    _ => None,
                })
                .unwrap()
        } else {
            let b = function.locals.iter().find(|l| l.name == "b").unwrap().id;
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|i| match i.kind {
                    InstructionKind::ByteBufferReserve { buffer, .. } => {
                        let source = function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .find(|i| i.result == Some(buffer))
                            .unwrap();
                        matches!(source.kind,InstructionKind::DescribeLocal{local} if local==b)
                            .then_some(buffer)
                    }
                    _ => None,
                })
                .unwrap()
        };
        let view = {
            let borrow = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ByteBufferBorrow { .. }))
                .unwrap();
            if let InstructionKind::ByteBufferBorrow { buffer, .. } = &mut borrow.kind {
                *buffer = resource;
            }
            borrow.result.unwrap()
        };
        let b = function
            .locals
            .iter()
            .find(|l| l.name == if valid == STORAGE { "edit" } else { "b" })
            .unwrap()
            .id;
        let mut candidate = plan;
        candidate.functions[function.id.0 as usize].holds[view.0 as usize].roots =
            vec![zil_ir::HoldRoot::Local(b)];
        // The only ordinary propagated view Load in OWNED has the same scalar window.
        for row in &mut candidate.functions[function.id.0 as usize].holds {
            if matches!(function.values[row.value.0 as usize],zil_ir::Type::Address{ref inner,..} if matches!(inner.as_ref(),zil_ir::Type::Slice{..}))
            {
                row.roots = vec![zil_ir::HoldRoot::Local(b)];
            }
        }
        raw_reject(changed, candidate);
        let module = typed(&invalid);
        assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    }
}

#[test]
fn terminal342_original_higher_rank_probe_runs_unchanged() {
    let text = include_str!("../../recovery/pending/life-higher-rank.zil");
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(temp.0.join("original")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let mut child = Command::new(&report.artifacts["executable"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("original HRTB program timed out");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

fn hrtb_boundary<T: std::fmt::Debug + PartialEq>(
    phase: Phase,
    run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    let expected = run(high).unwrap();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match run(n) {
            Ok(value) => {
                assert_eq!(value, expected);
                high = n;
            }
            Err(error) => {
                assert!(
                    error
                        .iter()
                        .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
                    "{error:?}"
                );
                low = n;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(high).unwrap(), expected);
    high
}

#[test]
fn terminal342_hrtb_budget_is_one_fresh_phase_ledger() {
    let unit = source(STORAGE);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limit = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expand = hrtb_boundary(Phase::Expand, |n| {
        zil_expand::expand_with_map_limits(&unit, &syntax, &limit(n)).map(|m| m.core)
    });
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = hrtb_boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::default(), &limit(n))
    });
    let module = typed(STORAGE);
    let analyze = hrtb_boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = hrtb_boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|m| m.fingerprint())
    });
    println!(
        "342 HRTB captured-storage N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N identical"
    );
}

#[test]
fn terminal342_hrtb_module_cache_and_interfaces_keep_numeric_relations() {
    let api = "pub fn invoke<F:for<life a> for<life b> Fn(&'a i64,&'b i64)->&'a i64>(f:&F,p:&i64,q:&i64)->&i64{return f(p,q)}";
    let main = "import api;fn number()->i64{return 42}fn result()->i64{let n:i64=#(number());let other:i64=7;let f=(p:&i64,q:&i64)=>p;return *api.invoke(&f,&n,&other)}";
    let project = |api: &str, main: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let temp = Temp::new();
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(api, main), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    assert!(
        cold.to_json().contains(r#""fixed_binder_count":0,"invocation_binder_count":2,"bindings":[{"base":{"parameter":0},"path":[],"binder":{"invocation":0}},{"base":{"parameter":1},"path":[],"binder":{"invocation":1}},{"base":"result","path":[],"binder":{"invocation":0}}],"classes":[]"#),
        "{}",
        cold.to_json()
    );
    for report in [
        session.compile_project(&project(api, main), &options),
        zil_driver::compile_project(&project(api, main), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.scalar_interfaces, cold.scalar_interfaces);
    }
    let original = cold
        .scalar_interfaces
        .iter()
        .find(|i| i.module == ModuleId(1))
        .unwrap();
    for alpha in [
        api.replace("life a", "life x")
            .replace("life b", "life y")
            .replace("'a", "'x")
            .replace("'b", "'y"),
        api.replace("for<life a> for<life b>", "for<life b> for<life a>"),
        api.replace("for<life a>", "for<life unused> for<life a>"),
    ] {
        let report = zil_driver::compile_project(&project(&alpha, main), &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let current = report
            .scalar_interfaces
            .iter()
            .find(|i| i.module == ModuleId(1))
            .unwrap();
        assert_eq!(
            current.signature_fingerprint,
            original.signature_fingerprint
        );
        assert_eq!(current.contract_fingerprint, original.contract_fingerprint);
    }
    let wide = api
        .replace("for<life a> for<life b>", "for<life a>")
        .replace("'b", "'a");
    let report = zil_driver::compile_project(&project(&wide, main), &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let current = report
        .scalar_interfaces
        .iter()
        .find(|i| i.module == ModuleId(1))
        .unwrap();
    assert_ne!(
        current.signature_fingerprint,
        original.signature_fingerprint
    );
    assert_ne!(current.contract_fingerprint, original.contract_fingerprint);
    for (changed_api, changed_main) in [
        (api.replace("->&'a i64>", "->&'b i64>"), main.to_string()),
        (api.to_string(), main.replace("q:&i64)=>p", "q:&i64)=>q")),
    ] {
        for report in [
            session.compile_project(&project(&changed_api, &changed_main), &options),
            zil_driver::compile_project(&project(&changed_api, &changed_main), &options),
        ] {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        }
    }
}

#[cfg(unix)]
#[test]
fn terminal342_hrtb_owned_preparation_failure_cleans_without_calling_body() {
    use std::os::unix::fs::PermissionsExt;
    let base = "extern \"C\" fn byte_test_attempts()->i32;extern \"C\" fn byte_test_releases()->i32;extern \"C\" fn byte_test_live()->i32;enum Fault{Stop}package Packet{manages data:ByteBuffer}fn packet()->Fault!Packet{let data=bytes_new();if !bytes_reserve(data,13){return Fault.Stop}return Packet{data:give data}}fn consume(give b:ByteBuffer){}fn apply<F:for<life a> FnOnce(&'a i64)->&'a i64>(f:F,p:&i64,packet:Packet)->&i64{return f(p)}fn work(counter:&mut i32)->Fault!i64{let b=bytes_new();bytes_reserve(b,4);let f=move(p:&i64)=>{*counter+=1;consume(give b);return p};let n:i64=42;return *apply(f,&n,packet()?)}fn result()->i32{var calls=0;let answer=work(&mut calls);unsafe{return byte_test_attempts()*100+byte_test_releases()*10000+byte_test_live()*10000000+calls*1000000}}";
    let temp = Temp::new();
    let compiler = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".into());
    let shim =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/byte-buffer/096/fault-allocator.c");
    let wrapper = temp.0.join("fault-clang.py");
    fs::write(&wrapper,format!("#!/usr/bin/env python3\nimport os,sys\ncompiler={}\nextra=['-x','c',{},'-Wl,--wrap=malloc','-Wl,--wrap=realloc','-Wl,--wrap=free']\nos.execvp(compiler,[compiler]+sys.argv[1:]+extra)\n",zil_base::json_string(&compiler),zil_base::json_string(&shim.to_string_lossy()))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for (text, expected) in [
        (base.to_string(), 10200),
        (
            base.replace("bytes_reserve(data,13)", "bytes_reserve(data,2)"),
            1020200,
        ),
    ] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("fault")),
                        clang: Some(wrapper.clone()),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], expected);
            }
        }
    }
}

#[test]
fn terminal342_hrtb_deep_wide_pointee_relations_pay_each_actual_signature() {
    let fields = (0..32)
        .map(|i| format!("f{i}:i64"))
        .collect::<Vec<_>>()
        .join(",");
    let values = (0..32)
        .map(|i| format!("f{i}:0"))
        .collect::<Vec<_>>()
        .join(",");
    let mut deep = "i64".to_owned();
    let mut value = "0".to_owned();
    for _ in 0..8 {
        deep = format!("({deep},)");
        value = format!("({value},)");
    }
    let text = format!(
        "struct Payload{{{fields},tail:{deep}}}fn pair<F:for<life a> Fn(&'a Payload)->&'a Payload,G:for<life b> Fn(&'b Payload)->&'b Payload>(f:&F,g:&G,p:&Payload)->&Payload{{let child=f(p);return g(child)}}fn main(){{let n=Payload{{{values},tail:{value}}};let f=(p:&Payload)=>p;let p=pair(&f,&f,&n);let q=pair(&f,&f,&n)}}"
    );
    let unit = source(&text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limit = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let types = hrtb_boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::default(), &limit(n))
    });
    let module = typed(&text);
    let analyze = hrtb_boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = hrtb_boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|v| v.fingerprint())
    });
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &module.target, &limit(verify)).is_ok());
    let errors =
        zil_verify::verify_module_bytes(&wire, &module.target, &limit(verify - 1)).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );
    assert!(zil_verify::verify_module_bytes(&wire, &module.target, &limit(verify)).is_ok());
    let mut changed = module.clone();
    let role = &mut changed
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap()
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[1];
    role.life.as_mut().unwrap().bindings.remove(0);
    assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
    let mut fake = plan;
    fake.module_fingerprint = changed.fingerprint();
    assert!(
        zil_verify::verify_module(
            changed.clone(),
            fake.clone(),
            &changed.target,
            &Limits::default()
        )
        .is_err()
    );
    let wire = zil_ir::wire::encode_module_candidate(&changed, &fake, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err());
    let mut nonfinite = typed(STORAGE);
    for f in &mut nonfinite.functions {
        f.callable_contract = None;
    }
    let result = zil_ir::Type::Address {
        mutable: false,
        inner: Box::new(zil_ir::Type::Package {
            name: "NonFiniteResult".into(),
            fields: vec![(
                "data".into(),
                zil_ir::Type::Managed(Box::new(zil_ir::Type::ByteBuffer)),
            )],
        }),
    };
    let d = &mut nonfinite.closures[0];
    d.result = result.clone();
    nonfinite.functions[d.function.0 as usize].return_type = result;
    let errors = zil_analyze::analyze(&nonfinite, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("finite Copy or slice pointee")),
        "{errors:?}"
    );
    let mut fake = zil_analyze::analyze(&typed(STORAGE), &Limits::default()).unwrap();
    fake.module_fingerprint = nonfinite.fingerprint();
    let errors = zil_verify::verify_module(
        nonfinite.clone(),
        fake.clone(),
        &nonfinite.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-LENDING"),
        "{errors:?}"
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&nonfinite, &fake, &Limits::default()).unwrap();
    let errors =
        zil_verify::verify_module_bytes(&wire, &nonfinite.target, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-LENDING"),
        "{errors:?}"
    );
    println!(
        "342 HRTB deep-wide pointee N: types={types} analyze={analyze} verify={verify}; raw/Wire N-1 ResourceLimit and fresh N identical"
    );
}

#[test]
fn terminal342_hrtb_raw_mixed_headers_preflight_before_recursive_relation() {
    let unit = source(STORAGE);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let original = zil_expand::expand(&unit, &syntax).unwrap();
    for deep in [false, true] {
        let mut core = original.clone();
        let f = core
            .functions
            .iter_mut()
            .find(|f| !f.callable_parameters.is_empty())
            .unwrap();
        f.state_parameters = vec![zil_ir::CoreStateParameter {
            index: 0,
            bindings: vec![],
        }];
        let mut inner = zil_ir::Type::Managed(Box::new(zil_ir::Type::ByteBuffer));
        let limits = if deep {
            for _ in 0..32 {
                inner = zil_ir::Type::Tuple(vec![inner]);
            }
            Limits {
                max_call_depth: 16,
                ..Limits::default()
            }
        } else {
            let mut fields = vec![zil_ir::Type::I64; 256];
            fields.push(inner);
            inner = zil_ir::Type::Tuple(fields);
            Limits {
                max_analysis_steps: 128,
                ..Limits::default()
            }
        };
        f.callable_parameters[0].parameters[0] = zil_ir::Type::Address {
            mutable: false,
            inner: Box::new(inner),
        };
        let errors = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.phase == Phase::Types && d.category == ResultCategory::ResourceLimit),
            "deep={deep}: {errors:?}"
        );
    }
}

#[test]
fn terminal342_hrtb_project_identity_uses_remaining_type_budget() {
    let api = "pub fn invoke<F:for<life a> Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}";
    let main =
        "import api;fn main()->i64{let n:i64=42;let f=(p:&i64)=>p;return *api.invoke(&f,&n)}";
    let units = vec![
        SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
        SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
    ];
    let syntax = units
        .iter()
        .map(|u| zil_syntax::parse(u).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = vec![
        vec![zil_ir::CoreImport {
            alias: "api".into(),
            module: ModuleId(1),
            span: syntax[0].items[0].span,
        }],
        vec![],
    ];
    let expanded = zil_expand::expand_program_with_limits(
        &units,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    let program = zil_ir::CoreProgram {
        root: ModuleId(0),
        source_fingerprint: zil_base::Fingerprint::of(format!("{main}\n{api}").as_bytes()),
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    };
    let n = hrtb_boundary(Phase::Types, |n| {
        zil_types::check_program(
            &program,
            &TargetProfile::default(),
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    });
    let typed =
        zil_types::check_program(&program, &TargetProfile::default(), &Limits::default()).unwrap();
    assert!(typed.module.functions.iter().any(|f| {
        f.callable_contract
            .as_ref()
            .is_some_and(|c| c.roles.iter().any(|r| r.life.is_some()))
    }));
    let plan = zil_analyze::analyze(&typed.module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        typed.module.clone(),
        plan.clone(),
        &typed.module.target,
        &Limits::default(),
    )
    .unwrap();
    let bytes =
        zil_ir::wire::encode_module_candidate(&typed.module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&bytes, &typed.module.target, &Limits::default()).unwrap();
    println!("342 HRTB project identity Types N={n}; N-1 ResourceLimit and fresh N identical");
}

#[test]
fn terminal342_hrtb_inline_parameter_window_and_capture_field_stay_distinct() {
    let prefix = "struct Pair{left:i64;right:i64}fn apply<F:for<life a> Fn(&'a Pair)->&'a i64>(f:&F,p:&Pair)->&i64{return f(p)}";
    let direct = format!(
        "{prefix}fn result()->i64{{let pair=Pair{{left:42,right:7}};let f=(p:&Pair)=>&p.left;return *apply(&f,&pair)}}"
    );
    let captured = format!(
        "{prefix}fn result()->i64{{let pair=Pair{{left:42,right:7}};let outside=Pair{{left:7,right:9}};let f=(p:&Pair)=>{{let note=&outside.left;let seen=*note;return &p.left}};return *apply(&f,&pair)}}"
    );
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for text in [&direct, &captured] {
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("window")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
    let invalid = captured.replace("return &p.left", "return &outside.left");
    let report = compile_source(&source(&invalid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let module = typed(&captured);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut changed = module.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.environment.is_some())
        .unwrap();
    let held = &plan.functions[f.id.0 as usize].holds;
    let capture_field = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if matches!(i.kind, InstructionKind::AddressField { .. }) {
                i.result.filter(|id| {
                    held.iter().any(|h| {
                        h.value == *id
                            && h.roots
                                .iter()
                                .any(|r| matches!(r, zil_ir::HoldRoot::Capture(_)))
                    })
                })
            } else {
                None
            }
        })
        .expect("actual capture-field address is present");
    assert!(matches!(
        f.values[capture_field.0 as usize],
        zil_ir::Type::Address { .. }
    ));
    for b in &mut f.blocks {
        if matches!(b.terminator, zil_ir::Terminator::Return(_)) {
            b.terminator = zil_ir::Terminator::Return(Some(capture_field));
        }
    }
    assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
    let mut fake = plan;
    fake.module_fingerprint = changed.fingerprint();
    let errors = zil_verify::verify_module(
        changed.clone(),
        fake.clone(),
        &changed.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-CALLABLE-LIFE"),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&changed, &fake, &Limits::default()).unwrap();
    let errors =
        zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-CALLABLE-LIFE"),
        "{errors:?}"
    );
}

#[test]
fn terminal342_hrtb_closed_environment_bound_order_retains_relation() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    let mut relations = vec![];
    for bound in [
        "ClosedEnvironment + for<life a> Fn(&'a i64)->&'a i64",
        "for<life a> Fn(&'a i64)->&'a i64 + ClosedEnvironment",
    ] {
        let text = format!(
            "fn apply<F:{bound}>(f:F,p:&i64)->&i64{{return f(p)}}fn result()->i64{{let n:i64=42;let offset:i64=7;let f=move(p:&i64)=>{{let seen=offset;return p}};return *apply(f,&n)}}"
        );
        let module = typed(&text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        let relation = module
            .functions
            .iter()
            .find_map(|f| f.callable_contract.as_ref())
            .unwrap()
            .roles[0]
            .life
            .clone()
            .expect("selected actual Fn bound retains HRTB relation");
        assert_eq!(
            relation,
            zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                binder_count: 1,
                parameter_lives: vec![Some(0)],
                result_life: Some(0)
            })
            .unwrap()
        );
        relations.push(relation);
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend: *backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(temp.0.join("closed")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                execute(&report.artifacts["executable"], 42);
            }
        }
        let bad = text.replace(
            "let f=move(p:&i64)=>{let seen=offset;return p}",
            "let f=(p:&i64)=>{let seen=&offset;let value=*seen;return p}",
        );
        let report = compile_source(&source(&bad), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(report.diagnostics.iter().any(|d| {
            d.message == "retained callable environments cannot capture borrowed positions or descriptions"
        }), "{}", report.to_json());
    }
    assert_eq!(relations[0], relations[1]);
}
