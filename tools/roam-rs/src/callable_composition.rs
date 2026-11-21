//! Independent oracle for scoped FnMut calls returning a tagged union.
use crate::search;
use serde_json::{Value, json};
use std::path::Path;

pub const CONTRACT: &str = "zil-callable-composition-native/1";
pub const PROPERTY: &str = "callable.receiver-result-and-cleanup";
const RULE_REFERENCE: &str = "spec/semantic-scopes.md#可调用值与失败出口的组合";
pub const MAX_PROGRAMS: usize = 32;
const RELEASE_SHAPES: usize = 7;
const SHAPES: [&str; 15] = [
    "", "h", "p", "hh", "hp", "ph", "pp",
    "hhh", "hhp", "hph", "hpp", "phh", "php", "pph", "ppp",
];
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
            .ok_or("expected a helper/projection path of depth at most three")?;
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
            // The helper receives a reference. A projected child is a place,
            // so passing that child to another helper needs an explicit loan.
            let receiver = if receiver.is_empty() && rest.starts_with('h') {
                "&mut "
            } else {
                receiver
            };
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
    cases(&SHAPES[..RELEASE_SHAPES])
}

fn extended() -> Vec<(String, Value)> {
    cases(&SHAPES)
}

fn cases(shapes: &[&str]) -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    for &shape in shapes {
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
                "E-TYPE-MISMATCH",
                "the value does not have the expected type"
            ) | (
                "E-ASSIGN-IMMUTABLE",
                "a place behind a shared address cannot be called as a mutable closure"
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
                || (code == "E-TYPE-MISMATCH"
                    && message == "the value does not have the expected type")
                || (code == "E-SAFETY-SOLVE" && message.contains("TypeMismatch"))
        }
        _ => false,
    };
    if matches {
        expected.clone()
    } else {
        observation
    }
}

pub fn campaign(root: &Path, commit: &str, jobs: usize, depth: usize) -> Result<Value, String> {
    let (shapes, cases): (&[&str], fn() -> Vec<(String, Value)>) = match depth {
        2 => (&SHAPES[..RELEASE_SHAPES], fixed),
        3 => (&SHAPES, extended),
        _ => return Err("callable depth must be 2 or 3".into()),
    };
    crate::bounded_campaign::run(
        &crate::bounded_campaign::Rules {
            contract_id: CONTRACT,
            rule_reference: RULE_REFERENCE,
            shapes,
            exits: &EXITS,
            premises: &["exclusive-receiver", "nominal-error"],
            operations: &["generic helper", "field projection"],
            max_depth: depth,
            rule: "§8.1.2.2",
            cases,
            contract,
            property,
        },
        root,
        commit,
        jobs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bounded_campaign::outcome, protocol};

    #[test]
    fn budget_after_a_partial_native_step_stays_incomplete() {
        assert_eq!(
            outcome(&json!({"status":"incomplete", "steps":1, "reason":"time-budget"})),
            "budget-exhausted"
        );
        assert_eq!(
            outcome(&json!({"status":"error", "steps":0, "reason":"timeout"})),
            "host-timeout"
        );
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
