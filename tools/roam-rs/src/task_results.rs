//! Independent oracle for a Task retaining two input ports and returning one.
use crate::search;
use serde_json::{Value, json};

pub const CONTRACT: &str = "zil-task-result-native/1";
pub const PROPERTY: &str = "task.retained-result-and-cleanup";
pub const MAX_PROGRAMS: usize = 32;
pub const RULE_REFERENCE: &str = "spec/semantic-scopes.md#跨阶段语义不变量与按操作组合的判定规则";
pub const SHAPES: [&str; 4] = ["compact", "compact-relay", "complete", "complete-relay"];
pub const EXITS: [&str; 4] = [
    "ready",
    "cancel-before-start",
    "cancel-after-pending",
    "allocation-error",
];
pub const PREMISES: [&str; 2] = ["worker-result-source", "active-input-write"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Ready,
    CancelBeforeStart,
    CancelAfterPending,
    AllocationError,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Premise {
    Complete,
    WorkerResultSource,
    ActiveInputWrite,
}

struct Program {
    complete_context: bool,
    relay: bool,
    exit: Exit,
    premise: Premise,
}

impl Program {
    fn parse(value: &Value) -> Result<Self, String> {
        if value.as_object().is_none_or(|fields| fields.len() != 3) {
            return Err("Task program needs exactly shape, exit and premise".into());
        }
        let (complete_context, relay) = match value["shape"].as_str() {
            Some("compact") => (false, false),
            Some("compact-relay") => (false, true),
            Some("complete") => (true, false),
            Some("complete-relay") => (true, true),
            _ => return Err("unknown Task context or relay path".into()),
        };
        let exit = match value["exit"].as_str() {
            Some("ready") => Exit::Ready,
            Some("cancel-before-start") => Exit::CancelBeforeStart,
            Some("cancel-after-pending") => Exit::CancelAfterPending,
            Some("allocation-error") => Exit::AllocationError,
            _ => return Err("unknown Task exit".into()),
        };
        let premise = match value["premise"].as_str() {
            Some("complete") => Premise::Complete,
            Some("worker-result-source") if exit == Exit::Ready => Premise::WorkerResultSource,
            Some("active-input-write") if exit == Exit::Ready => Premise::ActiveInputWrite,
            _ => return Err("unknown Task premise mutation or non-Ready control".into()),
        };
        Ok(Self {
            complete_context,
            relay,
            exit,
            premise,
        })
    }
}

pub fn validate(program: &Value) -> Result<(), String> {
    Program::parse(program).map(|_| ())
}

pub fn contract(program: &Value) -> Result<Value, String> {
    let p = Program::parse(program)?;
    if p.premise != Premise::Complete {
        return Ok(json!({"observation":{"kind":"safe-reject","premise":program["premise"]}}));
    }
    let (stdout, exit, polls, selected) = match p.exit {
        Exit::Ready => (
            "AB",
            42,
            &["Pending", "Ready", "Closed"][..],
            Some("pointer"),
        ),
        Exit::CancelBeforeStart => ("", 17, &["Cancelled", "Closed"][..], None),
        Exit::CancelAfterPending => ("A", 23, &["Pending", "Closed"][..], None),
        Exit::AllocationError => ("", 29, &[][..], None),
    };
    let allocated = p.exit != Exit::AllocationError;
    let stderr = if allocated {
        "frame:allocated\nframe:freed\n"
    } else {
        "frame:allocation-failed\n"
    };
    Ok(json!({
        "publicContextInputCoordinates": if p.complete_context { [1, 2] } else { [0, 1] },
        "workerInputCoordinates": [1, 2],
        "ownerRelays": u8::from(p.relay),
        "polls": polls,
        "selectedResultAfterDestroy": selected,
        "allocationAttempts": 1,
        "releasedFrames": u8::from(allocated),
        "observation": {"kind":"lifecycle","stdout":stdout,"stderr":stderr,"exit":exit},
    }))
}

pub fn expected(program: &Value) -> Result<Value, String> {
    Ok(contract(program)?["observation"].take())
}

pub fn property(program: &Value) -> &'static str {
    match program["premise"].as_str() {
        Some("worker-result-source") => "task.worker-result-source",
        Some("active-input-write") => "task.active-input-write",
        _ => PROPERTY,
    }
}

const PENDING: &str = r#"
    match task_poll(&mut owner){
      .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 91}}},
      .err(_)=>{return 92}
    }
"#;

const DESTROY: &str = r#"
    match task_poll_close(&mut owner){Poll.Pending()=>{return 94},Poll.Ready(_)=>{}}
    task_finish_destroy(give owner)
"#;

const CANCELLED: &str = r#"
    match task_poll(&mut owner){
      .ok(_)=>{return 96},
      .err(state)=>{match state{
        TaskPollState.Cancelled()=>{},
        TaskPollState.Complete()=>{return 97},
        TaskPollState.Invalid()=>{return 98}
      }}
    }
"#;

fn released_inputs(scratch: i32, number: i32, fault: i32) -> String {
    format!(
        r#"
       *scratch_parent={scratch}
       *q={number}
       let released=*scratch_parent
       let changed=*q
       let restored=*p
       return released+changed+restored-{scratch}-{number}+{fault}
    "#
    )
}

pub fn source(program: &Value, fault: bool) -> Result<String, String> {
    let p = Program::parse(program)?;
    let context = if p.complete_context {
        "(i32,&'a i32,&'b i32)"
    } else {
        "(&'a i32,&'b i32)"
    };
    let task = format!("Task<&'b i32,{context}>");
    let relay = if p.relay {
        format!(
            "fn relay<life a,life b>(give frame:{task})->{task} contract{{result(give)}}{{return give frame}}"
        )
    } else {
        String::new()
    };
    let transfer = if p.relay {
        "relay(give frame)"
    } else {
        "give frame"
    };
    let result = if p.premise == Premise::WorkerResultSource {
        "scratch"
    } else {
        "pointer"
    };
    let write = if p.premise == Premise::ActiveInputWrite {
        "*scratch_parent=99"
    } else {
        ""
    };
    let fault = i32::from(fault && p.premise == Premise::Complete);
    let failed = released_inputs(31, 29, fault);
    let successful = match p.exit {
        Exit::Ready | Exit::AllocationError => format!(
            r#"
            {write}
            {PENDING}
            match task_poll(&mut owner){{
                .ok(value)=>{{match value{{
                    Poll.Pending()=>{{return 93}},
                    Poll.Ready(pointer)=>{{
                        {DESTROY}
                        *scratch_parent=10
                        let released=*scratch_parent
                        let observed=*pointer
                        *q=43
                        let changed=*q
                        let restored=*p
                        return observed+released+changed+restored-96+{fault}
                    }}
                }}}},
                .err(_)=>{{return 95}}
            }}
        "#
        ),
        Exit::CancelBeforeStart | Exit::CancelAfterPending => {
            let (pending, cancelled, number) = if p.exit == Exit::CancelAfterPending {
                (PENDING, "", 23)
            } else {
                ("", CANCELLED, 17)
            };
            let released = released_inputs(13, number, fault);
            format!(
                "{pending}\n task_request_cancel(&mut owner)\n {cancelled}\n {DESTROY}\n {released}"
            )
        }
    };
    Ok(format!(
        r#"
extern "C" fn roam_task_emit(byte:i32);
fn worker<life a,life b>(prefix:i32,scratch:&'a i32,pointer:&'b i32)->&'b i32{{
    let local=prefix
    unsafe{{roam_task_emit(65)}}
    task_yield()
    let observed=*scratch
    unsafe{{roam_task_emit(66)}}
    return {result}
}}
fn start<life a,life b>(scratch:&'a i32,pointer:&'b i32)->TaskCreateError!{task} contract{{result(give)}}{{
    return task_create(worker,7,scratch,pointer)
}}
{relay}
fn main()->i32{{
    var scratch=9
    let scratch_parent=&mut scratch
    var number=42
    let p=&mut number
    let q=&mut *p
    let created=start(&*scratch_parent,&*q)
    match created{{
        .ok(frame)=>{{
            var owner={transfer}
            {successful}
        }},
        .err(_)=>{{{failed}}}
    }}
}}
"#
    ))
}

fn program(shape: &str, exit: &str, premise: &str) -> Value {
    json!({"shape":shape,"exit":exit,"premise":premise})
}

fn trace(programs: impl IntoIterator<Item = Value>) -> Value {
    json!({"format":"roam/1","contract":CONTRACT,"steps":programs.into_iter()
        .map(|program|json!({"op":"run","program":program})).collect::<Vec<_>>()})
}

pub fn fixed() -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    for shape in SHAPES {
        for exit in EXITS {
            cases.push((
                format!("{shape}-{exit}"),
                trace([program(shape, exit, "complete")]),
            ));
        }
        for premise in PREMISES {
            cases.push((
                format!("{shape}-reject-{premise}"),
                trace([program(shape, EXITS[0], premise)]),
            ));
        }
    }
    cases
}

pub fn generate(seed: i64, steps: usize) -> Value {
    let mut random = seed as u32;
    let cases = fixed();
    let programs = (0..steps.min(MAX_PROGRAMS)).map(|_| {
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
    for (index, step) in steps.iter().enumerate() {
        let Ok(program) = Program::parse(&step["program"]) else {
            continue;
        };
        let reductions: &[&str] = match (program.complete_context, program.relay) {
            (false, false) => &[],
            (false, true) | (true, false) => &["compact"],
            (true, true) => &["complete", "compact-relay"],
        };
        for shape in reductions {
            let mut reduced = case.clone();
            reduced["steps"][index]["program"]["shape"] = json!(shape);
            candidates.push(reduced);
        }
    }
    candidates
}

pub fn observed(expected: &Value, observation: Value, compile: &Value) -> Value {
    if expected["kind"] != "safe-reject" || compile["status"] != "SafeReject" {
        return observation;
    }
    let diagnostic = &compile["diagnostics"][0];
    let reason = (
        compile["phase"].as_str().unwrap_or(""),
        diagnostic["code"].as_str().unwrap_or(""),
        diagnostic["message"].as_str().unwrap_or(""),
    );
    let matches = match expected["premise"].as_str() {
        Some("worker-result-source") => {
            reason
                == (
                    "types",
                    "E-SEMANTIC-ORIGINAL",
                    "original address changes a rigid lifetime anchor",
                )
                || (reason.0 == "types" && reason.1 == "E-TYPE-MISMATCH")
                || (reason.0 == "analyze"
                    && reason.1 == "E-SAFETY-SOLVE"
                    && reason.2.contains("RegionEscape"))
        }
        Some("active-input-write") => {
            reason
                == (
                    "analyze",
                    "E-BORROW-CONFLICT",
                    "simultaneously live access overlaps an actual stored target/parent loan",
                )
                || (reason.0 == "analyze"
                    && reason.1 == "E-SAFETY-SOLVE"
                    && reason.2.contains("LoanConflict"))
        }
        _ => false,
    };
    if matches {
        expected.clone()
    } else {
        observation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_cases_cover_each_shape_exit_and_nearest_rejection() {
        let cases = fixed();
        assert_eq!(cases.len(), SHAPES.len() * (EXITS.len() + PREMISES.len()));
        for (shape, cases) in SHAPES.into_iter().zip(cases.chunks_exact(6)) {
            for (exit, (_, case)) in EXITS.into_iter().zip(cases) {
                let p = &case["steps"][0]["program"];
                assert_eq!(p, &program(shape, exit, "complete"));
                assert_eq!(expected(p).unwrap()["kind"], "lifecycle");
            }
            for (premise, (_, case)) in PREMISES.into_iter().zip(&cases[4..]) {
                let p = &case["steps"][0]["program"];
                assert_eq!(p, &program(shape, "ready", premise));
                assert_eq!(expected(p).unwrap()["kind"], "safe-reject");
            }
        }
        assert_eq!(generate(51, MAX_PROGRAMS + 1), generate(51, MAX_PROGRAMS));
    }

    #[test]
    fn shrinking_preserves_exit_and_the_single_changed_premise() {
        let original = trace([program("complete-relay", "ready", PREMISES[1])]);
        let candidates = candidates(&original);
        assert_eq!(candidates.len(), 2);
        for candidate in candidates {
            let shrunk = &candidate["steps"][0]["program"];
            assert_eq!(shrunk["exit"], "ready");
            assert_eq!(shrunk["premise"], PREMISES[1]);
            validate(shrunk).unwrap();
        }
        assert!(validate(&program("complete", "allocation-error", PREMISES[1])).is_err());
    }

    #[test]
    fn a_later_or_unrelated_diagnostic_does_not_establish_a_rejection() {
        let expected = expected(&program("compact-relay", "ready", PREMISES[0])).unwrap();
        let failure = json!({"kind":"compile-failure"});
        let intended = json!({"code":"E-SEMANTIC-ORIGINAL", "message":"original address changes a rigid lifetime anchor"});
        let unrelated = json!({"code":"E-SEMANTIC-ORIGINAL", "message":"other declaration"});
        let compile = |status, diagnostics| json!({"phase":"types","status":status,"diagnostics":diagnostics});
        for (status, diagnostics, want) in [
            ("SafeReject", json!([intended.clone()]), &expected),
            ("SafeReject", json!([unrelated, intended.clone()]), &failure),
            ("Unsupported", json!([intended]), &failure),
        ] {
            assert_eq!(
                &observed(&expected, failure.clone(), &compile(status, diagnostics)),
                want
            );
        }
    }
}
