//! Dual-backend (`clang-o0`, `clang-o2`, and `llvmapi-o0`/`llvmapi-o2` when enabled)
//! native execution tests for extended `std.fs` directory/metadata/rename APIs
//! and the canonical embedded `std.path` module.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("zil-stdlib-fs-path-{name}-{}", std::process::id()));
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

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual(dir: &Path, tag: &str, entry: &Path) -> (i32, Vec<u8>, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{tag}-{label}{}", std::env::consts::EXE_SUFFIX));
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));

        let code = out.status.code().unwrap_or(-1);
        if let Some((prev_code, ref prev_stdout, ref prev_stderr)) = expected {
            assert_eq!(
                code,
                prev_code,
                "exit code mismatch for {} on {label}",
                entry.display()
            );
            assert_eq!(
                out.stdout,
                *prev_stdout,
                "stdout mismatch for {} on {label}",
                entry.display()
            );
            assert_eq!(
                out.stderr,
                *prev_stderr,
                "stderr mismatch for {} on {label}",
                entry.display()
            );
        } else {
            expected = Some((code, out.stdout, out.stderr));
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn std_fs_dir_metadata_rename_and_read_dir_execute_natively() {
    let dir = temp_dir("fs-dir-meta");
    let work_root = dir.join("work");
    let work_root_str = work_root.to_str().expect("utf8 path").replace('\\', "/");

    let long_name = if cfg!(target_os = "windows") {
        "long_entry_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789"
    } else {
        "long_entry_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_0123456789_012345"
    };
    let dev_null_check = if cfg!(unix) {
        r#"
    // Special device file (/dev/null) is neither regular file nor directory.
    let dev_null_meta = fs.metadata("/dev/null")?
    if dev_null_meta.is_dir || dev_null_meta.is_file { return 34 }
    if fs.is_dir("/dev/null") || fs.is_file("/dev/null") { return 35 }
"#
    } else {
        ""
    };

    let src = format!(
        r#"
import std.fs
import std.io
import std.path

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {{
    if a.len() != b.len() {{ return false }}
    var i: i64 = 0
    while i < a.len() {{
        if a[i] != b[i] {{ return false }}
        i += 1
    }}
    return true
}}

fn main() -> io.Error!i32 {{
    let root = "{work_root_str}"

    // Clean start: root does not exist yet.
    if fs.exists(root) {{ return 1 }}
    if fs.is_dir(root) {{ return 2 }}
    if fs.is_file(root) {{ return 3 }}

    // 1. mkdir + metadata checks.
    fs.mkdir(root)?
    if !fs.exists(root) {{ return 4 }}
    if !fs.is_dir(root) {{ return 5 }}
    if fs.is_file(root) {{ return 6 }}
    let root_meta = fs.metadata(root)?
    if !root_meta.is_dir || root_meta.is_file {{ return 7 }}

    // Duplicate mkdir must fail.
    match fs.mkdir(root) {{
        .ok(_) => {{ return 8 }},
        .err(_) => {{}}
    }}

    // 2. mkdir_all nested, trailing slash, repeated slashes, and idempotency.
    let nested = path.join(root, "a//b/c/")?
    fs.mkdir_all(bytes_view(nested))?
    let nested_clean = path.normalize(bytes_view(nested))?
    if !fs.is_dir(bytes_view(nested_clean)) {{ return 9 }}
    // Idempotent mkdir_all on existing directory and root "/".
    fs.mkdir_all(bytes_view(nested_clean))?
    fs.mkdir_all("/")?

    // 3. read_dir on empty directory skips "." and "..".
    let empty_entries = fs.read_dir(bytes_view(nested_clean))?
    if store_len(empty_entries) != 0 {{ return 10 }}

    // 4. write_bytes + metadata / is_file / is_dir / file_size.
    let f_empty = path.join(bytes_view(nested_clean), "empty.dat")?
    fs.write_bytes(bytes_view(f_empty), "")?
    if !fs.is_file(bytes_view(f_empty)) {{ return 11 }}
    if fs.is_dir(bytes_view(f_empty)) {{ return 12 }}
    if fs.file_size(bytes_view(f_empty))? != 0 {{ return 13 }}

    let payload = "hello-fs-metadata-\0-tail-12345"
    let f_data = path.join(bytes_view(nested_clean), "alpha.txt")?
    fs.write_bytes(bytes_view(f_data), payload)?
    let meta = fs.metadata(bytes_view(f_data))?
    if !meta.is_file || meta.is_dir {{ return 14 }}
    if meta.size != payload.len() {{ return 15 }}
    if fs.file_size(bytes_view(f_data))? != payload.len() {{ return 16 }}

    // mkdir_all on an existing regular file must fail.
    match fs.mkdir_all(bytes_view(f_data)) {{
        .ok(_) => {{ return 17 }},
        .err(_) => {{}}
    }}

    // Create additional entries including dotfile, a long filename, and a subdirectory.
    let f_dot = path.join(bytes_view(nested_clean), ".hidden")?
    fs.write_bytes(bytes_view(f_dot), "dot")?
    let long_name = "{long_name}"
    let f_long = path.join(bytes_view(nested_clean), long_name)?
    fs.write_bytes(bytes_view(f_long), "long")?
    let sub_dir = path.join(bytes_view(nested_clean), "sub_dir_long_name_0123456789")?
    fs.mkdir(bytes_view(sub_dir))?
{dev_null_check}
    // 5. read_dir returns exact entries and skips "." / "..".
    let entries = fs.read_dir(bytes_view(nested_clean))?
    if store_len(entries) != 5 {{ return 18 }}
    var seen_empty: i32 = 0
    var seen_alpha: i32 = 0
    var seen_dot: i32 = 0
    var seen_long: i32 = 0
    var seen_sub: i32 = 0
    while store_len(entries) > 0 {{
        match store_pop(entries) {{
            .err(_) => {{ return 19 }},
            .ok(item) => {{
                let name = bytes_view(item)
                if bytes_eq(name, ".") || bytes_eq(name, "..") {{ return 20 }}
                if bytes_eq(name, "empty.dat") {{ seen_empty += 1 }}
                else if bytes_eq(name, "alpha.txt") {{ seen_alpha += 1 }}
                else if bytes_eq(name, ".hidden") {{ seen_dot += 1 }}
                else if bytes_eq(name, long_name) {{ seen_long += 1 }}
                else if bytes_eq(name, "sub_dir_long_name_0123456789") {{ seen_sub += 1 }}
                else {{ return 21 }}
            }}
        }}
    }}
    if seen_empty != 1 || seen_alpha != 1 || seen_dot != 1 || seen_long != 1 || seen_sub != 1 {{ return 22 }}

    // Repeated read_dir calls exercise DirStream Drop / closedir cleanly without leaking fds.
    var iter: i32 = 0
    while iter < 48 {{
        let batch = fs.read_dir(bytes_view(nested_clean))?
        if store_len(batch) != 5 {{ return 23 }}
        iter += 1
    }}

    // 6. rename file (within dir and across dirs) and rename directory.
    let f_renamed = path.join(bytes_view(nested_clean), "beta.txt")?
    fs.rename(bytes_view(f_data), bytes_view(f_renamed))?
    if fs.exists(bytes_view(f_data)) {{ return 24 }}
    if !fs.is_file(bytes_view(f_renamed)) {{ return 25 }}
    let read_back = fs.read_to_bytes(bytes_view(f_renamed))?
    if !bytes_eq(bytes_view(read_back), payload) {{ return 26 }}

    let sub_renamed = path.join(bytes_view(nested_clean), "sub_renamed")?
    fs.rename(bytes_view(sub_dir), bytes_view(sub_renamed))?
    if fs.exists(bytes_view(sub_dir)) {{ return 27 }}
    if !fs.is_dir(bytes_view(sub_renamed)) {{ return 28 }}

    // Cross-directory rename into sub_renamed.
    let f_moved = path.join(bytes_view(sub_renamed), "moved.txt")?
    fs.rename(bytes_view(f_renamed), bytes_view(f_moved))?
    if fs.exists(bytes_view(f_renamed)) {{ return 36 }}
    if fs.file_size(bytes_view(f_moved))? != payload.len() {{ return 37 }}

    // 7. remove_dir fails while non-empty, succeeds after cleanup.
    match fs.remove_dir(bytes_view(nested_clean)) {{
        .ok(_) => {{ return 29 }},
        .err(_) => {{}}
    }}

    match fs.remove_file(bytes_view(f_empty)) {{
        .ok(_) => {{}},
        .err(_) => {{ return 30 }}
    }}
    match fs.remove_file(bytes_view(f_moved)) {{
        .ok(_) => {{}},
        .err(_) => {{ return 31 }}
    }}
    match fs.remove_file(bytes_view(f_dot)) {{
        .ok(_) => {{}},
        .err(_) => {{ return 32 }}
    }}
    match fs.remove_file(bytes_view(f_long)) {{
        .ok(_) => {{}},
        .err(_) => {{ return 38 }}
    }}
    fs.remove_dir(bytes_view(sub_renamed))?
    fs.remove_dir(bytes_view(nested_clean))?
    let p_b = path.parent(bytes_view(nested_clean))
    fs.remove_dir(p_b)?
    let p_a = path.parent(p_b)
    fs.remove_dir(p_a)?
    fs.remove_dir(root)?
    if fs.exists(root) {{ return 33 }}

    return 0
}}
"#
    );
    let entry = write_source(&dir, "main.zil", &src);

    let (code, stdout, stderr) = build_and_run_dual(&dir, "fs-dir-meta", &entry);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
    assert!(!work_root.exists());
}

#[test]
fn std_fs_error_paths_execute_natively() {
    let dir = temp_dir("fs-errors");
    let missing = dir.join("missing_dir");
    let missing_str = missing.to_str().expect("utf8 path").replace('\\', "/");
    let regular = dir.join("regular_file.txt");
    let regular_str = regular.to_str().expect("utf8 path").replace('\\', "/");

    let src = format!(
        r#"
import std.fs
import std.io

fn main() -> io.Error!i32 {{
    let missing = "{missing_str}"
    let regular = "{regular_str}"
    fs.write_bytes(regular, "x")?

    // Missing path queries.
    if fs.is_dir(missing) {{ return 1 }}
    if fs.is_file(missing) {{ return 2 }}
    if fs.is_dir("") {{ return 3 }}
    if fs.is_file("bad\0path") {{ return 4 }}

    match fs.metadata(missing) {{
        .ok(_) => {{ return 5 }},
        .err(e) => {{
            match e {{
                io.Error.Read() => {{}},
                _ => {{ return 6 }}
            }}
        }}
    }}
    match fs.metadata("bad\0path") {{
        .ok(_) => {{ return 7 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 8 }}
            }}
        }}
    }}
    match fs.file_size("") {{
        .ok(_) => {{ return 9 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 10 }}
            }}
        }}
    }}

    // read_dir errors: missing dir, regular file, empty path, embedded NUL.
    match fs.read_dir(missing) {{
        .ok(_) => {{ return 11 }},
        .err(e) => {{
            match e {{
                io.Error.Open() => {{}},
                _ => {{ return 12 }}
            }}
        }}
    }}
    match fs.read_dir(regular) {{
        .ok(_) => {{ return 13 }},
        .err(e) => {{
            match e {{
                io.Error.Open() => {{}},
                _ => {{ return 14 }}
            }}
        }}
    }}
    match fs.read_dir("") {{
        .ok(_) => {{ return 15 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 16 }}
            }}
        }}
    }}
    match fs.read_dir("bad\0dir") {{
        .ok(_) => {{ return 17 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 18 }}
            }}
        }}
    }}

    // mkdir / mkdir_all / remove_dir / rename error paths.
    match fs.mkdir("bad\0dir") {{
        .ok(_) => {{ return 19 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 20 }}
            }}
        }}
    }}
    match fs.mkdir_all("") {{
        .ok(_) => {{ return 21 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 22 }}
            }}
        }}
    }}
    match fs.mkdir_all("bad\0nested/dir") {{
        .ok(_) => {{ return 23 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 24 }}
            }}
        }}
    }}
    match fs.remove_dir(missing) {{
        .ok(_) => {{ return 25 }},
        .err(e) => {{
            match e {{
                io.Error.Write() => {{}},
                _ => {{ return 26 }}
            }}
        }}
    }}
    match fs.remove_dir("bad\0dir") {{
        .ok(_) => {{ return 27 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 28 }}
            }}
        }}
    }}
    match fs.rename(missing, regular) {{
        .ok(_) => {{ return 29 }},
        .err(e) => {{
            match e {{
                io.Error.Write() => {{}},
                _ => {{ return 30 }}
            }}
        }}
    }}
    match fs.rename(regular, "bad\0dst") {{
        .ok(_) => {{ return 31 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 32 }}
            }}
        }}
    }}
    match fs.rename("", regular) {{
        .ok(_) => {{ return 33 }},
        .err(e) => {{
            match e {{
                io.Error.InvalidPath() => {{}},
                _ => {{ return 34 }}
            }}
        }}
    }}

    match fs.remove_file(regular) {{
        .ok(_) => {{}},
        .err(_) => {{ return 35 }}
    }}
    return 0
}}
"#
    );
    let entry = write_source(&dir, "main.zil", &src);

    let (code, stdout, stderr) = build_and_run_dual(&dir, "fs-errors", &entry);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[test]
fn std_path_all_operations_execute_natively() {
    let dir = temp_dir("path-ops");
    let src = r#"
import std.io
import std.path

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn check_join(base: &[u8], child: &[u8], want: &[u8]) -> io.Error!bool {
    let got = path.join(base, child)?
    return bytes_eq(bytes_view(got), want)
}

fn check_norm(input: &[u8], want: &[u8]) -> io.Error!bool {
    let got = path.normalize(input)?
    return bytes_eq(bytes_view(got), want)
}

fn main() -> io.Error!i32 {
    // 1. join
    if !check_join("/a/b", "c", "/a/b/c")? { return 1 }
    if !check_join("/a/b/", "c", "/a/b/c")? { return 2 }
    if !check_join("/", "foo", "/foo")? { return 3 }
    if !check_join("/a/b", "/c/d", "/c/d")? { return 4 }
    if !check_join("rel", "/", "/")? { return 5 }
    if !check_join("", "child", "child")? { return 6 }
    if !check_join("/base", "", "/base")? { return 7 }
    if !check_join("/base/", "", "/base/")? { return 8 }
    if !check_join("", "", "")? { return 9 }
    if !check_join("a", "b/c", "a/b/c")? { return 10 }

    // 2. file_name
    if !bytes_eq(path.file_name("/a/b/c.txt"), "c.txt") { return 20 }
    if !bytes_eq(path.file_name("/a/b/c.txt///"), "c.txt") { return 21 }
    if !bytes_eq(path.file_name("foo"), "foo") { return 22 }
    if !bytes_eq(path.file_name("foo/"), "foo") { return 23 }
    if !bytes_eq(path.file_name("/foo"), "foo") { return 24 }
    if !bytes_eq(path.file_name("/"), "") { return 25 }
    if !bytes_eq(path.file_name("///"), "") { return 26 }
    if !bytes_eq(path.file_name(""), "") { return 27 }
    if !bytes_eq(path.file_name(".gitignore"), ".gitignore") { return 28 }
    if !bytes_eq(path.file_name("/repo/.gitignore"), ".gitignore") { return 29 }

    // 3. parent
    if !bytes_eq(path.parent("/a/b/c"), "/a/b") { return 30 }
    if !bytes_eq(path.parent("/a/b/c/"), "/a/b") { return 31 }
    if !bytes_eq(path.parent("/a//b///"), "/a") { return 32 }
    if !bytes_eq(path.parent("/foo"), "/") { return 33 }
    if !bytes_eq(path.parent("/foo/"), "/") { return 34 }
    if !bytes_eq(path.parent("//foo"), "/") { return 35 }
    if !bytes_eq(path.parent("a/b"), "a") { return 36 }
    if !bytes_eq(path.parent("a/b/"), "a") { return 37 }
    if !bytes_eq(path.parent("foo"), "") { return 38 }
    if !bytes_eq(path.parent("foo/"), "") { return 39 }
    if !bytes_eq(path.parent("/"), "") { return 40 }
    if !bytes_eq(path.parent("///"), "") { return 41 }
    if !bytes_eq(path.parent(""), "") { return 42 }

    // 4. extension
    if !bytes_eq(path.extension("foo.txt"), "txt") { return 50 }
    if !bytes_eq(path.extension("archive.tar.gz"), "gz") { return 51 }
    if !bytes_eq(path.extension("/dir.v1/file.zil"), "zil") { return 52 }
    if !bytes_eq(path.extension("/dir.v1/file"), "") { return 53 }
    if !bytes_eq(path.extension(".gitignore"), "") { return 54 }
    if !bytes_eq(path.extension("/repo/.gitignore"), "") { return 55 }
    if !bytes_eq(path.extension(".config.toml"), "toml") { return 56 }
    if !bytes_eq(path.extension("trailing."), "") { return 57 }
    if !bytes_eq(path.extension("."), "") { return 58 }
    if !bytes_eq(path.extension(".."), "") { return 59 }
    if !bytes_eq(path.extension("/"), "") { return 60 }
    if !bytes_eq(path.extension(""), "") { return 61 }
    if !bytes_eq(path.extension("dir/file.rs/"), "rs") { return 62 }

    // 5. normalize
    if !check_norm("", "")? { return 70 }
    if !check_norm("/", "/")? { return 71 }
    if !check_norm("///", "/")? { return 72 }
    if !check_norm("/.", "/")? { return 73 }
    if !check_norm("/..", "/")? { return 74 }
    if !check_norm("/../../a/./b/../c///", "/a/c")? { return 75 }
    if !check_norm("/a//b///c/", "/a/b/c")? { return 76 }
    if !check_norm(".", ".")? { return 77 }
    if !check_norm("./", ".")? { return 78 }
    if !check_norm("a/..", ".")? { return 79 }
    if !check_norm("a/b/../c", "a/c")? { return 80 }
    if !check_norm("a/../../b", "../b")? { return 81 }
    if !check_norm("../a/../b", "../b")? { return 82 }
    if !check_norm("../../a/b/..", "../../a")? { return 83 }
    if !check_norm("./a//./b/", "a/b")? { return 84 }
    if !check_norm("..", "..")? { return 85 }
    if !check_norm("a/.hidden/..foo", "a/.hidden/..foo")? { return 86 }
    if !check_norm("./..", "..")? { return 87 }
    if !check_norm("/a/..", "/")? { return 88 }

    return 0
}
"#;
    let entry = write_source(&dir, "main.zil", src);

    let (code, stdout, stderr) = build_and_run_dual(&dir, "path-ops", &entry);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}
