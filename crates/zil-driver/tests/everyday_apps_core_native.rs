//! Dual-backend (`clang-o0`, `clang-o2`, and `llvmapi-o0`/`llvmapi-o2` when `llvm-api` is enabled)
//! end-to-end integration tests for the core four everyday multi-module ZIL applications:
//! 1. `examples/everyday-apps/text-stats-and-file-search/`
//! 2. `examples/everyday-apps/csv-processor/`
//! 3. `examples/everyday-apps/json-processor/`
//! 4. `examples/everyday-apps/cli-todo-app/`
//!
//! Each application is compiled from its `Zil.toml` manifest via `ProjectManifest::open`,
//! executed with real CLI arguments and real filesystem/data I/O across both backends and
//! optimization levels, verified for clean error handling and resource cleanup, and paired
//! with a nearest safety-negative control verifying `ResultCategory::SafeReject`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization,
    ProjectManifest, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-everyday-apps-core-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
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

fn build_project_all_backends_with_c(
    app_rel_dir: &str,
    out_dir: &Path,
    tag: &str,
    extra_c_sources: &[PathBuf],
) -> Vec<(&'static str, PathBuf)> {
    let project_dir = repo_root().join(app_rel_dir);
    let default_opts = CompileOptions::default();
    let manifest = ProjectManifest::open(&project_dir, &default_opts.limits).unwrap_or_else(|e| {
        panic!(
            "failed to open Zil.toml in {}: {e:?}",
            project_dir.display()
        )
    });
    let manifest = Arc::new(manifest);

    let mut binaries = Vec::new();
    for (label, backend, opt) in dual_configs() {
        let exe = out_dir.join(format!("{tag}-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            build_inputs: manifest.fingerprint,
            project: Some(Arc::clone(&manifest)),
            extra_c_sources: extra_c_sources.to_vec(),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&manifest.entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {app_rel_dir} ({label}): {:?}",
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);
        binaries.push((label, exe));
    }
    binaries
}

fn build_project_all_backends(
    app_rel_dir: &str,
    out_dir: &Path,
    tag: &str,
) -> Vec<(&'static str, PathBuf)> {
    build_project_all_backends_with_c(app_rel_dir, out_dir, tag, &[])
}

fn run_cli_with_env(
    exe: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn run_cli(exe: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_safe_reject(name: &str, src: &str) {
    let dir = temp_dir(name);
    let entry = dir.join("main.zil");
    std::fs::write(&entry, src).expect("write negative source");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for {name}, got {:?}: {:?}",
        report.status,
        report.diagnostics
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn everyday_app_text_stats_and_file_search_dual_backend() {
    let work = temp_dir("text-stats");
    let data = work.join("corpus");
    std::fs::create_dir_all(data.join("nested")).expect("create corpus dirs");
    let alpha_path = data.join("alpha.txt");
    std::fs::write(
        &alpha_path,
        "  hello ZIL world  \nsecond line here\nthird ZIL line\n",
    )
    .expect("write alpha.txt");
    std::fs::write(
        data.join("nested/beta.txt"),
        "nested file\n  match ZIL token  \n",
    )
    .expect("write beta.txt");
    let bad_utf8 = work.join("bad_utf8.bin");
    std::fs::write(&bad_utf8, [0xffu8, 0xfeu8, 0x80u8]).expect("write bad utf8");

    let binaries = build_project_all_backends(
        "examples/everyday-apps/text-stats-and-file-search",
        &work,
        "text-stats",
    );
    let data_str = data.to_str().expect("utf8 path");
    let alpha_str = alpha_path.to_str().expect("utf8 path");
    let bad_utf8_str = bad_utf8.to_str().expect("utf8 path");
    let missing = work.join("does-not-exist");
    let missing_str = missing.to_str().expect("utf8 path");

    let expected_stats = concat!(
        "FILE alpha.txt lines=3 words=9 bytes=52\n",
        "FILE nested/beta.txt lines=2 words=5 bytes=32\n",
        "TOTAL files=2 lines=5 words=14 bytes=84\n",
    );
    let expected_single = concat!(
        "FILE alpha.txt lines=3 words=9 bytes=52\n",
        "TOTAL files=1 lines=3 words=9 bytes=52\n",
    );
    let expected_search = concat!(
        "MATCH alpha.txt:1:hello ZIL world\n",
        "MATCH alpha.txt:3:third ZIL line\n",
        "MATCH nested/beta.txt:2:match ZIL token\n",
        "TOTAL_MATCHES 3\n",
    );
    let expected_summary = concat!(
        "FILE alpha.txt lines=3 words=9 bytes=52\n",
        "MATCH alpha.txt:1:hello ZIL world\n",
        "MATCH alpha.txt:3:third ZIL line\n",
        "FILE nested/beta.txt lines=2 words=5 bytes=32\n",
        "MATCH nested/beta.txt:2:match ZIL token\n",
        "TOTAL files=2 lines=5 words=14 bytes=84 matches=3\n",
    );

    for (label, exe) in &binaries {
        let (code, out, err) = run_cli(exe, &["stats", data_str]);
        assert_eq!(code, 0, "{label} stats failed: stderr={err}");
        assert!(err.is_empty(), "{label} stats stderr: {err}");
        assert_eq!(out, expected_stats, "{label} stats output mismatch");

        let (code, out, err) = run_cli(exe, &["stats", alpha_str]);
        assert_eq!(code, 0, "{label} single-file stats failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, expected_single, "{label} single-file stats mismatch");

        let (code, out, err) = run_cli(exe, &["search", data_str, "ZIL"]);
        assert_eq!(code, 0, "{label} search failed: stderr={err}");
        assert!(err.is_empty(), "{label} search stderr: {err}");
        assert_eq!(out, expected_search, "{label} search output mismatch");

        let (code, out, err) = run_cli(exe, &["summary", data_str, "ZIL"]);
        assert_eq!(code, 0, "{label} summary failed: stderr={err}");
        assert!(err.is_empty(), "{label} summary stderr: {err}");
        assert_eq!(out, expected_summary, "{label} summary output mismatch");

        // Error paths: missing path, invalid UTF-8, bad args, empty query.
        let (code, _out, err) = run_cli(exe, &["stats", missing_str]);
        assert_eq!(code, 1, "{label} missing path should exit 1");
        assert!(!err.is_empty(), "{label} missing path should write stderr");

        let (code, _out, err) = run_cli(exe, &["stats", bad_utf8_str]);
        assert_eq!(code, 1, "{label} invalid utf8 should exit 1");
        assert!(!err.is_empty(), "{label} invalid utf8 should write stderr");

        let (code, _out, err) = run_cli(exe, &["search", data_str, ""]);
        assert_eq!(code, 2, "{label} empty query should exit 2");
        assert!(!err.is_empty(), "{label} empty query should write stderr");

        let (code, _out, err) = run_cli(exe, &["unknown", data_str]);
        assert_eq!(code, 2, "{label} unknown mode should exit 2");
        assert!(!err.is_empty(), "{label} unknown mode should write stderr");
    }

    // Paired nearest safety-negative control: use-after-move of owned directory entry Store.
    assert_safe_reject(
        "neg-text-stats-use-after-move",
        r#"
import std.fs
import std.io

fn consume_entries(give items: Store<Managed<ByteBuffer>>) -> i64 {
    return store_len(items)
}

fn main() -> io.Error!i32 {
    let entries = fs.read_dir(".")?
    let first = consume_entries(give entries)
    let stale = store_len(entries)
    if first == stale { return 0 }
    return 1
}
"#,
    );

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn everyday_app_csv_processor_dual_backend() {
    let work = temp_dir("csv-processor");
    let input_csv = work.join("sales.csv");
    std::fs::write(
        &input_csv,
        "region,item,amount\r\nwest,\"Widget, \"\"Pro\"\"\",120\r\neast,Gadget,-15\nwest,Adapter,85\nwest,\"Cable, USB\",45\n",
    )
    .expect("write sales.csv");

    let bad_quote_csv = work.join("bad_quote.csv");
    std::fs::write(&bad_quote_csv, "a,b\n\"unterminated,1\n").expect("write bad_quote.csv");

    let mismatch_csv = work.join("mismatch.csv");
    std::fs::write(&mismatch_csv, "a,b,c\n1,2\n").expect("write mismatch.csv");

    let non_numeric_csv = work.join("non_numeric.csv");
    std::fs::write(&non_numeric_csv, "region,amount\nwest,not_a_number\n")
        .expect("write non_numeric.csv");

    let binaries = build_project_all_backends(
        "examples/everyday-apps/csv-processor",
        &work,
        "csv-processor",
    );

    let input_str = input_csv.to_str().expect("utf8 path");
    let bad_quote_str = bad_quote_csv.to_str().expect("utf8 path");
    let mismatch_str = mismatch_csv.to_str().expect("utf8 path");
    let non_numeric_str = non_numeric_csv.to_str().expect("utf8 path");

    let expected_normalized = concat!(
        "region,item,amount\n",
        "west,\"Widget, \"\"Pro\"\"\",120\n",
        "east,Gadget,-15\n",
        "west,Adapter,85\n",
        "west,\"Cable, USB\",45\n",
    );
    let expected_filtered_csv = concat!(
        "region,item,amount\n",
        "west,\"Widget, \"\"Pro\"\"\",120\n",
        "west,Adapter,85\n",
        "west,\"Cable, USB\",45\n",
    );

    for (label, exe) in &binaries {
        // 1. normalize to output file
        let norm_out = work.join(format!("normalized-{label}.csv"));
        let norm_out_str = norm_out.to_str().expect("utf8 path");
        let (code, out, err) = run_cli(exe, &["normalize", input_str, norm_out_str]);
        assert_eq!(code, 0, "{label} normalize failed: stderr={err}");
        assert!(out.is_empty());
        assert!(err.is_empty());
        assert_eq!(
            std::fs::read_to_string(&norm_out).expect("read normalized csv"),
            expected_normalized,
            "{label} normalized CSV mismatch"
        );

        // 2. agg across all rows
        let (code, out, err) = run_cli(exe, &["agg", input_str, "amount"]);
        assert_eq!(code, 0, "{label} agg failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(
            out, "SUMMARY count=4 sum=235 min=-15 max=120\n",
            "{label} agg output mismatch"
        );

        // 3. filter-agg to stdout
        let (code, out, err) = run_cli(exe, &["filter-agg", input_str, "region", "west", "amount"]);
        assert_eq!(code, 0, "{label} filter-agg stdout failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(
            out,
            format!("{expected_filtered_csv}SUMMARY count=3 sum=250 min=45 max=120\n"),
            "{label} filter-agg stdout mismatch"
        );

        // 4. filter-agg to file
        let filtered_out = work.join(format!("filtered-{label}.csv"));
        let filtered_out_str = filtered_out.to_str().expect("utf8 path");
        let (code, out, err) = run_cli(
            exe,
            &[
                "filter-agg",
                input_str,
                "region",
                "west",
                "amount",
                filtered_out_str,
            ],
        );
        assert_eq!(code, 0, "{label} filter-agg file failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "SUMMARY count=3 sum=250 min=45 max=120\n");
        assert_eq!(
            std::fs::read_to_string(&filtered_out).expect("read filtered csv"),
            expected_filtered_csv
        );

        // 5. Error paths leave target output uncreated on failure.
        let untouched_out = work.join(format!("untouched-{label}.csv"));
        let untouched_str = untouched_out.to_str().expect("utf8 path");
        let (code, _out, err) = run_cli(exe, &["normalize", bad_quote_str, untouched_str]);
        assert_eq!(code, 1, "{label} unterminated quote should exit 1");
        assert!(!err.is_empty());
        assert!(
            !untouched_out.exists(),
            "{label} output must not be created on parse failure"
        );

        let (code, _out, err) = run_cli(exe, &["agg", mismatch_str, "a"]);
        assert_eq!(code, 1, "{label} column mismatch should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &["agg", input_str, "missing_col"]);
        assert_eq!(code, 1, "{label} missing column should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &["agg", non_numeric_str, "amount"]);
        assert_eq!(code, 1, "{label} non-numeric cell should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &["bad-cmd", input_str, "amount"]);
        assert_eq!(code, 2, "{label} bad args should exit 2");
        assert!(!err.is_empty());

        // 6. RFC 4180 boundaries: embedded newlines inside quotes, empty fields, trailing empty
        // field, CRLF/LF mix, and input without a trailing newline.
        let rfc_csv = work.join(format!("rfc4180-{label}.csv"));
        std::fs::write(
            &rfc_csv,
            "k,note,v\r\n,\",\",10\r\n\"multi\nline\",\"a\"\"b\",20\nlast,,",
        )
        .expect("write rfc4180 csv");
        let rfc_out = work.join(format!("rfc4180-out-{label}.csv"));
        let rfc_csv_str = rfc_csv.to_str().expect("utf8 path");
        let rfc_out_str = rfc_out.to_str().expect("utf8 path");
        let (code, out, err) = run_cli(exe, &["normalize", rfc_csv_str, rfc_out_str]);
        assert_eq!(code, 0, "{label} rfc4180 normalize failed: stderr={err}");
        assert!(out.is_empty());
        assert!(err.is_empty());
        assert_eq!(
            std::fs::read_to_string(&rfc_out).expect("read rfc4180 normalized"),
            "k,note,v\n,\",\",10\n\"multi\nline\",\"a\"\"b\",20\nlast,,\n",
            "{label} rfc4180 normalized CSV mismatch"
        );

        let stray_quote = work.join(format!("stray-quote-{label}.csv"));
        std::fs::write(&stray_quote, "a,b\nfoo\"bar,1\n").expect("write stray_quote.csv");
        let (code, _out, err) = run_cli(
            exe,
            &[
                "normalize",
                stray_quote.to_str().expect("utf8 path"),
                untouched_str,
            ],
        );
        assert_eq!(
            code, 1,
            "{label} stray quote in unquoted field should exit 1"
        );
        assert!(!err.is_empty());

        let after_quote = work.join(format!("after-quote-{label}.csv"));
        std::fs::write(&after_quote, "a,b\n\"ok\"trailing,1\n").expect("write after_quote.csv");
        let (code, _out, err) = run_cli(
            exe,
            &[
                "normalize",
                after_quote.to_str().expect("utf8 path"),
                untouched_str,
            ],
        );
        assert_eq!(
            code, 1,
            "{label} trailing char after closing quote should exit 1"
        );
        assert!(!err.is_empty());
    }

    // Paired nearest safety-negative control: returning a dangling reference to a local CsvRow buffer.
    assert_safe_reject(
        "neg-csv-dangling-cell-ref",
        r#"
import std.string

package CsvRow {
    manages fields: Store<string.String>;
}

fn dangling_cell() -> &[u8] {
    let row = CsvRow { fields: store_new() }
    let s = string.empty()
    let _ = store_push(row.fields, s)
    let first = store_at(row.fields, 0)
    return string.as_bytes(first)
}

fn main() -> i32 {
    let view = dangling_cell()
    if view.len() == 0 { return 0 }
    return 1
}
"#,
    );

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn everyday_app_json_processor_dual_backend() {
    let work = temp_dir("json-processor");
    let sample_json = work.join("sample.json");
    std::fs::write(
        &sample_json,
        r#"
{
  "service": "zil-core",
  "active": true,
  "retired": false,
  "note": null,
  "version": -42,
  "ratio": 3.14,
  "greeting": "line1\n\t\"caf\u00e9\"\\ok",
  "unicode": "\u4E2D\u6587\uD83D\uDE00\uD83C\uDF89\u001f",
  "items": [
    { "id": 10, "tags": ["alpha", "beta"] },
    { "id": 20, "tags": [] }
  ]
}
"#,
    )
    .expect("write sample.json");

    let trailing_json = work.join("trailing.json");
    std::fs::write(&trailing_json, r#"{"a": 1} extra"#).expect("write trailing.json");

    let unterminated_json = work.join("unterminated.json");
    std::fs::write(&unterminated_json, r#"{"a": "oops}"#).expect("write unterminated.json");

    let trailing_comma_json = work.join("trailing_comma.json");
    std::fs::write(&trailing_comma_json, r#"[1, 2,]"#).expect("write trailing_comma.json");

    let leading_zero_json = work.join("leading_zero.json");
    std::fs::write(&leading_zero_json, r#"{"n": 01}"#).expect("write leading_zero.json");

    let lone_high_surrogate_json = work.join("lone_high_surrogate.json");
    std::fs::write(&lone_high_surrogate_json, r#"{"bad": "\uD83D"}"#)
        .expect("write lone_high_surrogate.json");

    let high_plus_bmp_json = work.join("high_plus_bmp.json");
    std::fs::write(&high_plus_bmp_json, r#"{"bad": "\uD83D\u0041"}"#)
        .expect("write high_plus_bmp.json");

    let lone_low_surrogate_json = work.join("lone_low_surrogate.json");
    std::fs::write(&lone_low_surrogate_json, r#"{"bad": "\uDE00"}"#)
        .expect("write lone_low_surrogate.json");

    let raw_control_json = work.join("raw_control.json");
    std::fs::write(&raw_control_json, "{\"bad\": \"a\x01b\"}").expect("write raw_control.json");

    let binaries = build_project_all_backends(
        "examples/everyday-apps/json-processor",
        &work,
        "json-processor",
    );

    let sample_str = sample_json.to_str().expect("utf8 path");
    let trailing_str = trailing_json.to_str().expect("utf8 path");
    let unterminated_str = unterminated_json.to_str().expect("utf8 path");
    let trailing_comma_str = trailing_comma_json.to_str().expect("utf8 path");
    let leading_zero_str = leading_zero_json.to_str().expect("utf8 path");
    let lone_high_str = lone_high_surrogate_json.to_str().expect("utf8 path");
    let high_plus_bmp_str = high_plus_bmp_json.to_str().expect("utf8 path");
    let lone_low_str = lone_low_surrogate_json.to_str().expect("utf8 path");
    let raw_control_str = raw_control_json.to_str().expect("utf8 path");

    let expected_canonical = "{\"service\":\"zil-core\",\"active\":true,\"retired\":false,\"note\":null,\"version\":-42,\"ratio\":3.14,\"greeting\":\"line1\\n\\t\\\"café\\\"\\\\ok\",\"unicode\":\"中文😀🎉\\u001f\",\"items\":[{\"id\":10,\"tags\":[\"alpha\",\"beta\"]},{\"id\":20,\"tags\":[]}]}";

    for (label, exe) in &binaries {
        // 1. validate
        let (code, out, err) = run_cli(exe, &["validate", sample_str]);
        assert_eq!(code, 0, "{label} validate failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "VALID\n");

        // 2. format to file and verify deterministic round-trip (including UTF-16 surrogate pairs & control-char escaping)
        let roundtrip_one = work.join(format!("roundtrip-one-{label}.json"));
        let roundtrip_two = work.join(format!("roundtrip-two-{label}.json"));
        let rt1_str = roundtrip_one.to_str().expect("utf8 path");
        let rt2_str = roundtrip_two.to_str().expect("utf8 path");

        let (code, out, err) = run_cli(exe, &["format", sample_str, rt1_str]);
        assert_eq!(code, 0, "{label} format1 failed: stderr={err}");
        assert!(out.is_empty());
        assert!(err.is_empty());
        let first_bytes = std::fs::read_to_string(&roundtrip_one).expect("read rt1");
        assert_eq!(
            first_bytes, expected_canonical,
            "{label} canonical JSON mismatch"
        );

        let (code, out, err) = run_cli(exe, &["format", rt1_str, rt2_str]);
        assert_eq!(code, 0, "{label} format2 failed: stderr={err}");
        assert!(out.is_empty());
        assert!(err.is_empty());
        let second_bytes = std::fs::read_to_string(&roundtrip_two).expect("read rt2");
        assert_eq!(
            second_bytes, first_bytes,
            "{label} round-trip serialization must be byte-identical"
        );

        // 3. query nested paths including UTF-16 surrogate-decoded supplementary Unicode
        let (code, out, err) = run_cli(exe, &["query", sample_str, "items.0.tags.1"]);
        assert_eq!(code, 0, "{label} query items.0.tags.1 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "\"beta\"\n");

        let (code, out, err) = run_cli(exe, &["query", sample_str, "version"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} query version stderr: {err}");
        assert_eq!(out, "-42\n");

        let (code, out, err) = run_cli(exe, &["query", sample_str, "greeting"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} query greeting stderr: {err}");
        assert_eq!(out, "\"line1\\n\\t\\\"café\\\"\\\\ok\"\n");

        let (code, out, err) = run_cli(exe, &["query", sample_str, "unicode"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} query unicode stderr: {err}");
        assert_eq!(out, "\"中文😀🎉\\u001f\"\n");

        let (code, out, err) = run_cli(exe, &["query", sample_str, "."]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} query root stderr: {err}");
        assert_eq!(out, format!("{expected_canonical}\n"));

        // 4. Reject invalid JSON inputs (including lone/mismatched UTF-16 surrogates & raw control chars) cleanly without creating output files.
        let untouched = work.join(format!("bad-out-{label}.json"));
        let untouched_str = untouched.to_str().expect("utf8 path");
        for (case_name, bad_path) in [
            ("trailing", trailing_str),
            ("unterminated", unterminated_str),
            ("trailing-comma", trailing_comma_str),
            ("leading-zero", leading_zero_str),
            ("lone-high-surrogate", lone_high_str),
            ("high-plus-bmp", high_plus_bmp_str),
            ("lone-low-surrogate", lone_low_str),
            ("raw-control-char", raw_control_str),
        ] {
            let (code, _out, err) = run_cli(exe, &["format", bad_path, untouched_str]);
            assert_eq!(code, 1, "{label} {case_name} should exit 1");
            assert!(!err.is_empty(), "{label} {case_name} should write stderr");
            assert!(
                !untouched.exists(),
                "{label} {case_name} must not create output file"
            );
        }

        // 5. Missing key / out-of-bounds index / bad args
        let (code, _out, err) = run_cli(exe, &["query", sample_str, "items.9.id"]);
        assert_eq!(code, 1, "{label} OOB query should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &["query", sample_str, "no_such_key"]);
        assert_eq!(code, 1, "{label} missing key should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &["unknown", sample_str]);
        assert_eq!(code, 2, "{label} bad args should exit 2");
        assert!(!err.is_empty());
    }

    // Paired nearest safety-negative control: mutating owned JsonDocument Store while an element borrow is still live.
    assert_safe_reject(
        "neg-json-borrow-conflict-on-store-push",
        r#"
import std.string

enum JsonNode {
    Null,
    Str(string.String),
}

package JsonDoc {
    manages nodes: Store<JsonNode>;
}

fn main() -> i32 {
    let doc = JsonDoc { nodes: store_new() }
    let _ = store_reserve(doc.nodes, 4)
    let _ = store_push(doc.nodes, JsonNode.Null)
    let borrowed = store_at(doc.nodes, 0)
    let _ = store_push(doc.nodes, JsonNode.Null)
    match *borrowed {
        JsonNode.Null() => { return 0 },
        JsonNode.Str(_) => { return 1 },
    }
}
"#,
    );

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn everyday_app_cli_todo_app_dual_backend() {
    let work = temp_dir("cli-todo-app");
    let binaries =
        build_project_all_backends("examples/everyday-apps/cli-todo-app", &work, "cli-todo-app");

    for (label, exe) in &binaries {
        // Store path inside nested non-existent directory to exercise fs.mkdir_all + atomic rename.
        let store_path = work.join(format!("state-{label}/nested/todos.db"));
        let store_str = store_path.to_str().expect("utf8 path");
        let tmp_path = work.join(format!("state-{label}/nested/todos.db.tmp"));

        // 1. Empty store lists EMPTY and zero stats.
        let (code, out, err) = run_cli(exe, &[store_str, "list"]);
        assert_eq!(code, 0, "{label} empty list failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "EMPTY\n");

        let (code, out, err) = run_cli(exe, &[store_str, "stats"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} empty stats stderr: {err}");
        assert_eq!(out, "STATS total=0 done=0 pending=0\n");

        // 2. Add 3 todos across separate process invocations (including tab/backslash escaping).
        let (code, out, err) = run_cli(exe, &[store_str, "add", "  Write ZIL parser  "]);
        assert_eq!(code, 0, "{label} add 1 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "ADDED 1\n");
        assert!(store_path.exists());
        assert!(!tmp_path.exists(), "{label} temp file must be renamed away");

        let (code, out, err) = run_cli(exe, &[store_str, "add", "Verify\\atomic\tsave"]);
        assert_eq!(code, 0, "{label} add 2 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "ADDED 2\n");

        let (code, out, err) = run_cli(exe, &[store_str, "add", "Ship release notes"]);
        assert_eq!(code, 0, "{label} add 3 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "ADDED 3\n");

        // 3. Mark #2 done, verify list & stats.
        let (code, out, err) = run_cli(exe, &[store_str, "done", "2"]);
        assert_eq!(code, 0, "{label} done 2 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "DONE 2\n");

        let (code, out, err) = run_cli(exe, &[store_str, "list"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} list stderr: {err}");
        assert_eq!(
            out,
            "[ ] 1: Write ZIL parser\n[x] 2: Verify\\atomic\tsave\n[ ] 3: Ship release notes\n"
        );

        let (code, out, err) = run_cli(exe, &[store_str, "stats"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} stats stderr: {err}");
        assert_eq!(out, "STATS total=3 done=1 pending=2\n");

        // 4. Remove #1, add another item (ID must be 4, not reused), verify list & stats.
        let (code, out, err) = run_cli(exe, &[store_str, "remove", "1"]);
        assert_eq!(code, 0, "{label} remove 1 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "REMOVED 1\n");

        let (code, out, err) = run_cli(exe, &[store_str, "add", "Review PR"]);
        assert_eq!(code, 0, "{label} add 4 failed: stderr={err}");
        assert!(err.is_empty());
        assert_eq!(out, "ADDED 4\n");

        let (code, out, err) = run_cli(exe, &[store_str, "list"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} list after remove stderr: {err}");
        assert_eq!(
            out,
            "[x] 2: Verify\\atomic\tsave\n[ ] 3: Ship release notes\n[ ] 4: Review PR\n"
        );

        let (code, out, err) = run_cli(exe, &[store_str, "stats"]);
        assert_eq!(code, 0);
        assert!(err.is_empty(), "{label} stats after remove stderr: {err}");
        assert_eq!(out, "STATS total=3 done=1 pending=2\n");

        // 5. Snapshot store bytes and verify every error command leaves store uncorrupted.
        let snapshot = std::fs::read(&store_path).expect("snapshot store");

        let (code, _out, err) = run_cli(exe, &[store_str, "done", "999"]);
        assert_eq!(code, 1, "{label} unknown ID done should exit 1");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &[store_str, "remove", "not_a_number"]);
        assert_eq!(code, 2, "{label} invalid ID should exit 2");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &[store_str, "done", "0"]);
        assert_eq!(code, 2, "{label} zero ID should exit 2");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &[store_str, "add", "   "]);
        assert_eq!(code, 2, "{label} empty title should exit 2");
        assert!(!err.is_empty());

        let (code, _out, err) = run_cli(exe, &[store_str, "unknown"]);
        assert_eq!(code, 2, "{label} unknown command should exit 2");
        assert!(!err.is_empty());

        assert_eq!(
            std::fs::read(&store_path).expect("re-read store"),
            snapshot,
            "{label} error commands must not corrupt existing todo store"
        );
        assert!(
            !tmp_path.exists(),
            "{label} no leftover temp file after error commands"
        );

        // 6. Corrupt store file is rejected cleanly with exit 1.
        let corrupt = work.join(format!("corrupt-{label}.db"));
        let corrupt_str = corrupt.to_str().expect("utf8 path");
        std::fs::write(&corrupt, "BROKEN HEADER\n").expect("write corrupt store");
        let (code, _out, err) = run_cli(exe, &[corrupt_str, "list"]);
        assert_eq!(code, 1, "{label} corrupt store should exit 1");
        assert!(!err.is_empty());
    }

    // Paired nearest safety-negative control: use-after-move of owned TodoList package.
    assert_safe_reject(
        "neg-todo-use-after-move-list",
        r#"
import std.string

package TodoItem {
    pub id: i64;
    pub title: string.String;
}

package TodoList {
    pub next_id: i64;
    manages items: Store<TodoItem>;
}

fn archive_list(list: TodoList) -> i64 {
    return store_len(list.items)
}

fn main() -> i32 {
    let list = TodoList { next_id: 1, items: store_new() }
    let first = archive_list(list)
    let second = archive_list(list)
    if first == second { return 0 }
    return 1
}
"#,
    );

    let _ = std::fs::remove_dir_all(&work);
}

#[cfg(target_os = "linux")]
#[test]
fn everyday_apps_oom_injection_cleanup_dual_backend() {
    let work = temp_dir("oom-injection");
    let oom_probe = work.join("oom_realloc_probe.c");
    std::fs::write(
        &oom_probe,
        r#"
#include <stddef.h>
#include <stdlib.h>

extern void *__libc_realloc(void *ptr, size_t size);

static long g_fail_at = -1;
static long g_calls = 0;

static void init_fail_point(void) {
    if (g_fail_at != -1) return;
    const char *env = getenv("ZIL_OOM_FAIL_AT");
    if (!env || env[0] == '\0') {
        g_fail_at = 0;
        return;
    }
    long parsed = strtol(env, NULL, 10);
    g_fail_at = parsed > 0 ? parsed : 0;
}

void *realloc(void *ptr, size_t size) {
    init_fail_point();
    if (g_fail_at > 0) {
        g_calls++;
        if (g_calls == g_fail_at) {
            return NULL;
        }
    }
    return __libc_realloc(ptr, size);
}
"#,
    )
    .expect("write oom_realloc_probe.c");

    let corpus = work.join("corpus");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    std::fs::write(corpus.join("doc.txt"), "hello ZIL\nworld ZIL\n").expect("write doc.txt");

    let csv_in = work.join("data.csv");
    std::fs::write(&csv_in, "k,v\na,10\nb,20\na,30\n").expect("write data.csv");

    let json_in = work.join("data.json");
    std::fs::write(&json_in, r#"{"a":[1,2,3],"b":{"c":"hello"}}"#).expect("write data.json");

    let todo_db = work.join("todos.db");
    let initial_todo = "NEXT 2\n1\t0\tInitial task\n";
    std::fs::write(&todo_db, initial_todo).expect("write initial todos.db");
    let todo_tmp = work.join("todos.db.tmp");

    let text_bins = build_project_all_backends_with_c(
        "examples/everyday-apps/text-stats-and-file-search",
        &work,
        "oom-text",
        std::slice::from_ref(&oom_probe),
    );
    let csv_bins = build_project_all_backends_with_c(
        "examples/everyday-apps/csv-processor",
        &work,
        "oom-csv",
        std::slice::from_ref(&oom_probe),
    );
    let json_bins = build_project_all_backends_with_c(
        "examples/everyday-apps/json-processor",
        &work,
        "oom-json",
        std::slice::from_ref(&oom_probe),
    );
    let todo_bins = build_project_all_backends_with_c(
        "examples/everyday-apps/cli-todo-app",
        &work,
        "oom-todo",
        std::slice::from_ref(&oom_probe),
    );

    let corpus_str = corpus.to_str().expect("utf8");
    let csv_in_str = csv_in.to_str().expect("utf8");
    let json_in_str = json_in.to_str().expect("utf8");
    let todo_db_str = todo_db.to_str().expect("utf8");

    let mut text_oom_hits = 0usize;
    let mut csv_oom_hits = 0usize;
    let mut json_oom_hits = 0usize;
    let mut todo_oom_hits = 0usize;

    for fail_at in [1usize, 3, 6, 10, 15] {
        let fail_str = fail_at.to_string();
        let envs = [("ZIL_OOM_FAIL_AT", fail_str.as_str())];

        for (label, exe) in &text_bins {
            let (code, _out, _err) = run_cli_with_env(exe, &["summary", corpus_str, "ZIL"], &envs);
            assert!(
                matches!(code, Some(0) | Some(1) | Some(2)),
                "text-stats OOM({fail_at}) on {label} crashed with status {code:?}"
            );
            if code != Some(0) {
                text_oom_hits += 1;
            }
        }

        for (label, exe) in &csv_bins {
            let csv_out = work.join(format!("oom-csv-{label}-{fail_at}.csv"));
            let csv_out_str = csv_out.to_str().expect("utf8");
            let (code, _out, _err) =
                run_cli_with_env(exe, &["normalize", csv_in_str, csv_out_str], &envs);
            assert!(
                matches!(code, Some(0) | Some(1) | Some(2)),
                "csv-processor OOM({fail_at}) on {label} crashed with status {code:?}"
            );
            if code != Some(0) {
                csv_oom_hits += 1;
                assert!(
                    !csv_out.exists(),
                    "csv-processor OOM({fail_at}) on {label} must not leave partial output file"
                );
            }
        }

        for (label, exe) in &json_bins {
            let json_out = work.join(format!("oom-json-{label}-{fail_at}.json"));
            let json_out_str = json_out.to_str().expect("utf8");
            let (code, _out, _err) =
                run_cli_with_env(exe, &["format", json_in_str, json_out_str], &envs);
            assert!(
                matches!(code, Some(0) | Some(1) | Some(2)),
                "json-processor OOM({fail_at}) on {label} crashed with status {code:?}"
            );
            if code != Some(0) {
                json_oom_hits += 1;
                assert!(
                    !json_out.exists(),
                    "json-processor OOM({fail_at}) on {label} must not leave partial output file"
                );
            }
        }

        for (label, exe) in &todo_bins {
            std::fs::write(&todo_db, initial_todo).expect("reset todos.db");
            let _ = std::fs::remove_file(&todo_tmp);
            let (code, _out, _err) =
                run_cli_with_env(exe, &[todo_db_str, "add", "Should not corrupt"], &envs);
            assert!(
                matches!(code, Some(0) | Some(1) | Some(2)),
                "cli-todo-app OOM({fail_at}) on {label} crashed with status {code:?}"
            );
            if code != Some(0) {
                todo_oom_hits += 1;
                assert_eq!(
                    std::fs::read_to_string(&todo_db).expect("read todos.db after OOM"),
                    initial_todo,
                    "cli-todo-app OOM({fail_at}) on {label} corrupted existing todos.db"
                );
                assert!(
                    !todo_tmp.exists(),
                    "cli-todo-app OOM({fail_at}) on {label} left stray temp file"
                );
            }
        }
    }

    assert!(text_oom_hits > 0, "expected OOM hits for text-stats");
    assert!(csv_oom_hits > 0, "expected OOM hits for csv-processor");
    assert!(json_oom_hits > 0, "expected OOM hits for json-processor");
    assert!(todo_oom_hits > 0, "expected OOM hits for cli-todo-app");

    let _ = std::fs::remove_dir_all(&work);
}
