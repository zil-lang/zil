//! Bounded independent byte-storage model and two source emitters.
use crate::{protocol, search};
use serde_json::{Value, json};

pub mod owned;

pub const CONTRACT: &str = "zil-byte-buffer-native/1";
pub const PROPERTY: &str = "byte-buffer.native-c-model";
pub const MAX_PROGRAMS: usize = 8;
const MAX_OPS: usize = 16;

#[derive(Clone, Copy)]
enum Op {
    Reserve(i64),
    Push(u8),
    Fill(usize, usize, u8),
    Relay,
    Reset,
}
fn number(v: &Value, low: i64, high: i64) -> Result<i64, String> {
    protocol::integer(v)
        .filter(|n| (low..=high).contains(n))
        .ok_or_else(|| "byte-storage operand outside bounded domain".into())
}
fn parse(program: &Value) -> Result<(Vec<Op>, bool), String> {
    let xs = program["operations"]
        .as_array()
        .filter(|xs| xs.len() <= MAX_OPS)
        .ok_or("byte-storage operations must be an array of at most 16")?;
    let ops = xs
        .iter()
        .map(|v| match v["op"].as_str() {
            Some("reserve") => Ok(Op::Reserve(number(&v["capacity"], -1, 64)?)),
            Some("push") => Ok(Op::Push(number(&v["value"], 0, 255)? as u8)),
            Some("fill") => Ok(Op::Fill(
                number(&v["lo"], 0, 64)? as usize,
                number(&v["hi"], 0, 64)? as usize,
                number(&v["value"], 0, 255)? as u8,
            )),
            Some("relay") => Ok(Op::Relay),
            Some("reset") => Ok(Op::Reset),
            _ => Err("unknown byte-storage operation".into()),
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((ops, program["early"].as_bool().ok_or("early must be bool")?))
}
pub fn validate(program: &Value) -> Result<(), String> {
    parse(program).map(|_| ())
}
pub fn expected(program: &Value) -> Result<Value, String> {
    let (ops, early) = parse(program)?;
    let (mut bytes, mut capacity) = (Vec::<u8>::new(), 0usize);
    let mut stdout = String::new();
    let mut snapshot = |ok: bool, bytes: &[u8], capacity: usize| {
        stdout += &format!(
            "{}{:02X}{capacity:02X}",
            if ok { 'T' } else { 'F' },
            bytes.len()
        );
        for byte in bytes {
            stdout += &format!("{byte:02X}");
        }
        stdout.push(';');
    };
    snapshot(true, &bytes, capacity);
    for op in ops {
        let ok = match op {
            Op::Reserve(n) if n < 0 => false,
            Op::Reserve(n) => {
                capacity = capacity.max(n as usize);
                true
            }
            Op::Push(v) if bytes.len() < capacity => {
                bytes.push(v);
                true
            }
            Op::Push(_) => false,
            Op::Fill(lo, hi, v) if lo <= hi && hi <= bytes.len() => {
                bytes[lo..hi].fill(v);
                true
            }
            Op::Fill(..) => false,
            Op::Relay => true,
            Op::Reset => {
                bytes.clear();
                capacity = 0;
                true
            }
        };
        snapshot(ok, &bytes, capacity);
    }
    Ok(json!({"kind":"lifecycle","stdout":stdout,"stderr":"","exit":if early {7} else {0}}))
}
pub fn source(program: &Value, mutation: bool) -> Result<String, String> {
    let (ops, early) = parse(program)?;
    let mut body = String::from("let b0=bytes_new();dump(b0,true);\n");
    let mut carrier = 0;
    for (i, op) in ops.iter().enumerate() {
        let b = format!("b{carrier}");
        let expr = match op {
            Op::Reserve(n) => format!("bytes_reserve({b},i64({n}))"),
            Op::Push(v) if !mutation => format!("bytes_push({b},u8({v}))"),
            Op::Push(_) => "false".into(),
            Op::Fill(lo, hi, v) => format!("fill({b},i64({lo}),i64({hi}),u8({v}))"),
            Op::Relay | Op::Reset => {
                carrier += 1;
                body += &match op {
                    Op::Relay => format!("let b{carrier}=relay(give {b});\n"),
                    _ => format!("consume(give {b});let b{carrier}=bytes_new();\n"),
                };
                "true".into()
            }
        };
        body += &format!("let ok{i}={expr};dump(b{carrier},ok{i});\n");
    }
    body += if early {
        "if true {return 7;}return 0;"
    } else {
        "return 0;"
    };
    Ok(format!(
        r#"
extern "C" fn putchar(c:i32)->i32;
fn emit(c:i32){{unsafe{{putchar(c);}}}}
fn digit(v:i64){{if v<10 {{emit(i32(v+48));}}else{{emit(i32(v+55));}}}}
fn hex(v:i64){{digit(v/16);digit(v%16);}}
fn dump(b:ByteBuffer,ok:bool){{
    if ok {{emit(84);}}else{{emit(70);}}
    hex(bytes_len(b));hex(bytes_capacity(b));
    let view=bytes_view(b);var i:i64=0;
    while i<view.len(){{hex(i64(view[i]));i+=1;}}emit(59);
}}
fn fill(b:ByteBuffer,lo:i64,hi:i64,v:u8)->bool{{
    if lo>hi || hi>bytes_len(b){{return false;}}
    let view=bytes_view_mut(b);let part=&mut view[lo..hi];var i:i64=0;
    while i<part.len(){{part[i]=v;i+=1;}}return true;
}}
fn relay(give b:ByteBuffer)->ByteBuffer contract{{result(give)}}{{return give b;}}
fn consume(give b:ByteBuffer){{}}
fn main()->i32{{{body}}}
"#
    ))
}
/// The C reference interprets operations on a bounded array. No oracle output
/// or ZIL compiler result enters this independent implementation.
pub fn c_source(program: &Value) -> Result<String, String> {
    let (ops, early) = parse(program)?;
    let mut rows = vec!["{0,0,0,0}".to_string()];
    for op in &ops {
        rows.push(match op {
            Op::Reserve(n) => format!("{{1,{n},0,0}}"),
            Op::Push(v) => format!("{{2,{v},0,0}}"),
            Op::Fill(lo, hi, v) => format!("{{3,{lo},{hi},{v}}}"),
            Op::Relay => "{4,0,0,0}".into(),
            Op::Reset => "{5,0,0,0}".into(),
        });
    }
    Ok(format!(
        r#"
#include <stdio.h>
typedef struct {{int kind,a,b,c;}} Op;
static const Op ops[]={{{rows}}};
static unsigned char bytes[64];
static unsigned length,capacity;
static void show(int ok){{
    printf("%c%02X%02X",ok?'T':'F',length,capacity);
    for(unsigned i=0;i<length;i++)printf("%02X",(unsigned)bytes[i]);
    putchar(';');
}}
int main(void){{
    show(1);
    for(unsigned i=1;i<sizeof(ops)/sizeof(ops[0]);i++){{
        Op o=ops[i];int ok=1;
        switch(o.kind){{
        case 1:if(o.a<0)ok=0;else if((unsigned)o.a>capacity)capacity=(unsigned)o.a;break;
        case 2:if(length==capacity)ok=0;else bytes[length++]=(unsigned char)o.a;break;
        case 3:if(o.a>o.b || (unsigned)o.b>length)ok=0;
               else for(int j=o.a;j<o.b;j++)bytes[j]=(unsigned char)o.c;break;
        case 4:break;
        case 5:length=capacity=0;break;
        default:return 99;
        }}show(ok);
    }}return {exit};
}}
"#,
        rows = rows.join(","),
        exit = if early { 7 } else { 0 }
    ))
}
pub fn program(operations: Vec<Value>, early: bool) -> Value {
    json!({"operations":operations,"early":early})
}
pub fn trace(programs: Vec<Value>, seed: Option<i64>) -> Value {
    let mut t = json!({"format":"roam/1","contract":CONTRACT,"steps":programs.into_iter().map(|program|json!({"op":"run","program":program})).collect::<Vec<_>>()});
    if let Some(seed) = seed {
        t["seed"] = json!(seed);
    }
    t
}
pub fn generate(seed: i64, steps: usize) -> Value {
    let mut state = seed as u32;
    let mut next = |n: usize| ((search::random_next(&mut state) as u64 * n as u64) >> 32) as usize;
    let programs = (0..steps)
        .map(|_| {
            let mut ops = vec![json!({"op":"reserve","capacity":next(9)})];
            for _ in 1..MAX_OPS {
                ops.push(match next(7) {
                    0 => json!({"op":"reserve","capacity":next(66) as i64-1}),
                    1 => json!({"op":"relay"}),
                    2 => json!({"op":"reset"}),
                    3 => json!({"op":"fill","lo":next(5),"hi":next(9),"value":next(256)}),
                    _ => json!({"op":"push","value":next(256)}),
                });
            }
            program(ops, next(2) == 1)
        })
        .collect();
    trace(programs, Some(seed))
}
pub fn fixed() -> Vec<(String, Value)> {
    let cases = vec![
        vec![
            json!({"op":"push","value":255}),
            json!({"op":"reserve","capacity":-1}),
            json!({"op":"fill","lo":0,"hi":0,"value":0}),
        ],
        vec![
            json!({"op":"reserve","capacity":2}),
            json!({"op":"push","value":0}),
            json!({"op":"push","value":255}),
            json!({"op":"push","value":1}),
            json!({"op":"reserve","capacity":1}),
            json!({"op":"fill","lo":0,"hi":2,"value":128}),
            json!({"op":"reserve","capacity":-1}),
            json!({"op":"relay"}),
            json!({"op":"reserve","capacity":64}),
            json!({"op":"push","value":7}),
        ],
        vec![
            json!({"op":"reserve","capacity":4}),
            json!({"op":"push","value":65}),
            json!({"op":"fill","lo":2,"hi":1,"value":7}),
            json!({"op":"fill","lo":0,"hi":2,"value":7}),
            json!({"op":"reset"}),
            json!({"op":"reserve","capacity":0}),
            json!({"op":"relay"}),
        ],
    ];
    cases
        .into_iter()
        .enumerate()
        .map(|(i, ops)| {
            (
                format!("byte-buffer-{i}"),
                trace(vec![program(ops.clone(), false), program(ops, true)], None),
            )
        })
        .collect()
}
pub fn mutation_case() -> Value {
    trace(
        vec![program(
            vec![
                json!({"op":"reserve","capacity":3}),
                json!({"op":"push","value":65}),
                json!({"op":"relay"}),
            ],
            true,
        )],
        Some(98),
    )
}
pub fn candidates(trace: &Value) -> Vec<Value> {
    if trace["contract"] == owned::CONTRACT {
        return owned::candidates(trace);
    }
    let mut out = Vec::new();
    if let Some(steps) = trace["steps"].as_array() {
        for (i, action) in steps.iter().enumerate() {
            if parse(&action["program"]).is_err() {
                continue;
            }
            for j in 0..action["program"]["operations"].as_array().unwrap().len() {
                let mut t = trace.clone();
                t["steps"][i]["program"]["operations"]
                    .as_array_mut()
                    .unwrap()
                    .remove(j);
                out.push(t);
            }
            if action["program"]["early"] == true {
                let mut t = trace.clone();
                t["steps"][i]["program"]["early"] = json!(false);
                out.push(t);
            }
        }
    }
    out.truncate(128);
    out
}
