use roam::{byte_buffer as model, host, profile::Profile, protocol};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf, bool);
impl Temp {
    fn new() -> Self {
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR").map(PathBuf::from);
        let path = base
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "roam-bytes098-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_roam"), path.join("worker")).unwrap();
        Self(path, base.is_some())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 && !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn run(t: &Temp, name: &str, case: &Value, fault: &str) -> Value {
    host::durable(&t.0.join(format!("{name}.case.json")), case).unwrap();
    let r = host::supervise(
        case,
        host::HostOptions {
            command: &t.0.join("worker"),
            args: vec![
                "worker".into(),
                format!("--fault={fault}"),
                "--budget-ms=30000".into(),
            ],
            timeout_ms: 30000,
            max_output: protocol::MAX_FRAME,
            attempt: &t.0.join(name),
        },
    )
    .unwrap()
    .result;
    host::durable(&t.0.join(format!("{name}.result.json")), &r).unwrap();
    r
}

#[test]
fn byte_storage_model_pins_capacity_failure_and_initialized_bytes() {
    let p = model::program(
        vec![
            json!({"op":"reserve","capacity":2}),
            json!({"op":"push","value":0}),
            json!({"op":"push","value":255}),
            json!({"op":"push","value":7}),
            json!({"op":"reserve","capacity":1}),
            json!({"op":"fill","lo":0,"hi":2,"value":128}),
            json!({"op":"relay"}),
            json!({"op":"reset"}),
        ],
        true,
    );
    assert_eq!(
        model::expected(&p).unwrap(),
        json!({"kind":"lifecycle","stdout":"T0000;T0002;T010200;T020200FF;F020200FF;T020200FF;T02028080;T02028080;T0000;","stderr":"","exit":7})
    );
    for invalid in [
        model::program(vec![json!({"op":"reserve","capacity":65})], false),
        model::program(vec![json!({"op":"push","value":-1})], false),
        model::program(vec![json!({"op":"relay"}); 17], false),
        json!({"operations":[],"early":0}),
    ] {
        assert!(model::expected(&invalid).is_err());
        assert!(model::source(&invalid, false).is_err());
        assert!(model::c_source(&invalid).is_err());
    }
    for seed in [0, 1, 7, 98, 0xffffffff] {
        let t = model::generate(seed, 8);
        assert_eq!(t, model::generate(seed, 8));
        assert_eq!(Profile::for_case(&t).unwrap(), Profile::ByteBuffer);
        for s in t["steps"].as_array().unwrap() {
            model::validate(&s["program"]).unwrap();
        }
        for c in model::candidates(&t) {
            protocol::validate_case(&c).unwrap();
            for s in c["steps"].as_array().unwrap() {
                model::validate(&s["program"]).unwrap();
            }
        }
    }
}

#[test]
fn byte_storage_native_c_and_model_agree_at_both_optimizations() {
    let t = Temp::new();
    let all = model::fixed()
        .into_iter()
        .flat_map(|(_, c)| {
            c["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["program"].clone())
                .collect::<Vec<_>>()
        })
        .collect();
    for (name, case, n) in [
        ("fixed", model::trace(all, None), 6),
        ("generated", model::generate(98, model::MAX_PROGRAMS), 8),
    ] {
        let r = run(&t, name, &case, "none");
        assert_eq!(r["status"], "pass", "{r}");
        assert_eq!(r["steps"], n);
        for e in r["nativeEvidence"].as_array().unwrap() {
            assert_eq!(e["variants"].as_object().unwrap().len(), 4);
            for key in ["zil-O0", "zil-O2"] {
                assert_eq!(e["variants"][key]["compile"]["verification_runs"], 1);
                assert_eq!(e["variants"][key]["compile"]["status"], "Success");
                assert!(e["variants"][key]["llvmSha256"].is_string());
                assert!(
                    e["variants"][key]["clangProcess"]["argv"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(format!("-{}", &key[4..])))
                );
            }
            for key in ["c-O0", "c-O2"] {
                assert_eq!(e["variants"][key]["compile"]["exit"], 0);
                assert!(e["variants"][key]["executableSha256"].is_string());
                assert!(
                    e["variants"][key]["compile"]["argv"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(format!("-{}", &key[2..])))
                );
            }
        }
    }
    let c = model::mutation_case();
    let bad = run(&t, "mutation", &c, "bytes-omit-push");
    assert_eq!(bad["status"], "fail", "{bad}");
    assert_eq!(bad["failure"]["propertyId"], model::PROPERTY);
    let e = &bad["nativeEvidence"][0];
    assert_eq!(e["observed"]["c-O0"], e["expected"]["c-O0"]);
    assert_eq!(e["observed"]["c-O2"], e["expected"]["c-O2"]);
    assert_ne!(e["observed"]["zil-O0"], e["expected"]["zil-O0"]);
    assert_eq!(
        run(&t, "empty", &model::trace(vec![], None), "none")["status"],
        "incomplete"
    );
    let invalid = model::trace(
        vec![model::program(
            vec![json!({"op":"push","value":256})],
            false,
        )],
        None,
    );
    let rejected = run(&t, "invalid", &invalid, "none");
    assert_eq!(rejected["status"], "invalid");
    assert_eq!(rejected["steps"], 0);
    let wrong_mutation = run(&t, "wrong-mutation", &c, "scalar-sub-to-add");
    assert_eq!(wrong_mutation["status"], "invalid");
    assert_eq!(wrong_mutation["steps"], 0);
    let mut unknown = c;
    unknown["contract"] = json!("zil-byte-buffer-native/unknown");
    let unsupported = run(&t, "unknown-contract", &unknown, "none");
    assert_eq!(unsupported["status"], "unsupported");
    assert_eq!(unsupported["steps"], 0);
}

#[test]
fn byte_storage_finding_replays_reduces_rejects_bad_candidate_and_closes() {
    queue_roundtrip(&model::mutation_case(), model::PROPERTY, model::CONTRACT);
}

fn queue_roundtrip(case: &Value, property: &str, contract: &str) {
    let t = Temp::new();
    let input = t.0.join("case.json");
    host::durable(&input, case).unwrap();
    let original = fs::read(&input).unwrap();
    let mut serial = 0;
    let mut invoke = |args: Vec<String>, expected: i32| {
        let output = std::process::Command::new(t.0.join("worker"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(&args)
            .output()
            .unwrap();
        let receipt = json!({"argv":args,"exit":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)});
        host::durable(&t.0.join(format!("command-{serial:03}.json")), &receipt).unwrap();
        serial += 1;
        assert_eq!(output.status.code(), Some(expected), "{receipt}");
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let queue = format!("--queue={}", t.0.join("queue").display());
    let found = invoke(
        vec![
            "hunt".into(),
            queue.clone(),
            format!("--case={}", input.display()),
            format!("--out={}", t.0.join("hunt").display()),
            "--cases=1".into(),
            "--fault=bytes-omit-push".into(),
            "--allow-dirty=true".into(),
        ],
        1,
    );
    assert_eq!(found["counts"]["fail"], 1);
    assert_eq!(found["scope"], contract);
    let id = format!("--id={}", found["findings"][0].as_str().unwrap());
    assert_eq!(
        invoke(
            vec![
                "issue".into(),
                "replay".into(),
                queue.clone(),
                id.clone(),
                format!("--out={}", t.0.join("replay").display())
            ],
            0
        )["reproduced"],
        true
    );
    let claim = invoke(
        vec![
            "issue".into(),
            "claim".into(),
            queue.clone(),
            id.clone(),
            "--owner=byte-storage-test".into(),
        ],
        0,
    );
    let token = format!("--token={}", claim["token"].as_str().unwrap());
    let reduced = invoke(
        vec![
            "issue".into(),
            "reduce".into(),
            queue.clone(),
            id.clone(),
            token.clone(),
            format!("--out={}", t.0.join("reduced").display()),
            "--max-runs=16".into(),
            "--budget-ms=30000".into(),
        ],
        0,
    );
    assert_eq!(reduced["sameFailure"], true);
    assert!(reduced["astAccepted"].as_u64().unwrap() > 0);
    assert_eq!(reduced["result"]["failure"]["propertyId"], property);
    let bad = invoke(
        vec![
            "issue".into(),
            "verify".into(),
            queue.clone(),
            id.clone(),
            token.clone(),
            "--allow-dirty=true".into(),
            "--fault=bytes-omit-push".into(),
        ],
        1,
    );
    assert_eq!(bad["closed"], false);
    let good = invoke(
        vec![
            "issue".into(),
            "verify".into(),
            queue,
            id,
            token,
            "--allow-dirty=true".into(),
        ],
        0,
    );
    assert_eq!(good["closed"], true);
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn owned_byte_model_pins_allocation_identity_failure_and_cleanup_order() {
    use model::owned;
    let ops = vec![
        json!({"op":"reserve","slot":0,"capacity":2}),
        json!({"op":"push","slot":0,"value":65}),
        json!({"op":"reserve","slot":1,"capacity":3}),
        json!({"op":"push","slot":1,"value":66}),
        json!({"op":"reserve","slot":0,"capacity":5}),
        json!({"op":"relay","slot":1}),
        json!({"op":"replace","slot":1,"capacity":4,"value":67}),
        json!({"op":"prepare_error","capacity":7,"value":68}),
        json!({"op":"take_refill","slot":0,"capacity":6,"value":69}),
    ];
    let prefix = "T0000/0000;A0102;T0002/0000;T010241/0000;A0203;T010241/0003;T010241/010342;F0105;F010241/010342;T010241/010342;A0304;D02;T010241/010443;A0407;E;D04;F010241/010443;A0506;D01;T010645/010443;";
    for (carrier, finish, tail) in [
        ("package", "normal", "D05;D03;L00;"),
        ("local", "normal", "D03;D05;L00;"),
        ("fnmut", "error", "E;D05;D03;L00;"),
        ("fnonce", "take1-error", "E;D03;D05;L00;"),
    ] {
        let p = owned::program(carrier, ops.clone(), 3, finish, false);
        assert_eq!(
            owned::expected(&p).unwrap(),
            json!({"kind":"lifecycle","stdout":format!("{prefix}{tail}"),"stderr":"","exit":0})
        );
    }
    for bad in [
        owned::program("invented", vec![], 0, "normal", false),
        owned::program("fnmut", vec![], 0, "take0", false),
        owned::program(
            "package",
            vec![json!({"op":"relay","slot":2})],
            0,
            "normal",
            false,
        ),
        owned::program(
            "package",
            vec![json!({"op":"relay","slot":0}); 17],
            0,
            "normal",
            false,
        ),
        owned::program("package", vec![], 65, "normal", false),
    ] {
        assert!(owned::expected(&bad).is_err());
        assert!(owned::source(&bad, false).is_err());
        assert!(owned::c_source(&bad).is_err());
    }
    for seed in [0, 1, 7, 102, 0xffffffff] {
        let trace = owned::generate(seed, 8);
        assert_eq!(trace, owned::generate(seed, 8));
        assert_eq!(trace["contract"], owned::CONTRACT);
        for s in trace["steps"].as_array().unwrap() {
            owned::validate(&s["program"]).unwrap();
            assert_eq!(s["program"]["failAt"], 0);
        }
        for candidate in owned::candidates(&trace) {
            protocol::validate_case(&candidate).unwrap();
            for s in candidate["steps"].as_array().unwrap() {
                owned::validate(&s["program"]).unwrap();
            }
        }
    }
}

#[test]
fn owned_byte_carriers_match_c_and_model_with_real_allocator_at_o0_o2() {
    use model::owned;
    let t = Temp::new();
    for (name, case) in owned::fixed() {
        let result = run(&t, &name, &case, "none");
        assert_eq!(result["status"], "pass", "{result}");
        assert_eq!(result["steps"], 2);
        for evidence in result["nativeEvidence"].as_array().unwrap() {
            assert_eq!(evidence["modelContract"], owned::CONTRACT);
            assert_eq!(evidence["model"], owned::MODEL);
            assert_eq!(evidence["variants"].as_object().unwrap().len(), 4);
            for key in ["zil-O0", "zil-O2"] {
                let detail = &evidence["variants"][key];
                assert_eq!(detail["compile"]["verification_runs"], 1);
                assert_eq!(
                    detail["clangProcess"]["byteObserver"]["model"],
                    owned::MODEL
                );
                assert!(detail["clangProcess"]["byteObserver"]["sha256"].is_string());
                assert!(
                    detail["clangProcess"]["argv"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("-Wl,--wrap=realloc"))
                );
            }
        }
    }
    let ops = vec![
        json!({"op":"reserve","slot":0,"capacity":2}),
        json!({"op":"push","slot":0,"value":65}),
        json!({"op":"reserve","slot":1,"capacity":3}),
        json!({"op":"replace","slot":0,"capacity":7,"value":66}),
        json!({"op":"push","slot":0,"value":67}),
    ];
    let cases = owned::trace(
        vec![
            owned::program("package", ops.clone(), 1, "normal", false),
            owned::program("fnmut", ops.clone(), 3, "normal", false),
            owned::program("fnonce", ops, 0, "take1", false),
        ],
        None,
    );
    assert_eq!(
        run(&t, "first-growth-replacement-partial", &cases, "none")["status"],
        "pass"
    );
    assert_eq!(
        run(
            &t,
            "max-generated",
            &owned::generate(102, model::MAX_PROGRAMS),
            "none"
        )["status"],
        "pass"
    );
    let empty = owned::trace(
        vec![owned::program("fnmut", vec![], 0, "normal", false)],
        None,
    );
    assert_eq!(
        run(&t, "empty-fnmut-program", &empty, "none")["status"],
        "pass"
    );
    let mut mislabeled = owned::mutation_case();
    mislabeled["contract"] = json!(model::CONTRACT);
    let rejected = run(&t, "wrong-model-version", &mislabeled, "none");
    assert_eq!(rejected["status"], "invalid");
    assert_eq!(rejected["steps"], 0);
}

#[test]
fn owned_byte_finding_uses_existing_frozen_replay_reduce_and_close() {
    queue_roundtrip(
        &model::owned::mutation_case(),
        model::owned::PROPERTY,
        model::owned::CONTRACT,
    );
}
