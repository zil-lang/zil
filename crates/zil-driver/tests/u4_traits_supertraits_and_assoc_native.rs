//! End-to-end dual-backend native execution and safety rejection tests for Wave 2 Batch U4:
//! 1. Supertraits (`trait Sub: Super`, transitive chains, default supertrait methods,
//!    supertrait entailment, missing supertrait impl rejection, cyclic supertrait rejection).
//! 2. Trait generic methods (explicit & inferred method type args, owning types, default
//!    generic trait methods, method-level generic bound enforcement).
//! 3. Associated type bounds (`T: Producer<Item = i64>`) and trait-qualified associated
//!    type normalization when multiple traits share an associated type name.
//! 4. Blanket impl generic bounds & `where`-clause enforcement (`impl<T: Weight> Score for T`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-u4-{name}-{}", std::process::id()));
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
        let exe = dir.join(format!("bin-{label}"));
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
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected `{name}` to be SafeReject, got {:?} with diagnostics: {:?}",
        report.status,
        report.diagnostics
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
fn supertraits_transitive_dispatch_entailment_and_negatives() {
    let dir = temp_dir("supertraits");
    let pos_src = r#"
trait A {
    fn a_val(self: &Self) -> i32;
    fn a_default(self: &Self) -> i32 {
        return 5
    }
}

trait B: A {
    fn b_val(self: &Self) -> i32;
}

trait C: B {
    fn c_val(self: &Self) -> i32;
}

struct Item {
    x: i32,
}

impl C for Item {
    fn c_val(self: &Self) -> i32 {
        return self.x * 3
    }
}

impl B for Item {
    fn b_val(self: &Self) -> i32 {
        return self.x * 2
    }
}

impl A for Item {
    fn a_val(self: &Self) -> i32 {
        return self.x
    }
}

fn need_a<T: A>(v: &T) -> i32 {
    return v.a_val() + v.a_default()
}

fn use_c<T: C>(v: &T) -> i32 {
    let direct = v.a_val() + v.a_default() + v.b_val() + v.c_val()
    let entailed = need_a(v)
    return direct + entailed
}

fn main() -> i32 {
    let item = Item { x: 4 }
    // direct = 4 + 5 + 8 + 12 = 29; entailed = 4 + 5 = 9; total = 38
    return use_c(&item)
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    assert_eq!(build_and_run_dual(&dir, &entry), 38);

    // Negative 1: missing supertrait impl on `impl Sub for T`
    check_rejected(
        &dir,
        "neg_missing_supertrait",
        r#"
trait Super {
    fn base(self: &Self) -> i32;
}
trait Sub: Super {
    fn ext(self: &Self) -> i32;
}
struct S { v: i32 }
impl Sub for S {
    fn ext(self: &Self) -> i32 { return self.v }
}
fn main() -> i32 {
    let s = S { v: 1 }
    return s.ext()
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // Negative 2: cyclic supertrait hierarchy (`trait A: B; trait B: A`)
    check_rejected(
        &dir,
        "neg_cyclic_supertrait",
        r#"
trait A: B {
    fn fa(self: &Self) -> i32;
}
trait B: A {
    fn fb(self: &Self) -> i32;
}
fn main() -> i32 { return 0 }
"#,
        "E-GENERIC-ARGUMENT",
    );

    // Negative 3: reverse entailment (`T: A` does NOT entail `T: C`)
    check_rejected(
        &dir,
        "neg_reverse_entailment",
        r#"
trait A { fn a(self: &Self) -> i32; }
trait B: A { fn b(self: &Self) -> i32; }
fn need_b<T: B>(x: &T) -> i32 { return x.b() }
fn from_a<T: A>(x: &T) -> i32 { return need_b(x) }
fn main() -> i32 { return 0 }
"#,
        "E-GENERIC-ARGUMENT",
    );
}

#[test]
fn trait_generic_methods_inferred_explicit_and_owning_native() {
    let dir = temp_dir("trait-generic-methods");
    let pos_src = r#"
package OwnedBox {
    val: i32,
}

trait Mapper {
    fn pass<U>(self: &Self, x: U) -> U;
    fn tag_copy<U: Copy>(self: &Self, x: U) -> U {
        return x
    }
}

struct Identity {
    bias: i32,
}

impl Mapper for Identity {
    fn pass<U>(self: &Self, x: U) -> U {
        return x
    }
}

fn call_generic<M: Mapper>(m: &M, a: i32) -> i32 {
    let explicit = m.pass::<i32>(a)
    let inferred = m.tag_copy(11)
    let owned = OwnedBox { val: 19 }
    let moved = m.pass(owned)
    return explicit + inferred + moved.val
}

fn main() -> i32 {
    let id = Identity { bias: 1 }
    let direct_explicit = id.pass::<i32>(7)
    let direct_inferred = id.pass(5)
    let from_generic = call_generic(&id, 3)
    // direct_explicit(7) + direct_inferred(5) + from_generic(3 + 11 + 19 = 33) = 45
    return direct_explicit + direct_inferred + from_generic
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    assert_eq!(build_and_run_dual(&dir, &entry), 45);

    // Negative: passing non-Copy owning `package` to a trait generic method requiring `U: Copy`
    check_rejected(
        &dir,
        "neg_trait_method_copy_bound",
        r#"
package NonCopy {
    val: i32,
}
trait Copier {
    fn dup<U: Copy>(self: &Self, x: U) -> U;
}
struct S {}
impl Copier for S {
    fn dup<U: Copy>(self: &Self, x: U) -> U { return x }
}
fn main() -> i32 {
    let s = S {}
    let nc = NonCopy { val: 9 }
    let out = s.dup(nc)
    return out.val
}
"#,
        "E-GENERIC-ARGUMENT",
    );
}

#[test]
fn assoc_type_bounds_and_trait_qualified_normalization() {
    let dir = temp_dir("assoc-bounds");
    let pos_src = r#"
trait Producer {
    type Item;
    fn produce(self: &Self) -> Self.Item;
}

trait Flagged {
    type Item;
    fn flag(self: &Self) -> Self.Item;
}

struct Dual {
    n: i64,
    ok: bool,
}

impl Producer for Dual {
    type Item = i64;
    fn produce(self: &Self) -> Self.Item {
        return self.n
    }
}

impl Flagged for Dual {
    type Item = bool;
    fn flag(self: &Self) -> Self.Item {
        return self.ok
    }
}

fn read_i64<T: Producer<Item = i64>>(p: &T) -> i64 {
    return p.produce() + 10
}

fn forward_i64<T>(p: &T) -> i64 where T: Producer<Item = i64> {
    return read_i64(p)
}

fn main() -> i32 {
    let d = Dual { n: 21, ok: true }
    let v: i64 = forward_i64(&d)
    let b: bool = d.flag()
    if v == 31 && b {
        return 32
    }
    return 0
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    // forward_i64(&d) = 21 + 10 = 31; b is true => 32
    assert_eq!(build_and_run_dual(&dir, &entry), 32);

    // Negative: mismatched associated type (`Item = bool` passed to `Producer<Item = i64>`)
    check_rejected(
        &dir,
        "neg_assoc_type_mismatch",
        r#"
trait Producer {
    type Item;
    fn produce(self: &Self) -> Self.Item;
}
struct BoolProd { flag: bool }
impl Producer for BoolProd {
    type Item = bool;
    fn produce(self: &Self) -> Self.Item { return self.flag }
}
fn read_i64<T: Producer<Item = i64>>(p: &T) -> i64 {
    return p.produce()
}
fn main() -> i32 {
    let bp = BoolProd { flag: true }
    let _x: i64 = read_i64(&bp)
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );
}

#[test]
fn blanket_impl_bounds_and_where_clauses_native_and_safety() {
    let dir = temp_dir("blanket-bounds");
    let pos_src = r#"
trait Weight {
    fn weight(self: &Self) -> i32;
}

trait Score {
    fn score(self: &Self) -> i32;
}

trait Bonus {
    fn bonus(self: &Self) -> i32;
}

impl<T: Weight> Score for T {
    fn score(self: &Self) -> i32 {
        return self.weight() * 2
    }
}

impl<T> Bonus for T where T: Weight {
    fn bonus(self: &Self) -> i32 {
        return self.weight() + 5
    }
}

struct Rock {
    kg: i32,
}

impl Weight for Rock {
    fn weight(self: &Self) -> i32 {
        return self.kg
    }
}

fn need_score<X: Score>(x: &X) -> i32 {
    return x.score()
}

fn from_weight<X: Weight>(x: &X) -> i32 {
    return need_score(x) + x.bonus()
}

fn main() -> i32 {
    let r = Rock { kg: 9 }
    // direct score = 18; from_weight = 18 + (9 + 5) = 32; total = 50
    return r.score() + from_weight(&r)
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    assert_eq!(build_and_run_dual(&dir, &entry), 50);

    // Negative 1: calling blanket-implemented method on type that does NOT implement `Weight`
    check_rejected(
        &dir,
        "neg_blanket_missing_bound_method",
        r#"
trait Weight {
    fn weight(self: &Self) -> i32;
}
trait Score {
    fn score(self: &Self) -> i32;
}
impl<T: Weight> Score for T {
    fn score(self: &Self) -> i32 {
        return self.weight() * 2
    }
}
struct Feather {}
fn main() -> i32 {
    let f = Feather {}
    return f.score()
}
"#,
        "E-",
    );

    // Negative 2: passing type without `Weight` to `fn need_score<X: Score>`
    check_rejected(
        &dir,
        "neg_blanket_missing_bound_generic",
        r#"
trait Weight {
    fn weight(self: &Self) -> i32;
}
trait Score {
    fn score(self: &Self) -> i32;
}
impl<T: Weight> Score for T {
    fn score(self: &Self) -> i32 {
        return self.weight() * 2
    }
}
struct Feather {}
fn need_score<X: Score>(x: &X) -> i32 {
    return x.score()
}
fn main() -> i32 {
    let f = Feather {}
    return need_score(&f)
}
"#,
        "E-GENERIC-ARGUMENT",
    );
}
