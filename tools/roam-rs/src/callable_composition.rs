//! Independent oracle for scoped FnMut calls returning a tagged union.
use crate::{host, protocol, search};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

pub const CONTRACT: &str = "zil-callable-composition-native/1";
pub const PROPERTY: &str = "callable.receiver-result-and-cleanup";
const RULE_REFERENCE: &str = "spec/semantic-scopes.md#可调用值与失败出口的组合";
pub const MAX_PROGRAMS: usize = 32;
const SHAPES: [&str; 7] = ["", "h", "p", "hh", "hp", "ph", "pp"];
const EXITS: [&str; 4] = ["early", "ok", "decode", "replace"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Premise {
    Complete,
    SharedReceiver,
    OtherErrorType,
}
struct Program<'a> {
    shape: &'a str,
    input: usize,
    premise: Premise,
}
impl<'a> Program<'a> {
    fn parse(value: &'a Value) -> Result<Self, String> {
        let shape = value["shape"]
            .as_str()
            .filter(|s| SHAPES.contains(s))
            .ok_or("expected a helper/projection path of depth at most two")?;
        let input = EXITS
            .iter()
            .position(|exit| value["exit"] == *exit)
            .ok_or("unknown callable exit")?;
        let premise = match value["premise"].as_str() {
            Some("complete") => Premise::Complete,
            Some("exclusive-receiver") if input == 0 => Premise::SharedReceiver,
            Some("nominal-error") if input == 0 => Premise::OtherErrorType,
            _ => return Err("unknown or redundant premise mutation".into()),
        };
        Ok(Self {
            shape,
            input,
            premise,
        })
    }
}

pub fn validate(program: &Value) -> Result<(), String> {
    Program::parse(program).map(|_| ())
}
/// The model describes ownership immediately after invocation. The move into
/// the closure creates A; the outer scope drops whichever token remains live.
fn contract(program: &Value) -> Result<Value, String> {
    let p = Program::parse(program)?;
    if p.premise != Premise::Complete {
        return Ok(json!({"observation":{"kind":"safe-reject","premise":program["premise"]}}));
    }
    let (result, survivor, replaced, emitted, exit) = match p.input {
        0 => (json!({"ok":65}), 'A', None, 'A', 0),
        1 => (json!({"ok":66}), 'B', Some('A'), 'B', 0),
        2 => (json!({"err":"Fault.Decode"}), 'A', None, 'D', 2),
        3 => (json!({"err":"Fault.Prepare"}), 'A', None, 'P', 3),
        _ => unreachable!(),
    };
    let stdout: String = replaced.into_iter().chain([emitted, survivor]).collect();
    let cleanup: Vec<_> = replaced.into_iter().chain([survivor]).collect();
    Ok(
        json!({"return":result, "tokenLiveAtReturn":survivor, "cleanupOrder":cleanup,
        "observation":{"kind":"lifecycle","stdout":stdout,"stderr":"","exit":exit}}),
    )
}
pub fn expected(program: &Value) -> Result<Value, String> {
    Ok(contract(program)?["observation"].take())
}

fn slot_type(depth: usize) -> String {
    (0..depth).fold("F".to_owned(), |ty, _| format!("Slot<{ty}>"))
}

fn invocation(
    shape: &str,
    place: &str,
    receiver: &str,
    input: &str,
    helpers: &mut Vec<String>,
) -> String {
    match shape.as_bytes().split_first() {
        Some((b'p', rest)) => {
            let rest = std::str::from_utf8(rest).expect("ASCII path");
            invocation(rest, &format!("{place}.callback"), receiver, input, helpers)
        }
        Some((b'h', rest)) => {
            let rest = std::str::from_utf8(rest).expect("ASCII path");
            let name = format!("relay{}", shape.len());
            let projections = rest.bytes().filter(|b| *b == b'p').count();
            let param = slot_type(projections);
            let value_bound = if projections == 0 { "" } else { "Value + " };
            let nested = invocation(rest, "root", "", "input", helpers);
            helpers.push(format!(
                "fn {name}<F:{value_bound}FnMut(i32)->Fault!i32>(root:&mut {param},input:i32)->Fault!i32{{return {nested}}}"
            ));
            format!("{name}({receiver}{place},{input})")
        }
        None => format!("({receiver}{place})({input})"),
        _ => unreachable!(),
    }
}

pub fn property(program: &Value) -> &'static str {
    match program["premise"].as_str() {
        Some("exclusive-receiver") => "callable.exclusive-receiver",
        Some("nominal-error") => "callable.nominal-error",
        _ => PROPERTY,
    }
}

pub fn source(program: &Value, fault: bool) -> Result<String, String> {
    let p = Program::parse(program)?;
    let receiver = if p.premise == Premise::SharedReceiver {
        "&"
    } else {
        "&mut "
    };
    let constructor_error = if p.premise == Premise::OtherErrorType {
        "OtherFault"
    } else {
        "Fault"
    };
    let input = if fault && p.premise == Premise::Complete {
        (p.input + 1) % EXITS.len()
    } else {
        p.input
    };
    let mut helpers = Vec::new();
    let call = invocation(
        p.shape,
        "selected",
        receiver,
        &input.to_string(),
        &mut helpers,
    );
    let mut factory = "make()".to_string();
    for _ in p.shape.bytes().filter(|b| *b == b'p') {
        factory = format!("hold({factory})");
    }
    let slot = if p.shape.contains('p') {
        "resource SlotTag{marker:i32}\npackage Slot<F:Value>{manages marker:SlotTag;callback:F}\nimpl Drop for SlotTag{fn drop(self:Self){}}\nfn hold<F:Value>(callback:F)->Slot<F>{return Slot::<F>{marker:SlotTag{marker:0},callback:callback}}"
    } else {
        ""
    };
    Ok(format!(
        r#"
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){{unsafe{{putchar(code);}}}}
enum Fault{{Decode,Prepare}}
enum OtherFault{{Decode,Prepare}}
resource Token{{code:i32}}
package State{{manages token:Token}}
impl Drop for Token{{fn drop(self:Self){{emit(.code);}}}}
{slot}
fn decode(n:i32)->{constructor_error}!i32{{if n==2{{return {constructor_error}.Decode}}return n}}
fn prepare(n:i32)->{constructor_error}!i32{{if n==3{{return {constructor_error}.Prepare}}return 66}}
fn make()->impl FnMut(i32)->{constructor_error}!i32{{
    var state=State{{token:Token{{code:65}}}}
    return move(n)=>{{
        if n==0{{return state.token.code}}
        let command=decode(n)?
        state.token=Token{{code:prepare(command)?}}
        return state.token.code
    }}
}}
{}
fn main()->i32{{
    var selected={factory}
    let result:Fault!i32={call}
    match result{{
        .ok(value)=>{{emit(value);return 0}},
        .err(error)=>{{
            match error{{Fault.Decode()=>{{emit(68);return 2}},Fault.Prepare()=>{{emit(80);return 3}}}}
        }}
    }}
}}
"#,
        helpers.join("\n")
    ))
}

fn program(shape: &str, exit: &str, premise: &str) -> Value {
    json!({"shape":shape,"exit":exit,"premise":premise})
}
fn trace(programs: impl IntoIterator<Item = Value>) -> Value {
    json!({"format":"roam/1","contract":CONTRACT,"steps":
        programs.into_iter().map(|program|json!({"op":"run","program":program})).collect::<Vec<_>>()})
}
pub fn fixed() -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    for shape in SHAPES {
        let label = if shape.is_empty() { "direct" } else { shape };
        for exit in EXITS {
            cases.push((
                format!("{label}-{exit}"),
                trace([program(shape, exit, "complete")]),
            ));
        }
        for premise in ["exclusive-receiver", "nominal-error"] {
            cases.push((
                format!("{label}-reject-{premise}"),
                trace([program(shape, "early", premise)]),
            ));
        }
    }
    cases
}
pub fn generate(seed: i64, steps: usize) -> Value {
    let mut random = seed as u32;
    let cases = fixed();
    let programs = (0..steps).map(|_| {
        let index =
            ((u64::from(search::random_next(&mut random)) * cases.len() as u64) >> 32) as usize;
        cases[index].1["steps"][0]["program"].clone()
    });
    let mut result = trace(programs);
    result["seed"] = json!(seed);
    result
}
pub fn candidates(case: &Value) -> Vec<Value> {
    let Some(steps) = case["steps"].as_array() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if let Ok(p) = Program::parse(&step["program"]) {
            for position in 0..p.shape.len() {
                let mut shape = p.shape.to_string();
                shape.remove(position);
                let mut reduced = case.clone();
                reduced["steps"][i]["program"]["shape"] = json!(shape);
                candidates.push(reduced);
            }
        }
    }
    candidates
}

/// A compiler result is evidence only for the expected reason. In particular
/// an unresolved symbol, Unsupported or a budget failure cannot pass a negative.
pub fn observed(expected: &Value, observation: Value, compile: &Value) -> Value {
    if expected["kind"] != "safe-reject" || compile["status"] != "SafeReject" {
        return observation;
    }
    let first = &compile["diagnostics"][0];
    let code = first["code"].as_str().unwrap_or("");
    let message = first["message"].as_str().unwrap_or("");
    let matches = match expected["premise"].as_str() {
        Some("exclusive-receiver") => matches!(
            (code, message),
            (
                "E-SEMANTIC-ORIGINAL",
                "original callable receiver does not grant its invocation mode"
            ) | (
                "E-TYPE-MISMATCH",
                "helper requires its concrete environment passing mode"
            ) | (
                "E-ADDRESS-PERMISSION",
                "a shared address cannot grant exclusive access"
            ) | (
                "E-ADDRESS-PERMISSION",
                "shared address cannot be reborrowed exclusively"
            )
        ),
        Some("nominal-error") => {
            let declarations: Vec<_> = first["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|note| note.as_str()?.split_once(" constrained declaration = "))
                .filter(|(ordinal, _)| matches!(*ordinal, "first" | "second"))
                .map(|(_, name)| name)
                .collect();
            (code == "E-ERROR-INJECTION"
                && message.contains("_OtherFault")
                && message.contains("_Fault"))
                || (code == "E-TYPE-MISMATCH"
                    && message == "different concrete aggregate types"
                    && declarations.iter().any(|name| name.ends_with("_Fault"))
                    && declarations
                        .iter()
                        .any(|name| name.ends_with("_OtherFault")))
        }
        _ => false,
    };
    if matches {
        expected.clone()
    } else {
        observation
    }
}

pub(crate) fn first_compile_failure(evidence: &Value) -> Option<&Value> {
    fn failed_compile(compile: &Value) -> Option<&Value> {
        (!compile["status"].is_null() && compile["status"] != "Success").then_some(compile)
    }
    failed_compile(&evidence["compile"]).or_else(|| {
        ["zil-O0", "zil-O2", "llvm-O0", "llvm-O2"]
            .into_iter()
            .find_map(|route| failed_compile(&evidence["variants"][route]["compile"]))
    })
}

pub(crate) fn compile_location(compile: &Value) -> Value {
    json!({"phase":compile["phase"], "code":compile["diagnostics"][0]["code"],
        "source":compile["diagnostics"][0]["span"], "category":compile["status"]})
}

fn failure_location(result: &Value) -> Option<(&str, &str)> {
    if result["status"] == "pass" {
        return None;
    }
    first_compile_failure(&result["nativeEvidence"][0])
        .map(|compile| {
            (
                compile["phase"].as_str().unwrap_or("compile"),
                compile["diagnostics"][0]["code"]
                    .as_str()
                    .unwrap_or("unspecified"),
            )
        })
        .or_else(|| {
            let issue = &result["issues"][0];
            Some((
                issue["phase"].as_str()?,
                issue["name"].as_str().unwrap_or("unspecified"),
            ))
        })
        .or_else(|| (result["status"] == "fail").then_some(("runtime", "observation-mismatch")))
        .or_else(|| result["status"].as_str().map(|status| ("harness", status)))
}

fn outcome(result: &Value) -> &'static str {
    if result["status"] == "incomplete" || result["reason"] == "time-budget" {
        "budget-exhausted"
    } else if result["status"] == "error" && result["reason"] == "timeout" {
        "host-timeout"
    } else if result["steps"] != 1 {
        "unreached"
    } else if result["status"] == "pass" {
        "matched"
    } else if let Some(compile) = first_compile_failure(&result["nativeEvidence"][0]) {
        match compile["status"].as_str() {
            Some("ResourceLimit") => "budget-exhausted",
            Some("Unsupported") => "unsupported",
            Some("SafeReject") => "rejected-at-first-mismatch",
            _ => "compiler-error",
        }
    } else {
        "observation-mismatch"
    }
}

fn run_case(
    root: &Path,
    snapshot: &Path,
    toolchain: &Value,
    label: &str,
    case: &Value,
) -> Result<Value, String> {
    let mut case = case.clone();
    crate::native::bind_identity(&mut case, toolchain)?;
    let expected = expected(&case["steps"][0]["program"])?;
    let attempt = root.join(label);
    let run = host::supervise(
        &case,
        host::HostOptions {
            command: &snapshot.join("roam"),
            args: vec!["worker".into(), "--budget-ms=22000".into()],
            timeout_ms: 24_000,
            max_output: protocol::MAX_FRAME,
            attempt: &attempt,
        },
    );
    let evidence = match run {
        Ok(run) => run.result,
        Err(error) => json!({"status":"error","reason":error}),
    };
    fs::create_dir_all(&attempt).map_err(|e| e.to_string())?;
    let proof = json!({"label":label,"expected":expected,"result":evidence});
    host::durable(&attempt.join("rule-result.json"), &proof)?;
    // Per-case evidence retains the complete diagnostic and codegen receipts.
    let program = &case["steps"][0]["program"];
    let first_failure =
        failure_location(&evidence).map(|(phase, code)| json!({"phase":phase,"code":code}));
    Ok(
        json!({"label":label,"expected":expected,"status":evidence["status"],
        "steps":evidence["steps"],"reason":evidence.get("reason"),
        "shape":program["shape"],"property":property(program),"reference":RULE_REFERENCE,
        "outcome":outcome(&evidence),"exit":program["exit"],"premise":program["premise"],
        "firstFailure":first_failure,"failure":evidence.get("failure")}),
    )
}

pub fn campaign(root: &Path, commit: &str, jobs: usize) -> Result<Value, String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let snapshot = host::freeze(&exe, &root.join("snapshots"), commit)?;
    let worker_sha = snapshot
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("frozen worker identity is missing")?;
    let toolchain = crate::native::identity()?;
    let cases = fixed();
    let plan = cases
        .iter()
        .map(|(label, case)| {
            let program = &case["steps"][0]["program"];
            Ok(
                json!({"label":label,"program":program,"property":property(program),
            "reference":RULE_REFERENCE,"contract":contract(program)?}),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    host::durable(&root.join("expectations.json"), &json!(plan))?;
    let mut results = std::thread::scope(|scope| {
        let snapshot = &snapshot;
        let toolchain = &toolchain;
        let cases = &cases;
        let workers: Vec<_> = (0..jobs)
            .map(|worker| {
                scope.spawn(move || {
                    cases
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(jobs)
                        .map(|(index, (label, case))| {
                            run_case(root, snapshot, toolchain, label, case).map(|row| (index, row))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
            })
            .collect();
        let mut rows = Vec::with_capacity(cases.len());
        for worker in workers {
            rows.extend(
                worker
                    .join()
                    .map_err(|_| "callable combination worker panicked")??,
            );
        }
        rows.sort_by_key(|(index, _)| *index);
        Ok::<Vec<_>, String>(rows.into_iter().map(|(_, row)| row).collect())
    })?;
    let passed = |row: &Value| row["status"] == "pass" && row["steps"] == 1;
    let positive_selected = SHAPES.len() * EXITS.len();
    let negative_selected = SHAPES.len() * 2;
    let mut positive_passed = 0;
    let mut negative_passed = 0;
    for family in results.chunks_exact_mut(EXITS.len() + 2) {
        for row in family.iter_mut() {
            row["publishedScope"] = json!(true);
        }
        positive_passed += family[..EXITS.len()]
            .iter()
            .filter(|row| passed(row))
            .count();
        let baseline = passed(&family[0]);
        for negative in &mut family[EXITS.len()..] {
            negative["positiveControlPassed"] = json!(baseline);
            negative_passed += usize::from(baseline && passed(negative));
        }
    }
    let complete = results.len() == positive_selected + negative_selected
        && positive_passed == positive_selected
        && negative_passed == negative_selected;
    let mut failures = BTreeMap::new();
    for row in &results {
        if let Some(phase) = row["firstFailure"]["phase"].as_str() {
            let key = (
                row["property"].as_str().expect("generated property"),
                phase,
                row["firstFailure"]["code"]
                    .as_str()
                    .expect("diagnostic code"),
                row["publishedScope"].as_bool().expect("classified path"),
            );
            let (count, _) = failures.entry(key).or_insert((0, &row["label"]));
            *count += 1;
        }
    }
    let failure_groups: Vec<_> = failures
        .into_iter()
        .map(
            |((property, phase, code, published_scope), (count, case))| {
                json!({"property":property,"phase":phase,"code":code,
                "publishedScope":published_scope,"count":count,"firstCase":case})
            },
        )
        .collect();
    let report = json!({
        "contract":CONTRACT, "candidate":commit, "roamSha256":worker_sha,
        "bound":{"maximumDepth":2,"operations":["generic helper","field projection"],"exits":EXITS,
            "shapes":SHAPES, "rule":"§8.1.2.2"},
        "status":if complete {"pass"} else {"fail"},
        "positive":{"selected":positive_selected,"passed":positive_passed},
        "negative":{"selected":negative_selected,"passed":negative_passed},
        "targetSpace":{"selection":"exhaustive","sampling":0,
            "positive":{"selected":positive_selected,"passed":positive_passed},
            "negative":{"selected":negative_selected,"passed":negative_passed},"complete":complete},
        "boundary":{"selected":0},
        "failureGroups":failure_groups,
        "cases":results
    });
    host::durable(&root.join("report.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_after_a_partial_native_step_stays_incomplete() {
        assert_eq!(outcome(&json!({"status":"incomplete", "steps":1, "reason":"time-budget"})),
            "budget-exhausted");
        assert_eq!(outcome(&json!({"status":"error", "steps":0, "reason":"timeout"})),
            "host-timeout");
    }

    #[test]
    fn negative_uses_the_first_diagnostic_and_its_receiver_rule() {
        let expected = expected(&program("ph", "early", "exclusive-receiver")).unwrap();
        let observation = json!({"kind":"compile-failure"});
        let intended = json!({"code":"E-TYPE-MISMATCH",
            "message":"helper requires its concrete environment passing mode"});
        let unrelated =
            json!({"code":"E-TYPE-MISMATCH", "message":"different concrete aggregate types"});
        for (diagnostics, want) in [
            (json!([intended.clone()]), &expected),
            (json!([unrelated, intended]), &observation),
        ] {
            let compile = json!({"status":"SafeReject","diagnostics":diagnostics});
            assert_eq!(&observed(&expected, observation.clone(), &compile), want);
        }
    }

    #[test]
    fn nominal_rejection_requires_both_original_declarations() {
        let expected = expected(&program("hh", "early", "nominal-error")).unwrap();
        let unknown = json!({"kind":"compile-failure"});
        for (other, want) in [("OtherFault", &expected), ("OtherResource", &unknown)] {
            let compile = json!({"status":"SafeReject","diagnostics":[{
                "code":"E-TYPE-MISMATCH","message":"different concrete aggregate types",
                "notes":["first constrained declaration = @module0:$module0_Fault",
                    format!("second constrained declaration = @module0:$module0_{other}")]
            }]});
            assert_eq!(&observed(&expected, unknown.clone(), &compile), want);
        }
    }

    #[test]
    fn shrinking_cannot_replace_the_first_mismatch_with_another_stage() {
        let mut first = protocol::failure(PROPERTY, 1, 0, json!(42), json!(null));
        first["failure"]["firstMismatch"] = json!({"phase":"types","code":"E-TYPE-MISMATCH",
            "source":{"start":18,"end":19},"category":"SafeReject"});
        let mut next = first.clone();
        next["failure"]["firstMismatch"]["source"]["start"] = json!(7);
        assert_eq!(protocol::failure_key(&first), protocol::failure_key(&next));
        next["failure"]["firstMismatch"]["phase"] = json!("analyze");
        assert_ne!(protocol::failure_key(&first), protocol::failure_key(&next));
    }
}
