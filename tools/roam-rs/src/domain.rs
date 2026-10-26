//! Independent Rust adapter; the model does not consume driver state transitions.
use crate::protocol::{self, CONTRACT};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, SourceUnit};
use zil_db::CancellationToken;
use zil_driver::{CompilationSession, CompileOptions, CompileReport};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub fn initial() -> Value {
    json!({"source":"zero","target":"native-llvm","cancel":"none","disk":true,"tokens":1_000_000,"nodes":1_000_000,"analysis":10_000_000})
}
pub fn valid(a: &Value) -> bool {
    let value = a["value"].as_str().unwrap_or("");
    match a["op"].as_str().unwrap_or("") {
        "check" => a.as_object().is_some_and(|x| x.len() == 1),
        "edit" => ["zero", "one", "ct", "type-error", "syntax-error"].contains(&value),
        "target" => ["native-llvm", "alternate-llvm"].contains(&value),
        "cancel" => ["none", "before", "parse", "types", "verify"].contains(&value),
        "cache" => ["on", "off", "corrupt", "clear"].contains(&value),
        "limit" => {
            ["tokens", "nodes", "analysis"].contains(&a["name"].as_str().unwrap_or(""))
                && protocol::integer(&a["value"]).is_some_and(|n| (0..=10_000_000).contains(&n))
        }
        _ => false,
    }
}
fn model_step(model: &mut Value, action: &Value) {
    match action["op"].as_str().unwrap_or("") {
        "edit" => model["source"] = action["value"].clone(),
        "target" | "cancel" => model[action["op"].as_str().unwrap()] = action["value"].clone(),
        "limit" => model[action["name"].as_str().unwrap()] = action["value"].clone(),
        "cache" if action["value"] == "on" || action["value"] == "off" => {
            model["disk"] = json!(action["value"] == "on")
        }
        _ => {}
    }
}
fn source(name: &str) -> &'static str {
    match name {
        "zero" => "fn main() -> i32 { return 0 }",
        "one" => "fn main() -> i32 { return 1 }",
        "ct" => "fn main() -> i32 { return #(20 + 22) }",
        "type-error" => "fn main() -> bool { return 1 }",
        "syntax-error" => "fn main( -> i32 { return 1 }",
        _ => unreachable!("validated source"),
    }
}
fn compile(
    session: &mut CompilationSession,
    input: &SourceUnit,
    options: &CompileOptions,
    cancel: &str,
) -> CompileReport {
    let options = CompileOptions {
        cancellation: CancellationToken::default(),
        ..options.clone()
    };
    let token = options.cancellation.clone();
    if cancel == "before" {
        token.cancel();
    }
    session.compile_source_with_observer(input, &options, &mut |event| {
        if event.phase.as_str() == cancel {
            token.cancel();
        }
    })
}
struct Real {
    session: CompilationSession,
    options: CompileOptions,
    root: PathBuf,
    source: String,
    cancel: String,
    disk: bool,
    fault: bool,
    last: Option<(String, CompileReport)>,
}
impl Real {
    fn init(fault: bool) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "zil-roam-rs-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self {
            session: CompilationSession::default(),
            options: CompileOptions::default(),
            root,
            source: "zero".into(),
            cancel: "none".into(),
            disk: true,
            fault,
            last: None,
        })
    }
    fn state(&self) -> Value {
        json!({"source":self.source,"target":self.options.target.id,"cancel":self.cancel,"disk":self.disk,
            "tokens":self.options.limits.max_tokens,"nodes":self.options.limits.max_nodes,"analysis":self.options.limits.max_analysis_steps})
    }
    fn observation(&self) -> Value {
        json!({"kind":"state","state":self.state(),"active":self.session.active_query_count(),"cached":self.session.cached_query_count()})
    }
    fn step(&mut self, a: &Value) -> Result<Value, String> {
        let v = a["value"].as_str().unwrap_or("");
        match a["op"].as_str().unwrap_or("") {
            "edit" => self.source = v.into(),
            "target" => self.options.target.id = v.into(),
            "cancel" => self.cancel = v.into(),
            "limit" => {
                let n = protocol::integer(&a["value"]).unwrap() as usize;
                match a["name"].as_str().unwrap() {
                    "tokens" => self.options.limits.max_tokens = n,
                    "nodes" => self.options.limits.max_nodes = n,
                    "analysis" => self.options.limits.max_analysis_steps = n,
                    _ => unreachable!(),
                }
            }
            "cache" => match v {
                "on" => self.disk = true,
                "off" => self.disk = false,
                "clear" => {
                    self.session.clear();
                    if self.root.join("cache").exists() {
                        fs::remove_dir_all(self.root.join("cache")).map_err(|e| e.to_string())?;
                    }
                }
                "corrupt" => {
                    if self.root.join("cache").exists() {
                        for e in fs::read_dir(self.root.join("cache")).map_err(|e| e.to_string())? {
                            let path = e.map_err(|e| e.to_string())?.path();
                            if path.is_file() {
                                fs::write(path, b"ZILRAW01truncated").map_err(|e| e.to_string())?;
                            }
                        }
                    }
                }
                _ => unreachable!(),
            },
            "check" => {
                let input = SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    self.root.join("input.zil").to_string_lossy(),
                    source(&self.source),
                );
                let options = CompileOptions {
                    cache_dir: self.disk.then(|| self.root.join("cache")),
                    ..self.options.clone()
                };
                let stale = self
                    .last
                    .as_ref()
                    .filter(|(name, _)| self.fault && *name != self.source);
                let warm = if let Some((_, r)) = stale {
                    r.clone()
                } else {
                    let r = compile(&mut self.session, &input, &options, &self.cancel);
                    self.last = Some((self.source.clone(), r.clone()));
                    r
                };
                let mut cold_session = CompilationSession::default();
                let cold = compile(&mut cold_session, &input, &self.options, &self.cancel);
                let warm: Value =
                    serde_json::from_str(&warm.to_json()).map_err(|e| e.to_string())?;
                let cold: Value =
                    serde_json::from_str(&cold.to_json()).map_err(|e| e.to_string())?;
                return Ok(
                    json!({"kind":"check","state":self.state(),"warm":warm,"cold":cold,"active":self.session.active_query_count(),"cold_active":cold_session.active_query_count(),"cached":self.session.cached_query_count()}),
                );
            }
            _ => unreachable!(),
        }
        Ok(self.observation())
    }
    fn dispose(&self) -> Result<(), String> {
        fs::remove_dir_all(&self.root).map_err(|e| e.to_string())
    }
}
fn check(model: &Value, o: &Value, steps: usize, step: i64) -> Option<Value> {
    let eq = |p: &str, e: Value, a: Value| {
        (!protocol::equal(&e, &a)).then(|| protocol::failure(p, steps, step, e, a))
    };
    if let Some(v) = eq("session.state", model.clone(), o["state"].clone()) {
        return Some(v);
    }
    if let Some(v) = eq("session.no-active-query", json!(0), o["active"].clone()) {
        return Some(v);
    }
    if o["kind"] != "check" {
        return None;
    }
    if let Some(v) = eq(
        "session.fresh-no-active-query",
        json!(0),
        o["cold_active"].clone(),
    ) {
        return Some(v);
    }
    let semantic = |r: &Value| json!({"status":r["status"],"phase":r["phase"],"diagnostics":r["diagnostics"],"input":r["input_fingerprint"]});
    if let Some(v) = eq(
        "session.warm-cold",
        semantic(&o["cold"]),
        semantic(&o["warm"]),
    ) {
        return Some(v);
    }
    for name in ["warm", "cold"] {
        let r = &o[name];
        if r["status"] == "Success"
            && let Some(v) = eq(
                &format!("session.verify-{name}"),
                json!(1),
                r["verification_runs"].clone(),
            )
        {
            return Some(v);
        }
        for event in r["query_events"].as_array().unwrap() {
            if event["phase"] == "verify"
                && let Some(v) = eq(
                    "session.verify-not-cached",
                    json!("executed"),
                    event["disposition"].clone(),
                )
            {
                return Some(v);
            }
        }
    }
    if model["cancel"] == "before"
        && let Some(v) = eq(
            "session.cancel-before",
            json!("Cancelled"),
            o["warm"]["status"].clone(),
        )
    {
        return Some(v);
    }
    if model["cancel"] == "none"
        && model["tokens"] == 1_000_000
        && model["nodes"] == 1_000_000
        && model["analysis"] == 10_000_000
    {
        let category = if model["source"].as_str().unwrap().ends_with("error") {
            "SafeReject"
        } else {
            "Success"
        };
        if let Some(v) = eq(
            "session.fixture-category",
            json!(category),
            o["warm"]["status"].clone(),
        ) {
            return Some(v);
        }
    }
    None
}

pub struct Options {
    pub max_steps: usize,
    pub budget_ms: u64,
    pub history: bool,
    pub fault: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_steps: 256,
            budget_ms: 10_000,
            history: true,
            fault: false,
        }
    }
}
/// before must synchronously acknowledge a persisted checkpoint; after is sent
/// immediately after a real action completes, even if subsequent model/check fails.
pub fn execute(
    trace: &Value,
    options: &Options,
    mut before: impl FnMut(usize, &Value) -> Result<(), String>,
    mut after: impl FnMut(usize) -> Result<(), String>,
) -> Value {
    if let Err(e) = protocol::validate_case(trace) {
        return protocol::issue("invalid", 0, "case", -1, "TypeError", &e);
    }
    if trace["contract"] != CONTRACT {
        return protocol::issue(
            "unsupported",
            0,
            "init",
            -1,
            "Unsupported",
            &format!("Contract {}", trace["contract"].as_str().unwrap()),
        );
    }
    if let Some(cap) = trace["requires"].as_array().and_then(|xs| xs.first()) {
        return protocol::issue(
            "unsupported",
            0,
            "init",
            -1,
            "Unsupported",
            &format!("Capability {}", cap.as_str().unwrap()),
        );
    }
    if trace["schedule"]
        .as_array()
        .is_some_and(|xs| !xs.is_empty())
    {
        return protocol::issue(
            "unsupported",
            0,
            "init",
            -1,
            "Unsupported",
            "Explicit schedule replay is not implemented",
        );
    }
    let start = Instant::now();
    let budget = || start.elapsed() >= Duration::from_millis(options.budget_ms);
    let incomplete =
        |steps, reason| json!({"status":"incomplete","steps":steps,"issues":[],"reason":reason});
    if budget() {
        return incomplete(0, "time-budget");
    }
    let mut model = initial();
    if trace
        .get("initial")
        .is_some_and(|v| !protocol::equal(v, &model))
    {
        return protocol::issue(
            "error",
            0,
            "model-init",
            -1,
            "Error",
            "unsupported initial compiler state",
        );
    }
    if budget() {
        return incomplete(0, "time-budget");
    }
    let mut real = match Real::init(options.fault) {
        Ok(v) => v,
        Err(e) => return protocol::issue("error", 0, "driver-init", -1, "Error", &e),
    };
    let mut history = Vec::new();
    let mut steps = 0;
    let mut index = -1;
    let mut output = protocol::result("pass", 0);
    let mut observed = real.observation();
    let mut record = |phase: &str, step: i64, value: Option<Value>| {
        if options.history {
            let mut h = json!({"phase":phase,"step":step});
            if let Some(v) = value {
                h[if phase == "call" { "action" } else { "value" }] = v;
            }
            history.push(h);
        }
    };
    'run: {
        if budget() {
            output = incomplete(0, "time-budget");
            break 'run;
        }
        record("record", index, Some(observed.clone()));
        if let Some(v) = check(&model, &observed, steps, index) {
            output = v;
            break 'run;
        }
        for (i, action) in trace["steps"].as_array().unwrap().iter().enumerate() {
            index = i as i64;
            if budget() {
                output = incomplete(steps, "time-budget");
                break 'run;
            }
            if steps >= options.max_steps {
                output = incomplete(steps, "step-budget");
                break 'run;
            }
            if !valid(action) {
                output = protocol::issue(
                    "invalid",
                    steps,
                    "validate",
                    index,
                    "InvalidCase",
                    &format!("Precondition at step {i}"),
                );
                break 'run;
            }
            if budget() {
                output = incomplete(steps, "time-budget");
                break 'run;
            }
            if let Err(e) = before(i, action) {
                output = protocol::issue("error", steps, "before-step", index, "Error", &e);
                break 'run;
            }
            if budget() {
                output = incomplete(steps, "time-budget");
                break 'run;
            }
            record("call", index, Some(action.clone()));
            observed = match real.step(action) {
                Ok(o) => o,
                Err(e) => {
                    output = protocol::issue("error", steps, "driver-step", index, "Error", &e);
                    break 'run;
                }
            };
            steps += 1;
            if let Err(e) = after(i) {
                output = protocol::issue("error", steps, "driver-step", index, "Error", &e);
                break 'run;
            }
            record("complete", index, None);
            model_step(&mut model, action);
            record("record", index, Some(observed.clone()));
            if let Some(v) = check(&model, &observed, steps, index) {
                output = v;
                break 'run;
            }
        }
        if budget() {
            output = incomplete(steps, "time-budget");
            break 'run;
        }
        // Original profile has no done predicate, but observes once after done.
        record(
            "record",
            trace["steps"].as_array().unwrap().len() as i64,
            Some(observed),
        );
        output["steps"] = json!(steps);
    }
    if let Err(e) = real.dispose() {
        output["issues"]
            .as_array_mut()
            .unwrap()
            .push(json!({"phase":"deferred-dispose","step":index,"name":"Error","message":e}));
        output["status"] = json!("error");
    }
    if output["status"] == "pass" && budget() {
        output = incomplete(steps, "time-budget");
    }
    if options.history {
        output["history"] = json!(history);
    }
    output
}
