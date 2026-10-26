//! Deterministic exploration/reduction. A seed is provenance, never replay input.
use crate::protocol::{self, CONTRACT};
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};

pub fn trace(steps: Vec<Value>) -> Value {
    json!({"format":"roam/1","contract":CONTRACT,"steps":steps})
}
pub fn action(op: &str, value: &str) -> Value {
    json!({"op":op,"value":value})
}
pub fn check() -> Value {
    json!({"op":"check"})
}
pub fn limit(name: &str, value: u64) -> Value {
    json!({"op":"limit","name":name,"value":value})
}
pub const SEEDS: [i64; 16] = [
    0, 1, 2, 3, 7, 11, 17, 31, 63, 127, 255, 1024, 65535, 0x12345678, 0xdeadbeef, 0xffffffff,
];
pub fn random_next(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_add(0x6D2B79F5);
    let mut t = (*seed ^ (*seed >> 15)).wrapping_mul(1 | *seed);
    t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t) ^ t);
    t ^ (t >> 14)
}
pub fn generate(seed: i64, steps: usize) -> Value {
    let mut state = seed as u32;
    let mut ops = vec![check(), check()];
    for (op, choices) in [
        (
            "edit",
            &["zero", "one", "ct", "type-error", "syntax-error"][..],
        ),
        ("target", &["native-llvm", "alternate-llvm"][..]),
        (
            "cancel",
            &["none", "before", "parse", "types", "verify"][..],
        ),
        ("cache", &["on", "off", "corrupt", "clear"][..]),
    ] {
        for v in choices {
            ops.push(action(op, v));
        }
    }
    for n in ["tokens", "nodes", "analysis"] {
        ops.push(limit(n, 1));
        ops.push(limit(
            n,
            if n == "analysis" {
                10_000_000
            } else {
                1_000_000
            },
        ));
    }
    let mut path = (0..steps)
        .map(|_| {
            let index = ((random_next(&mut state) as u64 * ops.len() as u64) >> 32) as usize;
            ops[index].clone()
        })
        .collect::<Vec<_>>();
    path.extend([
        action("cancel", "none"),
        limit("tokens", 1_000_000),
        limit("nodes", 1_000_000),
        limit("analysis", 10_000_000),
        action("edit", "ct"),
        check(),
        check(),
    ]);
    let mut value = trace(path);
    value["seed"] = json!(seed);
    value
}
pub fn fixed() -> Vec<(String, Value)> {
    let c = check;
    let a = action;
    let l = limit;
    let mut paths = vec![
        ("reuse".into(), trace(vec![c(), c()])),
        (
            "edits".into(),
            trace(vec![
                c(),
                a("edit", "one"),
                c(),
                a("edit", "type-error"),
                c(),
                a("edit", "syntax-error"),
                c(),
                a("edit", "ct"),
                c(),
                c(),
            ]),
        ),
        (
            "target".into(),
            trace(vec![
                c(),
                a("target", "alternate-llvm"),
                c(),
                c(),
                a("target", "native-llvm"),
                c(),
            ]),
        ),
        (
            "disk".into(),
            trace(vec![
                c(),
                a("cache", "corrupt"),
                c(),
                a("cache", "off"),
                c(),
                a("cache", "clear"),
                c(),
                a("cache", "on"),
                c(),
                c(),
            ]),
        ),
    ];
    for v in ["before", "parse", "types", "verify"] {
        paths.push((
            format!("cancel-{v}"),
            trace(vec![
                c(),
                a("cancel", v),
                c(),
                a("cancel", "none"),
                c(),
                c(),
            ]),
        ));
    }
    for n in ["tokens", "nodes", "analysis"] {
        paths.push((
            format!("limit-{n}"),
            trace(vec![
                c(),
                l(n, 1),
                c(),
                l(
                    n,
                    if n == "analysis" {
                        10_000_000
                    } else {
                        1_000_000
                    },
                ),
                c(),
                c(),
            ]),
        ));
    }
    paths
}
pub fn alphabet() -> Vec<Value> {
    vec![
        check(),
        action("edit", "type-error"),
        action("target", "alternate-llvm"),
        action("cancel", "before"),
        limit("tokens", 1),
        action("cache", "corrupt"),
        action("cache", "clear"),
    ]
}
/// History itself is the state key: cache/session histories cannot be coalesced.
pub fn bfs(
    alphabet: &[Value],
    depth: usize,
    cap: usize,
    mut replay: impl FnMut(&Value) -> Result<Value, String>,
) -> Result<Value, String> {
    let mut queue = VecDeque::from([Vec::new()]);
    let mut states = 1;
    let mut edges = 0;
    let mut bounded = false;
    let mut capped = false;
    let first = replay(&trace(Vec::new()))?;
    if first["status"] != "pass" {
        return Ok(
            json!({"states":1,"edges":0,"fail":1,"bad":[{"path":[],"why":first}],"complete":false,"stopReason":"failure","capped":false,"depth":depth}),
        );
    }
    while let Some(path) = queue.pop_front() {
        if path.len() >= depth {
            bounded |= !alphabet.is_empty();
            continue;
        }
        for a in alphabet {
            let mut next = path.clone();
            next.push(a.clone());
            edges += 1;
            let result = replay(&trace(next.clone()))?;
            if result["status"] != "pass" {
                return Ok(
                    json!({"states":states,"edges":edges,"fail":1,"bad":[{"path":next,"why":result}],"complete":false,"stopReason":"failure","capped":capped,"depth":depth}),
                );
            }
            if states >= cap {
                capped = true;
                continue;
            }
            states += 1;
            queue.push_back(next);
        }
    }
    Ok(
        json!({"states":states,"edges":edges,"fail":0,"bad":[],"complete":!bounded&&!capped,"stopReason":if capped{"cap"}else if bounded{"depth"}else{"exhausted"},"capped":capped,"depth":depth}),
    )
}
pub fn shrink(
    original: &Value,
    max_runs: usize,
    budget_ms: u64,
    mut replay: impl FnMut(&Value) -> Result<Value, String>,
) -> Result<Value, String> {
    protocol::validate_case(original)?;
    let start = Instant::now();
    let mut runs = 0;
    let mut reduced = original.clone();
    let mut result = json!({"status":"incomplete","steps":0,"issues":[],"reason":"shrink-budget"});
    let mut stopped = "complete";
    let budget = |runs| runs < max_runs && start.elapsed() < Duration::from_millis(budget_ms);
    let mut run = |c: &Value| match replay(c) {
        Ok(r) => r,
        Err(e) => protocol::issue("error", 0, "replay", -1, "Error", &e),
    };
    if !budget(runs) {
        stopped = "budget";
    } else {
        result = run(&reduced);
        runs += 1;
        if result["status"] != "fail"
            || result["failure"]["propertyId"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            stopped = "not-reproduced";
        } else {
            let property = result["failure"]["propertyId"].clone();
            let mut seen = HashSet::from([reduced.to_string()]);
            let mut size = reduced["steps"]
                .as_array()
                .unwrap()
                .len()
                .div_ceil(2)
                .max(1);
            while !reduced["steps"].as_array().unwrap().is_empty() && budget(runs) {
                let mut changed = false;
                let len = reduced["steps"].as_array().unwrap().len();
                for i in (0..len).step_by(size) {
                    if !budget(runs) {
                        break;
                    }
                    let mut candidate = reduced.clone();
                    candidate["steps"]
                        .as_array_mut()
                        .unwrap()
                        .drain(i..(i + size).min(len));
                    if !seen.insert(candidate.to_string()) {
                        continue;
                    }
                    let next = run(&candidate);
                    runs += 1;
                    if next["status"] == "fail" && next["failure"]["propertyId"] == property {
                        reduced = candidate;
                        result = next;
                        changed = true;
                        break;
                    }
                }
                if changed {
                    size = size.min(reduced["steps"].as_array().unwrap().len().max(1));
                    continue;
                }
                if size == 1 {
                    break;
                }
                size = (size / 2).max(1);
            }
            if !budget(runs) {
                stopped = "budget";
            }
        }
    }
    Ok(json!({"original":original,"reduced":reduced,"result":result,"runs":runs,"stopped":stopped}))
}
