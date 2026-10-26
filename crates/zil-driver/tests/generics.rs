use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
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
fn generic_arity_constraints_and_bounds_fail_closed() {
    for text in [
        "fn id<T>(x:T)->T=>x\nfn result()->i32=>id::<i32,i64>(1)",
        "fn choose<T>(x:T,y:T)->T=>x\nfn result()->i32 {let x:i32=1;let y:i64=2;return choose(x,y)}",
        "fn id<T: Copy>(x:T)->T=>x\nfn result()->i32=>1",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
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
