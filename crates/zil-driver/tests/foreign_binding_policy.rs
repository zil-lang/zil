//! Linux binding policy: actual ZIL -> verified LLVM -> native ABI/loader controls.
#![cfg(target_os = "linux")]
use std::{
    fs,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use zil_backend_llvm::{EmitOptions, EntryPoint};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile, json_string};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(name: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "binding108-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, keep.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn clang() -> Command {
    Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
}
fn execute(case: &Case, label: &str, mut command: Command, input: &[u8]) -> Output {
    let args = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|s| json_string(&s.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(",");
    let environment = command
        .get_envs()
        .map(|(key, value)| {
            format!(
                "{}:{}",
                json_string(&key.to_string_lossy()),
                value.map_or_else(|| "null".into(), |s| json_string(&s.to_string_lossy()))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        case.0.join(format!("{label}.command.json")),
        format!("{{\"argv\":[{args}],\"environment\":{{{environment}}}}}"),
    )
    .unwrap();
    fs::write(case.0.join(format!("{label}.stdin")), input).unwrap();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    fs::write(case.0.join(format!("{label}.stdout")), &output.stdout).unwrap();
    fs::write(case.0.join(format!("{label}.stderr")), &output.stderr).unwrap();
    fs::write(
        case.0.join(format!("{label}.status")),
        format!("{:?}\n", output.status),
    )
    .unwrap();
    output
}
fn checked(case: &Case, label: &str, command: Command) {
    let output = execute(case, label, command, &[]);
    assert!(output.status.success(), "{label}: {output:?}");
}
fn emit(case: &Case, label: &str, text: &str, target: TargetProfile) -> String {
    fs::write(case.0.join(format!("{label}.zil")), text).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "binding108.zil", text);
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Lower,
            entry: Some("value".into()),
            emit_dir: Some(case.0.join(format!("{label}-phases"))),
            target: target.clone(),
            ..CompileOptions::default()
        },
    );
    fs::write(
        case.0.join(format!("{label}.compile.json")),
        report.to_json(),
    )
    .unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    // Driver's Lower stop intentionally precedes LLVM emission. The ABI oracle
    // needs external objects, so emit through the public independently checked
    // source pipeline and perform the explicit native link below.
    let limits = Limits::default();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    fs::write(
        case.0.join(format!("{label}.candidate.bin")),
        zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap(),
    )
    .unwrap();
    let verified = zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
    let entry = verified
        .module()
        .functions
        .iter()
        .find(|f| f.name == "value")
        .unwrap()
        .id;
    let lir = zil_lower::lower(&verified).unwrap();
    let llvm = zil_backend_llvm::emit_with_options(
        &verified,
        &lir,
        &EmitOptions {
            entry: EntryPoint::ReturnValue(entry),
        },
    )
    .unwrap();
    fs::write(case.0.join(format!("{label}.ll")), &llvm).unwrap();
    llvm
}
fn link(
    case: &Case,
    name: &str,
    input: &Path,
    object: &Path,
    opt: &str,
    flags: &[&str],
) -> PathBuf {
    let output = case.0.join(name);
    let mut command = clang();
    command
        .args(["--target=x86_64-unknown-linux-gnu", opt, "-Werror"])
        .arg(input)
        .arg(object)
        .args(flags)
        .arg("-o")
        .arg(&output);
    checked(case, &format!("link-{name}"), command);
    output
}
fn inspect_machine(case: &Case, name: &str, path: &Path) {
    let mut command = Command::new("objdump");
    command.args(["-d", "-Mintel"]).arg(path);
    checked(case, &format!("{name}-assembly"), command);
    let mut command = Command::new("readelf");
    command.arg("-Wr").arg(path);
    checked(case, &format!("{name}-relocations"), command);
}

const SOURCE: &str = r#"
extern "C" fn getchar()->i32;
extern "C" fn tick()->i32;
extern "C" fn echo8(x:u8)->u8;
extern "C" fn echo16(x:u16)->u16;
extern "C" fn selected(x:i32)->i32;
extern "C" fn mutate(bytes:&mut [u8;4])->i32
contract{provides(noescape(bytes));effects(ffi);};
fn value()->i64{unsafe{
    let byte=u8(getchar());
    if tick()!=1{return -1}if tick()!=2{return -2}
    if echo8(255u8)!=255u8{return -3}
    if echo16(60000u16)!=60000u16{return -4}
    var bytes=[1u8,2u8,3u8,4u8];
    if mutate(&mut bytes)!=23{return -5}
    if bytes[0]!=9u8||bytes[1]!=7u8||bytes[2]!=3u8||bytes[3]!=4u8{return -6}
    return i64(selected(i32(byte)))
}}
"#;
const ORACLE: &str = r#"
typedef unsigned char U8; typedef unsigned short U16;
static int calls;
int tick(void){return ++calls;}
U8 echo8(U8 x){return x;} U16 echo16(U16 x){return x;}
int selected(int x){return x+40;}
int mutate(U8 *p){if(p[0]!=1||p[1]!=2||p[2]!=3||p[3]!=4)return -1;p[0]=9;p[1]=7;return 23;}
"#;

#[test]
fn policy_is_linux_only_and_keeps_narrow_abi_and_effect_contracts() {
    let case = Case::new("emission");
    let linux = emit(&case, "linux", SOURCE, TargetProfile::host_llvm());
    let windows = emit(&case, "windows", SOURCE, TargetProfile::windows_x64_llvm());
    assert!(!windows.contains("nonlazybind"));
    for name in ["getchar", "tick", "echo8", "echo16", "selected", "mutate"] {
        let line = linux
            .lines()
            .find(|line| line.starts_with("declare ") && line.contains(&format!("@{name}(")))
            .unwrap();
        assert!(line.ends_with(" nonlazybind"), "{line}");
        for forbidden in [
            "readonly",
            "readnone",
            "nounwind",
            "nofree",
            "nosync",
            "nocallback",
            "nocapture",
            "noalias",
            "dso_local",
            "hidden",
        ] {
            assert!(!line.contains(forbidden), "{line}");
        }
        let expected = line
            .replace("zeroext ", "")
            .replace(" zeroext", "")
            .replace(" nonlazybind", "");
        assert!(windows.lines().any(|line| line == expected), "{expected}");
    }
    assert!(linux.contains("declare zeroext i8 @echo8(i8 zeroext) nonlazybind"));
    assert!(linux.contains("declare zeroext i16 @echo16(i16 zeroext) nonlazybind"));
    assert!(linux.contains("call zeroext i8 @echo8(i8 zeroext"));
    assert!(linux.contains("call zeroext i16 @echo16(i16 zeroext"));
    assert!(linux.contains("@llvm.trap"));
}

#[test]
fn actual_object_shared_preload_and_static_calls_keep_values_order_and_checks() {
    for opt in ["-O0", "-O2"] {
        let case = Case::new(opt);
        let llvm = emit(&case, "source", SOURCE, TargetProfile::host_llvm());
        fs::write(case.0.join("oracle.c"), ORACLE).unwrap();
        fs::write(
            case.0.join("preload.c"),
            "int selected(int x){return x+80;}\n",
        )
        .unwrap();
        let object = case.0.join("oracle.o");
        let library = case.0.join("libbinding108.so");
        let preload = case.0.join("liboverride108.so");
        let mut command = clang();
        command
            .args(["-std=c11", "-O2", "-Werror", "-c"])
            .arg(case.0.join("oracle.c"))
            .arg("-o")
            .arg(&object);
        checked(&case, "oracle-object", command);
        for (label, input, output) in [
            ("oracle-shared", "oracle.c", &library),
            ("preload-shared", "preload.c", &preload),
        ] {
            let mut command = clang();
            command
                .args(["-std=c11", "-O2", "-Werror", "-shared", "-fPIC"])
                .arg(case.0.join(input))
                .arg("-o")
                .arg(output);
            checked(&case, label, command);
        }
        // This is a machine attribute ablation, not the separately frozen
        // production compiler arm used in the performance experiment.
        fs::write(case.0.join("ablation.ll"), llvm.replace(" nonlazybind", "")).unwrap();
        for (arm, file) in [
            ("candidate", "source.ll"),
            ("attribute-ablation", "ablation.ll"),
        ] {
            for (kind, extra, flags) in [
                ("object", &object, vec![]),
                ("shared", &library, vec!["-Wl,-rpath,$ORIGIN"]),
                ("static", &object, vec!["-static"]),
            ] {
                let name = format!("{arm}-{kind}");
                let binary = link(&case, &name, &case.0.join(file), extra, opt, &flags);
                let output = execute(&case, &format!("run-{name}"), Command::new(&binary), &[3]);
                assert!(output.status.success(), "{name}: {output:?}");
                assert_eq!(output.stdout, b"43\n");
                assert!(output.stderr.is_empty());
                if kind == "shared" {
                    let mut command = Command::new(&binary);
                    command.env("LD_PRELOAD", &preload);
                    let output = execute(&case, &format!("preloaded-{name}"), command, &[3]);
                    assert!(output.status.success(), "{output:?}");
                    assert_eq!(output.stdout, b"83\n");
                    assert!(output.stderr.is_empty());
                    inspect_machine(&case, &name, &binary);
                    let relocations =
                        fs::read_to_string(case.0.join(format!("{name}-relocations.stdout")))
                            .unwrap();
                    let selected = relocations
                        .lines()
                        .find(|line| line.contains("selected"))
                        .unwrap();
                    assert!(
                        selected.contains(if arm == "candidate" {
                            "GLOB_DAT"
                        } else {
                            "JUMP_SLOT"
                        }),
                        "{selected}"
                    );
                }
                if kind == "static" {
                    let mut command = Command::new("readelf");
                    command.arg("-l").arg(&binary);
                    checked(&case, &format!("{name}-segments"), command);
                    assert!(
                        !fs::read_to_string(case.0.join(format!("{name}-segments.stdout")))
                            .unwrap()
                            .contains("INTERP")
                    );
                }
                let output = execute(&case, &format!("trap-{name}"), Command::new(&binary), &[]);
                assert_eq!(output.status.signal(), Some(4), "{output:?}");
                assert!(output.stdout.is_empty());
            }
        }
    }
}

#[test]
fn loader_resolver_timing_is_explicitly_observed_not_mislabeled_equivalent() {
    let case = Case::new("ifunc");
    let text = "extern \"C\" fn phase_now()->i32;extern \"C\" fn choose(x:i32)->i32;fn value()->i64{unsafe{let before=phase_now();let value=choose(41);let after=phase_now();return i64(before*100+value+after*10)}}";
    let llvm = emit(&case, "source", text, TargetProfile::host_llvm());
    fs::write(case.0.join("ablation.ll"), llvm.replace(" nonlazybind", "")).unwrap();
    fs::write(
        case.0.join("ifunc.c"),
        r#"
typedef int (*Impl)(int);
static volatile int phase;
static int implementation(int value){return value+1;}
static Impl resolve(void){phase=1;return implementation;}
int choose(int value) __attribute__((ifunc("resolve")));
int phase_now(void){return phase;}
"#,
    )
    .unwrap();
    let library = case.0.join("libifunc108.so");
    let mut command = clang();
    command
        .args(["-std=c11", "-O2", "-shared", "-fPIC", "-Werror"])
        .arg(case.0.join("ifunc.c"))
        .arg("-o")
        .arg(&library);
    checked(&case, "ifunc-shared", command);
    for opt in ["-O0", "-O2"] {
        for (arm, file, expected) in [
            ("candidate", "source.ll", b"152\n".as_slice()),
            ("attribute-ablation", "ablation.ll", b"52\n".as_slice()),
        ] {
            let name = format!("{arm}{opt}");
            let binary = link(
                &case,
                &name,
                &case.0.join(file),
                &library,
                opt,
                &["-Wl,-rpath,$ORIGIN"],
            );
            let output = execute(&case, &format!("run-{name}"), Command::new(&binary), &[]);
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty());
            inspect_machine(&case, &name, &binary);
        }
    }
}

#[test]
fn used_missing_symbol_still_fails_link_and_unused_declaration_is_not_bound() {
    let case = Case::new("missing");
    for (kind, text) in [
        (
            "used",
            "extern \"C\" fn missing108()->i32;fn value()->i64{unsafe{return i64(missing108())}}",
        ),
        (
            "unused",
            "extern \"C\" fn missing108()->i32;fn value()->i64{return 42}",
        ),
    ] {
        let llvm = emit(&case, kind, text, TargetProfile::host_llvm());
        for (arm, text) in [
            ("candidate", llvm.clone()),
            ("attribute-ablation", llvm.replace(" nonlazybind", "")),
        ] {
            let name = format!("{kind}-{arm}");
            let input = case.0.join(format!("{name}.ll"));
            fs::write(&input, text).unwrap();
            let output = case.0.join(&name);
            let mut command = clang();
            command
                .args(["--target=x86_64-unknown-linux-gnu", "-O2", "-Werror"])
                .arg(&input)
                .arg("-o")
                .arg(&output);
            let result = execute(&case, &format!("link-{name}"), command, &[]);
            if kind == "used" {
                assert!(!result.status.success());
                assert!(String::from_utf8_lossy(&result.stderr).contains("missing108"));
                assert!(!output.exists());
            } else {
                assert!(result.status.success(), "{result:?}");
                let result = execute(&case, &format!("run-{name}"), Command::new(output), &[]);
                assert!(result.status.success(), "{result:?}");
                assert_eq!(result.stdout, b"42\n");
            }
        }
    }
}
