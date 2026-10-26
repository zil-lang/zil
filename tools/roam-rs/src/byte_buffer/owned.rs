//! Versioned owning-carrier model. The C interpreter computes its own result.
use crate::{protocol, search};
use serde_json::{Value, json};

pub const CONTRACT: &str = "zil-byte-buffer-native/2";
pub const MODEL: &str = "owned-byte/1";
pub const PROPERTY: &str = "byte-buffer.owned-c-model/1";
const MAX_OPS: usize = 16;
const OBSERVER: &str = include_str!("observer.c");

#[derive(Clone, Copy, Debug, PartialEq)]
enum Carrier {
    Local,
    Package,
    FnMut,
    FnOnce,
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Reserve(usize, i64),
    Push(usize, u8),
    Fill(usize, usize, usize, u8),
    Relay(usize),
    Reset(usize),
    Replace(usize, i64, u8),
    TakeRefill(usize, i64, u8),
    PrepareError(i64, u8),
}
struct Program {
    carrier: Carrier,
    fail_at: u8,
    take: Option<usize>,
    error: bool,
    early: bool,
    ops: Vec<Op>,
}
fn number(v: &Value, lo: i64, hi: i64) -> Result<i64, String> {
    protocol::integer(v)
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| "owned byte operand outside bounded domain".into())
}
fn parse(v: &Value) -> Result<Program, String> {
    if v["model"] != MODEL {
        return Err("unknown owned byte model".into());
    }
    let carrier = match v["carrier"].as_str() {
        Some("local") => Carrier::Local,
        Some("package") => Carrier::Package,
        Some("fnmut") => Carrier::FnMut,
        Some("fnonce") => Carrier::FnOnce,
        _ => return Err("unknown owned byte carrier".into()),
    };
    let (take, error) = match v["finish"].as_str() {
        Some("normal") => (None, false),
        Some("error") => (None, true),
        Some("take0") => (Some(0), false),
        Some("take1") => (Some(1), false),
        Some("take0-error") => (Some(0), true),
        Some("take1-error") => (Some(1), true),
        _ => return Err("unknown owned byte finish".into()),
    };
    if carrier == Carrier::FnMut && take.is_some() {
        return Err("FnMut must leave its environment complete".into());
    }
    let fail_at = number(&v["failAt"], 0, 64)? as u8;
    let early = v["early"].as_bool().ok_or("early must be bool")?;
    let xs = v["operations"]
        .as_array()
        .filter(|xs| xs.len() <= MAX_OPS)
        .ok_or("owned byte operations exceed sixteen")?;
    let ops = xs
        .iter()
        .map(|o| {
            let slot = || number(&o["slot"], 0, 1).map(|n| n as usize);
            let cap = || number(&o["capacity"], -1, 64);
            let byte = || number(&o["value"], 0, 255).map(|n| n as u8);
            Ok(match o["op"].as_str() {
                Some("reserve") => Op::Reserve(slot()?, cap()?),
                Some("push") => Op::Push(slot()?, byte()?),
                Some("fill") => Op::Fill(
                    slot()?,
                    number(&o["lo"], 0, 64)? as usize,
                    number(&o["hi"], 0, 64)? as usize,
                    byte()?,
                ),
                Some("relay") => Op::Relay(slot()?),
                Some("reset") => Op::Reset(slot()?),
                Some("replace") => Op::Replace(slot()?, cap()?, byte()?),
                Some("take_refill") => Op::TakeRefill(slot()?, cap()?, byte()?),
                Some("prepare_error") => Op::PrepareError(cap()?, byte()?),
                _ => return Err("unknown owned byte operation".into()),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Program {
        carrier,
        fail_at,
        take,
        error,
        early,
        ops,
    })
}
pub fn validate(v: &Value) -> Result<(), String> {
    parse(v).map(|_| ())
}
pub fn fail_at(v: &Value) -> Result<u8, String> {
    parse(v).map(|p| p.fail_at)
}
pub fn observer_source(n: u8) -> Result<String, String> {
    if n > 64 {
        return Err("observer failure position exceeds domain".into());
    }
    Ok(format!("#define ROAM_FAIL_AT {n}u\n{OBSERVER}"))
}

#[derive(Default)]
struct Buffer {
    bytes: Vec<u8>,
    capacity: usize,
    id: usize,
}
struct Model {
    next: usize,
    attempts: usize,
    fail_at: usize,
    output: String,
}
impl Model {
    fn reserve(&mut self, b: &mut Buffer, n: i64) -> bool {
        if n < 0 {
            return false;
        }
        if n as usize <= b.capacity {
            return true;
        }
        self.attempts += 1;
        if self.attempts == self.fail_at {
            self.output += &format!("F{:02X}{n:02X};", b.id);
            return false;
        }
        if b.id == 0 {
            self.next += 1;
            b.id = self.next;
            self.output += &format!("A{:02X}{n:02X};", b.id);
        } else {
            self.output += &format!("G{:02X}{n:02X};", b.id);
        }
        b.capacity = n as usize;
        true
    }
    fn drop_buffer(&mut self, b: &mut Buffer) {
        if b.id != 0 {
            self.output += &format!("D{:02X};", b.id);
        }
        *b = Buffer::default();
    }
    fn fresh(&mut self, n: i64, v: u8) -> (Buffer, bool) {
        let mut b = Buffer::default();
        let ok = self.reserve(&mut b, n);
        if ok && b.capacity > 0 {
            b.bytes.push(v);
        }
        (b, ok)
    }
    fn show(&mut self, b: &[Buffer; 2], ok: bool) {
        self.output.push(if ok { 'T' } else { 'F' });
        for (i, x) in b.iter().enumerate() {
            if i > 0 {
                self.output.push('/');
            }
            self.output += &format!("{:02X}{:02X}", x.bytes.len(), x.capacity);
            for v in &x.bytes {
                self.output += &format!("{v:02X}");
            }
        }
        self.output.push(';');
    }
}
pub fn expected(v: &Value) -> Result<Value, String> {
    let p = parse(v)?;
    let mut m = Model {
        next: 0,
        attempts: 0,
        fail_at: p.fail_at as usize,
        output: String::new(),
    };
    let mut b = [Buffer::default(), Buffer::default()];
    m.show(&b, true);
    for op in p.ops {
        let ok = match op {
            Op::Reserve(s, n) => m.reserve(&mut b[s], n),
            Op::Push(s, v) => {
                if b[s].bytes.len() < b[s].capacity {
                    b[s].bytes.push(v);
                    true
                } else {
                    false
                }
            }
            Op::Fill(s, lo, hi, v) => {
                if lo <= hi && hi <= b[s].bytes.len() {
                    b[s].bytes[lo..hi].fill(v);
                    true
                } else {
                    false
                }
            }
            Op::Relay(_) => true,
            Op::Reset(s) => {
                m.drop_buffer(&mut b[s]);
                true
            }
            Op::Replace(s, n, v) => {
                let (mut fresh, ok) = m.fresh(n, v);
                if ok {
                    m.drop_buffer(&mut b[s]);
                    b[s] = fresh;
                } else {
                    m.drop_buffer(&mut fresh);
                }
                ok
            }
            Op::TakeRefill(s, n, v) => {
                let mut old = std::mem::take(&mut b[s]);
                let (fresh, _) = m.fresh(n, v);
                b[s] = fresh;
                m.drop_buffer(&mut old);
                true
            }
            Op::PrepareError(n, v) => {
                let (mut fresh, _) = m.fresh(n, v);
                m.output += "E;";
                m.drop_buffer(&mut fresh);
                false
            }
        };
        m.show(&b, ok);
    }
    if p.error {
        m.output += "E;";
    }
    if let Some(s) = p.take {
        m.drop_buffer(&mut b[s]);
    }
    for s in if p.carrier == Carrier::Local {
        [1, 0]
    } else {
        [0, 1]
    } {
        m.drop_buffer(&mut b[s]);
    }
    m.output += "L00;";
    Ok(json!({"kind":"lifecycle","stdout":m.output,"stderr":"","exit":if p.early{7}else{0}}))
}

const PRELUDE: &str = r#"
extern "C" fn putchar(c:i32)->i32;
extern "C" fn roam_byte_finish()->i32;
enum Fault{Stop}
fn emit(c:i32){unsafe{putchar(c);}}
fn digit(v:i64){if v<10{emit(i32(v+48));}else{emit(i32(v+55));}}
fn hex(v:i64){digit(v/16);digit(v%16);}
fn dump(b:ByteBuffer){hex(bytes_len(b));hex(bytes_capacity(b));let w=bytes_view(b);var i:i64=0;while i<w.len(){hex(i64(w[i]));i+=1;}}
fn show(a:ByteBuffer,b:ByteBuffer,ok:bool){if ok{emit(84);}else{emit(70);}dump(a);emit(47);dump(b);emit(59);}
fn fill(b:ByteBuffer,lo:i64,hi:i64,v:u8)->bool{if lo>hi||hi>bytes_len(b){return false;}let w=bytes_view_mut(b);let part=&mut w[lo..hi];var i:i64=0;while i<part.len(){part[i]=v;i+=1;}return true;}
fn make(n:i64,v:u8)->ByteBuffer contract{result(create)}{let b=bytes_new();if bytes_reserve(b,n){bytes_push(b,v);}return give b;}
fn relay(give b:ByteBuffer)->ByteBuffer contract{result(give)}{return give b;}
fn consume(give b:ByteBuffer){}
fn stop()->Fault!i32{emit(69);emit(59);return Fault.Stop;}
fn prepare_error(n:i64,v:u8)->Fault!i32{let fresh=make(n,v);return stop()?;}
package Pair{tag:u8;manages left:ByteBuffer;middle:i64;manages right:ByteBuffer;tail:u16}
fn pair()->Pair{return Pair{tag:9u8,left:bytes_new(),middle:i64(101),right:bytes_new(),tail:17u16};}
fn finish_pair(p:Pair,fail:bool)->Fault!i32{if fail{return stop()?;}return 0;}
"#;
fn names(carrier: Carrier) -> [String; 2] {
    if carrier == Carrier::Local {
        ["b0".into(), "b1".into()]
    } else {
        ["p.left".into(), "p.right".into()]
    }
}
fn show_expr(names: &[String; 2], ok: &str) -> String {
    format!("show({},{},{ok});", names[0], names[1])
}
fn action(op: Op, i: usize, names: &[String; 2], mutation: bool, callback: bool) -> String {
    let b = |s: usize| &names[s];
    let (code, ok) = match op {
        Op::Reserve(s, n) => (String::new(), format!("bytes_reserve({},i64({n}))", b(s))),
        Op::Push(s, v) => (
            String::new(),
            if mutation {
                "false".into()
            } else {
                format!("bytes_push({},u8({v}))", b(s))
            },
        ),
        Op::Fill(s, lo, hi, v) => (
            String::new(),
            format!("fill({},i64({lo}),i64({hi}),u8({v}))", b(s)),
        ),
        Op::Relay(s) => (
            format!("let old{i}=give {};{}=relay(give old{i});", b(s), b(s)),
            "true".into(),
        ),
        Op::Reset(s) => (format!("{}=bytes_new();", b(s)), "true".into()),
        Op::Replace(s, n, v) => (
            format!(
                "let fresh{i}=bytes_new();let ready{i}=bytes_reserve(fresh{i},i64({n}));if ready{i}{{bytes_push(fresh{i},u8({v}));{}=give fresh{i};}}",
                b(s)
            ),
            format!("ready{i}"),
        ),
        Op::TakeRefill(s, n, v) => (
            format!(
                "let old{i}=give {};{}=make(i64({n}),u8({v}));consume(give old{i});",
                b(s),
                b(s)
            ),
            "true".into(),
        ),
        Op::PrepareError(n, v) if callback => {
            return format!("let fresh{i}=make(i64({n}),u8({v}));return stop()?;");
        }
        Op::PrepareError(n, v) => (
            format!("let failed{i}=prepare_error(i64({n}),u8({v}));"),
            "false".into(),
        ),
    };
    format!(
        "{code}let ok{i}={ok};{}",
        show_expr(names, &format!("ok{i}"))
    )
}
fn ending(p: &Program, names: &[String; 2]) -> String {
    if p.carrier == Carrier::FnOnce && p.take.is_none() {
        return format!("return finish_pair(p,{})?;", p.error);
    }
    let mut s = String::new();
    if let Some(slot) = p.take {
        s += &format!("let detached=give {};", names[slot]);
    }
    if p.error {
        s += "return stop()?;";
    } else {
        if p.take.is_some() {
            s += "consume(give detached);";
        }
        s += "return 0;";
    }
    s
}
// Managed local replacement is deliberately not a language feature. Each
// bounded step transfers the two real owners through ordinary give parameters;
// replacement creates a fresh local before consuming the old owner. The final
// parameter frame has the declared b0,b1 order, hence reverse local cleanup.
fn local_source(p: &Program, mutation: bool) -> String {
    let mut text = String::new();
    for (i, op) in p.ops.iter().enumerate() {
        let mut n = names(Carrier::Local);
        text += &format!("fn step{i}(give b0:ByteBuffer,give b1:ByteBuffer)->Fault!i32{{");
        let next = |n: &[String; 2]| format!("return step{}(give {},give {})?;", i + 1, n[0], n[1]);
        match *op {
            Op::Relay(slot) => {
                text += &format!("let moved=relay(give {});", n[slot]);
                n[slot] = "moved".into();
                text += &show_expr(&n, "true");
            }
            Op::Reset(slot) | Op::TakeRefill(slot, _, _) => {
                text += match *op {
                    Op::TakeRefill(_, cap, value) => {
                        format!("let fresh=make(i64({cap}),u8({value}));")
                    }
                    _ => "let fresh=bytes_new();".into(),
                }
                .as_str();
                text += &format!("consume(give {});", n[slot]);
                n[slot] = "fresh".into();
                text += &show_expr(&n, "true");
            }
            Op::Replace(slot, cap, value) => {
                text += &format!(
                    "let fresh=bytes_new();let ready=bytes_reserve(fresh,i64({cap}));if ready{{bytes_push(fresh,u8({value}));consume(give {});",
                    n[slot]
                );
                let mut changed = n.clone();
                changed[slot] = "fresh".into();
                text += &show_expr(&changed, "true");
                text += &next(&changed);
                text += "}";
                text += &show_expr(&n, "false");
            }
            _ => text += &action(*op, i, &n, mutation, false),
        }
        text += &next(&n);
        text += "}";
    }
    text += &format!(
        "fn step{}(give b0:ByteBuffer,give b1:ByteBuffer)->Fault!i32{{{}}}",
        p.ops.len(),
        ending(p, &names(Carrier::Local))
    );
    text += "fn work()->Fault!i32{let b0=bytes_new();let b1=bytes_new();show(b0,b1,true);return step0(give b0,give b1)?;}";
    text
}
pub fn source(v: &Value, mutation: bool) -> Result<String, String> {
    let p = parse(v)?;
    let n = names(p.carrier);
    let mut text = PRELUDE.to_string();
    if p.carrier == Carrier::Local {
        text += &local_source(&p, mutation);
    } else if p.carrier == Carrier::FnMut {
        // The finite input may shrink to zero mutations. Keep the declared
        // opaque FnMut kind exact without relying on unsupported Fn adaptation.
        text += "fn factory()->impl FnMut(i64)->Fault!i32{var p=pair();var calls:i64=0;return move(index)=>{calls+=1;";
        text += &format!(
            "if index==i64(-2){{{}return 0;}}if index==i64(-1){{{}return 0;}}",
            show_expr(&n, "true"),
            show_expr(&n, "false")
        );
        for (i, op) in p.ops.iter().enumerate() {
            text += &format!(
                "if index==i64({i}){{{}{}}}",
                action(*op, i, &n, mutation, true),
                if matches!(op, Op::PrepareError(..)) {
                    ""
                } else {
                    "return 0;"
                }
            );
        }
        text += "return stop()?;};}fn work()->Fault!i32{var f=factory();let initial=f(i64(-2));";
        for (i, op) in p.ops.iter().enumerate() {
            text += &format!("let result{i}=f(i64({i}));");
            if matches!(op, Op::PrepareError(..)) {
                text += &format!("let after{i}=f(i64(-1));");
            }
        }
        text += if p.error {
            "return f(i64(-3))?;}"
        } else {
            "return 0;}"
        };
    } else {
        if p.carrier == Carrier::FnOnce {
            text += "fn factory()->impl FnOnce()->Fault!i32{var p=pair();return move()=>{";
        } else {
            text += "fn work()->Fault!i32{";
            text += if p.carrier == Carrier::Local {
                "var b0=bytes_new();var b1=bytes_new();"
            } else {
                "var p=pair();"
            };
        }
        text += &show_expr(&n, "true");
        for (i, op) in p.ops.iter().enumerate() {
            text += &action(*op, i, &n, mutation, false);
        }
        text += &ending(&p, &n);
        text += "}";
        if p.carrier == Carrier::FnOnce {
            text += ";}fn work()->Fault!i32{let f=factory();return f()?;}";
        }
    }
    text += &format!(
        "fn main()->i32{{let result=work();unsafe{{if roam_byte_finish()!=0{{return 92;}}}}return {};}}",
        if p.early { 7 } else { 0 }
    );
    Ok(text)
}

pub fn c_source(v: &Value) -> Result<String, String> {
    let p = parse(v)?;
    let mut rows = vec!["{0,0,0,0,0}".to_string()];
    for op in &p.ops {
        rows.push(match op {
            Op::Reserve(s, n) => format!("{{1,{s},{n},0,0}}"),
            Op::Push(s, v) => format!("{{2,{s},{v},0,0}}"),
            Op::Fill(s, lo, hi, v) => format!("{{3,{s},{lo},{hi},{v}}}"),
            Op::Relay(s) => format!("{{4,{s},0,0,0}}"),
            Op::Reset(s) => format!("{{5,{s},0,0,0}}"),
            Op::Replace(s, n, v) => format!("{{6,{s},{n},{v},0}}"),
            Op::TakeRefill(s, n, v) => format!("{{7,{s},{n},{v},0}}"),
            Op::PrepareError(n, v) => format!("{{8,0,{n},{v},0}}"),
        });
    }
    Ok(format!(
        r#"
#include <stdio.h>
#include <string.h>
typedef struct{{unsigned char bytes[64];unsigned len,cap,id;}}Buffer;
typedef struct{{int op,slot,a,b,c;}}Op;
static const Op operations[]={{{rows}}};
static Buffer buffers[2];static unsigned next_id,attempts;
static int reserve(Buffer *b,int n){{if(n<0)return 0;if((unsigned)n<=b->cap)return 1;attempts++;if(attempts=={fail}u){{printf("F%02X%02X;",b->id,(unsigned)n);return 0;}}if(!b->id){{b->id=++next_id;printf("A%02X%02X;",b->id,(unsigned)n);}}else printf("G%02X%02X;",b->id,(unsigned)n);b->cap=(unsigned)n;return 1;}}
static void drop(Buffer *b){{if(b->id)printf("D%02X;",b->id);memset(b,0,sizeof(*b));}}
static int fresh(Buffer *b,int n,unsigned char value){{memset(b,0,sizeof(*b));int ok=reserve(b,n);if(ok&&b->cap)b->bytes[b->len++]=value;return ok;}}
static void show(int ok){{putchar(ok?'T':'F');for(unsigned s=0;s<2;s++){{if(s)putchar('/');Buffer *b=&buffers[s];printf("%02X%02X",b->len,b->cap);for(unsigned i=0;i<b->len;i++)printf("%02X",(unsigned)b->bytes[i]);}}putchar(';');}}
int main(void){{show(1);for(unsigned i=1;i<sizeof(operations)/sizeof(operations[0]);i++){{Op o=operations[i];Buffer *b=&buffers[o.slot],temporary;int ok=1;switch(o.op){{
case 1:ok=reserve(b,o.a);break;
case 2:if(b->len==b->cap)ok=0;else b->bytes[b->len++]=(unsigned char)o.a;break;
case 3:if(o.a>o.b||(unsigned)o.b>b->len)ok=0;else for(int j=o.a;j<o.b;j++)b->bytes[j]=(unsigned char)o.c;break;
case 4:break;
case 5:drop(b);break;
case 6:ok=fresh(&temporary,o.a,(unsigned char)o.b);if(ok){{drop(b);*b=temporary;}}else drop(&temporary);break;
case 7:temporary=*b;fresh(b,o.a,(unsigned char)o.b);drop(&temporary);break;
case 8:fresh(&temporary,o.a,(unsigned char)o.b);fputs("E;",stdout);drop(&temporary);ok=0;break;
default:return 99;}}show(ok);}}
{error}{take}drop(&buffers[{first}]);drop(&buffers[{second}]);fputs("L00;",stdout);return {exit};}}
"#,
        rows = rows.join(","),
        fail = p.fail_at,
        error = if p.error { "fputs(\"E;\",stdout);" } else { "" },
        take = p
            .take
            .map(|s| format!("drop(&buffers[{s}]);"))
            .unwrap_or_default(),
        first = if p.carrier == Carrier::Local { 1 } else { 0 },
        second = if p.carrier == Carrier::Local { 0 } else { 1 },
        exit = if p.early { 7 } else { 0 }
    ))
}

pub fn program(
    carrier: &str,
    operations: Vec<Value>,
    fail_at: u8,
    finish: &str,
    early: bool,
) -> Value {
    json!({"model":MODEL,"carrier":carrier,"operations":operations,"failAt":fail_at,"finish":finish,"early":early})
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
    let programs=(0..steps).map(|_|{
        let carrier=["local","package","fnmut","fnonce"][next(4)];let mut ops=Vec::new();
        for _ in 0..MAX_OPS{let slot=next(2);ops.push(match next(10){0=>json!({"op":"relay","slot":slot}),1=>json!({"op":"reset","slot":slot}),2=>json!({"op":"replace","slot":slot,"capacity":next(10),"value":next(256)}),3=>json!({"op":"take_refill","slot":slot,"capacity":next(10),"value":next(256)}),4=>json!({"op":"prepare_error","capacity":next(10),"value":next(256)}),5=>json!({"op":"fill","slot":slot,"lo":next(5),"hi":next(9),"value":next(256)}),6|7=>json!({"op":"reserve","slot":slot,"capacity":next(66)as i64-1}),_=>json!({"op":"push","slot":slot,"value":next(256)})});}
        let finish=if carrier=="fnmut"{["normal","error"][next(2)]}else{["normal","error","take0","take1","take0-error","take1-error"][next(6)]};program(carrier,ops,0,finish,next(2)==1)
    }).collect();
    trace(programs, Some(seed))
}
pub fn fixed() -> Vec<(String, Value)> {
    let ops = vec![
        json!({"op":"reserve","slot":0,"capacity":2}),
        json!({"op":"push","slot":0,"value":65}),
        json!({"op":"reserve","slot":1,"capacity":3}),
        json!({"op":"push","slot":1,"value":66}),
        json!({"op":"reserve","slot":0,"capacity":4}),
        json!({"op":"fill","slot":0,"lo":0,"hi":1,"value":90}),
        json!({"op":"relay","slot":0}),
        json!({"op":"replace","slot":1,"capacity":5,"value":67}),
        json!({"op":"prepare_error","capacity":7,"value":68}),
        json!({"op":"push","slot":0,"value":69}),
        json!({"op":"take_refill","slot":1,"capacity":6,"value":70}),
    ];
    ["local", "package", "fnmut", "fnonce"]
        .into_iter()
        .map(|carrier| {
            (
                format!("owned-{carrier}"),
                trace(
                    vec![
                        program(carrier, ops.clone(), 0, "normal", false),
                        program(
                            carrier,
                            ops.clone(),
                            3,
                            if carrier == "fnmut" {
                                "error"
                            } else {
                                "take1-error"
                            },
                            true,
                        ),
                    ],
                    None,
                ),
            )
        })
        .collect()
}
pub fn mutation_case() -> Value {
    trace(
        vec![program(
            "package",
            vec![
                json!({"op":"reserve","slot":0,"capacity":3}),
                json!({"op":"push","slot":0,"value":65}),
                json!({"op":"relay","slot":0}),
            ],
            0,
            "take0-error",
            true,
        )],
        Some(102),
    )
}
pub fn candidates(trace: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(xs) = trace["steps"].as_array() {
        for (i, s) in xs.iter().enumerate() {
            if parse(&s["program"]).is_err() {
                continue;
            }
            for j in 0..s["program"]["operations"].as_array().unwrap().len() {
                let mut c = trace.clone();
                c["steps"][i]["program"]["operations"]
                    .as_array_mut()
                    .unwrap()
                    .remove(j);
                out.push(c);
            }
            for (key, value) in [
                ("early", json!(false)),
                ("failAt", json!(0)),
                ("finish", json!("normal")),
            ] {
                if s["program"][key] != value {
                    let mut c = trace.clone();
                    c["steps"][i]["program"][key] = value;
                    out.push(c);
                }
            }
        }
    }
    out.truncate(128);
    out
}
