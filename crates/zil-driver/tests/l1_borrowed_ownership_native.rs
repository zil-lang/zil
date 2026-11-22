//! End-to-end native execution and safety rejection tests for L1 Batch 4:
//! 1. Affine `&mut`-view `struct`s (`ValueClass::Affine` without cleanup debt).
//! 2. Exclusive reborrow of `&mut T` fields behind `&mut S` (avoiding `MoveFromBorrowed`).
//! 3. Non-`move` closure reborrow of `let target: &mut T` (avoiding `WriteThroughShared`).
//! 4. Soundness gate: `FnMut` exclusive capture reborrow cannot escape into outliving storage.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l1-b4-{name}-{}", std::process::id()));
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
fn affine_mut_view_struct_native_and_safety() {
    let dir = temp_dir("affine-mut-view");
    let pos_src = r#"
struct View<life a> {
    value: &'a mut i32
}

fn identity<T: Value>(value: T) -> T {
    return value
}

fn update<life a, life b>(first: &'a mut i32, second: &'b mut i32) {
    let pair = identity::<(View<'a>, View<'b>)>((
        View::<'a> { value: first },
        View::<'b> { value: second },
    ))
    var (left, right) = pair
    *left.value += 3
    *right.value += 5
}

fn main() -> i32 {
    var a: i32 = 20
    var b: i32 = 14
    update(&mut a, &mut b)
    return a + b
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // 23 + 19 = 42
    assert_eq!(code, 42);

    // Negative 1: using an affine `&mut`-view struct twice must be rejected (use-after-move)
    check_rejected(
        &dir,
        "neg_view_use_twice",
        r#"
struct View<life a> {
    value: &'a mut i32
}

fn take<life a>(v: View<'a>) -> i32 {
    return *v.value
}

fn main() -> i32 {
    var x: i32 = 10
    let v = View::<'_> { value: &mut x }
    let a = take(v)
    let b = take(v)
    return a + b
}
"#,
        "E-SAFETY",
    );

    // Negative 2: `struct` still cannot declare `manages` fields (must be `package`)
    check_rejected(
        &dir,
        "neg_struct_manages",
        r#"
struct Bad {
    manages buf: ByteBuffer
}

fn main() -> i32 {
    return 0
}
"#,
        "E-PARSE-MANAGED-FIELD",
    );
}

#[test]
fn reborrow_mut_field_behind_mut_ref_native_and_safety() {
    let dir = temp_dir("reborrow-mut-field");
    let pos_src = r#"
package Session<life a> {
    manages bytes: ByteBuffer
    pointer: &'a mut i32
}

fn make_session<life a>(pointer: &'a mut i32) -> Session<'a> {
    var bytes = bytes_new()
    bytes_reserve(bytes, 2)
    bytes_push(bytes, 65u8)
    return Session::<'a> { bytes: give bytes, pointer: pointer }
}

fn main() -> i32 {
    var n: i32 = 32
    var extra: i32 = 0
    if true {
        var s = make_session(&mut n)
        let sref = &mut s
        let p = sref.pointer
        *p += 10
        if bytes_len(s.bytes) == 1 {
            extra = 1
        }
    }
    return n + extra
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // 42 + 1 = 43
    assert_eq!(code, 43);

    // Negative 1: moving an OWNED `ByteBuffer` out of `&mut Session` must STILL be rejected
    check_rejected(
        &dir,
        "neg_move_owned_from_borrowed",
        r#"
package Session<life a> {
    manages bytes: ByteBuffer
    pointer: &'a mut i32
}

fn steal<life a>(session: &mut Session<'a>) -> ByteBuffer {
    return session.bytes
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: exclusively reborrowing `&mut i32` field through SHARED `&Session` must be rejected
    check_rejected(
        &dir,
        "neg_mut_reborrow_through_shared",
        r#"
package Session<life a> {
    manages bytes: ByteBuffer
    pointer: &'a mut i32
}

fn bad<life a>(session: &Session<'a>) {
    let p = session.pointer
    *p += 1
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );
}

#[test]
fn closure_reborrow_mut_capture_native_and_fnmut_escape_rejection() {
    let dir = temp_dir("closure-reborrow-mut");
    let pos_src = r#"
package Packet {
    manages data: ByteBuffer
    n: i32
}

fn main() -> i32 {
    var packet = Packet { data: bytes_new(), n: 10 }
    bytes_push(&mut packet.data, 1u8)
    let target = &mut packet
    var reset = () -> void => {
        var fresh = bytes_new()
        bytes_push(&mut fresh, 2u8)
        bytes_push(&mut fresh, 3u8)
        *target = Packet { data: fresh, n: 40 }
    }
    reset()
    let extra: i32 = if bytes_len(&packet.data) == 2 { 2 } else { 0 }
    return packet.n + extra
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // 40 + 2 = 42
    assert_eq!(code, 42);

    // Negative 1 (P0 soundness): `FnMut` capturing `&'a mut i32` by move cannot escape it into outliving `&mut Holder<'a>`
    check_rejected(
        &dir,
        "neg_fnmut_exclusive_capture_escape",
        r#"
struct Holder<life a> {
    pointer: &'a mut i32
}

fn hold<life a>(pointer: &'a mut i32) -> Holder<'a> {
    return Holder::<'a> { pointer: pointer }
}

fn make<life a>(holder: &mut Holder<'a>, other: &'a mut i32) -> impl FnMut() -> i32 {
    return move () => {
        holder.pointer = other
        return 0
    }
}

fn main() -> i32 {
    var a: i32 = 20
    var b: i32 = 22
    var holder = hold(&mut a)
    var f = make(&mut holder, &mut b)
    f()
    return f()
}
"#,
        "E-SAFETY",
    );

    // Negative 2: reading `packet.n` while `reset` (holding `&mut *target`) is still live and called later
    check_rejected(
        &dir,
        "neg_closure_reborrow_conflict",
        r#"
package Packet {
    manages data: ByteBuffer
    n: i32
}

fn main() -> i32 {
    var packet = Packet { data: bytes_new(), n: 10 }
    let target = &mut packet
    var reset = () -> void => {
        *target = Packet { data: bytes_new(), n: 40 }
    }
    let conflict = packet.n
    reset()
    return conflict
}
"#,
        "E-SAFETY",
    );
}
