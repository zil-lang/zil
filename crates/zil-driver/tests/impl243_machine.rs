//! Real source to the shared LLVM machine builder. Not independent review.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const ORIGINAL03: &str =
    include_str!("../../../evidence/impl/243/original_q_sources/03-unit-store-session.zil");
const ORIGINAL05: &str =
    include_str!("../../../evidence/impl/243/original_q_sources/05-owned-option-session.zil");
const NESTED: &str = r#"
extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
fn read()->i32{unsafe{return getchar()}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
enum Inner{Nil,Pair(manages Mark,manages Mark)}
enum Outer{Empty,Pair(manages Mark,Inner),Copy(i32)}
fn make(mode:i32)->Outer{
 if mode==1{return Outer.Empty}
 if mode==2{return Outer.Pair(Mark{code:65},Inner.Nil)}
 if mode==3{return Outer.Copy(7)}
 return Outer.Pair(Mark{code:65},Inner.Pair(Mark{code:67},Mark{code:68}))
}
fn main()->i32{
 let input=read()
 emit(83)
 let value=make(input-48)
 match value{
  Outer.Pair(a,Inner.Pair(c,d))=>{}
  Outer.Pair(_,Inner.Nil())=>{}
  Outer.Empty()=>{}
  Outer.Copy(7)=>{emit(75);return 0}
  Outer.Copy(n)=>{return n-7}
 }
 emit(90)
 return 0
}
"#;
const ENTRY: &str = r#"
extern "C" fn getchar()->i32;
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
fn read()->i32{unsafe{return getchar()}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Item{manages mark:Mark;manages bytes:ByteBuffer}
package Items{manages values:Store<Item>}
enum Fault{Empty,Failed(Items)}
fn main()->Fault!i32{
 let input=read()
 emit(83)
 let values:Store<Item>=store_new()
 if !store_reserve(values,1){return Fault.Empty}
 if input==49{
  let bytes=bytes_new()
  if !bytes_reserve(bytes,1){return Fault.Empty}
  bytes_push(bytes,7u8)
  store_push(values,Item{mark:Mark{code:69},bytes:give bytes})
  return Fault.Failed(Items{values:give values})
 }
 emit(90)
 return 0
}
"#;
fn dir(name: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!(
        "impl243-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn run(binary: &Path, input: &str) -> std::process::Output {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn native(name: &str, source: &str, rows: &[(&str, i32, &str)]) {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let path = dir(&format!("{name}-{backend:?}-{optimization:?}"));
            fs::write(path.join("main.zil"), source).unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    emit_dir: Some(path.join("phases")),
                    output: Some(path.join("program")),
                    ..CompileOptions::default()
                },
            );
            fs::write(path.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let actual = report.native_codegen.as_ref().unwrap();
            assert_eq!(actual.backend, backend);
            assert!(actual.fallback.is_none());
            for (input, exit, stdout) in rows {
                let output = run(Path::new(&report.artifacts["executable"]), input);
                fs::write(path.join(format!("input-{input}.stdout")), &output.stdout).unwrap();
                fs::write(path.join(format!("input-{input}.stderr")), &output.stderr).unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(*exit),
                    "{name}/{input}: {output:?}"
                );
                assert_eq!(output.stdout, stdout.as_bytes(), "{name}/{input}");
                assert!(output.stderr.is_empty());
            }
        }
    }
}
#[test]
fn impl243_original_store_product_error_and_owned_option_reach_both_machine_paths() {
    native("original03", ORIGINAL03, &[("0", 0, "SRA"), ("1", 1, "SA")]);
    native("original05", ORIGINAL05, &[("0", 0, "SRA"), ("1", 0, "SR")]);
}
#[test]
fn impl243_recursive_match_multi_field_cleanup_and_copy_terminal() {
    native(
        "nested",
        NESTED,
        &[
            ("0", 0, "SACDZ"),
            ("1", 0, "SZ"),
            ("2", 0, "SAZ"),
            ("3", 0, "SK"),
        ],
    );
}
#[test]
fn impl243_entry_owned_enum_error_cleans_nested_store_payload() {
    native("entry", ENTRY, &[("0", 0, "SZ"), ("1", 1, "SE")]);
}

#[cfg(all(unix, feature = "llvm-api"))]
#[test]
fn impl243_actual_api_object_entry_faults_settle_only_selected_live_payload() {
    use std::os::unix::fs::PermissionsExt;
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/173/allocator_probe.c");
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = dir(&format!("entry-faults-{optimization:?}"));
        fs::write(path.join("main.zil"), ENTRY).unwrap();
        let wrapper = path.join("capture-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&path.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "entry.zil", ENTRY),
            &CompileOptions {
                backend: NativeBackend::LlvmApi,
                clang: Some(wrapper),
                optimization,
                stop_after: Stage::Build,
                emit_dir: Some(path.join("phases")),
                output: Some(path.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let actual = report.native_codegen.as_ref().unwrap();
        assert_eq!(actual.backend, NativeBackend::LlvmApi);
        assert!(actual.fallback.is_none());
        assert_eq!(actual.actual_units, 1);
        assert!(!actual.object_reports.is_empty());
        assert!(
            actual
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true"))
        );
        let probe = path.join("probe");
        let link = Command::new(&clang)
            .arg(path.join("api.o"))
            .arg(&shim)
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&probe)
            .output()
            .unwrap();
        fs::write(path.join("link.stdout"), &link.stdout).unwrap();
        fs::write(path.join("link.stderr"), &link.stderr).unwrap();
        assert!(link.status.success(), "{link:?}");
        for input in ["0", "1"] {
            let requests = if input == "0" { 1 } else { 2 };
            for fail in 0..=requests + 1 {
                let mut child = Command::new(&probe)
                    .env("ZIL_TEST_FAIL_AT", fail.to_string())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                fs::write(
                    path.join(format!("input-{input}-fail-{fail}.stdout")),
                    &output.stdout,
                )
                .unwrap();
                fs::write(
                    path.join(format!("input-{input}-fail-{fail}.stderr")),
                    &output.stderr,
                )
                .unwrap();
                let failed = fail > 0 && fail <= requests;
                let expected = if failed {
                    "S"
                } else if input == "0" {
                    "SZ"
                } else {
                    "SE"
                };
                assert_eq!(
                    output.status.code(),
                    Some(i32::from(failed || input == "1")),
                    "{input}/{fail}: {output:?}"
                );
                assert_eq!(output.stdout, expected.as_bytes());
                let calls = if failed { fail } else { requests };
                let allocated = if failed { fail - 1 } else { requests };
                let stats = String::from_utf8(output.stderr).unwrap();
                for fragment in [
                    format!("\"alloc_calls\":{calls},"),
                    format!("\"allocated\":{allocated},"),
                    format!("\"frees\":{allocated},"),
                    format!("\"failures\":{},", usize::from(failed)),
                    "\"live\":0,".into(),
                ] {
                    assert!(
                        stats.contains(&fragment),
                        "{input}/{fail}: missing {fragment} in {stats}"
                    );
                }
            }
        }
    }
}
