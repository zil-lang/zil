//! The portable Roam wire profile. Scheduling is intentionally unsupported.
use serde_json::{Value, json};

pub const CONTRACT: &str = "zil-compilation-session/1";
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const MAX_CASE: usize = 1024 * 1024;
pub const MAX_STEPS: usize = 256;

pub fn json_valid(value: &Value) -> Result<(), String> {
    match value {
        Value::Number(n) => {
            let f = n.as_f64().ok_or("Non-JSON number")?;
            if !f.is_finite() || (f == 0.0 && f.is_sign_negative()) {
                return Err("Non-JSON number".into());
            }
        }
        Value::Array(xs) => {
            for x in xs {
                json_valid(x)?;
            }
        }
        Value::Object(xs) => {
            for x in xs.values() {
                json_valid(x)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn parse(text: &[u8], limit: usize) -> Result<Value, String> {
    if text.len() > limit {
        return Err("JSON byte quota exceeded".into());
    }
    let value: Value = serde_json::from_slice(text).map_err(|e| e.to_string())?;
    json_valid(&value)?;
    Ok(value)
}

pub fn integer(value: &Value) -> Option<i64> {
    let f = value.as_f64()?;
    (f.is_finite() && f.abs() <= 9_007_199_254_740_991.0 && f.fract() == 0.0).then_some(f as i64)
}

/// JSON object ordering and JSON number spelling do not define protocol identity.
pub fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}

pub fn validate_case(value: &Value) -> Result<(), String> {
    json_valid(value)?;
    // Enforce the same byte profile for API-created Values as for case files,
    // without allocating an additional encoded copy just to count its size.
    struct Count {
        used: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_CASE.saturating_sub(self.used) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "case byte quota exceeded",
                ));
            }
            self.used += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count { used: 0 }, value).map_err(|e| e.to_string())?;
    let obj = value.as_object().ok_or("Expected roam case")?;
    if value["format"] != "roam/1" || value["contract"].as_str().is_none_or(str::is_empty) {
        return Err("Unsupported case format or missing contract".into());
    }
    let steps = value["steps"]
        .as_array()
        .ok_or("Expected action objects with non-empty op")?;
    for a in steps {
        if !a.is_object() || a["op"].as_str().is_none_or(str::is_empty) {
            return Err("Expected action objects with non-empty op".into());
        }
    }
    if obj.get("requires").is_some_and(|r| {
        r.as_array()
            .is_none_or(|xs| xs.iter().any(|x| !x.is_string()))
    }) {
        return Err("Expected capability names".into());
    }
    if obj.get("schedule").is_some_and(|r| !r.is_array()) {
        return Err("Expected schedule array".into());
    }
    if obj.get("seed").is_some_and(|v| integer(v).is_none()) {
        return Err("Expected integer seed".into());
    }
    if obj.get("meta").is_some_and(|v| !v.is_object()) {
        return Err("Expected metadata object".into());
    }
    Ok(())
}

pub fn result(status: &str, steps: usize) -> Value {
    json!({"status":status,"steps":steps,"issues":[]})
}
pub fn issue(
    status: &str,
    steps: usize,
    phase: &str,
    step: i64,
    name: &str,
    message: &str,
) -> Value {
    json!({"status":status,"steps":steps,"issues":[{"phase":phase,"step":step,"name":name,"message":message}]})
}
pub fn host_error(steps: usize, reason: &str, message: &str) -> Value {
    let name = if reason == "timeout" {
        "Timeout"
    } else if reason.starts_with("protocol:") {
        "ProtocolError"
    } else {
        "WorkerError"
    };
    json!({"status":"error","steps":steps,"reason":reason,"issues":[{"phase":"host","name":name,"message":message}]})
}
pub fn failure(property: &str, steps: usize, step: i64, expected: Value, actual: Value) -> Value {
    let mut value = issue("fail", steps, "check", step, "PropertyFailure", property);
    value["failure"] =
        json!({"propertyId":property,"message":property,"expected":expected,"actual":actual});
    value
}
pub fn validate_result(value: &Value, max_steps: usize) -> Result<(), String> {
    json_valid(value)?;
    let status = value["status"].as_str().ok_or("invalid worker result")?;
    if ![
        "pass",
        "fail",
        "invalid",
        "error",
        "unsupported",
        "incomplete",
    ]
    .contains(&status)
    {
        return Err("invalid worker status".into());
    }
    let steps = integer(&value["steps"]).ok_or("invalid worker steps")?;
    if steps < 0 || steps as usize > max_steps {
        return Err("invalid worker steps".into());
    }
    let issues = value["issues"].as_array().ok_or("invalid worker issues")?;
    for issue in issues {
        if !issue.is_object()
            || ["phase", "name", "message"]
                .iter()
                .any(|k| !issue[k].is_string())
        {
            return Err("invalid worker issues".into());
        }
        if let Some(s) = issue.get("step")
            && integer(s).is_none_or(|s| s < -1 || s as i128 > max_steps as i128)
        {
            return Err("invalid worker issue step".into());
        }
    }
    if let Some(f) = value.get("failure") {
        if f["propertyId"].as_str().is_none_or(str::is_empty)
            || !f["message"].is_string()
            || !["fail", "error"].contains(&status)
        {
            return Err("invalid worker failure".into());
        }
    } else if status == "fail" {
        return Err("failure missing property identity".into());
    }
    if value.get("reason").is_some_and(|v| !v.is_string()) {
        return Err("invalid worker reason".into());
    }
    if value.get("history").is_some_and(|v| {
        v.as_array()
            .is_none_or(|xs| xs.iter().any(|v| !v["phase"].is_string()))
    }) {
        return Err("invalid worker history".into());
    }
    Ok(())
}

pub fn failure_key(value: &Value) -> Option<Value> {
    if value["status"] == "pass" {
        None
    } else if value["status"] == "fail" {
        Some(json!(["fail", value["failure"]["propertyId"]]))
    } else {
        Some(json!([
            value["status"],
            value.get("reason").unwrap_or(&Value::Null),
            value["issues"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|v| json!([v["phase"], v["name"]]))
                .collect::<Vec<_>>()
        ]))
    }
}
