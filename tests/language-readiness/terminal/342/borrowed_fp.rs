//! Fresh code-pointer relations retain current caller loans, never lexical names.
use super::*;
use zil_driver::{NativeBackend, Optimization};
use zil_ir::{Type, ValueId};
const HIGHER_RANK: &str = include_str!("higher_rank.zil");
const SELECT: &str = "fn first(p:&mut i64,q:&mut i64)->&mut i64{return p}fn result()->i64{let f:for<life a> for<life b> fn(&'a mut i64,&'b mut i64)->&'a mut i64=first;var x:i64=40;var y:i64=1;let p=f(&mut x,&mut y);y=2;*p=*p+y;return x}";
#[test]
fn terminal342_borrowed_pointer_invocation_retains_source_relation() {
    let temp = Temp::new();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for text in [
        HIGHER_RANK,
        SELECT,
        "type Handler=for<life a> fn(&'a mut i64)->&'a mut i64;fn selected()->Type{return Handler}fn id(p:&mut i64)->&mut i64{return p}fn result()->i64{let f:#(selected())=id;var x:i64=40;let p=f(&mut x);*p=*p+2;return x}",
        "import std.syntax as syntax;type Handler=for<life a> fn(&'a mut i64)->&'a mut i64;fn selected()->Syntax<Type>{return syntax.type_ref(Handler)}fn id(p:&mut i64)->&mut i64{return p}fn result()->i64{let f:#splice(selected())=id;var x:i64=40;let p=f(&mut x);*p=*p+2;return x}",
        "fn identity(p:&i64)->&i64{return p}fn result()->i64{let f:for<life a> fn(&'a i64)->&'a i64=identity;let x:i64=40;let y:i64=2;return *f(&x)+*f(&y)}",
        "fn tail(p:&mut [i64])->&mut [i64]{return &mut p[1..]}fn result()->i64{let f:for<life a> fn(&'a mut [i64])->&'a mut [i64]=tail;var x:[i64;3]=[7,20,20];let p=f(&mut x[..]);p[1]=22;return p[0]+p[1]}",
        "fn result()->i64{let f:for<life a> fn(&'a mut i64)->&'a mut i64=(p:&mut i64)=>p;var x:i64=40;let p=f(&mut x);*p=*p+2;return x}",
        "struct Handler{call:for<life a> fn(&'a mut i64)->&'a mut i64}fn id(p:&mut i64)->&mut i64{return p}fn make()->Handler{return Handler{call:id}}fn result()->i64{let h=make();let f=h.call;var x:i64=40;let p=f(&mut x);*p=*p+2;return x}",
        "fn id(p:&mut i64)->&mut i64{return p}fn factory()->for<life a> fn(&'a mut i64)->&'a mut i64{return id}fn result()->i64{let f=factory();var x:i64=40;let p=f(&mut x);*p=*p+2;return x}",
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
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(
                    llvm.contains("call ptr %v.") || llvm.contains("call { ptr, i64 } %v."),
                    "{llvm}"
                );
            }
        }
    }
}
#[test]
fn terminal342_borrowed_pointer_wrong_target_and_call_windows_reject() {
    for text in [
        SELECT.replace("return p}","return q}"),
        HIGHER_RANK.replace("*p=*p+2;", "first=2;*p=*p+2;"),
        "fn pair(p:&mut i64,q:&mut i64){}fn result()->i64{let f:for<life a> fn(&'a mut i64,&'a mut i64)=pair;var x:i64=1;f(&mut x,&mut x);return x}".into(),
        "fn id(p:&mut i64)->&mut i64{return p}fn result()->i64{let f:for<life a> fn(&'a mut i64)->&'a mut i64=id;var x:i64=40;let p=f(&mut x);let q=f(&mut x);*p=*q;return x}".into(),
        "fn id(p:&i64)->&i64{return p}fn result()->i64{let f:for<life a> fn(&'a mut i64)->&'a mut i64=id;return 0}".into(),
        "fn result()->i64{var x:i64=42;let f:for<life a> fn(&'a i64)->&'a i64=(p:&i64)=>&x;return *f(&x)}".into(),
    ] {
        let report=compile_source(&source(&text),&CompileOptions::default());
        assert_eq!(report.status,ResultCategory::SafeReject,"{text}: {}",report.to_json());
        assert!(report.artifacts.is_empty());
    }
    for text in [
        "fn main(){let x:for<life a> i64=1;}",
        "fn id(p:&i64)->&i64{return p}fn main(){let f:for<life a> fn(&'a i64)->&'b i64=id;}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn terminal342_borrowed_pointer_raw_target_relation_and_indirect_aliases_are_independent() {
    let module = typed(SELECT);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    let mut wrong_root = module.clone();
    let target = wrong_root
        .functions
        .iter_mut()
        .find(|f| f.name == "first")
        .unwrap();
    let q = target
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind,InstructionKind::TakeLocal{local} if local==target.parameters[1])
                .then_some(i.result)
                .flatten()
        });
    // Add a real load of the second input; source target intentionally never read it.
    let q = q.unwrap_or_else(|| {
        let value = ValueId(target.values.len() as u32);
        target
            .values
            .push(target.locals[target.parameters[1].0 as usize].ty.clone());
        target.blocks[0].instructions.push(zil_ir::Instruction {
            result: Some(value),
            kind: InstructionKind::TakeLocal {
                local: target.parameters[1],
            },
            span: target.span,
        });
        value
    });
    target.blocks[0].terminator = zil_ir::Terminator::Return(Some(q));
    let mut malformed = module.clone();
    for f in &mut malformed.functions {
        for ty in f
            .values
            .iter_mut()
            .chain(f.locals.iter_mut().map(|l| &mut l.ty))
        {
            if let Type::BorrowedFunctionPointer { life, .. } = ty {
                life.bindings
                    .iter_mut()
                    .find(|r| r.position.base == zil_ir::StorageBase::Result)
                    .unwrap()
                    .binder = zil_ir::StorageBinder::Invocation(99);
            }
        }
    }
    let mut overlap = module.clone();
    for f in &mut overlap.functions {
        for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::CallIndirect { arguments, .. } = &mut instruction.kind {
                arguments[1] = arguments[0];
            }
        }
    }
    for (changed, producer_rejects) in [(wrong_root, true), (malformed, true), (overlap, false)] {
        if producer_rejects {
            assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
            .unwrap();
        assert!(
            zil_verify::verify_module(
                changed.clone(),
                candidate,
                &changed.target,
                &Limits::default()
            )
            .is_err()
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err()
        );
    }
}
#[test]
fn terminal342_borrowed_pointer_phase_budgets_are_exact_and_fresh() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = n;
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{errors:?}"
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
    let unit = source(SELECT);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limit = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expand = boundary(Phase::Expand, |n| {
        zil_expand::expand_with_map_limits(&unit, &syntax, &limit(n)).map(|m| m.core)
    });
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::default(), &limit(n))
    });
    let module = typed(SELECT);
    let analyze = boundary(Phase::Analyze, |n| zil_analyze::analyze(&module, &limit(n)));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limit(n))
            .map(|v| v.fingerprint())
    });
    println!(
        "342 borrowed pointer N: expand={expand} types={types} analyze={analyze} verify={verify}; N-1 ResourceLimit, fresh N identical"
    );
}
#[test]
fn terminal342_borrowed_pointer_module_cache_rechecks_contracts_and_callers() {
    let temp = Temp::new();
    let api = "pub fn invoke(f:for<life a> for<life b> fn(&'a mut i64,&'b mut i64)->&'a mut i64,p:&mut i64,q:&mut i64)->&mut i64{return f(p,q)}";
    let main = "import api;fn first(p:&mut i64,q:&mut i64)->&mut i64{return p}fn constant()->i64{return 2}fn result()->i64{let f:for<life a> for<life b> fn(&'a mut i64,&'b mut i64)->&'a mut i64=first;var x:i64=40;var y:i64=1;let p=api.invoke(f,&mut x,&mut y);y=#(constant());*p=*p+y;return x}";
    let project = |api: &str, main: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(api, main), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    for warm in [
        session.compile_project(&project(api, main), &options),
        zil_driver::compile_project(&project(api, main), &options),
    ] {
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert!(warm.cache_hit);
        assert_eq!(warm.verification_runs, 1);
        assert_eq!(warm.scalar_interfaces, cold.scalar_interfaces);
    }
    let renamed = api.replace("life a", "life z").replace("'a", "'z");
    for warm in [
        session.compile_project(&project(&renamed, main), &options),
        zil_driver::compile_project(&project(&renamed, main), &options),
    ] {
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        for (actual, expected) in warm.scalar_interfaces.iter().zip(&cold.scalar_interfaces) {
            assert_eq!(actual.exports, expected.exports);
            assert_eq!(actual.signature_fingerprint, expected.signature_fingerprint);
            assert_eq!(actual.contract_fingerprint, expected.contract_fingerprint);
            assert_eq!(actual.result_origins, expected.result_origins);
        }
    }
    for (api, main) in [
        (api.replace("return f(p,q)", "return q"), main.to_owned()),
        (api.to_owned(), main.replace("*p=*p+y;", "x=0;*p=*p+y;")),
        (
            api.replace("->&'a mut i64,p:", "->&'b mut i64,p:"),
            main.to_owned(),
        ),
    ] {
        for report in [
            session.compile_project(&project(&api, &main), &options),
            zil_driver::compile_project(&project(&api, &main), &options),
        ] {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(report.artifacts.is_empty());
        }
    }
}

#[test]
fn terminal342_borrowed_pointer_binders_are_canonical_and_invocation_local() {
    let expected = typed(SELECT);
    let original = expected
        .functions
        .iter()
        .flat_map(|f| &f.values)
        .find(|ty| matches!(ty, Type::BorrowedFunctionPointer { .. }))
        .unwrap();
    for text in [
        SELECT.replace("for<life a> for<life b>", "for<life b> for<life a>"),
        SELECT.replace("life a", "life x").replace("'a", "'x"),
        SELECT.replace(
            "for<life a> for<life b>",
            "for<life unused> for<life a> for<life b>",
        ),
    ] {
        let changed = typed(&text);
        let pointer = changed
            .functions
            .iter()
            .flat_map(|f| &f.values)
            .find(|ty| matches!(ty, Type::BorrowedFunctionPointer { .. }))
            .unwrap();
        assert_eq!(pointer, original);
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    let vacuous = "fn id(p:i64)->i64{return p}fn result()->i64{let f:for<life unused> fn(i64)->i64=id;return f(42)}";
    assert_eq!(
        compile_source(&source(vacuous), &CompileOptions::default()).status,
        ResultCategory::Success
    );
    let plan = zil_analyze::analyze(&expected, &Limits::default()).unwrap();
    for (count, shift) in [(3, 0), (3, 1)] {
        let mut changed = expected.clone();
        for function in &mut changed.functions {
            for ty in function
                .values
                .iter_mut()
                .chain(function.locals.iter_mut().map(|l| &mut l.ty))
            {
                if let Type::BorrowedFunctionPointer { life, .. } = ty {
                    life.invocation_binder_count = count;
                    for row in &mut life.bindings {
                        let zil_ir::StorageBinder::Invocation(tag) = &mut row.binder else {
                            panic!("unexpected fixed binder")
                        };
                        *tag += shift;
                    }
                }
            }
        }
        assert!(zil_analyze::analyze(&changed, &Limits::default()).is_err());
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &Limits::default())
            .unwrap();
        assert!(
            zil_verify::verify_module(
                changed.clone(),
                candidate,
                &changed.target,
                &Limits::default()
            )
            .is_err()
        );
        assert!(
            zil_verify::verify_module_bytes(&wire, &changed.target, &Limits::default()).is_err()
        );
    }
}

#[test]
fn terminal342_borrowed_pointer_ct_wire_retains_dag_relation_with_bounded_metadata() {
    use zil_ir::ct_artifact::{CtScalarTy, CtTypeNode, CtTypeNodeId, CtTypeRef, CtValue};
    let value = CtValue::Type(CtTypeRef {
        nodes: vec![
            CtTypeNode::Primitive(CtScalarTy::I64),
            CtTypeNode::Address {
                mutable: true,
                inner: CtTypeNodeId(0),
            },
            CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(1), CtTypeNodeId(1)],
                result: CtTypeNodeId(1),
                life: zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 2,
                    parameter_lives: vec![Some(0), Some(1)],
                    result_life: Some(0),
                })
                .unwrap(),
            },
        ],
        root: CtTypeNodeId(2),
    });
    let bytes = zil_ir::ct_wire::encode_value(&value, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::ct_wire::decode_value(&bytes, &Limits::default()).unwrap(),
        value
    );
    let mut oversized = value;
    let CtValue::Type(ref mut ty) = oversized else {
        unreachable!()
    };
    let CtTypeNode::BorrowedFunctionPointer { life, .. } = &mut ty.nodes[2] else {
        unreachable!()
    };
    life.invocation_binder_count = u32::MAX;
    assert_eq!(
        zil_ir::ct_wire::encode_value(&oversized, &Limits::default()),
        Err(zil_ir::ct_wire::WireError::Limit)
    );
}
