//! End-to-end native execution and safety rejection tests for L1 core composition:
//! 1. Explicit lifetime arguments on record/package construction (`View::<'a> { ... }`).
//! 2. `manages` resource fields inside tuple enum variants (`enum E { Own(manages ByteBuffer) }`).
//! 3. Reference patterns (`ref x` / `ref mut x`) in `match` and `let`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l1-comp-{name}-{}", std::process::id()));
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
fn explicit_lifetime_record_construction_native_and_safety() {
    let dir = temp_dir("lifetime-record");
    let pos_src = r#"
struct View<life a> {
    x: &'a i32
}

struct PairView<life a, T> {
    first: &'a T
    bonus: i32
}

fn read_view<life a>(v: View<'a>) -> i32 {
    return *v.x
}

fn make_pair<life a>(r: &'a i32, bonus: i32) -> PairView<'a, i32> {
    return PairView::<'a, i32> { first: r, bonus: bonus }
}

fn main() -> i32 {
    let base: i32 = 19
    let v = View::<'_> { x: &base }
    let p = make_pair(&base, 23)
    return read_view(v) + *p.first + p.bonus
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 61);

    // Negative 1: dangling View<'a> escaping local scope
    check_rejected(
        &dir,
        "neg_dangling",
        r#"
struct View<life a> {
    x: &'a i32
}

fn bad<life a>() -> View<'a> {
    let local: i32 = 7
    return View::<'a> { x: &local }
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: mutating base while View<'_> borrow is still live
    check_rejected(
        &dir,
        "neg_alias_mut",
        r#"
struct View<life a> {
    x: &'a i32
}

fn main() -> i32 {
    var base: i32 = 10
    let v = View::<'_> { x: &base }
    base = 20
    return *v.x
}
"#,
        "E-SAFETY",
    );

    // Negative 3: wrong number of explicit type arguments alongside lifetime argument
    check_rejected(
        &dir,
        "neg_wrong_type_arity",
        r#"
struct PairView<life a, T> {
    first: &'a T
    bonus: i32
}

fn main() -> i32 {
    let base: i32 = 1
    let _p = PairView::<'_, i32, i32> { first: &base, bonus: 2 }
    return 0
}
"#,
        "E-TYPE-ARGUMENT",
    );
}

#[test]
fn managed_enum_tuple_fields_native_and_safety() {
    let dir = temp_dir("managed-enum");
    let pos_src = r#"
enum Holder {
    Owned(manages ByteBuffer),
    Pair(manages ByteBuffer, manages ByteBuffer),
    Empty
}

fn inspect_and_consume(flag: bool) -> i32 {
    var h: Holder = Holder.Empty
    if flag {
        var buf: ByteBuffer = bytes_new()
        bytes_push(&mut buf, 11)
        bytes_push(&mut buf, 31)
        h = Holder.Owned(buf)
    }
    match h {
        Holder.Owned(b) => {
            if bytes_len(&b) == 2 {
                return 42
            }
            return 1
        },
        Holder.Pair(b1, _) => {
            if bytes_len(&b1) == 1 {
                return 10
            }
            return 2
        },
        Holder.Empty() => {
            return 7
        }
    }
}

fn partial_move_pair() -> i32 {
    var a: ByteBuffer = bytes_new()
    bytes_push(&mut a, 1)
    var b: ByteBuffer = bytes_new()
    bytes_push(&mut b, 2)
    bytes_push(&mut b, 3)
    let h: Holder = Holder.Pair(a, b)
    match h {
        Holder.Pair(first, _) => {
            if bytes_len(&first) == 1 {
                return 10
            }
            return 0
        },
        Holder.Owned(_) => { return 0 },
        Holder.Empty() => { return 0 }
    }
}

fn drop_without_match() -> i32 {
    var buf: ByteBuffer = bytes_new()
    bytes_push(&mut buf, 99)
    let _h: Holder = Holder.Owned(buf)
    return 5
}

fn main() -> i32 {
    let a = inspect_and_consume(true)
    let b = inspect_and_consume(false)
    let c = drop_without_match()
    let d = partial_move_pair()
    return a + b + c + d
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // a = 42, b = 7, c = 5, d = 10 => 64
    assert_eq!(code, 64);

    // Negative 1: `manages` on non-resource type
    check_rejected(
        &dir,
        "neg_non_resource",
        r#"
enum Bad {
    Num(manages i32)
}

fn main() -> i32 {
    return 0
}
"#,
        "E-RESOURCE-TYPE",
    );

    // Negative 2: use-after-move of managed enum payload
    check_rejected(
        &dir,
        "neg_use_after_move",
        r#"
enum Holder {
    Owned(manages ByteBuffer),
    Empty
}

fn main() -> i32 {
    let buf: ByteBuffer = bytes_new()
    let h: Holder = Holder.Owned(buf)
    let _moved: Holder = h
    match h {
        Holder.Owned(_) => { return 1 },
        Holder.Empty() => { return 0 }
    }
}
"#,
        "E-SAFETY",
    );
}

#[test]
fn reference_patterns_native_and_safety() {
    let dir = temp_dir("ref-patterns");
    let pos_src = r#"
enum Entry {
    Val(i32),
    Empty
}

enum BufBox {
    Has(manages ByteBuffer),
    None
}

fn mutate_entry(e: &mut Entry) -> i32 {
    match *e {
        Entry.Val(ref mut x) => {
            *x += 15
            return *x
        },
        Entry.Empty() => {
            return 0
        }
    }
}

fn read_buf_len(b: &BufBox) -> i32 {
    match *b {
        BufBox.Has(ref buf) => {
            if bytes_len(buf) == 3 {
                return 3
            }
            return 0
        },
        BufBox.None() => {
            return 0
        }
    }
}

fn main() -> i32 {
    // 1. `ref mut x` mutates payload in place without moving enum.
    var e: Entry = Entry.Val(10)
    let _ = mutate_entry(&mut e)
    var after_mut: i32 = 0
    match e {
        Entry.Val(ref x) => { after_mut = *x },
        Entry.Empty() => { after_mut = 0 }
    }

    // 2. `ref buf` borrows a managed ByteBuffer inside an enum without moving it.
    var buf: ByteBuffer = bytes_new()
    bytes_push(&mut buf, 1)
    bytes_push(&mut buf, 2)
    bytes_push(&mut buf, 3)
    let box_val: BufBox = BufBox.Has(buf)
    let first_len: i32 = read_buf_len(&box_val)
    let second_len: i32 = read_buf_len(&box_val)

    // 3. Irrefutable `let` destructuring with `ref` and `ref mut`.
    var pair: (i32, i32) = (4, 6)
    let (ref a, ref mut b) = pair
    *b += *a
    let sum_pair: i32 = pair.0 + pair.1

    return after_mut + first_len + second_len + sum_pair
}
"#;
    let entry = write_source(&dir, "pos.zil", pos_src);
    let code = build_and_run_dual(&dir, &entry);
    // after_mut = 25, first_len = 3, second_len = 3, sum_pair = 4 + 10 = 14 => 45
    assert_eq!(code, 45);

    // Negative 1: `ref mut` on immutable `&Entry` must be rejected
    check_rejected(
        &dir,
        "neg_ref_mut_on_immut_ref",
        r#"
enum Entry {
    Val(i32),
    Empty
}

fn bad(e: &Entry) -> i32 {
    match *e {
        Entry.Val(ref mut x) => {
            *x = 20
            return *x
        },
        Entry.Empty() => {
            return 0
        }
    }
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: mutating scrutinee while `ref x` borrow is active
    check_rejected(
        &dir,
        "neg_ref_alias_mut",
        r#"
enum Entry {
    Val(i32),
    Empty
}

fn main() -> i32 {
    var e: Entry = Entry.Val(10)
    match e {
        Entry.Val(ref x) => {
            e = Entry.Empty
            return *x
        },
        Entry.Empty() => {
            return 0
        }
    }
}
"#,
        "E-SAFETY",
    );

    // Negative 3: escaping `ref x` borrowing a local
    check_rejected(
        &dir,
        "neg_ref_escape",
        r#"
enum Entry {
    Val(i32),
    Empty
}

fn bad<life a>() -> &'a i32 {
    let e: Entry = Entry.Val(5)
    match e {
        Entry.Val(ref x) => { return x },
        Entry.Empty() => {
            let z: i32 = 0
            return &z
        }
    }
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );
}
