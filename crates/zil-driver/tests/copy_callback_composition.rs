//! Copy data specialization preserves one actual callback environment and its obligations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
    json_string,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "review194-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, saved.is_some())
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
            .collect(),
    }
}
fn compile(
    case: &Case,
    files: &[(&str, &str)],
    optimization: Option<Optimization>,
) -> CompileReport {
    for (name, text) in files {
        fs::write(case.0.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
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
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, files: &[(&str, &str)], stdout: &str) {
    native_outcome(label, files, stdout, 0)
}
fn native_outcome(label: &str, files: &[(&str, &str)], stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, files, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let executable = &report.artifacts["executable"];
        let result = Command::new(executable).output().unwrap();
        fs::write(case.0.join("stdout.log"), &result.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &result.stderr).unwrap();
        fs::write(case.0.join("native.json"), format!(
            "{{\"command\":[{}],\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout\":{},\"expected_stdout\":{}}}\n",
            json_string(&executable.to_string_lossy()), result.status.code().map_or("null".into(), |n| n.to_string()),
            json_string(&String::from_utf8_lossy(&result.stdout)), json_string(stdout))).unwrap();
        assert_eq!(result.status.code(), Some(exit), "{label}: {result:?}");
        assert_eq!(result.stdout, stdout.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty(), "{label}: {result:?}");
    }
}

const MODULES: &[(&str, &str)] = &[
    (
        "main",
        "import first\nimport second\nimport api\ntype Alias=first.Item;fn main()->i32{let f=move()=>1;let a=api.keep(&f,first.Item{n:3});let b=api.keep(&f,second.Item{n:4});let again:Alias=api.keep(&f,first.Item{n:5});return i32(a.n+b.n+again.n)-12}",
    ),
    ("first", "pub struct Item{pub n:i64}"),
    ("second", "pub struct Item{pub n:i64}"),
    (
        "api",
        "pub fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}",
    ),
];

const COPY_TUPLE_IDENTITY: &str = "fn identity<T:Copy>(value:T)->T{let(x,_)=(value,());return x}fn result()->i32{return identity(42)}";

#[test]
fn generic_tuple_binding_original_definition_reaches_verify_and_native() {
    let case = Case::new("tuple-original321");
    let report = compile(&case, &[("main", COPY_TUPLE_IDENTITY)], None);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    native(
        "tuple-original321",
        &[(
            "main",
            &format!("{COPY_TUPLE_IDENTITY}fn main()->i32{{return result()-42}}"),
        )],
        "",
    );
    native(
        "tuple-symbolic-shape321",
        &[(
            "main",
            "fn keep<T:Copy>(value:T)->T{let(value,(other,_)):(T,(T,void))=(value,(value,()));var(rebound,)=(other,);rebound=value;let()=();return rebound}fn main()->i32{return keep(21)+keep(21)-42}",
        )],
        "",
    );
    native(
        "tuple-evaluate-once321",
        &[(
            "main",
            "fn tick(count:&mut i32){*count+=1}fn keep<T:Copy>(value:T,count:&mut i32)->T{let(x,_)=(value,tick(count));return x}fn main()->i32{var count=0;let value=keep(42,&mut count);return value+count-43}",
        )],
        "",
    );
    native(
        "tuple-cross-module-callable321",
        &[
            (
                "main",
                "import api\nfn main()->i32{let f=()=>1;let data:[i64;2]=[21,21];let result=api.keep(&f,data);return i32(result[0]+result[1])-42}",
            ),
            (
                "api",
                "pub fn keep<T:Copy,F:Fn()->i32>(f:&F,value:T)->T{let(x,_)=(value,f());return x}",
            ),
        ],
        "",
    );
}

#[test]
fn generic_tuple_binding_unused_definitions_keep_symbolic_capabilities() {
    for text in [
        "fn hidden<T:Copy>(value:T)->T{let(x,_)=(value,());return x}fn main(){}",
        "fn hidden<T:Copy>(value:T)->T{let(_, (x,_))=(value,(value,()));return x}fn main(){}",
        "fn hidden<T:Copy>(value:T)->T{var(x,_)=(value,());x=value;return x}fn main(){}",
        "fn zero<T:Copy>(value:T)->void{}fn hidden<T:Copy>(value:T)->T{let()=zero(value);return value}fn main(){}",
    ] {
        let case = Case::new("tuple-unused321");
        let report = compile(&case, &[("main", text)], None);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for (label, definition, message) in [
        (
            "short",
            "fn hidden<T:Copy>(value:T){let(x,)=(value,());}",
            "arity mismatch",
        ),
        (
            "long",
            "fn hidden<T:Copy>(value:T){let(x,y,z)=(value,());}",
            "arity mismatch",
        ),
        (
            "nested",
            "fn hidden<T:Copy>(value:T){let(x,(y,))=(value,(value,()));}",
            "arity mismatch",
        ),
        (
            "unit",
            "fn hidden<T:Copy>(value:T){let()=(value,());}",
            "arity mismatch",
        ),
        (
            "duplicate",
            "fn hidden<T:Copy>(value:T){let(x,(x,_))=(value,(value,()));}",
            "duplicate generic tuple binding",
        ),
        (
            "shape",
            "fn hidden<T:Copy>(value:T){let(x,y)=value;}",
            "requires tuple type before instantiation",
        ),
        (
            "noncopy-discard",
            "fn hidden<T:StoreElement>(value:T){let(_,_)=(value,());}",
            "Copy bound",
        ),
        (
            "callable-discard",
            "fn hidden<F:FnOnce()->i32>(f:F){let(_,_)=(f,());}",
            "Copy bound",
        ),
        (
            "address-discard",
            "fn hidden<T:Copy>(value:T,p:&i32){let(_,_)=(value,p);}",
            "cannot contain addresses",
        ),
        (
            "wrong-type",
            "fn hidden<T:Copy>(value:T){let(x,_):(T,bool)=(value,());}",
            "types must agree",
        ),
        (
            "let-write",
            "fn hidden<T:Copy>(value:T){let(x,_)=(value,());x=value;}",
            "writable place",
        ),
        (
            "concrete-result",
            "fn hidden<T:Copy>(value:T)->T{let(x,_)=(42,());return x}",
            "types must agree",
        ),
        (
            "numeric",
            "fn hidden<T:Copy>(value:T)->T{let(x,_)=(value,());return x+x}",
            "requires a declared bound",
        ),
        (
            "receiver-name",
            "fn hidden<T:Copy>(value:T){let(self,_)=(value,());}",
            "reserved for receiver binding",
        ),
        (
            "primitive-name",
            "fn hidden<T:Copy>(value:T){let(i32,_)=(value,());}",
            "reserved",
        ),
    ] {
        let case = Case::new(label);
        let report = compile(
            &case,
            &[("main", &format!("{definition}fn main(){{}}"))],
            None,
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::ResourceLimit,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn generic_tuple_binding_refutable_and_reference_patterns_stay_closed() {
    for pattern in ["(ref x,_)", "(ref mut x,_)", "(1,_)"] {
        let source = format!("fn hidden<T:Copy>(value:T){{let{pattern}=(value,());}}fn main(){{}}");
        let case = Case::new("tuple-pattern321");
        let report = compile(&case, &[("main", &source)], None);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("names, discards and tuple patterns")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_tuple_binding_symbolic_work_has_exact_fresh_budget() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "tuple-budget321.zil",
        COPY_TUPLE_IDENTITY,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let run = |n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|expanded| expanded.core)
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    let expected = run(high).unwrap();
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = middle;
            }
            Err(errors) => {
                assert!(!errors.is_empty());
                assert!(
                    errors
                        .iter()
                        .all(|d| d.phase == Phase::Expand
                            && d.category == ResultCategory::ResourceLimit),
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    let errors = run(high - 1).unwrap_err();
    assert!(!errors.is_empty());
    assert!(
        errors
            .iter()
            .all(|d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit),
        "{errors:?}"
    );
    assert_eq!(run(high).unwrap(), expected);
    println!("321 original Expand N={high}; N-1 ResourceLimit; fresh N Success");
}

fn modules_baseline() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let p = project(MODULES);
    let asts = p
        .modules
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = asts
        .iter()
        .map(|ast| {
            ast.items
                .iter()
                .filter_map(|item| {
                    if let zil_syntax::ast::ItemKind::Import(import) = &item.kind {
                        Some(zil_ir::CoreImport {
                            alias: import
                                .alias
                                .clone()
                                .unwrap_or_else(|| import.path.last().unwrap().clone()),
                            module: p.paths[&import.path],
                            span: item.span,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let expanded = zil_expand::expand_program_with_limits(
        &p.modules,
        &asts.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    let core = zil_ir::CoreProgram {
        root: p.root,
        source_fingerprint: Fingerprint::of(
            MODULES
                .iter()
                .flat_map(|(_, s)| s.bytes())
                .collect::<Vec<_>>()
                .as_slice(),
        ),
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    };
    let module = zil_types::check_program(&core, &TargetProfile::host_llvm(), &Limits::default())
        .unwrap()
        .module;
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let case = Case::new("cross-module-baseline");
    for (name, s) in MODULES {
        fs::write(case.0.join(format!("{name}.zil")), s).unwrap();
    }
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    (module, plan)
}

#[test]
fn equal_spelled_nominals_from_different_modules_keep_distinct_instances() {
    native("same-name-modules", MODULES, "");
    let (m, p) = modules_baseline();
    let instances = m
        .functions
        .iter()
        .filter(|f| f.callable_contract.is_some())
        .collect::<Vec<_>>();
    assert_eq!(
        instances.len(),
        2,
        "transparent alias must reuse first.Item"
    );
    let a = &instances[0].locals[instances[0].parameters[1].0 as usize].ty;
    let b = &instances[1].locals[instances[1].parameters[1].0 as usize].ty;
    assert_ne!(a, b);
    let (
        zil_ir::Type::Record {
            name: an,
            fields: af,
        },
        zil_ir::Type::Record {
            name: bn,
            fields: bf,
        },
    ) = (a, b)
    else {
        panic!("record instances required")
    };
    assert_ne!(an, bn);
    assert_eq!(af, bf, "equal layout is intentional");
    assert_eq!(
        instances[0].callable_contract.as_ref().unwrap().roles[0].closure,
        instances[1].callable_contract.as_ref().unwrap().roles[0].closure
    );
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    let (m2, p2) = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(m, m2);
    assert_eq!(p, p2);
}

#[test]
fn a_cross_module_wrong_instance_is_rejected_with_matching_candidate_identity() {
    let (mut m, mut p) = modules_baseline();
    let ids = m
        .functions
        .iter()
        .filter(|f| f.callable_contract.is_some())
        .map(|f| f.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    let mut changed = 0;
    for f in &mut m.functions {
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let zil_ir::InstructionKind::Call { function, .. } = &mut i.kind
                && *function == ids[1]
            {
                *function = ids[0];
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    let case = Case::new("wrong-cross-module-instance");
    // Resource analysis is an untrusted producer and need not reject a pure
    // Copy type mismatch; preserve its actual result and demand Verify rejection.
    let analyzed = zil_analyze::analyze(&m, &Limits::default());
    fs::write(case.0.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    p.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    fs::write(case.0.join("module.txt"), format!("{m:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{p:#?}")).unwrap();
    fs::write(case.0.join("candidate.wire"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(m.clone(), p, &m.target, &Limits::default()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ),
    ] {
        let errors = result.unwrap_err();
        fs::write(case.0.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == "E-VERIFY-CALL"),
            "{errors:#?}"
        );
    }
}

#[test]
fn slice_elements_use_copy_specialization_without_copying_exclusive_windows() {
    let prefix = "fn leaf<T:Copy,F:Fn(&mut [T],T)->T>(f:&F,s:&mut[T],x:T)->T{return f(s,x)}fn twice<T:Copy,F:Fn(&mut [T],T)->T>(f:&F,s:&mut[T],x:T)->T{let a=leaf(f,s,x);return leaf(f,s,a)}fn main()->i32{var values:[i64;2]=[1,2];let f=(s:&mut[i64],n:i64)=>{s[0]+=n;return s[0]};";
    native(
        "slice-and-copy",
        &[(
            "main",
            &format!("{prefix}let n=twice(&f,&mut values[..],i64(3));return i32(n+values[0])-16}}"),
        )],
        "",
    );
    let text = format!(
        "{prefix}let window=&mut values[..];let child=&mut window[0];let n=twice(&f,window,i64(3));return i32(n+*child)}}"
    );
    let case = Case::new("slice-child-remains-live");
    let r = compile(&case, &[("main", &text)], None);
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Verify);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        r.to_json()
    );
}

#[test]
fn finite_repeated_generic_record_preserves_copy_across_helper_boundaries() {
    native(
        "finite-repeated-nominal",
        &[(
            "main",
            "struct Nest<T>{value:T}type Deep=Nest<Nest<i64>>;fn dup<T:Copy>(x:T)->(T,T){return(x,x)}fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}fn main()->i32{let f=move()=>1;let original:Deep=Nest::<Nest<i64>>{value:Nest::<i64>{value:7}};let pair=dup(keep(&f,original));return i32(pair.0.value.value+pair.1.value.value)-14}",
        )],
        "",
    );
}

#[test]
fn copy_data_and_owned_mutable_callback_survive_continue_and_break() {
    let text = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{n:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.n)}}}
fn fold<T:Copy,F:FnMut(T)->T>(f:&mut F,initial:T,n:i32)->T{
    var value=initial;
    for index in 0..n{
        if index==1{continue}
        if index>=5{break}
        value=f(value)
    }
    return value
}
fn main()->i32{
    let ticket=Ticket{n:65};var calls=0;
    var f=move(x:[i64;2])=>{ticket.n;calls+=1;return [x[0]+x[1],x[1]+1]};
    let initial:[i64;2]=[0,1];
    let zero=fold(&mut f,initial,0);
    let result=fold(&mut f,initial,6);
    return i32(zero[0]+zero[1]+result[0]+result[1])-16
}
"#;
    native("copy-flow-control-owned-cleanup", &[("main", text)], "A");
}
