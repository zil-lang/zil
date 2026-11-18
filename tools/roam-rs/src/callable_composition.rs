//! Independent oracle for scoped FnMut calls returning a tagged union.
use crate::{host, protocol, search};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub const CONTRACT: &str = "zil-callable-composition-native/1";
pub const PROPERTY: &str = "callable.receiver-result-and-cleanup";
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
pub fn expected(program: &Value) -> Result<Value, String> {
    let p = Program::parse(program)?;
    if p.premise != Premise::Complete {
        return Ok(json!({"kind":"safe-reject","premise":program["premise"]}));
    }
    // Trace one initially-owned 'A' resource. Successful replacement drops A
    // and installs B. The failing initializer leaves A in the original slot.
    let (stdout, exit) = match p.input {
        0 => ("AA", 0),
        1 => ("ABB", 0),
        2 => ("DA", 2),
        3 => ("PA", 3),
        _ => unreachable!(),
    };
    Ok(json!({"kind":"lifecycle","stdout":stdout,"stderr":"","exit":exit}))
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
            let param = slot_type(rest.bytes().filter(|b| *b == b'p').count());
            let nested = invocation(rest, "root", "&mut ", "input", helpers);
            helpers.push(format!(
                "fn {name}<F:FnMut(i32)->Fault!i32>(root:&mut {param},input:i32)->Fault!i32{{return {nested}}}"
            ));
            format!("{name}({receiver}{place},{input})")
        }
        None => format!("({receiver}{place})({input})"),
        _ => unreachable!(),
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
        "resource SlotTag{marker:i32}\npackage Slot<F>{manages marker:SlotTag;callback:F}\nimpl Drop for SlotTag{fn drop(self:Self){}}\nfn hold<F>(callback:F)->Slot<F>{return Slot::<F>{marker:SlotTag{marker:0},callback:callback}}"
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
impl Drop for Token{{fn drop(self:Self){{emit(.code);}}}}
{slot}
fn decode(n:i32)->{constructor_error}!i32{{if n==2{{return {constructor_error}.Decode}}return n}}
fn prepare(n:i32)->{constructor_error}!i32{{if n==3{{return {constructor_error}.Prepare}}return 66}}
fn make()->impl FnMut(i32)->{constructor_error}!i32{{
    var state=Token{{code:65}}
    return move(n)=>{{
        if n==0{{return state.code}}
        let command=decode(n)?
        state=Token{{code:prepare(command)?}}
        return state.code
    }}
}}
{}
fn main()->i32{{
    var selected={factory}
    let result:Fault!i32={call}
    match result{{
        .ok(value)=>{{emit(value);return 0}},
        .err(error)=>{{
            match error{{Fault.Decode=>{{emit(68);return 2}},Fault.Prepare=>{{emit(80);return 3}}}}
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
    let expected_codes: &[&str] = if expected["premise"] == "exclusive-receiver" {
        &[
            "E-CALL-MUTABLE-BINDING",
            "E-TYPE-MISMATCH",
            "E-ADDRESS-PERMISSION",
        ]
    } else {
        &[
            "E-GENERIC-REQUIREMENT",
            "E-TRAIT-BOUND",
            "E-TYPE-MISMATCH",
            "E-VERIFY-SEMANTIC",
        ]
    };
    if compile["diagnostics"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["code"]
                .as_str()
                .is_some_and(|code| expected_codes.contains(&code))
        })
    }) {
        expected.clone()
    } else {
        observation
    }
}

pub fn campaign(root: &Path, commit: &str) -> Result<Value, String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let snapshot = host::freeze(&exe, &root.join("snapshots"), commit)?;
    let toolchain = crate::native::identity()?;
    let cases = fixed();
    let mut results = Vec::with_capacity(cases.len());
    for (label, mut case) in cases {
        crate::native::bind_identity(&mut case, &toolchain)?;
        let expected = expected(&case["steps"][0]["program"])?;
        let run = host::supervise(
            &case,
            host::HostOptions {
                command: &snapshot.join("roam"),
                args: vec!["worker".into(), "--budget-ms=22000".into()],
                timeout_ms: 24_000,
                max_output: protocol::MAX_FRAME,
                attempt: &root.join(&label),
            },
        );
        let evidence = match run {
            Ok(run) => run.result,
            Err(error) => json!({"status":"error","reason":error}),
        };
        let proof = json!({"label":label,"expected":expected,"result":evidence});
        host::durable(&root.join(&label).join("rule-result.json"), &proof)?;
        // Keep the summary compact while per-case evidence retains diagnostics
        // and all four real backend/optimization observations.
        results.push(
            json!({"label":label,"expected":expected,"status":evidence["status"],
            "steps":evidence["steps"],"reason":evidence.get("reason")}),
        );
    }
    let passed = |row: &Value| row["status"] == "pass" && row["steps"] == 1;
    let mut negative_passed = 0;
    for (index, family) in results.chunks_exact_mut(6).enumerate() {
        let baseline = passed(&family[0]);
        for negative in &mut family[4..] {
            negative["positiveControlPassed"] = json!(baseline);
            negative_passed += usize::from(baseline && passed(negative));
        }
        debug_assert_eq!(
            family[0]["label"],
            format!(
                "{}-early",
                if index == 0 { "direct" } else { SHAPES[index] }
            )
        );
    }
    let report = json!({
        "contract":CONTRACT, "candidate":commit, "roamSha256":host::digest(&exe)?,
        "bound":{"maximumDepth":2,"operations":["generic helper","field projection"],"exits":EXITS},
        "status": if results.iter().all(passed) {"pass"} else {"fail"},
        "positive":{"selected":28,"passed":results.iter().filter(|row| row["expected"]["kind"] == "lifecycle" && passed(row)).count()},
        "negative":{"selected":14,"passed":negative_passed},
        "cases":results
    });
    host::durable(&root.join("report.json"), &report)?;
    Ok(report)
}
