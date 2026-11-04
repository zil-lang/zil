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
                life.binder_count = 2;
                life.parameter_lives = vec![Some(0), Some(1)];
                life.result_life = Some(1);
            }
            1 => {
                life.binder_count = 2;
                life.parameter_lives = vec![Some(1), Some(1)];
                life.result_life = Some(1);
            }
            _ => {
                life.binder_count = u32::MAX;
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
        if text == MULTI {
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if let InstructionKind::Call {
                    function,
                    arguments,
                } = &mut i.kind
                    && *function == helper
                {
                    arguments[2] = arguments[1];
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
            let parent = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match i.kind {
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: true,
                    } => Some(address),
                    _ => None,
                })
                .unwrap();
            for block in &mut f.blocks {
                if let Some(at) = block.instructions.iter().position(
                    |i| matches!(i.kind,InstructionKind::Call{function,..} if function==helper),
                ) {
                    block.instructions.insert(
                        at + 1,
                        zil_ir::Instruction {
                            result: None,
                            kind: InstructionKind::WriteAddress {
                                address: parent,
                                value: scalar,
                            },
                            span: block.instructions[at].span,
                        },
                    );
                    break;
                }
            }
        }
        raw_reject(changed, plan);
    }
}
#[test]
fn terminal342_hrtb_capture_paths_substitute_actual_storage_through_helpers() {
    assert_eq!(zil_base::SCHEMA_VERSION, 36);
    assert_eq!(zil_ir::wire::WIRE_VERSION, 36);
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
