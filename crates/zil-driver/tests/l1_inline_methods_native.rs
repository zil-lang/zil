//! End-to-end native execution, visibility, cleanup, and safety rejection tests
//! for L1 Batch 3 (`struct`/`package` inline methods).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l1-b3-{name}-{}", std::process::id()));
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
fn inline_struct_and_package_methods_native_dual_backend() {
    let dir = temp_dir("struct-package-methods");
    let pos_src = r#"
struct Counter {
    val: i32

    fn new(start: i32) -> Self {
        return Self { val: start }
    }

    fn get(self: &Self) -> i32 {
        return self.val
    }

    fn add(self: &mut Self, delta: i32) {
        self.val += delta
    }

    fn into_doubled(self: Self) -> i32 {
        return self.val * 2
    }
}

struct GenericHost<T: Copy, #N: i32, life a> {
    item: T

    fn identity<T: Copy, #N: i32, life a>(value: T) -> T {
        return value
    }

    fn number<T: Copy, #N: i32, life a>() -> i32 {
        return N
    }

    fn read(self: &Self) -> T {
        return self.item
    }
}

package BufHolder {
    manages data: ByteBuffer
    tag: i32

    fn create(tag: i32) -> Self {
        return Self { data: bytes_new(), tag: tag }
    }

    fn push_byte(self: &mut Self, b: u8) {
        bytes_push(&mut self.data, b)
    }

    fn check_len(self: &Self, expected: i64) -> i32 {
        if bytes_len(&self.data) == expected {
            return self.tag
        }
        return 0
    }

    fn consume_and_tag(self: Self) -> i32 {
        if bytes_len(&self.data) == 2 {
            return self.tag + 1
        }
        return 0
    }
}

fn main() -> i32 {
    // 1. Struct static + &self + &mut self + by-value self
    var c = Counter.new(10)
    c.add(5)
    let g = c.get()
    let d = c.into_doubled()

    // 2. Generic host + method generic/const/life shadowing
    let flag: bool = GenericHost::<i32, 7, '_>.identity::<bool, 11, '_>(true)
    let shadowed_n: i32 = GenericHost::<i32, 7, '_>.number::<bool, 11, '_>()
    let h = GenericHost::<i32, 7, '_> { item: 3 }
    let host_item = h.read()

    // 3. Managed package inline methods + deterministic cleanup
    var holder = BufHolder.create(4)
    holder.push_byte(1)
    holder.push_byte(2)
    let t1 = holder.check_len(2)
    let t2 = holder.consume_and_tag()

    if !flag {
        return 1
    }
    // g=15, d=30, shadowed_n=11, host_item=3, t1=4, t2=5 => 68
    return g + d + shadowed_n + host_item + t1 + t2
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 68);
}

#[test]
fn inline_methods_cross_module_visibility_native_and_reject() {
    let dir = temp_dir("cross-mod-visibility");
    write_source(
        &dir,
        "helper.zil",
        r#"
pub struct Vault {
    v: i32

    pub fn open(v: i32) -> Self {
        return Self { v: v }
    }

    pub fn read(self: &Self) -> i32 {
        return self.v
    }

    fn secret(self: &Self) -> i32 {
        return self.v + 100
    }

    fn secret_static() -> i32 {
        return 99
    }
}
"#,
    );
    let main_ok = write_source(
        &dir,
        "main.zil",
        r#"
import helper as helper;

fn main() -> i32 {
    let v = helper.Vault.open(42)
    return v.read()
}
"#,
    );
    let code = build_and_run_dual(&dir, &main_ok);
    assert_eq!(code, 42);

    let main_bad_inst = write_source(
        &dir,
        "bad_inst.zil",
        r#"
import helper as helper;

fn main() -> i32 {
    let v = helper.Vault.open(10)
    return v.secret()
}
"#,
    );
    check_rejected_path(&main_bad_inst, "private_instance_method", "E-VISIBILITY");

    let main_bad_static = write_source(
        &dir,
        "bad_static.zil",
        r#"
import helper as helper;

fn main() -> i32 {
    return helper.Vault.secret_static()
}
"#,
    );
    check_rejected_path(&main_bad_static, "private_static_method", "E-VISIBILITY");
}

#[test]
fn inline_methods_safety_and_receiver_rejections() {
    let dir = temp_dir("safety-rejects");

    // 1. Calling `&mut Self` method on immutable local
    check_rejected(
        &dir,
        "neg_mut_on_immut",
        r#"
struct Counter {
    v: i32
    fn inc(self: &mut Self) {
        self.v += 1
    }
}

fn main() -> i32 {
    let c = Counter { v: 1 }
    c.inc()
    return c.v
}
"#,
        "E-ASSIGN-IMMUTABLE",
    );

    // 2. Calling `&mut Self` method while shared borrow is active
    check_rejected(
        &dir,
        "neg_mut_alias",
        r#"
struct Counter {
    v: i32
    fn inc(self: &mut Self) {
        self.v += 1
    }
}

fn main() -> i32 {
    var c = Counter { v: 1 }
    let r = &c.v
    c.inc()
    return *r
}
"#,
        "E-SAFETY",
    );

    // 3. Calling by-value `self: Self` method twice on a non-Copy managed package (use-after-move)
    check_rejected(
        &dir,
        "neg_use_after_move_self",
        r#"
package Holder {
    manages data: ByteBuffer
    fn consume(self: Self) -> i64 {
        return bytes_len(&self.data)
    }
}

fn main() -> i32 {
    let h = Holder { data: bytes_new() }
    let _a = h.consume()
    let _b = h.consume()
    return 0
}
"#,
        "E-SAFETY",
    );

    // 4. Returning dangling local borrow from an inline method
    check_rejected(
        &dir,
        "neg_dangling_method_return",
        r#"
struct S {
    fn bad<life a>(self: &Self) -> &'a i32 {
        let local: i32 = 7
        return &local
    }
}

fn main() -> i32 {
    let s = S {}
    let r = s.bad()
    return *r
}
"#,
        "E-SAFETY",
    );

    // 5. Calling a static associated method on an instance receiver
    check_rejected(
        &dir,
        "neg_static_on_instance",
        r#"
struct S {
    fn make() -> Self {
        return Self {}
    }
}

fn main() -> i32 {
    let s = S {}
    let _ = s.make()
    return 0
}
"#,
        "E-CALL-ARITY",
    );

    // 6. Duplicate inline method names on the same aggregate
    check_rejected(
        &dir,
        "neg_duplicate_method",
        r#"
struct S {
    fn dup(self: &Self) -> i32 { return 1 }
    fn dup(self: &Self) -> i32 { return 2 }
}

fn main() -> i32 {
    return 0
}
"#,
        "E-NAME-DUPLICATE",
    );
}
