use roam::{host, profile::Profile, protocol, resource_closure as model};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "roam-resource-closure-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_roam"), p.join("worker")).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("preserved fixture {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn run(root: &Temp, name: &str, trace: &Value, fault: &str) -> Value {
    host::supervise(
        trace,
        host::HostOptions {
            command: &root.0.join("worker"),
            args: vec![
                "worker".into(),
                format!("--fault={fault}"),
                "--budget-ms=30000".into(),
            ],
            timeout_ms: 30000,
            max_output: protocol::MAX_FRAME,
            attempt: &root.0.join(name),
        },
    )
    .unwrap()
    .result
}
#[test]
fn independent_lifecycle_model_covers_capture_call_and_cleanup_order() {
    for (mode, calls, stdout) in [
        ("shared", 2, "AAA"),
        ("descriptor", 2, "BCC"),
        ("mutable", 2, "BCC"),
        ("consume", 0, "KA"),
        ("consume", 1, "BK"),
        ("uncalled", 0, "AK"),
        ("transfer", 2, "AAA"),
    ] {
        let p = model::program(mode, 65, calls, false);
        assert_eq!(
            model::expected(&p).unwrap(),
            json!({"kind":"lifecycle","stdout":stdout,"stderr":"","exit":0})
        );
    }
    assert_eq!(
        model::expected(&model::program("shared", 65, 0, true)).unwrap()["exit"],
        7
    );
    for p in [
        model::program("missing", 65, 0, false),
        model::program("shared", 1, 0, false),
        model::program("shared", 65, 5, false),
        model::program("consume", 65, 2, false),
        model::program("uncalled", 65, 1, false),
        json!({"mode":"shared","initial":65,"calls":1,"early":0}),
    ] {
        assert!(model::validate(&p).is_err());
        assert!(model::expected(&p).is_err());
        assert!(model::source(&p, false).is_err());
    }
}
#[test]
fn generation_and_shrinking_keep_the_model_contract() {
    for seed in [0, 1, 7, 53, 0xffffffff] {
        let a = model::generate(seed, 20);
        assert_eq!(a, model::generate(seed, 20));
        protocol::validate_case(&a).unwrap();
        assert_eq!(Profile::for_case(&a).unwrap(), Profile::ResourceClosure);
        for action in a["steps"].as_array().unwrap() {
            model::validate(&action["program"]).unwrap();
        }
        for candidate in model::candidates(&a) {
            for action in candidate["steps"].as_array().unwrap() {
                model::validate(&action["program"]).unwrap();
            }
        }
    }
    let case = model::mutation_case();
    let p = &case["steps"][0]["program"];
    let expectation = model::expected(p).unwrap();
    assert_ne!(
        model::source(p, false).unwrap(),
        model::source(p, true).unwrap()
    );
    assert_eq!(model::expected(p).unwrap(), expectation);
    assert_eq!(
        Profile::named("resource-closure-native")
            .unwrap()
            .contract(),
        model::CONTRACT
    );
}
#[test]
fn supervised_native_lifecycles_and_controlled_failure_replay() {
    let root = Temp::new();
    let fixed = model::fixed()
        .into_iter()
        .flat_map(|(_, case)| {
            case["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["program"].clone())
                .collect::<Vec<_>>()
        })
        .collect();
    let trace = model::trace(fixed, None);
    let actual = run(&root, "fixed", &trace, "none");
    assert_eq!(actual["status"], "pass", "{actual}");
    assert_eq!(actual["steps"], 12);
    let generated = model::generate(53, 8);
    let actual = run(&root, "generated", &generated, "none");
    assert_eq!(actual["status"], "pass", "{actual}");
    assert_eq!(actual["steps"], 8);
    let case = model::mutation_case();
    let failed = run(&root, "injected", &case, "closure-omit-call");
    assert_eq!(failed["status"], "fail", "{failed}");
    assert!(failed.to_string().contains(model::PROPERTY));
    let replay = run(&root, "replay", &case, "closure-omit-call");
    assert_eq!(
        protocol::failure_key(&failed),
        protocol::failure_key(&replay)
    );
    let clean = run(&root, "clean", &case, "none");
    assert_eq!(clean["status"], "pass", "{clean}");
    let empty = run(&root, "empty", &model::trace(vec![], None), "none");
    assert_eq!(empty["status"], "incomplete");
    let mut invalid = case;
    invalid["steps"][0]["program"]["calls"] = json!(99);
    assert_eq!(run(&root, "invalid", &invalid, "none")["status"], "invalid");
}

#[test]
fn lifecycle_finding_uses_the_existing_queue_replay_and_shrink_pipeline() {
    let root = Temp::new();
    let input = root.0.join("case.json");
    host::durable(&input, &model::mutation_case()).unwrap();
    let invoke = |arguments: Vec<String>, code: i32| {
        let output = std::process::Command::new(root.0.join("worker"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let queue = format!("--queue={}", root.0.join("queue").display());
    let found = invoke(
        vec![
            "hunt".into(),
            queue.clone(),
            format!("--case={}", input.display()),
            format!("--out={}", root.0.join("hunt").display()),
            "--cases=1".into(),
            "--fault=closure-omit-call".into(),
            "--allow-dirty=true".into(),
        ],
        1,
    );
    assert_eq!(found["counts"]["fail"], 1);
    let id = found["findings"][0].as_str().unwrap();
    let finding = format!("--id={id}");
    let replay = invoke(
        vec![
            "issue".into(),
            "replay".into(),
            queue.clone(),
            finding.clone(),
            format!("--out={}", root.0.join("replay").display()),
        ],
        0,
    );
    assert_eq!(replay["reproduced"], true);
    let claim = invoke(
        vec![
            "issue".into(),
            "claim".into(),
            queue.clone(),
            finding.clone(),
            "--owner=resource-oracle-test".into(),
        ],
        0,
    );
    let token = format!("--token={}", claim["token"].as_str().unwrap());
    let shrink = invoke(
        vec![
            "issue".into(),
            "reduce".into(),
            queue.clone(),
            finding.clone(),
            token.clone(),
            format!("--out={}", root.0.join("reduced").display()),
            "--max-runs=20".into(),
            "--budget-ms=10000".into(),
        ],
        0,
    );
    assert_eq!(shrink["sameFailure"], true);
    assert!(shrink["astAccepted"].as_u64().unwrap() > 0);
    assert_eq!(shrink["result"]["failure"]["propertyId"], model::PROPERTY);
    let verified = invoke(
        vec![
            "issue".into(),
            "verify".into(),
            queue,
            finding,
            token,
            "--allow-dirty=true".into(),
        ],
        0,
    );
    assert_eq!(verified["closed"], true);
    let original = fs::read(&input).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&original).unwrap(),
        model::mutation_case()
    );
}
