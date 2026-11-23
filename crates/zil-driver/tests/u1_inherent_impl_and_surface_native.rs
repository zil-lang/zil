//! End-to-end dual-backend native execution and SafeReject tests for Wave 1 Batch U1:
//! - Inherent `impl` blocks (`struct`, `package`, `enum`, generic/bounded, primitive)
//! - Static associated function dispatch (concrete types, enums, `Self`, primitives, concrete & generic trait static methods)
//! - Pipelines (`left |> f`, `left |> g(a, b)`, closures, owning values)
//! - Record update syntax (`Point { ...base, x: 99 }`, owning fields, privacy, plain-struct restriction)
//! - Character literals (`'a'`, escapes, Unicode scalars, `match` patterns)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-u1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
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

fn check_rejected_path(entry: &Path, name: &str, expected_code_prefix: &str) {
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(entry, &opts);
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

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    check_rejected_path(&entry, name, expected_code_prefix);
}

#[test]
fn inherent_impl_struct_package_enum_generic_primitive_native_dual_backend() {
    let dir = temp_dir("inherent-impl-all");
    let src = r#"
struct Counter {
    val: i64
}

impl Counter {
    pub fn zero() -> Self {
        return Self { val: 0 }
    }

    pub fn from_val(v: i64) -> Self {
        return Self { val: Self.zero().val + v }
    }

    pub fn get(self: &Self) -> i64 {
        return self.val
    }

    pub fn add(self: &mut Self, delta: i64) {
        self.val = self.val + delta
    }

    pub fn consume(self: Self) -> i64 {
        return self.val
    }
}

package Packet {
    manages buf: ByteBuffer
    tag: i64
}

impl Packet {
    pub fn make(tag: i64) -> Self {
        var b = bytes_new()
        bytes_push(&mut b, 10u8)
        bytes_push(&mut b, 20u8)
        return Self { buf: b, tag: tag }
    }

    pub fn score(self: &Self) -> i64 {
        return self.tag + bytes_len(&self.buf)
    }

    pub fn push_byte(self: &mut Self, v: u8) {
        bytes_push(&mut self.buf, v)
    }
}

enum Status {
    Idle
    Active(i64)
}

impl Status {
    pub fn make(v: i64) -> Self {
        if v <= 0 {
            return Self.Idle
        }
        return Self.Active(v)
    }

    pub fn weight(self: &Self) -> i64 {
        match *self {
            Status.Idle() => { return 1 }
            Status.Active(n) => { return n }
        }
    }
}

trait Scorable {
    fn score(self: &Self) -> i64;
}

struct Weight {
    w: i64
}

impl Scorable for Weight {
    fn score(self: &Self) -> i64 {
        return self.w
    }
}

struct Pair<T> {
    first: T
    second: T
}

impl<T> Pair<T> {
    pub fn new(a: T, b: T) -> Self {
        return Self { first: a, second: b }
    }
}

impl<T: Scorable> Pair<T> {
    pub fn total_score(self: &Self) -> i64 {
        return self.first.score() + self.second.score()
    }
}

impl i64 {
    pub fn base_offset() -> Self {
        return 3
    }

    pub fn double_plus_base(self: Self) -> Self {
        return self * 2 + Self.base_offset()
    }
}

fn main() -> i32 {
    var c = Counter.from_val(10)
    c.add(5)
    let c_val = c.get()
    let c_done = c.consume()

    var pkt = Packet.make(4)
    pkt.push_byte(30u8)
    let pkt_score = pkt.score()

    let s1 = Status.make(0)
    let s2 = Status.make(7)
    let s_sum = s1.weight() + s2.weight()

    let p = Pair::<Weight>.new(Weight { w: 2 }, Weight { w: 5 })
    let p_score = p.total_score()

    let n: i64 = 4
    let prim = n.double_plus_base() + i64.base_offset()

    // c_val=15, c_done=15, pkt_score=7, s_sum=8, p_score=7, prim=14
    // total = 15 + 15 + 7 + 8 + 7 + 14 = 66
    return i32(c_val + c_done + pkt_score + s_sum + p_score + prim)
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 66);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn static_trait_assoc_dispatch_concrete_and_generic_native_dual_backend() {
    let dir = temp_dir("static-trait-assoc");
    let src = r#"
trait DefaultVal {
    type Output
    fn default_val() -> Self.Output;
}

impl DefaultVal for i32 {
    type Output = i32
    fn default_val() -> i32 {
        return 11
    }
}

struct Seed {
    x: i32
}

impl DefaultVal for Seed {
    type Output = i32
    fn default_val() -> i32 {
        return 19
    }
}

enum Choice {
    A
    B(i32)
}

impl DefaultVal for Choice {
    type Output = i32
    fn default_val() -> i32 {
        return 13
    }
}

fn read_default<T: DefaultVal>() -> T.Output {
    return T.default_val()
}

fn main() -> i32 {
    let a = i32.default_val()
    let b = Seed.default_val()
    let c = Choice.default_val()
    let d = read_default::<Seed>()
    // 11 + 19 + 13 + 19 = 62
    return a + b + c + d
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 62);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inherent_impl_visibility_and_duplicate_rejections() {
    let dir = temp_dir("visibility-and-dups");
    write_source(
        &dir,
        "vault.zil",
        r#"
pub struct Vault {
    pub count: i32
    secret: i32
}

impl Vault {
    pub fn open(v: i32) -> Self {
        return Self { count: v, secret: v * 2 }
    }

    fn hidden_new() -> Self {
        return Self { count: 0, secret: 0 }
    }

    pub fn read_secret(self: &Self) -> i32 {
        return self.secret
    }

    fn private_peek(self: &Self) -> i32 {
        return self.secret
    }
}
"#,
    );

    let ok_main = write_source(
        &dir,
        "main_ok.zil",
        r#"
import vault

fn main() -> i32 {
    let v = vault.Vault.open(21)
    return v.read_secret()
}
"#,
    );
    assert_eq!(build_and_run_dual(&dir, &ok_main), 42);

    let priv_static = write_source(
        &dir,
        "main_priv_static.zil",
        r#"
import vault

fn main() -> i32 {
    let v = vault.Vault.hidden_new()
    return v.count
}
"#,
    );
    check_rejected_path(&priv_static, "private-inherent-static", "E-VISIBILITY");

    let priv_instance = write_source(
        &dir,
        "main_priv_instance.zil",
        r#"
import vault

fn main() -> i32 {
    let v = vault.Vault.open(5)
    return v.private_peek()
}
"#,
    );
    check_rejected_path(&priv_instance, "private-inherent-instance", "E-VISIBILITY");

    check_rejected(
        &dir,
        "dup-same-impl",
        r#"
struct S { x: i32 }
impl S {
    fn f(self: &Self) -> i32 { return self.x }
    fn f(self: &Self) -> i32 { return self.x + 1 }
}
fn main() -> i32 { return 0 }
"#,
        "E-NAME-DUPLICATE",
    );

    check_rejected(
        &dir,
        "dup-across-impls",
        r#"
struct S { x: i32 }
impl S {
    fn f(self: &Self) -> i32 { return self.x }
}
impl S {
    fn f(self: &Self) -> i32 { return self.x + 1 }
}
fn main() -> i32 { return 0 }
"#,
        "E-NAME-DUPLICATE",
    );

    check_rejected(
        &dir,
        "dup-inline-and-impl",
        r#"
struct S {
    x: i32
    fn f(self: &Self) -> i32 { return self.x }
}
impl S {
    fn f(self: &Self) -> i32 { return self.x + 1 }
}
fn main() -> i32 { return 0 }
"#,
        "E-NAME-DUPLICATE",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pipelines_record_update_and_char_literals_native_dual_backend() {
    let dir = temp_dir("surface-ergonomics");
    let src = r#"
struct Point {
    x: i32
    y: i32
    z: i32
}

package OwnedBuf {
    manages buf: ByteBuffer
}

struct Holder {
    tag: i32
    pkt: OwnedBuf
}

fn inc(x: i32) -> i32 {
    return x + 1
}

fn add_mul(x: i32, a: i32, b: i32) -> i32 {
    return (x + a) * b
}

fn make_owned() -> OwnedBuf {
    var b = bytes_new()
    bytes_push(&mut b, 1u8)
    bytes_push(&mut b, 2u8)
    return OwnedBuf { buf: b }
}

fn push_and_len(pkt: OwnedBuf, v: u8) -> i32 {
    var p = pkt
    bytes_push(&mut p.buf, v)
    return i32(bytes_len(&p.buf))
}

fn classify_char(c: u32) -> i32 {
    match c {
        'a' => { return 10 }
        '\n' => { return 20 }
        '\u{0041}' => { return 30 }
        _ => { return 1 }
    }
}

fn main() -> i32 {
    // Pipelines: ((3 |> inc) |> add_mul(2, 3)) |> ((v) => v - 1) = ((4 + 2) * 3) - 1 = 17
    let p_val = 3 |> inc |> add_mul(2, 3) |> ((v: i32) => v - 1)
    // Owning value through pipeline: len = 3
    let owned_pipe = make_owned() |> push_and_len(9u8)

    // Record update (Copy struct)
    let p1 = Point { x: 1, y: 2, z: 3 }
    let p2 = Point { ...p1, x: 10 }
    let pt_sum = p1.x + p2.x + p2.y + p2.z // 1 + 10 + 2 + 3 = 16

    // Record update (struct with owning field moved to updated value)
    let h1 = Holder { tag: 5, pkt: make_owned() }
    let h2 = Holder { ...h1, tag: 9 }
    let h_sum = h2.tag + i32(bytes_len(&h2.pkt.buf)) // 9 + 2 = 11

    // Character literals & match patterns
    let c1 = classify_char('a')      // 10
    let c2 = classify_char('\n')     // 20
    let c3 = classify_char('\u{41}') // 30
    let c4 = if 'b' == 98u32 && '\0' == 0u32 { 2 } else { 0 }

    // 17 + 3 + 16 + 11 + 10 + 20 + 30 + 2 = 109
    return p_val + owned_pipe + pt_sum + h_sum + c1 + c2 + c3 + c4
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    assert_eq!(build_and_run_dual(&dir, &entry), 109);

    // Negative 1: cross-module record update copying a private field must be rejected.
    write_source(
        &dir,
        "secret_rec.zil",
        r#"
pub struct SecretPoint {
    pub x: i32
    y: i32
}

impl SecretPoint {
    pub fn make(x: i32, y: i32) -> Self {
        return Self { x: x, y: y }
    }
}
"#,
    );
    let priv_update = write_source(
        &dir,
        "priv_update.zil",
        r#"
import secret_rec

fn main() -> i32 {
    let p = secret_rec.SecretPoint.make(1, 2)
    let q = secret_rec.SecretPoint { ...p, x: 9 }
    return q.x
}
"#,
    );
    check_rejected_path(&priv_update, "private-field-record-update", "E-VISIBILITY");

    // Negative 2: using moved non-Copy base after record update must be rejected by ownership check.
    check_rejected(
        &dir,
        "use-after-record-update-move",
        r#"
package OwnedBuf {
    manages buf: ByteBuffer
}
struct Holder {
    tag: i32
    pkt: OwnedBuf
}
fn main() -> i32 {
    let p = OwnedBuf { buf: bytes_new() }
    let h1 = Holder { tag: 1, pkt: p }
    let h2 = Holder { ...h1, tag: 2 }
    return h1.tag + h2.tag
}
"#,
        "E-",
    );

    // Negative 3: record update on `package` is forbidden by design spec §15.5.
    check_rejected(
        &dir,
        "package-record-update",
        r#"
package Pkg {
    x: i32
    y: i32
}
fn main() -> i32 {
    let p = Pkg { x: 1, y: 2 }
    let q = Pkg { ...p, x: 3 }
    return q.x
}
"#,
        "E-RECORD-UPDATE",
    );

    let _ = std::fs::remove_dir_all(&dir);
}
