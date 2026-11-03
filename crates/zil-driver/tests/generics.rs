use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

#[test]
fn generic_enum_error_injection_preserves_native_error_and_success_arms() {
    use zil_driver::{NativeBackend, Optimization};
    for text in [
        include_str!("../../../tests/generic-error-enum/direct.zil"),
        include_str!("../../../tests/generic-error-enum/concrete.zil"),
        include_str!("../../../tests/generic-error-enum/helper.zil"),
        include_str!("../../../tests/generic-error-enum/annotated.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context-if.zil"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for (call, declaration) in [
        ("Fault.NoMemory", ""),
        ("fail()", "fn fail()->Fault!i32{return Fault.NoMemory}"),
        ("error", ""),
    ] {
        for (input, status, stdout) in [(0, 0, b"7".as_slice()), (1, 1, b"".as_slice())] {
            let binding = if call == "error" {
                "let error:Fault=Fault.NoMemory;"
            } else {
                ""
            };
            let text = format!(
                "extern \"C\" fn putchar(code:i32)->i32;enum Fault{{NoMemory}}{declaration}fn run<F:Fn()->i32>(f:F)->Fault!i32{{if f()!=0{{{binding}return {call}}}return 7}}fn main()->Fault!i32{{let f=()->i32=>{input};let value=run(f)?;unsafe{{putchar(48+value)}}return value-7}}"
            );
            for backend in &backends {
                for optimization in [Optimization::O0, Optimization::O2] {
                    let out = std::env::temp_dir().join(format!(
                        "zil-generic-enum-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    ));
                    fs::create_dir(&out).unwrap();
                    let report = compile_source(
                        &source(&text),
                        &CompileOptions {
                            backend: *backend,
                            optimization,
                            stop_after: Stage::Build,
                            output: Some(out.join("program")),
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.verification_runs, 1);
                    let codegen = report.native_codegen.as_ref().unwrap();
                    assert_eq!(codegen.backend, *backend);
                    assert!(codegen.fallback.is_none());
                    let actual = Command::new(&report.artifacts["executable"])
                        .output()
                        .unwrap();
                    assert_eq!(actual.status.code(), Some(status), "{text}: {actual:?}");
                    assert_eq!(actual.stdout, stdout, "{text}");
                    assert!(actual.stderr.is_empty(), "{actual:?}");
                    fs::remove_dir_all(out).unwrap();
                }
            }
        }
    }
    // Preserve the frozen inputs; expose their union return at the native
    // entry so the observed error tag depends on the generic constructor.
    for original in [
        include_str!("../../../tests/generic-error-enum/unique-context.zil"),
        include_str!("../../../tests/generic-error-enum/unique-context-if.zil"),
    ] {
        let main = "fn main()->i32{let f=()->i32=>1;run(f);return 0}";
        assert_eq!(original.matches(main).count(), 1);
        let text = original.replace(
            main,
            "fn main()->Choice<i32>!i32{let f=()->i32=>1;return run(f)}",
        );
        for backend in &backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = std::env::temp_dir().join(format!(
                    "zil-generic-enum-unique-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&out).unwrap();
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend: *backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(out.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, *backend);
                assert!(codegen.fallback.is_none());
                let actual = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(1), "{text}: {actual:?}");
                assert!(actual.stdout.is_empty(), "{actual:?}");
                assert!(actual.stderr.is_empty(), "{actual:?}");
                fs::remove_dir_all(out).unwrap();
            }
        }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn copy_record_bound_runtime_nested_and_phantom_arguments() {
    native(
        "struct Box<T:Copy>{value:T} struct Pair<T:Copy,U>{first:T;second:U} struct Tag<T:Copy>{n:i32} struct A{x:i32} struct B{x:i32} fn wrap<T:Copy>(value:T)->Box<T>{return Box::<T>{value}} fn result()->i32{let a=wrap(7);let b=Box::<Box<i32>>{value:a};let p=Pair::<i32,bool>{first:3,second:true};let left=Tag::<A>{n:2};let right=Tag::<B>{n:5};return b.value.value+p.first+left.n+right.n}",
        17,
    );
    for text in [
        "struct Box<T:Copy>{value:T} struct Outer<U:Copy>{inner:Box<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} struct Pair<T:Copy,U>{first:T;second:U} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn copy_record_bound_original_ct_source_reaches_native() {
    use zil_driver::Optimization;
    for text in [
        include_str!("../../../tests/language-readiness/pending/ct-type-generic-record.zil"),
        "struct Box<T:Copy>{value:T} fn select(x:Box<i32>)->Type{if x.value==7{return i32}return i64} fn main()->i32{let n:#(select(Box::<i32>{value:8}))=3000000000;return i32(n-3000000000)}",
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = std::env::temp_dir().join(format!(
                "zil-copy-record-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&out).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    output: Some(out.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let actual = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            assert!(actual.stdout.is_empty(), "{actual:?}");
            assert!(actual.stderr.is_empty(), "{actual:?}");
            fs::remove_dir_all(out).unwrap();
        }
    }
}

#[test]
fn copy_record_bound_unused_and_phantom_obligations_are_checked() {
    for text in [
        "struct Box<T:Copy>{value:T} struct Bad<U>{inner:Box<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} struct Bad<U>{inner:Phantom<U>} fn result()->i32{return 0}",
        "struct U{n:i32} struct Phantom<T:Copy>{n:i32} struct Bad<U>{inner:Phantom<U>} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn unused<U>(v:Phantom<U>)->i32{return 0} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn unused(v:Phantom<&i32>)->i32{return 0} fn result()->i32{return 0}",
        "struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<&i32>{n:1};return x.n}",
        "struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<[&i32;0]>{n:1};return x.n}",
        "resource R{n:i32} struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Phantom::<R>{n:1};return x.n}",
        "struct Box<T:Copy>{value:T} fn result()->i32{var n=1;let x=Box::<&mut i32>{value:&mut n};return n}",
        "struct Box<T:Copy>{value:T} struct Bad<T:Copy>{inner:Box<T,i32>} fn result()->i32{return 0}",
        "struct Box<T:Copy>{value:T} struct Bad<T:Copy>{inner:Box} fn result()->i32{return 0}",
        "struct Bad<T:Copy+Copy>{value:T} fn result()->i32{return 0}",
        "struct Bad<Copy,T:Copy>{n:i32} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            "E-GENERIC-ARGUMENT",
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    for text in [
        "package Box<T:Copy>{value:T} fn result()->i32{return 0}",
        "resource Box<T:Copy>{value:T} fn result()->i32{return 0}",
        "trait Mark{} struct Box<T:Mark>{value:T} fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn copy_record_bound_phantom_identity_is_nominal() {
    let report = compile_source(
        &source(
            "struct Phantom<T:Copy>{n:i32} fn read(x:Phantom<i32>)->i32{return x.n} fn result()->i32{return read(Phantom::<bool>{n:7})}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].code,
        "E-GENERIC-ARGUMENT",
        "{}",
        report.to_json()
    );
}

#[test]
fn copy_record_bound_expand_budget_is_exact_and_fresh() {
    for (label, text) in [
        (
            "bound",
            "struct Box<T:Copy>{value:T} struct Phantom<T:Copy>{n:i32} fn result()->i32{let x=Box::<Phantom<i32>>{value:Phantom::<i32>{n:7}};return x.value.n}",
        ),
        (
            "old-unbound",
            "struct Box<T>{value:T} fn result()->i32{let x=Box::<i32>{value:7};return x.value}",
        ),
    ] {
        let src = source(text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let check = |steps| {
            zil_expand::expand_with_map_limits(
                &src,
                &ast,
                &zil_base::Limits {
                    max_analysis_steps: steps,
                    ..zil_base::Limits::default()
                },
            )
        };
        let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
        assert!(check(high).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if check(middle).is_ok() {
                high = middle
            } else {
                low = middle + 1
            }
        }
        let failed = check(low - 1).unwrap_err();
        assert_eq!(failed[0].phase, Phase::Expand);
        assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
        assert!(check(low).is_ok());
        assert!(check(low).is_ok());
        eprintln!("320 {label} Expand exact work limit: {low}");
    }
}

#[test]
fn generic_error_union_templates_defer_potential_equality_to_actual_instances() {
    for (definition, call, phase, code) in [
        (
            "fn run<U:Copy>(w:U)->U!i32{let n:i32=7;return n}",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn run<U:Copy>(w:U)->U!i32=>7;",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn run<U:Copy>(w:U)->U!i32{let n:U!i32=7;return n}",
            "run(INPUT)",
            Phase::Types,
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "fn sink<T:Copy>(v:T!i32,w:T)->i32{return 0}fn run<U:Copy>(w:U)->i32{return sink(7,w)}",
            "run(INPUT)",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "fn sink<T:Copy>(w:T,v:T!i32)->i32{return 0}fn run<U:Copy>(w:U)->i32{return sink(w,7)}",
            "run(INPUT)",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        for text in [
            format!("{definition}fn main(){{}}"),
            format!(
                "{definition}fn main()->i32{{{};return 0}}",
                call.replace("INPUT", "true")
            ),
        ] {
            let report = compile_source(&source(&text), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
        }
        let text = format!(
            "{definition}fn main()->i32{{{};return 0}}",
            call.replace("INPUT", "0")
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
}

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generic.zil", text)
}
fn run(path: &Path, expected: i64) {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native generic program timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{result:?}");
    assert_eq!(result.stdout, format!("{expected}\n").as_bytes());
}
fn native(text: &str, expected: i64) -> String {
    let out = std::env::temp_dir().join(format!(
        "zil-generics-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&out).unwrap();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    run(&report.artifacts["executable"], expected);
    let optimized = out.join("optimized");
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    run(&optimized, expected);
    fs::remove_dir_all(&out).unwrap();
    llvm
}

#[test]
fn concrete_instantiations_inference_recursive_reuse_and_hygiene() {
    let llvm = native(
        "fn identity<T>(T: T)->T => T\nfn relay<T>(x:T,n:i32)->T { if n==0 {return x} return relay::<T>(x,n-1) }\nfn choose<T>(x:T,y:T,b:bool)->T {if b {return x} return y}\nfn result()->i64 {let x:i64=4000; let y:i64=23; let a=choose(x,y,true);return relay(identity(a),4)+identity::<i64>(23)}",
        4023,
    );
    assert!(llvm.contains("call i64"));
    // Repeated same-type calls share one emitted instance.
    let count = llvm
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("zil_fn_"))
        .count();
    assert!(count <= 6, "unbounded duplicate instances: {count}");
}
#[test]
fn bool_and_distinct_integer_instantiations() {
    native(
        "fn id<T>(x:T)->T=>x\nfn result()->i64 {let small=id::<i32>(7);let big=id::<i64>(2147483648);if id(true) && small==7 {return big} return 0}",
        2147483648,
    );
}
#[test]
fn generic_body_cannot_gain_operations_or_copy_from_an_instance() {
    for text in [
        "fn bad<T>(x:T)->T=>x+x\nfn result()->i32=>bad(2)",
        "fn bad<T>(x:T)->(T,T)=>(x,x)\nfn result()->i32=>1",
        "fn bad<T>(x:T)->T=>42\nfn result()->i32=>1",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn symbolic_result_and_nested_calls_do_not_collapse_distinct_parameters() {
    for text in [
        "fn bad<T,U>(x:T)->U=>x\nfn result()->i32=>bad::<i32,i32>(1)",
        "fn fixed(x:i32)->i32=>x\nfn bad<T>(x:T)->i32=>fixed(x)\nfn result()->i32=>bad(1)",
        "fn id<T>(x:T)->T=>x\nfn bad<T,U>(x:T)->U=>id::<U>(x)\nfn result()->i32=>bad::<i32,i32>(1)",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-GENERIC-ARGUMENT",
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn generic_arity_constraints_and_structural_copy_bound() {
    for (index, text) in [
        "fn id<T>(x:T)->T=>x\nfn result()->i32=>id::<i32,i64>(1)",
        "fn choose<T>(x:T,y:T)->T=>x\nfn result()->i32 {let x:i32=1;let y:i64=2;return choose(x,y)}",
        "fn id<T: Copy>(x:T)->T=>x\nfn result()->i32=>1",
    ]
    .into_iter()
    .enumerate()
    {
        let report = compile_source(&source(text), &CompileOptions::default());
        if index == 2 {
            // 194 admits this exact previously unused structural Copy bound.
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
            assert_eq!(report.verification_runs, 1);
            continue;
        }
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn generic_instance_growth_is_bounded_before_type_building() {
    let source = source("fn id<T>(x:T)->T=>x\nfn result()->i32 {id::<i64>(2);return id::<i32>(1)}");
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let limits = zil_base::Limits {
        max_functions: 1,
        ..zil_base::Limits::default()
    };
    let error = zil_expand::expand_with_map_limits(&source, &parsed, &limits).unwrap_err();
    assert_eq!(error[0].phase, Phase::Expand);
    assert_eq!(error[0].code, "E-RESOURCE-LIMIT");
}
