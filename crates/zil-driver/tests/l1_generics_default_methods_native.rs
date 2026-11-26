//! End-to-end native execution and safety rejection tests for L1 Batch 2:
//! 1. Monomorphization of generic functions with reference-bearing type arguments
//!    (`FnSig.region_params` accounting across distinct lifetimes).
//! 2. Inherited default trait methods (`impl Trait for T {}` and partial overrides).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l1-b2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn build_and_run_dual(dir: &Path, entry: &Path) -> i32 {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    let mut expected_code = None;
    for (label, backend, opt) in configs {
        let exe = dir.join(format!("bin-{label}{}", std::env::consts::EXE_SUFFIX));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        if let Some(prev) = expected_code {
            assert_eq!(code, prev, "exit code mismatch on {label}");
        } else {
            expected_code = Some(code);
        }
    }
    expected_code.expect("at least one backend ran")
}

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "expected `{name}` to be rejected, but it succeeded"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic starting with `{expected_code_prefix}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn generic_monomorphization_with_reference_args_native_and_safety() {
    let dir = temp_dir("generic-ref-mono");
    let pos_src = r#"
fn identity<T: Value>(value: T) -> T {
    return value
}

fn swap_pair<A: Value, B: Value>(pair: (A, B)) -> (B, A) {
    let (a, b) = pair
    return (b, a)
}

fn combine<life a, life b>(first: &'a i32, second: &'b i32) -> i32 {
    let pair = identity::<(&'a i32, &'b i32)>((first, second))
    let (right, left) = swap_pair::<&'a i32, &'b i32>(pair)
    return *left + *right
}

fn bump<life a>(slot: &'a mut i32) -> i32 {
    let r = identity::<&'a mut i32>(slot)
    *r += 7
    return *r
}

fn main() -> i32 {
    var a: i32 = 15
    var b: i32 = 20
    let s = combine(&a, &b)
    let bumped = bump(&mut a)
    return s + bumped
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // s = 35, bumped = 22 => 57
    assert_eq!(code, 57);

    // Negative 1: escaping reference returned through generic identity
    check_rejected(
        &dir,
        "neg_escape_via_identity",
        r#"
fn identity<T: Value>(value: T) -> T {
    return value
}

fn bad<life a>() -> &'a i32 {
    let local: i32 = 9
    return identity::<&'a i32>(&local)
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: mutating referent while generic tuple-of-refs borrow is live
    check_rejected(
        &dir,
        "neg_alias_mut_tuple_ref",
        r#"
fn identity<T: Value>(value: T) -> T {
    return value
}

fn main() -> i32 {
    var x: i32 = 10
    var y: i32 = 20
    let pair = identity::<(&'_ i32, &'_ i32)>((&x, &y))
    x = 99
    let (left, right) = pair
    return *left + *right
}
"#,
        "E-SAFETY",
    );
}

#[test]
fn inherited_default_trait_methods_native_and_safety() {
    let dir = temp_dir("default-trait-methods");
    let pos_src = r#"
fn inc(x: i32) -> i32 {
    return x + 1
}

trait Factory {
    fn base(self: &Self) -> i32 {
        return 10
    }
    fn handler(self: &Self) -> fn(i32) -> i32 {
        return inc
    }
}

struct DefaultOnly {}
impl Factory for DefaultOnly {}

struct OverrideBase {}
impl Factory for OverrideBase {
    fn base(self: &Self) -> i32 {
        return 30
    }
}

fn main() -> i32 {
    let d = DefaultOnly {}
    let o = OverrideBase {}
    let f1 = d.handler()
    let f2 = o.handler()
    return f1(d.base()) + f2(o.base())
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // f1(10) = 11, f2(30) = 31 => 42
    assert_eq!(code, 42);

    // Negative 1: inherited default trait method returning dangling local borrow must be rejected
    check_rejected(
        &dir,
        "neg_default_method_dangling",
        r#"
trait BadTrait {
    fn leak<life a>(self: &Self) -> &'a i32 {
        let local: i32 = 5
        return &local
    }
}

struct S {}
impl BadTrait for S {}

fn main() -> i32 {
    let s = S {}
    let r = s.leak()
    return *r
}
"#,
        "E-",
    );

    // Negative 2: calling inherited `&mut Self` default method while shared borrow is active
    check_rejected(
        &dir,
        "neg_default_method_mut_alias",
        r#"
trait Mutator {
    fn touch(self: &mut Self) -> i32 {
        return 1
    }
}

struct Boxed {
    v: i32
}
impl Mutator for Boxed {}

fn main() -> i32 {
    var b = Boxed { v: 7 }
    let r = &b.v
    let _ = b.touch()
    return *r
}
"#,
        "E-SAFETY",
    );
}
