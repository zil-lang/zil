//! Actual static trait Source consumers, included by existing bounds target.
use super::*;
const CONCRETE: &str = include_str!("static-factory.zil");
const SYMBOLIC: &str = include_str!("symbolic-factory.zil");

#[test]
fn g242_original_static_and_symbolic_sources_reach_both_native_backends() {
    for text in [CONCRETE, SYMBOLIC] {
        let report = checked(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        native_value(&[("main", text)]);
    }
}

#[test]
fn g242_multiple_method_and_trait_actuals_and_ordered_arguments() {
    let text = "trait Measure<Tag:Copy>{fn score<A:Copy,B:Copy>(a:A,b:B,tag:Tag)->i32;}struct Cell{n:i32}struct Other{n:i32}impl Measure<i32> for Cell{fn score<X:Copy,Y:Copy>(a:X,b:Y,tag:i32)->i32{return tag+1}}impl Measure<i32> for Other{fn score<X:Copy,Y:Copy>(a:X,b:Y,tag:i32)->i32{return tag+2}}fn run<H:Measure<i32>>(x:i32)->i32{return H.score::<i32,bool>(3,true,x)}fn main()->i32{if Cell.score::<i32,bool>(3,true,7)!=8{return 1}if run::<Other>(8)!=10{return 2}return 0}";
    let r = checked(text);
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    native_value(&[("main", text)]);
}

#[test]
fn g242_cross_module_provider_and_generic_consumer() {
    let files = [
        (
            "main",
            "import model;import usecase;fn main()->i32{let value=usecase.build::<model.Cell>();return value.n-42}",
        ),
        ("api", "pub trait Factory{fn make<T:Copy>(value:T)->Self;}"),
        (
            "model",
            "import api;pub struct Cell{pub n:i32}impl api.Factory for Cell{pub fn make<T:Copy>(value:T)->Self{return Cell{n:42}}}",
        ),
        (
            "usecase",
            "import api;pub fn build<F:api.Factory>()->F{return F.make::<i32>(3)}",
        ),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    native_value(&files);
}

#[test]
fn g242_missing_bounds_and_unused_invalid_method_are_rejected() {
    assert_eq!(checked(CONCRETE).status, ResultCategory::Success);
    for text in [
        SYMBOLIC.replace("F:Factory", "F"),
        CONCRETE.replace("value:T)->Self{return Cell{n:7}", "value:T)->i32{return 7"),
    ] {
        let r = checked(&text);
        assert_eq!(
            r.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            r.to_json()
        );
        assert!(
            matches!(r.phase, Phase::Expand | Phase::Types),
            "{}",
            r.to_json()
        );
    }
}

fn native_value(files: &[(&str, &str)]) {
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "g242-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let exe = dir.join("program");
            let r = compile_project(
                &project(files),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(exe.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            assert_eq!(r.verification_runs, 1);
            let cg = r.native_codegen.as_ref().unwrap();
            assert_eq!(cg.backend, backend);
            assert!(cg.fallback.is_none());
            if backend == zil_driver::NativeBackend::LlvmApi {
                assert!(!cg.object_reports.is_empty());
                assert!(
                    cg.object_reports
                        .iter()
                        .all(|x| x.contains("\"input_kind\":\"builder\""))
                );
            }
            let run = Command::new(exe).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, b"0\n", "{run:?}");
            assert!(run.stderr.is_empty(), "{run:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

fn checked(text: &str) -> CompileReport {
    let path = std::path::PathBuf::from("/tmp/zil-g242-core")
        .join(format!("{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    compile_source(
        &source(text),
        &CompileOptions {
            emit_dir: Some(path),
            ..CompileOptions::default()
        },
    )
}

#[test]
fn g242_copy_bound_does_not_imply_numeric_capability() {
    let text = CONCRETE.replace("return Cell{n:7}", "return value+1");
    let r = checked(&text);
    assert_eq!(r.status, ResultCategory::Unsupported, "{}", r.to_json());
    assert_eq!(r.phase, Phase::Expand);
    assert!(r.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"
        && d.message == "operation on an unconstrained type parameter requires a declared bound"));
}
