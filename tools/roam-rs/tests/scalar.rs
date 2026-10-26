use roam::{domain, host, native, profile::Profile, protocol, scalar};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "roam-scalar-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!("preserved scalar fixture: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn l(n: i128) -> Value {
    scalar::lit(n)
}
fn b(op: &str, a: i128, c: i128) -> Value {
    scalar::bin(op, l(a), l(c))
}
fn expected(width: u32, mode: &str, expr: Value) -> Value {
    scalar::expected(&scalar::program(width, mode, expr)).unwrap()
}
fn run(root: &Temp, name: &str, trace: &Value, fault: &str) -> Value {
    host::supervise(
        trace,
        host::HostOptions {
            command: Path::new(env!("CARGO_BIN_EXE_roam")),
            args: vec![
                "worker".into(),
                format!("--fault={fault}"),
                "--budget-ms=30000".into(),
            ],
            timeout_ms: 30_000,
            max_output: protocol::MAX_FRAME,
            attempt: &root.path(name),
        },
    )
    .unwrap()
    .result
}

#[test]
fn independent_model_anchors_cover_width_traps_wrapping_and_lazy_evaluation() {
    for width in [32, 64] {
        let min = -(1i128 << (width - 1));
        let max = -min - 1;
        for (expr, value) in [
            (l(min), min),
            (l(max), max),
            (b("+%", max, 1), min),
            (b("-%", min, 1), max),
            (b("*%", max, 3), max - 2),
            (b("/", -17, 5), -3),
            (b("%", -17, 5), -2),
        ] {
            assert_eq!(
                expected(width, "expr", expr),
                json!({"kind":"value","value":value.to_string()})
            );
        }
        for expr in [
            b("+", max, 1),
            b("-", min, 1),
            b("*", max, 2),
            b("/", 7, 0),
            b("/", min, -1),
            b("%", min, -1),
            json!({"kind":"neg","value":l(min)}),
        ] {
            assert_eq!(
                expected(width, "expr", expr.clone()),
                json!({"kind":"trap"})
            );
            assert_eq!(expected(width, "ct", expr), json!({"kind":"ct-reject"}));
        }
        for (op, left, value) in [("&&", false, 0), ("||", true, 1)] {
            let mut p = scalar::program(width, "short", b("/", 1, 0));
            p["operator"] = json!(op);
            p["left"] = json!(left);
            assert_eq!(
                scalar::expected(&p).unwrap(),
                json!({"kind":"value","value":value.to_string()})
            );
        }
        let mut p = scalar::program(width, "loop", l(8));
        p["other"] = b("/", 1, 0);
        p["operator"] = json!("+");
        p["count"] = json!(0);
        assert_eq!(
            scalar::expected(&p).unwrap(),
            json!({"kind":"value","value":"8"})
        );
    }
    let p = scalar::program(64, "expr", b("-", 9, 4));
    assert_eq!(
        scalar::expected(&p).unwrap(),
        json!({"kind":"value","value":"5"})
    );
    assert_ne!(
        scalar::source(&p, false).unwrap(),
        scalar::source(&p, true).unwrap()
    );
    assert_eq!(
        scalar::expected(&p).unwrap(),
        json!({"kind":"value","value":"5"})
    );
}

#[test]
fn typed_generation_is_deterministic_and_invalid_input_never_becomes_expected() {
    for seed in [0, 1, 7, 31, 65535, 0xffffffff] {
        let a = scalar::generate(seed, 12);
        assert_eq!(a, scalar::generate(seed, 12));
        assert_eq!(a["steps"].as_array().unwrap().len(), 12);
        for a in a["steps"].as_array().unwrap() {
            scalar::validate(&a["program"]).unwrap();
            scalar::expected(&a["program"]).unwrap();
        }
    }
    for value in [
        json!(1),
        json!("01"),
        json!("+1"),
        json!("9223372036854775808"),
    ] {
        let mut p = scalar::program(64, "expr", l(1));
        p["expr"]["value"] = value;
        assert!(scalar::expected(&p).is_err());
    }
    let negative_zero = scalar::program(64, "expr", json!({"kind":"lit","value":"-0"}));
    assert_eq!(
        scalar::expected(&negative_zero).unwrap(),
        json!({"kind":"value","value":"0"})
    );
    // Existing TS zil-scalar-native/1 accepts up to 256 total expression nodes.
    // This balanced 255-node tree was rejected by the earlier Rust 128-node cap.
    fn tree(depth: usize) -> Value {
        if depth == 0 {
            l(1)
        } else {
            scalar::bin("+", tree(depth - 1), tree(depth - 1))
        }
    }
    let full = scalar::program(32, "expr", tree(7));
    assert_eq!(
        scalar::expected(&full).unwrap(),
        json!({"kind":"value","value":"128"})
    );
    let mut excessive = scalar::program(32, "locals", tree(7));
    excessive["other"] = scalar::bin("+", l(1), l(1));
    excessive["operator"] = json!("+");
    assert!(scalar::expected(&excessive).is_err());
    let interop = Command::new("node")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(["--experimental-transform-types", "--import", "./tools/roam/register.mjs", "--input-type=module", "-e", "import {validate,expected} from './tests/oracle/domain.mjs'; process.stdout.write(JSON.stringify(JSON.parse(process.argv[1]).map(p=>({valid:validate(p),expected:validate(p)?expected(p):null}))));"])
        .arg(json!([negative_zero,full,excessive]).to_string())
        .output().unwrap();
    assert!(
        interop.status.success(),
        "{}",
        String::from_utf8_lossy(&interop.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&interop.stdout).unwrap(),
        json!([
            {"valid":true,"expected":{"kind":"value","value":"0"}},
            {"valid":true,"expected":{"kind":"value","value":"128"}},
            {"valid":false,"expected":null}
        ])
    );
    assert!(Profile::named("missing").is_err());
    assert!(Profile::Scalar.fault("stale-after-edit").is_err());
    assert!(Profile::Session.fault("scalar-sub-to-add").is_err());
}

#[test]
fn actual_native_matches_independent_pins_and_generated_programs() {
    let root = Temp::new();
    let mut outcomes = std::collections::BTreeSet::new();
    let mut executed = 0;
    let mut cases = scalar::fixed();
    cases.push(("seed-7".into(), scalar::generate(7, 12)));
    cases.push(("seed-31".into(), scalar::generate(31, 8)));
    for (name, case) in cases {
        let result = run(&root, &name, &case, "none");
        assert_eq!(result["status"], "pass", "{result}");
        assert_eq!(result["steps"], case["steps"].as_array().unwrap().len());
        assert!(!root.path(&format!("{name}/native")).exists());
        for entry in result["nativeEvidence"].as_array().unwrap() {
            assert_eq!(entry["expected"], entry["observed"]);
            assert!(entry["source"].as_str().unwrap().contains("fn value()"));
            outcomes.insert(entry["observed"]["kind"].as_str().unwrap().to_owned());
            executed += 1;
            if entry["compile"]["status"] == "Success" {
                assert!(entry["executableSha256"].is_string());
                assert!(entry["llvmSha256"].is_string());
                assert_eq!(entry["clangProcess"]["exit"], 0);
            }
        }
    }
    assert!(executed >= 60);
    assert_eq!(
        outcomes,
        std::collections::BTreeSet::from(["value".into(), "trap".into(), "ct-reject".into()])
    );
}

#[test]
fn native_fault_is_observable_and_identity_budget_invalid_statuses_remain_distinct() {
    let root = Temp::new();
    let trace = scalar::mutation_case();
    let failed = run(&root, "mutation", &trace, "scalar-sub-to-add");
    assert_eq!(failed["status"], "fail");
    assert_eq!(failed["failure"]["propertyId"], scalar::PROPERTY);
    assert_eq!(failed["steps"], 2);
    assert_eq!(failed["nativeEvidence"][1]["expected"]["value"], "5");
    assert_eq!(failed["nativeEvidence"][1]["observed"]["value"], "13");
    assert_eq!(run(&root, "control", &trace, "none")["status"], "pass");
    let mut identity = trace.clone();
    native::bind_identity(&mut identity, &native::identity().unwrap()).unwrap();
    identity["meta"]["nativeToolchain"]["sha256"] = json!("wrong");
    let mismatch = run(&root, "identity", &identity, "none");
    assert_eq!(mismatch["status"], "error");
    assert_eq!(mismatch["steps"], 0);
    let mut invalid = trace.clone();
    invalid["steps"][0]["program"]["width"] = json!(128);
    assert_eq!(run(&root, "invalid", &invalid, "none")["status"], "invalid");
    assert_eq!(
        run(&root, "zero", &scalar::trace(vec![], None), "none")["status"],
        "incomplete"
    );
    let options = domain::Options {
        budget_ms: 0,
        ..domain::Options::default()
    };
    assert_eq!(
        native::execute(&trace, &options, |_, _| panic!(), |_| panic!())["status"],
        "incomplete"
    );
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let a = root.path("clang-a");
        let b = root.path("clang-b");
        let alias = root.path("clang-alias");
        let marker = root.path("wrong-compiler-ran");
        fs::write(&a, format!("#!/usr/bin/env python3\nimport sys,os\nif '--version' in sys.argv:\n os.unlink({:?});os.symlink({:?},{:?});print('controlled identity swap');sys.exit(0)\nsys.exit(2)\n", alias.to_string_lossy(),b.to_string_lossy(),alias.to_string_lossy())).unwrap();
        fs::write(
            &b,
            format!(
                "#!/usr/bin/env python3\nopen({:?},'w').write('unexpected')\n",
                marker.to_string_lossy()
            ),
        )
        .unwrap();
        for p in [&a, &b] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        symlink(&a, &alias).unwrap();
        let wrapper = "import os,sys; e=os.environ.copy(); e['ZIL_CLANG']=sys.argv[2]; os.execve(sys.argv[1],[sys.argv[1],'worker'],e)";
        let guarded = host::supervise(
            &scalar::trace(vec![scalar::program(32, "expr", l(1))], None),
            host::HostOptions {
                command: Path::new("python3"),
                args: vec![
                    "-c".into(),
                    wrapper.into(),
                    env!("CARGO_BIN_EXE_roam").into(),
                    alias.to_string_lossy().into_owned(),
                ],
                timeout_ms: 10_000,
                max_output: protocol::MAX_FRAME,
                attempt: &root.path("symlink-drift"),
            },
        )
        .unwrap()
        .result;
        assert_eq!(guarded["status"], "error", "{guarded}");
        assert!(guarded.to_string().contains("identity drift"), "{guarded}");
        assert!(!marker.exists(), "unbound replacement compiler executed");
    }
}

#[cfg(target_os = "linux")]
fn live(pid: u64) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .is_some_and(|s| {
            s.rsplit_once(')')
                .is_some_and(|(_, tail)| !tail.trim_start().starts_with('Z'))
        })
}

#[test]
#[cfg(target_os = "linux")]
fn native_timeout_and_output_quota_reclaim_owned_descendants_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = Temp::new();
    let mut unrelated = OwnedChild(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for mode in ["timeout", "flood"] {
        let script = root.path(&format!("clang-{mode}"));
        let pids = root.path(&format!("{mode}-pids.json"));
        let text = format!(
            "#!/usr/bin/env python3\nimport sys,os,subprocess,json,time\nif '--version' in sys.argv: print('controlled clang fixture');sys.exit(0)\np=subprocess.Popen(['/bin/sleep','30'])\nopen({:?},'w').write(json.dumps([os.getpid(),p.pid]))\nif {:?} == 'flood':\n while True: os.write(1,b'x'*4096)\nelse: time.sleep(30)\n",
            pids.to_string_lossy(),
            mode
        );
        fs::write(&script, text).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let wrapper = "import os,sys; e=os.environ.copy(); e['ZIL_CLANG']=sys.argv[2]; os.execve(sys.argv[1],[sys.argv[1],'worker'],e)";
        let trace = scalar::trace(vec![scalar::program(32, "expr", l(1))], None);
        let result = host::supervise(
            &trace,
            host::HostOptions {
                command: Path::new("python3"),
                args: vec![
                    "-c".into(),
                    wrapper.into(),
                    env!("CARGO_BIN_EXE_roam").into(),
                    script.to_string_lossy().into_owned(),
                ],
                timeout_ms: if mode == "timeout" { 1500 } else { 10_000 },
                max_output: protocol::MAX_FRAME,
                attempt: &root.path(mode),
            },
        )
        .unwrap()
        .result;
        assert_eq!(result["status"], "error", "{result}");
        if mode == "timeout" {
            assert_eq!(result["reason"], "timeout");
        } else {
            assert!(result.to_string().contains("output quota"), "{result}");
        }
        let ids: Vec<u64> = serde_json::from_slice(&fs::read(pids).unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while ids.iter().any(|p| live(*p)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            ids.iter().all(|p| !live(*p)),
            "owned descendants survived: {ids:?}"
        );
        assert!(
            unrelated.0.try_wait().unwrap().is_none(),
            "unrelated process was signalled"
        );
        assert!(!root.path(&format!("{mode}/native")).exists());
    }
}
