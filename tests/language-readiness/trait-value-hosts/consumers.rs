//! Ordinary scalar and enum impls use real nominal trait/provider identities.
use super::{NEXT, project};
use std::sync::atomic::Ordering;
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_project, compile_source,
};

const KEYS: &str = r#"
pub trait HashKey {
    fn hash(self:&Self)->i64;
    fn doubled(self:&Self)->i64{return self.hash()+self.hash()}
}
pub trait Equal {fn same(self:&Self,other:&Self)->bool;}
impl HashKey for i64 {fn hash(self:&Self)->i64{return *self}}
impl Equal for i64 {fn same(self:&Self,other:&Self)->bool{return *self==*other}}
impl HashKey for bool {fn hash(self:&Self)->i64{if *self{return 20}return 0}}
impl Equal for bool {fn same(self:&Self,other:&Self)->bool{return *self==*other}}
"#;
const QUERIES: &str = r#"
import keys;
pub fn score<T:keys.HashKey+keys.Equal>(a:&T,b:&T)->i64{
    if a.same(b){return a.doubled()}return 0
}
"#;
const MAIN: &str = r#"
import keys;import queries;
enum Color{Red,Blue(i64)}
impl keys.HashKey for Color {
    fn hash(self:&Self)->i64{
        match *self{Color.Red()=>{return 2},Color.Blue(value)=>{return value}}
    }
}
impl keys.Equal for Color {
    fn same(self:&Self,other:&Self)->bool{
        match *self{
            Color.Red()=>{match *other{Color.Red()=>{return true},Color.Blue(_)=>{return false}}},
            Color.Blue(value)=>{match *other{Color.Red()=>{return false},Color.Blue(second)=>{return value==second}}}
        }
    }
}
fn main()->i32{
    let number:i64=21;let flag=true;let color=Color.Blue(21);
    return i32(queries.score(&number,&number)+queries.score(&flag,&flag)+queries.score(&color,&color))
}
"#;

fn sources() -> zil_driver::ProjectSources {
    project(&[("main", MAIN), ("keys", KEYS), ("queries", QUERIES)])
}

#[test]
fn primitive_and_enum_impls_reach_generic_library_and_independent_verify() {
    let report = compile_project(
        &sources(),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

#[test]
fn ordinary_primitive_and_enum_impls_do_not_require_a_generic_caller() {
    for source in [
        "trait Read{fn read(self:&Self)->i64;}impl Read for i64{fn read(self:&Self)->i64{return *self}}fn main()->i32{let n:i64=42;return i32(n.read())}",
        "trait Identity{type Item:Copy;fn identity(self:&Self)->Self.Item;}impl Identity for i64{type Item=i64;fn identity(self:&Self)->i64{return *self}}fn read<T:Identity>(v:&T)->T.Item{return v.identity()}fn main()->i32{let n:i64=42;return i32(read(&n))}",
        "trait Cast<T:Copy>{fn value(self:&Self,witness:T)->T;}impl Cast<i32> for i64{fn value(self:&Self,witness:i32)->i32{return witness}}fn read<T:Cast<i32>>(v:&T)->i32{return v.value(42)}fn main()->i32{let n:i64=1;return read(&n)}",
        "trait Increment{fn advance(self:&mut Self)->i64;}impl Increment for i64{fn advance(self:&mut Self)->i64{*self+=1;return *self}}fn main()->i32{var n:i64=41;return i32(n.advance())}",
        "trait Value{fn number(self:Self)->i32;}impl Value for bool{fn number(self:Self)->i32{if self{return 42}return 0}}fn main()->i32{let flag=true;return flag.number()}",
        "enum Color{Red,Blue}trait Read{fn read(self:&Self)->i32;}impl Read for Color{fn read(self:&Self)->i32{match *self{Color.Red()=>{return 2},Color.Blue()=>{return 42}}}}fn main()->i32{let v=Color.Blue();return v.read()}",
    ] {
        let input = SourceUnit::new(FileId(0), ModuleId(0), "value-host.zil", source);
        let report = compile_source(
            &input,
            &CompileOptions {
                stop_after: Stage::Verify,
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
    }
}

#[test]
fn primitive_and_enum_impls_execute_default_and_explicit_methods() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-trait-value-hosts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let report = compile_project(
                &sources(),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
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
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let result = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                result.status.code(),
                Some(124),
                "{backend:?}/{optimization:?}: {result:?}"
            );
            assert!(result.stdout.is_empty());
            assert!(result.stderr.is_empty());
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}

#[test]
fn same_spelled_scalar_traits_keep_distinct_actual_providers() {
    let main = "import left;import right;fn main()->i32{let n:i64=21;return i32(left.read(&n)+right.read(&n))}";
    let left = "pub trait Key{fn hash(self:&Self)->i64;}impl Key for i64{fn hash(self:&Self)->i64{return *self}}pub fn read<T:Key>(v:&T)->i64{return v.hash()}";
    let right = left.replace("return *self", "return *self+100");
    let input = project(&[("main", main), ("left", left), ("right", &right)]);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-distinct-scalar-traits-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let report = compile_project(
                &input,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
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
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let actual = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                actual.status.code(),
                Some(142),
                "{backend:?}/{optimization:?}: {actual:?}"
            );
            assert!(actual.stdout.is_empty() && actual.stderr.is_empty());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    let wrong =
        "pub trait Key{fn hash(self:&Self)->i64;}pub fn read<T:Key>(v:&T)->i64{return v.hash()}";
    let report = compile_project(
        &project(&[("main", main), ("left", wrong), ("right", &right)]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn scalar_impls_keep_orphan_signature_duplicate_and_unused_definition_checks() {
    let api = "pub trait Key{fn hash(self:&Self)->i64;}";
    let orphan =
        "import api;impl api.Key for i64{fn hash(self:&Self)->i64{return *self}}fn main(){}";
    let report = compile_project(
        &project(&[("main", orphan), ("api", api)]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-TRAIT-OWNER"),
        "{}",
        report.to_json()
    );
    for program in [
        "trait Key{fn hash(self:&Self)->i64;}impl Key for i64{fn hash(self:&Self)->bool{return true}}fn main(){}",
        "trait Key{fn hash(self:&Self)->i64;}impl Key for i64{fn hash(self:&Self)->i64{return *self}}impl Key for i64{fn hash(self:&Self)->i64{return 1}}fn main(){}",
        "trait Key{fn hash(self:&Self)->i64;}impl Key for i64{fn hash(self:&Self)->i64{return *self}}fn unused<T:Key>(v:&T)->i64{return v.missing()}fn main(){}",
        "trait Increment{fn advance(self:&mut Self)->i64;}impl Increment for i64{fn advance(self:&mut Self)->i64{*self+=1;return *self}}fn main()->i32{var n:i64=41;let held=&mut n;n.advance();return i32(*held)}",
        "trait Key{fn hash(self:&Self)->i64;}trait Other{fn hash(self:&Self)->i64;}impl Other for bool{fn hash(self:&Self)->i64{return 1}}fn unused<T:Key>(v:&T)->i64{return v.hash()}fn main()->i32{let v=true;return i32(unused(&v))}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "wrong-value-host.zil", program);
        let report = compile_source(&source, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn every_ordinary_scalar_host_uses_its_complete_receiver_type() {
    for (host, value) in [
        ("void", "()"),
        ("bool", "true"),
        ("u8", "1u8"),
        ("u16", "1u16"),
        ("u64", "1u64"),
        ("usize", "1usize"),
        ("i32", "1"),
        ("i64", "1"),
        ("f32", "1.0f32"),
        ("f64", "1.0f64"),
    ] {
        let program = format!(
            "trait Read{{fn read(self:&Self)->i32;}}impl Read for {host}{{fn read(self:&Self)->i32{{return 42}}}}fn main()->i32{{let value:{host}={value};return value.read()}}"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "scalar-trait-receiver.zil", program);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{host}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn value_host_request_work_is_fresh_and_bounded() {
    let input = sources();
    let check = |work| {
        compile_project(
            &input,
            &CompileOptions {
                stop_after: Stage::Verify,
                limits: zil_base::Limits {
                    max_analysis_steps: work,
                    ..zil_base::Limits::default()
                },
                ..CompileOptions::default()
            },
        )
    };
    let (mut low, mut high) = (1, zil_base::Limits::default().max_analysis_steps);
    assert_eq!(check(high).status, ResultCategory::Success);
    while low < high {
        let mid = low + (high - low) / 2;
        let report = check(mid);
        if report.status == ResultCategory::Success {
            high = mid
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = mid + 1;
        }
    }
    for _ in 0..2 {
        let full = check(low);
        assert_eq!(full.status, ResultCategory::Success, "{}", full.to_json());
        assert_eq!(full.verification_runs, 1);
        let short = check(low - 1);
        assert_eq!(
            short.status,
            ResultCategory::ResourceLimit,
            "{}",
            short.to_json()
        );
        assert!(!full.cache_hit);
    }
    eprintln!("trait value hosts Source-to-Verify N={low}; N-1/fresh identical");
}

#[test]
fn scalar_impls_do_not_grant_copy_to_an_unused_symbolic_body() {
    let text = "trait Key{fn hash(self:&Self)->i64;}impl Key for i64{fn hash(self:&Self)->i64{return *self}}fn unused<T:Key>(v:&T)->T{return *v}fn main(){}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "symbolic-trait-copy.zil", text);
    let report = compile_source(&source, &CompileOptions::default());
    // This is the established symbolic-operation boundary: an available
    // concrete scalar impl cannot erase an unused definition's missing bound.
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.message
            == "operation on an unconstrained type parameter requires a declared bound")
    );
    let control = text.replace("<T:Key>", "<T:Key+Copy>");
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "symbolic-trait-copy-control.zil",
        control,
    );
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Verify,
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
}
