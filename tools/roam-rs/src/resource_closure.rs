//! Independent finite lifecycle oracle; no compiler safety fact is an input.
use crate::{protocol, search};
use serde_json::{Value, json};
pub const CONTRACT: &str = "zil-resource-closure-native/1";
pub const PROPERTY: &str = "resource-closure.native-equals-model";
pub const MAX_PROGRAMS: usize = 32;
const MODES: [&str; 6] = [
    "shared",
    "descriptor",
    "mutable",
    "consume",
    "uncalled",
    "transfer",
];
struct Program {
    mode: String,
    initial: i64,
    calls: usize,
    early: bool,
}
impl Program {
    fn parse(value: &Value) -> Result<Self, String> {
        let mode = value["mode"]
            .as_str()
            .filter(|mode| MODES.contains(mode))
            .ok_or("unknown resource closure mode")?;
        let initial = protocol::integer(&value["initial"])
            .filter(|n| (65..=90).contains(n))
            .ok_or("resource initial byte must be 65..90")?;
        let calls = protocol::integer(&value["calls"])
            .filter(|n| (0..=4).contains(n))
            .ok_or("resource call count must be 0..4")? as usize;
        if (mode == "consume" && calls > 1) || (mode == "uncalled" && calls != 0) {
            return Err("invalid lifecycle call count".into());
        }
        let early = value["early"].as_bool().ok_or("early flag must be bool")?;
        Ok(Self {
            mode: mode.into(),
            initial,
            calls,
            early,
        })
    }
    fn expected(&self) -> Value {
        let mut bytes = Vec::new();
        let mut value = self.initial as u8;
        match self.mode.as_str() {
            "uncalled" => bytes.extend([value, value + 10]),
            "consume" if self.calls == 0 => bytes.extend([value + 10, value]),
            "consume" => bytes.extend([value + 1, value + 10]),
            _ => {
                for _ in 0..self.calls {
                    if matches!(self.mode.as_str(), "descriptor" | "mutable") {
                        value += 1;
                    }
                    bytes.push(value);
                }
                bytes.push(value);
            }
        }
        json!({"kind":"lifecycle","stdout":String::from_utf8(bytes).unwrap(),"stderr":"","exit":if self.early {7} else {0}})
    }
}
pub fn validate(program: &Value) -> Result<(), String> {
    Program::parse(program).map(|_| ())
}
pub fn expected(program: &Value) -> Result<Value, String> {
    Ok(Program::parse(program)?.expected())
}
pub fn source(program: &Value, mutation: bool) -> Result<String, String> {
    let p = Program::parse(program)?;
    let calls = if mutation { 0 } else { p.calls };
    let mut body = format!("let r=make({});\n", p.initial);
    match p.mode.as_str() {
        "shared" => body += "let f=move()=>read(r);\n",
        "descriptor" => body += "let f=()=>read(r);\n",
        "mutable" => body += "var f=move()=>{bump(r,1);read(r)};\n",
        "transfer" => body += "let first=move()=>read(r);let f=first;\n",
        "uncalled" => {
            body += &format!(
                "let s=make({});let f=move()->i32=>{{read(r);read(s)}};\n",
                p.initial + 10
            )
        }
        "consume" => {
            body += &format!(
                "let s=make({});let f=move()=>{{read(s);consume(give r);}};\n",
                p.initial + 10
            )
        }
        _ => unreachable!(),
    }
    for _ in 0..calls {
        if p.mode == "descriptor" {
            body += "bump(r,1);\n";
        }
        if p.mode == "consume" {
            body += "f();\n";
        } else {
            body += "emit(f());\n";
        }
    }
    body += if p.early {
        "if true {return 7;} return 0;"
    } else {
        "return 0;"
    };
    Ok(format!(
        r#"
extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32) {{unsafe {{putchar(c);}}}}
resource R {{code:i32}}
impl Drop for R {{fn drop(self:Self) {{emit(.code);}}}}
fn make(code:i32)->R contract {{result(create)}} {{return R{{code}};}}
fn read(r:R)->i32 {{return r.code;}}
fn bump(r:R,n:i32) {{r.code=r.code+n;}}
fn consume(give r:R) {{bump(r,1);}}
fn main()->i32 {{{body}}}
"#
    ))
}
pub fn program(mode: &str, initial: i64, calls: usize, early: bool) -> Value {
    json!({"mode":mode,"initial":initial,"calls":calls,"early":early})
}
pub fn trace(programs: Vec<Value>, seed: Option<i64>) -> Value {
    let mut value = json!({"format":"roam/1","contract":CONTRACT,"steps":programs.into_iter().map(|program|json!({"op":"run","program":program})).collect::<Vec<_>>()});
    if let Some(seed) = seed {
        value["seed"] = json!(seed);
    }
    value
}
pub fn generate(seed: i64, steps: usize) -> Value {
    let mut random = seed as u32;
    let mut next = |n: usize| ((search::random_next(&mut random) as u64 * n as u64) >> 32) as usize;
    let programs = (0..steps)
        .map(|_| {
            let mode = MODES[next(MODES.len())];
            let calls = match mode {
                "uncalled" => 0,
                "consume" => next(2),
                _ => next(5),
            };
            program(mode, 65 + next(26) as i64, calls, next(2) == 1)
        })
        .collect();
    trace(programs, Some(seed))
}
pub fn fixed() -> Vec<(String, Value)> {
    MODES
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            let max = match *mode {
                "consume" => 1,
                "uncalled" => 0,
                _ => 4,
            };
            (
                format!("resource-closure-{i}"),
                trace(
                    vec![program(mode, 65, 0, false), program(mode, 90, max, true)],
                    None,
                ),
            )
        })
        .collect()
}
pub fn mutation_case() -> Value {
    trace(vec![program("mutable", 65, 2, false)], Some(53))
}
pub fn candidates(trace: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(actions) = trace["steps"].as_array() else {
        return out;
    };
    for (index, action) in actions.iter().enumerate() {
        let Ok(p) = Program::parse(&action["program"]) else {
            continue;
        };
        if p.calls > 0 {
            let mut t = trace.clone();
            t["steps"][index]["program"]["calls"] = json!(p.calls - 1);
            out.push(t);
        }
        if p.initial > 65 {
            let mut t = trace.clone();
            t["steps"][index]["program"]["initial"] = json!(65);
            out.push(t);
        }
        if p.early {
            let mut t = trace.clone();
            t["steps"][index]["program"]["early"] = json!(false);
            out.push(t);
        }
    }
    out.truncate(128);
    out
}
